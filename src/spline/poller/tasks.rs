//! Background task spawners: blockhash refresh, wallet WSS, balance fetch,
//! trader orders WS, and the Phoenix L2 book RPC subscription.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use phoenix_rise::accounts::{
    ConditionalOrderCollection, ConditionalOrderTrigger, StopLossDirection, StopLossOrderKind,
    StopLossTradeSide,
};
use phoenix_rise::types::{
    TraderStatePayload, TraderStateRowChangeKind, TraderStateStopLossTrigger,
};
use phoenix_rise::{
    get_conditional_orders_address, Direction, PhoenixHttpClient, PhoenixWSClient, Trader,
    TraderKey,
};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_types::config::RpcAccountInfoConfig;
use solana_signer::Signer;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tracing::warn;

use super::super::config::{rpc_http_url_from_env, ws_url_from_env, SplineConfig};
use super::super::format::pubkey_trader_prefix;
use super::super::gti::GtiHandle;
use super::super::parse::{parse_l2_book_from_market_account, L2Level};
use super::super::state::{BalanceUpdate, ClobLevel, L2BookStreamMsg, TxStatusMsg};
use super::super::top_positions::fetch_top_positions;
use super::super::trading::{
    fetch_phoenix_balance_and_position, OrderInfo, TopPositionEntry, TradingSide,
};
use super::super::tx::TxContext;
use super::{TxCtxMsg, L2_EMIT_MIN_INTERVAL, L2_SNAPSHOT_DEPTH, WSS_RETRY_CAP, WSS_RETRY_INIT};

pub(super) fn spawn_tx_context_task(
    kp: Arc<Keypair>,
    symbol: String,
    http: Arc<PhoenixHttpClient>,
    ctx_chan: UnboundedSender<TxCtxMsg>,
    status_chan: UnboundedSender<TxStatusMsg>,
) -> tokio::task::JoinHandle<()> {
    // `kp.pubkey()` is a v3 `Address`; the channel carries v2 `Pubkey` to stay
    // consistent with the rest of the Phoenix-side code. String bridge mirrors
    // the conversion done elsewhere.
    let wallet = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
        Ok(pk) => pk,
        Err(e) => {
            warn!(error = %e, "failed to convert wallet pubkey for TxContext task");
            // Return a no-op task so the caller always has a handle — avoids an
            // `Option<JoinHandle>` everywhere for a pure-logic failure.
            return tokio::spawn(async {});
        }
    };
    tokio::spawn(async move {
        match TxContext::new(&kp, &symbol, &http).await {
            Ok(ctx) => {
                let ctx = Arc::new(ctx);
                let _ = ctx_chan.send((wallet, symbol, ctx));
            }
            Err(e) => {
                warn!(error = %e, "TxContext init failed");
                let _ = status_chan.send(TxStatusMsg::SetStatus {
                    title: format!("Failed to load trading context: {}", e),
                    detail: String::new(),
                });
            }
        }
    })
}

pub(super) fn spawn_blockhash_refresh_task(
    tx_context: Arc<TxContext>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1800));
        loop {
            interval.tick().await;
            tx_context.push_blockhash().await;
        }
    })
}

