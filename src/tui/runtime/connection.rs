//! Runtime connection, market-switch, and shutdown helpers.

use std::io::Stdout;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use phoenix_rise::PhoenixHttpClient;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use solana_signer::Signer;
use tokio::sync::{mpsc::UnboundedReceiver, watch};

use super::super::config::{
    current_user_config, rpc_http_url_from_env, ws_url_from_env, SplineConfig,
};
use super::super::data::GtiHandle;
use super::super::i18n::strings;
use super::super::state::{L2BookStreamMsg, TuiState};
use super::super::terminal::restore_terminal;
use super::super::ui;
use super::redraw::{redraw_tui, redraw_tui_force};
use super::{tasks, Channels, FEED_REDRAW_MIN_INTERVAL};

pub(super) fn initial_config(
    market_list: &[super::super::state::MarketInfo],
    configs: &std::collections::HashMap<String, SplineConfig>,
) -> Result<SplineConfig, Box<dyn std::error::Error>> {
    let initial_symbol = market_list
        .iter()
        .find(|m| m.symbol == "SOL")
        .or_else(|| market_list.first())
        .map(|m| m.symbol.clone())
        .unwrap_or_else(|| "SOL".to_string());
    configs
        .get(&initial_symbol)
        .cloned()
        .ok_or_else(|| format!("no spline config for initial market {}", initial_symbol).into())
}