/// Subscribes to both the USDC ATA and the SOL wallet account on a single
/// shared `PubsubClient` connection. Both initial balances are fetched with one
/// `RpcClient`.
pub(super) fn spawn_wallet_wss(
    pubkey_bytes: [u8; 32],
    ws_url: String,
    usdc_tx: UnboundedSender<f64>,
    sol_tx: UnboundedSender<f64>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use solana_rpc_client::nonblocking::rpc_client::RpcClient;

        // Derive USDC ATA.
        let token_program_id =
            solana_pubkey::Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
                .expect("valid token program id");
        let ata_program_id =
            solana_pubkey::Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
                .expect("valid ata program id");
        let usdc_mint =
            solana_pubkey::Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
                .expect("valid usdc mint id");
        let wallet_pk = solana_pubkey::Pubkey::from(pubkey_bytes);
        let (ata, _) = solana_pubkey::Pubkey::find_program_address(
            &[
                pubkey_bytes.as_ref(),
                token_program_id.as_ref(),
                usdc_mint.as_ref(),
            ],
            &ata_program_id,
        );

        let account_cfg = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::processed()),
            ..Default::default()
        };

        // One RPC client for both initial balance fetches.
        {
            let rpc_client = RpcClient::new_with_commitment(
                rpc_http_url_from_env(),
                CommitmentConfig::processed(),
            );
            if let Ok(res) = rpc_client.get_token_account_balance(&ata).await {
                let _ = usdc_tx.send(res.ui_amount.unwrap_or(0.0));
            }
            if let Ok(lamports) = rpc_client.get_balance(&wallet_pk).await {
                let _ = sol_tx.send(lamports as f64 / 1_000_000_000.0);
            }
        }

        // One shared PubsubClient for both USDC and SOL subscriptions.
        let mut backoff = WSS_RETRY_INIT;
        loop {
            let pubsub = match PubsubClient::new(&ws_url).await {
                Ok(c) => c,
                Err(_) => {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };

            let (mut usdc_stream, usdc_unsub) = match pubsub
                .account_subscribe(&ata, Some(account_cfg.clone()))
                .await
            {
                Ok(s) => s,
                Err(_) => {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };

            let (mut sol_stream, sol_unsub) = match pubsub
                .account_subscribe(&wallet_pk, Some(account_cfg.clone()))
                .await
            {
                Ok(s) => s,
                Err(_) => {
                    usdc_unsub().await;
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };

            backoff = WSS_RETRY_INIT;

            // Drive both streams on the same connection.
            loop {
                tokio::select! {
                    msg = usdc_stream.next() => {
                        let Some(resp) = msg else { break };
                        if let Some(data) = resp.value.data.decode() {
                            if data.len() >= 72 {
                                let raw = u64::from_le_bytes(
                                    data[64..72].try_into().unwrap_or_default(),
                                );
                                let _ = usdc_tx.send(raw as f64 / 1_000_000.0);
                            }
                        }
                    }
                    msg = sol_stream.next() => {
                        let Some(resp) = msg else { break };
                        let _ = sol_tx.send(resp.value.lamports as f64 / 1_000_000_000.0);
                    }
                }
            }

            usdc_unsub().await;
            sol_unsub().await;
            tokio::time::sleep(WSS_RETRY_INIT).await;
        }
    })
}

/// If the HTTP fetch outlives this deadline the task exits and the periodic
/// `balance_interval` tick will schedule a fresh attempt. Prevents a stuck
/// upstream from pinning `balance_fetch_handle` to an unfinished state forever.
pub(super) const BALANCE_FETCH_TIMEOUT: Duration = Duration::from_millis(1500);

/// Upper bound on a single top-positions refresh cycle. The ActiveTraderBuffer
/// plus overflow arenas are a handful of sequential `getAccount` calls, so a
/// well-behaved RPC finishes in well under a second; this is a safety net for
/// a stalled endpoint so the refresh ticker can respawn cleanly.
pub(super) const TOP_POSITIONS_TIMEOUT: Duration = Duration::from_secs(5);

/// Spawn a one-shot top-positions refresh. The refresh scans the on-chain
/// `ActiveTraderBuffer`, converts every active position to display units
/// against the caller-supplied market configs + mark prices, and posts the
/// top-N (sorted by notional) through `tx`. Silent on empty/error — failures
/// just leave the existing list on screen.
pub(super) fn spawn_top_positions_refresh(
    rpc_url: String,
    configs: std::collections::HashMap<String, SplineConfig>,
    marks: std::collections::HashMap<String, f64>,
    gti_cache: GtiHandle,
    gti_refresh: Arc<tokio::sync::Notify>,
    tx: UnboundedSender<Vec<TopPositionEntry>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let fut = async {
            let cache_guard = gti_cache.read().await;
            fetch_top_positions(&rpc_url, &configs, &marks, cache_guard.as_ref()).await
        };
        match tokio::time::timeout(TOP_POSITIONS_TIMEOUT, fut).await {
            Ok(Ok(entries)) => {
                // If any entry has an unresolved trader (e.g. a brand-new
                // authority registered between GTI refreshes), nudge the GTI
                // loader so the next cycle shows the full pubkey.
                let had_miss = entries.iter().any(|e| e.trader.is_none());
                if had_miss {
                    gti_refresh.notify_one();
                }
                let _ = tx.send(entries);
            }
            Ok(Err(e)) => {
                warn!(error = %e, "top positions refresh failed");
            }
            Err(_) => {
                warn!("top positions refresh timed out");
            }
        }
    })
}

/// Referral code activated for brand-new Phoenix accounts on first wallet
/// connect.
const REFERRAL_CODE: &str = "COSMIC";

pub(super) fn spawn_balance_fetch(
    http: Arc<PhoenixHttpClient>,
    kp: Arc<Keypair>,
    symbol: String,
    balance_tx: UnboundedSender<BalanceUpdate>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let authority_v2 = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(error = %e, "failed to convert wallet pubkey for phoenix fetch");
                return;
            }
        };
        let Ok((phoenix_bal, position, all_positions)) = tokio::time::timeout(
            BALANCE_FETCH_TIMEOUT,
            fetch_phoenix_balance_and_position(&http, &authority_v2, &symbol),
        )
        .await
        else {
            warn!(symbol = %symbol, "phoenix balance fetch timed out");
            return;
        };
        let _ = balance_tx.send(BalanceUpdate {
            phoenix_collateral: phoenix_bal,
            position,
            all_positions,
        });
    })
}

/// On wallet connect: check whether the authority already has a Phoenix
/// account; if not, activate the `COSMIC` referral via the invite API so
/// subsequent trading calls succeed. Then kick off the initial balance/position
/// fetch.
pub(super) fn spawn_initial_connect_flow(
    http: Arc<PhoenixHttpClient>,
    kp: Arc<Keypair>,
    symbol: String,
    balance_tx: UnboundedSender<BalanceUpdate>,
    tx_status: UnboundedSender<TxStatusMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let authority_v2 = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(error = %e, "failed to convert wallet pubkey for initial connect flow");
                return;
            }
        };

        match http.traders().get_trader(&authority_v2).await {
            Ok(traders) if traders.is_empty() => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: "🐦‍🔥 No Phoenix account — registering…".to_string(),
                    detail: String::new(),
                });
                match http
                    .invite()
                    .activate_referral(&authority_v2, REFERRAL_CODE)
                    .await
                {
                    Ok(_) => {
                        let _ = tx_status.send(TxStatusMsg::SetStatus {
                            title: "🐦‍🔥 Phoenix account registered".to_string(),
                            detail: String::new(),
                        });
                    }
                    Err(e) => {
                        warn!(error = %e, "activate_referral failed");
                        let _ = tx_status.send(TxStatusMsg::SetStatus {
                            title: "❌ Phoenix registration failed".to_string(),
                            detail: format!("{}", e),
                        });
                    }
                }
            }
            Ok(_) => { /* account already present — nothing to do */ }
            Err(e) => {
                // Don't block the initial balance fetch on a transient get_trader error;
                // the 1.1s poll will retry anyway.
                warn!(error = %e, "initial get_trader failed; skipping referral check");
            }
        }

        let Ok((phoenix_bal, position, all_positions)) = tokio::time::timeout(
            BALANCE_FETCH_TIMEOUT,
            fetch_phoenix_balance_and_position(&http, &authority_v2, &symbol),
        )
        .await
        else {
            warn!(symbol = %symbol, "initial phoenix balance fetch timed out");
            return;
        };
        let _ = balance_tx.send(BalanceUpdate {
            phoenix_collateral: phoenix_bal,
            position,
            all_positions,
        });
    })
}