pub(super) fn duration_until_next_utc_second() -> Duration {
    let nanos = chrono::Utc::now().timestamp_subsec_nanos() as u64;
    Duration::from_nanos(1_000_000_000 - nanos)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_l2_book_msg(
    mut msg: L2BookStreamMsg,
    l2_book_rx: &mut UnboundedReceiver<L2BookStreamMsg>,
    state: &mut TuiState,
    cfg: &SplineConfig,
    gti_cache: &GtiHandle,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
    last_feed_paint: &mut Instant,
) {
    while let Ok(next) = l2_book_rx.try_recv() {
        msg = next;
    }
    if msg.symbol != cfg.symbol {
        return;
    }

    state.clob_bids = msg.bids;
    state.clob_asks = msg.asks;
    if last_feed_paint.elapsed() < FEED_REDRAW_MIN_INTERVAL {
        return;
    }

    let gti_guard = gti_cache.read().await;
    state.rebuild_merged_book(
        &cfg.symbol,
        current_user_config().show_clob,
        gti_guard.as_ref(),
        cfg.price_decimals,
    );
    drop(gti_guard);
    if state.last_parsed.is_some() {
        redraw_tui(terminal, state, cfg, rpc_host);
    } else {
        redraw_tui_force(terminal, state, cfg, rpc_host);
    }
    *last_feed_paint = Instant::now();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_full_rpc_reconnect(
    pending_full_reconnect: &mut bool,
    ws_url: &mut String,
    rpc_host: &mut String,
    l2_book_task: &mut tokio::task::JoinHandle<()>,
    l2_cfg_tx: &watch::Sender<SplineConfig>,
    l2_book_tx: &tokio::sync::mpsc::UnboundedSender<L2BookStreamMsg>,
    gti_cache: &GtiHandle,
    gti_refresh: &Arc<tokio::sync::Notify>,
    wallet_wss_handle: &mut Option<tokio::task::JoinHandle<()>>,
    blockhash_refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    trader_orders_handle: &mut Option<tokio::task::JoinHandle<()>>,
    tx_ctx_task: &mut Option<tokio::task::JoinHandle<()>>,
    liquidation_task: &mut tokio::task::JoinHandle<()>,
    state: &mut TuiState,
    cfg: &SplineConfig,
    balance_http: &Arc<PhoenixHttpClient>,
    channels: &Channels,
    configs: &std::collections::HashMap<String, SplineConfig>,
) -> bool {
    if !*pending_full_reconnect {
        return false;
    }

    *pending_full_reconnect = false;
    *ws_url = ws_url_from_env();
    *rpc_host = ui::rpc_host_from_urlish(&rpc_http_url_from_env());
    l2_book_task.abort();
    *l2_book_task = if current_user_config().show_clob {
        tasks::spawn_phoenix_l2_book_rpc(
            l2_cfg_tx.subscribe(),
            l2_book_tx.clone(),
            Arc::clone(gti_cache),
            Arc::clone(gti_refresh),
        )
    } else {
        tokio::spawn(async {})
    };
    gti_refresh.notify_one();
    abort_handle(wallet_wss_handle);
    abort_handle(blockhash_refresh_handle);

    // Liquidation feed task captures `ws_url`/`rpc_url` at spawn time, so
    // restart it on RPC change. The bounded buffer in
    // `LiquidationFeedView` already holds prior history, so the modal stays
    // populated across the swap.
    liquidation_task.abort();
    *liquidation_task = tasks::spawn_liquidation_feed_task(
        ws_url.clone(),
        rpc_http_url_from_env(),
        configs.clone(),
        channels.liquidation_tx.clone(),
    );

    if let Some(kp) = state.trading.keypair.clone() {
        state.trading.tx_context = None;
        *wallet_wss_handle = Some(tasks::spawn_wallet_wss(
            kp.pubkey().to_bytes(),
            ws_url.clone(),
            channels.wallet_usdc_tx.clone(),
            channels.wallet_sol_tx.clone(),
        ));
        // Reuse the existing shared `Trader` Arc so the rebuilt `TxContext`
        // sees the same mirror the trader-orders WS task writes into. If
        // the shared mirror is somehow missing, build a fresh empty one
        // AND write it back to `state.trading.shared_trader`.
        let shared_trader = match state.trading.shared_trader.as_ref() {
            Some(arc) => Arc::clone(arc),
            None => {
                let authority = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
                    Ok(pk) => pk,
                    Err(e) => {
                        tracing::warn!(error = %e, "shared_trader rebuild aborted: bad pubkey");
                        return true;
                    }
                };
                let arc = std::sync::Arc::new(std::sync::RwLock::new(
                    crate::tui::tx::TxContext::empty_trader_mirror(authority),
                ));
                state.trading.shared_trader = Some(Arc::clone(&arc));
                arc
            }
        };
        // Critical: reset `hydrated = false` so a cross-network RPC swap
        // (e.g. devnet → mainnet) doesn't let order builders read the old
        // network's subaccount layout while the new WS catches up. The
        // pre-swap snapshot's `Trader` value is left in place so the
        // bots-modal renderer can still see "last known" subaccounts, but
        // submit-time `snapshot_trader()` returns None until the new WS
        // pushes its first `Snapshot`.
        if let Ok(mut guard) = shared_trader.write() {
            guard.hydrated = false;
        }
        // Abort the OLD trader-orders task BEFORE spawning the new one —
        // otherwise both tasks hold the same shared_trader Arc and the
        // old (pre-swap) task's in-flight write can land AFTER the new
        // task's first apply, clobbering with stale data.
        if let Some(h) = trader_orders_handle.take() {
            h.abort();
        }
        let conditional_asset_symbols = configs
            .values()
            .map(|c| (c.asset_id, c.symbol.clone()))
            .collect();
        *trader_orders_handle = Some(tasks::spawn_trader_orders_ws(
            Arc::clone(&kp),
            channels.orders_tx.clone(),
            conditional_asset_symbols,
            Arc::clone(&shared_trader),
        ));
        // Old tx_ctx_task is fine to abort after spawning the new one (no
        // shared writable state — it just delivers a freshly-built
        // TxContext once and returns).
        let new_tx_ctx = tasks::spawn_tx_context_task(
            Arc::clone(&kp),
            cfg.symbol.clone(),
            Arc::clone(balance_http),
            Arc::clone(&shared_trader),
            channels.tx_ctx_tx.clone(),
            channels.tx_status.clone(),
        );
        if let Some(h) = tx_ctx_task.replace(new_tx_ctx) {
            h.abort();
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_pending_market_switch(
    new_symbol: String,
    configs: &std::collections::HashMap<String, SplineConfig>,
    state: &mut TuiState,
    cfg: &mut SplineConfig,
    l2_cfg_tx: &watch::Sender<SplineConfig>,
    balance_http: &Arc<PhoenixHttpClient>,
    channels: &Channels,
    tx_ctx_task: &mut Option<tokio::task::JoinHandle<()>>,
) -> bool {
    if let Some(new_cfg) = configs.get(&new_symbol).cloned() {
        state.begin_market_switch(&new_cfg.symbol);
        *cfg = new_cfg;
        let _ = l2_cfg_tx.send(cfg.clone());
        state
            .trading
            .set_status_title(format!("{} {}", strings().st_switched_to, cfg.symbol));

        // Bootstrap the spline view from a one-shot HTTP getAccountInfo. The
        // WSS account_subscribe only pushes when the account changes, so for
        // an idle market the "Switching to … market…" modal would otherwise
        // hang until the next on-chain spline write.
        tasks::spawn_spline_bootstrap_fetch(
            cfg.symbol.clone(),
            cfg.spline_collection.clone(),
            rpc_http_url_from_env(),
            channels.spline_bootstrap_tx.clone(),
        );

        if let Some(kp) = &state.trading.keypair {
            state.trading.tx_context = None;
            let shared_trader = match state.trading.shared_trader.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => {
                    let authority = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string())
                    {
                        Ok(pk) => pk,
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "market switch skipped: bad wallet pubkey"
                            );
                            return true;
                        }
                    };
                    let arc = std::sync::Arc::new(std::sync::RwLock::new(
                        crate::tui::tx::TxContext::empty_trader_mirror(authority),
                    ));
                    state.trading.shared_trader = Some(Arc::clone(&arc));
                    arc
                }
            };
            let new_tx_ctx = tasks::spawn_tx_context_task(
                Arc::clone(kp),
                cfg.symbol.clone(),
                Arc::clone(balance_http),
                shared_trader,
                channels.tx_ctx_tx.clone(),
                channels.tx_status.clone(),
            );
            if let Some(h) = tx_ctx_task.replace(new_tx_ctx) {
                h.abort();
            }
        }
        return true;
    }

    let s = strings();
    state.trading.set_status_title(format!(
        "{} {}{}",
        s.st_market_switch_failed, new_symbol, s.st_market_switch_failed_suf
    ));
    false
}