/// Convert the phoenix-types `side` string ("Bid" / "Ask") to our internal
/// `TradingSide`. The SDK formats the enum via `{:?}` so variant names come
/// through verbatim.
pub(super) fn trading_side_from_str(s: &str) -> TradingSide {
    match s {
        "Bid" | "Buy" | "Long" => TradingSide::Long,
        _ => TradingSide::Short,
    }
}

fn trigger_side_to_trading_side(side: StopLossTradeSide) -> TradingSide {
    match side {
        StopLossTradeSide::Bid => TradingSide::Long,
        StopLossTradeSide::Ask => TradingSide::Short,
    }
}

fn trigger_direction_to_phoenix(direction: StopLossDirection) -> Direction {
    match direction {
        StopLossDirection::GreaterThan => Direction::GreaterThan,
        StopLossDirection::LessThan => Direction::LessThan,
    }
}

fn conditional_order_type(kind: StopLossOrderKind) -> String {
    match kind {
        StopLossOrderKind::IOC => "Market".to_string(),
        StopLossOrderKind::Limit => "Limit".to_string(),
    }
}

fn push_conditional_trigger_row(
    rows: &mut Vec<OrderInfo>,
    symbol: &str,
    order_index: u8,
    order_sequence_number: u64,
    size_lots: u64,
    trigger: &ConditionalOrderTrigger,
) {
    if !trigger.is_active {
        return;
    }

    rows.push(OrderInfo {
        symbol: symbol.to_string(),
        order_sequence_number,
        side: trigger_side_to_trading_side(trigger.trade_side),
        order_type: conditional_order_type(trigger.order_kind),
        price_usd: 0.0,
        price_ticks: trigger.trigger_price,
        size_remaining: size_lots as f64,
        initial_size: size_lots as f64,
        reduce_only: true,
        is_stop_loss: true,
        conditional_order_index: Some(order_index),
        conditional_trigger_direction: Some(trigger_direction_to_phoenix(
            trigger.execution_direction,
        )),
    });
}

fn conditional_order_rows(
    collection: &ConditionalOrderCollection,
    asset_symbols: &HashMap<u32, String>,
) -> Vec<OrderInfo> {
    let mut rows = Vec::new();
    for (order_index, order) in collection.active_orders() {
        let Some(symbol) = asset_symbols.get(&order.asset_id) else {
            continue;
        };
        let size_lots = order.fillable_size.max(order.max_size);
        let base_sequence = 1_000_000_000 + u64::from(order_index) * 2;
        push_conditional_trigger_row(
            &mut rows,
            symbol,
            order_index,
            base_sequence,
            size_lots,
            &order.greater_trigger_order,
        );
        push_conditional_trigger_row(
            &mut rows,
            symbol,
            order_index,
            base_sequence + 1,
            size_lots,
            &order.less_trigger_order,
        );
    }
    rows
}

async fn fetch_conditional_order_rows(
    rpc: &RpcClient,
    address: &Pubkey,
    asset_symbols: &HashMap<u32, String>,
) -> Vec<OrderInfo> {
    let response = match rpc
        .get_account_with_commitment(address, rpc.commitment())
        .await
    {
        Ok(response) => response,
        Err(err) => {
            warn!(error = %err, "failed to fetch conditional-orders account");
            return Vec::new();
        }
    };
    let Some(account) = response.value.filter(|account| !account.data.is_empty()) else {
        return Vec::new();
    };

    match ConditionalOrderCollection::try_from_account_bytes(&account.data) {
        Ok(collection) => conditional_order_rows(&collection, asset_symbols),
        Err(err) => {
            warn!(error = %err, "failed to decode conditional-orders account");
            Vec::new()
        }
    }
}