#[allow(clippy::too_many_arguments)]
pub(super) fn cleanup_tasks(
    wallet_wss_handle: &mut Option<tokio::task::JoinHandle<()>>,
    balance_fetch_handle: &mut Option<tokio::task::JoinHandle<()>>,
    blockhash_refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    trader_orders_handle: &mut Option<tokio::task::JoinHandle<()>>,
    tx_ctx_task: &mut Option<tokio::task::JoinHandle<()>>,
    top_positions_handle: &mut Option<tokio::task::JoinHandle<()>>,
    l2_book_task: &mut tokio::task::JoinHandle<()>,
    gti_loader_task: &mut tokio::task::JoinHandle<()>,
    liquidation_task: &mut tokio::task::JoinHandle<()>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) {
    abort_handle(wallet_wss_handle);
    abort_handle(balance_fetch_handle);
    abort_handle(blockhash_refresh_handle);
    abort_handle(trader_orders_handle);
    abort_handle(tx_ctx_task);
    abort_handle(top_positions_handle);
    l2_book_task.abort();
    gti_loader_task.abort();
    liquidation_task.abort();
    restore_terminal(terminal);
}

fn abort_handle(handle: &mut Option<tokio::task::JoinHandle<()>>) {
    if let Some(h) = handle.take() {
        h.abort();
    }
}

pub(super) async fn sleep_before_reconnect() {
    tokio::time::sleep(Duration::from_secs(2)).await;
}