fn build_order_rows(
    trader: &Trader,
    stop_triggers: &HashMap<(String, String), TraderStateStopLossTrigger>,
    conditional_orders: &[OrderInfo],
) -> Vec<OrderInfo> {
    let mut orders: Vec<OrderInfo> = trader
        .all_orders()
        .iter()
        .map(|o| OrderInfo {
            symbol: o.symbol.clone(),
            order_sequence_number: o.order_sequence_number,
            side: trading_side_from_str(&o.side),
            order_type: o.order_type.clone(),
            price_usd: o.price_usd.to_string().parse::<f64>().unwrap_or(0.0),
            // `price_ticks` is i64 in the SDK but always non-negative for live orders;
            // clamp at 0 just in case to keep the cast safe.
            price_ticks: o.price_ticks.max(0) as u64,
            // UI size is filled in by the main loop using `configs`; raw lots are
            // carried in the `f64` as a fallback so the modal still shows magnitude.
            size_remaining: o.size_remaining_lots as f64,
            initial_size: o.initial_size_lots as f64,
            reduce_only: o.reduce_only,
            is_stop_loss: o.is_stop_loss,
            conditional_order_index: None,
            conditional_trigger_direction: None,
        })
        .collect();

    // Append synthetic rows for pending stop-loss triggers. These carry no size
    // (size is determined at fire-time from the opposing position) and no USD
    // price — the main loop converts `price_ticks` via the per-symbol config.
    for ((symbol, stop_id), sl) in stop_triggers {
        let trigger_ticks: u64 = sl.trigger.trigger_price_ticks.parse().unwrap_or(0);
        let osn: u64 = stop_id
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        let side = match sl.trigger.side {
            phoenix_rise::types::Side::Bid => TradingSide::Long,
            phoenix_rise::types::Side::Ask => TradingSide::Short,
        };
        orders.push(OrderInfo {
            symbol: symbol.clone(),
            order_sequence_number: osn,
            side,
            order_type: sl.trigger.kind.clone(),
            price_usd: 0.0,
            price_ticks: trigger_ticks,
            size_remaining: 0.0,
            initial_size: 0.0,
            reduce_only: true,
            is_stop_loss: true,
            conditional_order_index: None,
            conditional_trigger_direction: None,
        });
    }

    for conditional in conditional_orders {
        let duplicate = orders.iter().any(|order| {
            order.is_stop_loss
                && order.symbol == conditional.symbol
                && order.side == conditional.side
                && order.price_ticks == conditional.price_ticks
        });
        if !duplicate {
            orders.push(conditional.clone());
        }
    }

    orders
}

/// Spawn a persistent `PhoenixWSClient` subscription to the wallet's trader
/// state. Each update is applied to a local `Trader`, then `all_orders()` is
/// flattened into `Vec<OrderInfo>` and pushed to the main loop, which owns the
/// `configs` map needed for lot→UI conversion.
///
/// Uses raw base-lots in the payload; the main loop converts to UI units via
/// the live configs.
pub(super) fn spawn_trader_orders_ws(
    kp: Arc<Keypair>,
    orders_tx: UnboundedSender<Vec<OrderInfo>>,
    conditional_asset_symbols: HashMap<u32, String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let authority = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(error = %e, "failed to convert wallet pubkey for trader WS");
                return;
            }
        };

        let mut backoff = WSS_RETRY_INIT;
        loop {
            let client = match PhoenixWSClient::new_from_env() {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, "PhoenixWSClient::new_from_env failed; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };
            let (mut rx, handle) = match client.subscribe_to_trader_state(&authority) {
                Ok(pair) => pair,
                Err(e) => {
                    warn!(error = %e, "subscribe_to_trader_state failed; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };
            backoff = WSS_RETRY_INIT;

            let key = TraderKey::new(authority);
            let conditional_orders_address = get_conditional_orders_address(&key.pda());
            let conditional_rpc = RpcClient::new_with_commitment(
                rpc_http_url_from_env(),
                CommitmentConfig::processed(),
            );
            let mut trader = Trader::new(key);
            // Stop-loss triggers live on `TraderStatePositionRow`, not in
            // `subaccount.orders`, so the SDK's `Trader::all_orders()` never
            // surfaces them. Track them ourselves from the raw ws payload so
            // we can emit synthetic OrderInfo rows for the orders modal.
            //
            // Keyed by `(symbol, stop_loss_id)`. On `Snapshot` we rebuild from
            // scratch; on `Delta`, updated positions replace their symbol's
            // triggers wholesale (matching `TraderStatePositionRow` semantics,
            // where the triggers field is always the full current set), and
            // closed positions drop their symbol entirely.
            let mut stop_triggers: HashMap<(String, String), TraderStateStopLossTrigger> =
                HashMap::new();
            let mut conditional_orders = fetch_conditional_order_rows(
                &conditional_rpc,
                &conditional_orders_address,
                &conditional_asset_symbols,
            )
            .await;
            let mut conditional_interval = tokio::time::interval(Duration::from_millis(1500));
            conditional_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        let Some(msg) = msg else {
                            break;
                        };
                        trader.apply_update(&msg);

                        match &msg.content {
                            TraderStatePayload::Snapshot(s) => {
                                stop_triggers.clear();
                                for sub in &s.subaccounts {
                                    for pos in &sub.positions {
                                        for sl in &pos.position.stop_loss_triggers {
                                            stop_triggers.insert(
                                                (pos.symbol.clone(), sl.stop_loss_id.clone()),
                                                sl.clone(),
                                            );
                                        }
                                    }
                                }
                            }
                            TraderStatePayload::Delta(d) => {
                                for sub in &d.deltas {
                                    for pos_delta in &sub.positions {
                                        match pos_delta.change {
                                            TraderStateRowChangeKind::Closed => {
                                                stop_triggers
                                                    .retain(|(sym, _), _| sym != &pos_delta.symbol);
                                            }
                                            TraderStateRowChangeKind::Updated => {
                                                stop_triggers
                                                    .retain(|(sym, _), _| sym != &pos_delta.symbol);
                                                if let Some(row) = &pos_delta.position {
                                                    for sl in &row.stop_loss_triggers {
                                                        stop_triggers.insert(
                                                            (
                                                                pos_delta.symbol.clone(),
                                                                sl.stop_loss_id.clone(),
                                                            ),
                                                            sl.clone(),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if orders_tx.send(build_order_rows(&trader, &stop_triggers, &conditional_orders)).is_err() {
                            // Receiver dropped (poller shutting down).
                            drop(handle);
                            return;
                        }
                    }
                    _ = conditional_interval.tick() => {
                        conditional_orders = fetch_conditional_order_rows(
                            &conditional_rpc,
                            &conditional_orders_address,
                            &conditional_asset_symbols,
                        )
                        .await;
                        if orders_tx.send(build_order_rows(&trader, &stop_triggers, &conditional_orders)).is_err() {
                            drop(handle);
                            return;
                        }
                    }
                }
            }

            // Stream closed — reconnect. Drop handle first so the old subscription is
            // released.
            drop(handle);
            tokio::time::sleep(backoff).await;
        }
    })
}

/// Resolve raw per-trader L2 levels into display-ready `(price, qty, trader)`
/// tuples using the cached GTI. Returns `had_miss = true` if any trader pointer
/// was unresolved (either because the cache hasn't loaded yet or a new trader
/// registered after the last refresh), so the caller can nudge the loader.
pub(super) fn resolve_levels(
    cache: Option<&super::super::gti::GtiCache>,
    bids_raw: &[L2Level],
    asks_raw: &[L2Level],
) -> (Vec<ClobLevel>, Vec<ClobLevel>, bool) {
    let mut had_miss = false;
    let mut resolve_side = |raw: &[L2Level]| -> Vec<ClobLevel> {
        raw.iter()
            .filter_map(|lvl| match cache.and_then(|c| c.resolve(lvl.trader_id)) {
                Some(pk) => Some((lvl.price, lvl.qty, pubkey_trader_prefix(&pk))),
                // Drop unresolved rows — they'll reappear on the next emit tick once
                // the loader refreshes. Rendering a placeholder would flash noise.
                None => {
                    had_miss = true;
                    None
                }
            })
            .collect()
    };
    let bids = resolve_side(bids_raw);
    let asks = resolve_side(asks_raw);
    (bids, asks, had_miss)
}

/// Maintain an L2 book for the current market via Solana `accountSubscribe` on
/// the Phoenix market (orderbook) account. Re-subscribes on market switch using
/// the same pubsub connection, and re-reads `ws_url_from_env()` on each
/// reconnect so RPC URL changes take effect.
pub(super) fn spawn_phoenix_l2_book_rpc(
    mut cfg_rx: watch::Receiver<SplineConfig>,
    l2_tx: tokio::sync::mpsc::UnboundedSender<L2BookStreamMsg>,
    gti_cache: GtiHandle,
    gti_refresh: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let account_config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::processed()),
            ..Default::default()
        };
        let mut backoff = WSS_RETRY_INIT;

        'outer: loop {
            let ws_url = ws_url_from_env();
            let pubsub = match PubsubClient::new(&ws_url).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(url = %ws_url, error = %e, "L2 pubsub connect failed; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };
            backoff = WSS_RETRY_INIT;

            'sub: loop {
                let mut cfg = cfg_rx.borrow().clone();
                let market_pk = match Pubkey::from_str(&cfg.market_pubkey) {
                    Ok(pk) => pk,
                    Err(e) => {
                        warn!(
                            market = %cfg.market_pubkey,
                            error = %e,
                            "invalid market pubkey for L2 book"
                        );
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        break 'sub;
                    }
                };

                let (mut stream, unsub) = match pubsub
                    .account_subscribe(&market_pk, Some(account_config.clone()))
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(
                            symbol = %cfg.symbol,
                            error = %e,
                            "L2 market account_subscribe failed; reconnecting"
                        );
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        break 'sub;
                    }
                };

                // Coalesce bursts: keep the latest parsed snapshot and emit on the throttle
                // tick.
                let mut emit_ticker = tokio::time::interval(L2_EMIT_MIN_INTERVAL);
                emit_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut pending: Option<(Vec<L2Level>, Vec<L2Level>)> = None;

                'inner: loop {
                    tokio::select! {
                        r = cfg_rx.changed() => {
                            if r.is_err() {
                                unsub().await;
                                return;
                            }
                            let new_cfg = cfg_rx.borrow().clone();
                            if new_cfg.market_pubkey != cfg.market_pubkey {
                                unsub().await;
                                // Re-subscribe on the same pubsub connection for the new market.
                                continue 'sub;
                            }
                            // Same market but other fields (tick_size, base_lot_decimals,
                            // symbol) may have changed — refresh so we don't parse the next
                            // orderbook payload against stale conversion params.
                            cfg = new_cfg;
                        }
                        response = stream.next() => {
                            let Some(response) = response else {
                                unsub().await;
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                break 'sub;
                            };
                            let Some(data) = response.value.data.decode() else {
                                continue;
                            };
                            if let Some(levels) = parse_l2_book_from_market_account(
                                data,
                                cfg.tick_size,
                                cfg.base_lot_decimals,
                                L2_SNAPSHOT_DEPTH,
                            ) {
                                pending = Some(levels);
                            }
                        }
                        _ = emit_ticker.tick() => {
                            let Some((bids_raw, asks_raw)) = pending.take() else {
                                continue 'inner;
                            };
                            // Resolve trader ids under a shared read lock. Stuck on a miss?
                            // Nudge the loader; it debounces internally.
                            let (bids, asks, had_miss) = {
                                let cache = gti_cache.read().await;
                                resolve_levels(cache.as_ref(), &bids_raw, &asks_raw)
                            };
                            if had_miss {
                                gti_refresh.notify_one();
                            }
                            if l2_tx
                                .send(L2BookStreamMsg {
                                    symbol: cfg.symbol.clone(),
                                    bids,
                                    asks,
                                })
                                .is_err()
                            {
                                unsub().await;
                                return;
                            }
                        }
                    }
                }
            }
            // 'sub fell through: reconnect the pubsub with a fresh env read.
            continue 'outer;
        }
    })
}
