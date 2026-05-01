//! Live liquidation feed task.
//!
//! Subscribes (via `logsSubscribe`, filtered by the Phoenix Eternal program
//! id) to confirmed transactions touching the program. For each tx whose log
//! lines hint at a `Liquidate` instruction, we fetch the full transaction,
//! flatten the inner instructions, and parse `MarketEvent::Liquidation`s out
//! of the self-CPI event stream. Each liquidation is converted to display
//! units against the locally-cached `SplineConfig` map and shipped to the TUI
//! through `liquidation_tx`.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures_util::StreamExt;
use phoenix_eternal_types::events::{
    parse_events_from_inner_instructions_with_context, InnerInstructionContext,
};
use phoenix_eternal_types::program_ids::PHOENIX_ETERNAL_PROGRAM_ID;
use phoenix_eternal_types::MarketEvent;
use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_types::config::{
    RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_signature::Signature;
use solana_transaction_status_client_types::option_serializer::OptionSerializer;
use solana_transaction_status_client_types::{
    EncodedTransaction, UiInnerInstructions, UiInstruction, UiMessage, UiTransactionEncoding,
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

use super::super::super::config::SplineConfig;
use super::super::super::format::pubkey_trader_prefix;
use super::super::super::state::LiquidationEntry;
use super::{WSS_RETRY_CAP, WSS_RETRY_INIT};

/// How many recently-seen tx signatures to remember so we don't re-fetch the
/// same tx if `logsSubscribe` re-emits it (some RPCs do under reconnect or
/// commitment-level transitions).
const SIGNATURE_DEDUP_CAP: usize = 256;

/// Cap on a single `get_transaction` RPC. Liquidation latency is tolerable;
/// hanging on a stalled RPC is not.
const GET_TX_TIMEOUT: Duration = Duration::from_secs(8);

/// Spawn the long-running liquidation subscription. Reconnects on its own with
/// exponential backoff capped at [`WSS_RETRY_CAP`]. The task lives for the
/// duration of the process; closing the modal does not stop it (so the buffer
/// stays warm).
pub(in crate::tui::runtime) fn spawn_liquidation_feed_task(
    ws_url: String,
    rpc_url: String,
    configs: HashMap<String, SplineConfig>,
    tx: UnboundedSender<LiquidationEntry>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Asset id → cloned config so the feed can resolve symbol/decimals on
        // the fly without locking shared state. Built once; we accept that
        // dynamically-added markets won't appear until the next process
        // restart — the alternative (a watch channel) is far more plumbing
        // than this informational feature warrants.
        let configs_by_asset: HashMap<u32, SplineConfig> = configs
            .values()
            .map(|c| (c.asset_id, c.clone()))
            .collect();

        let rpc = Arc::new(RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        ));

        let mut backoff = WSS_RETRY_INIT;
        let mut seen_signatures: HashSet<String> = HashSet::with_capacity(SIGNATURE_DEDUP_CAP);
        let mut seen_order: std::collections::VecDeque<String> =
            std::collections::VecDeque::with_capacity(SIGNATURE_DEDUP_CAP);

        loop {
            let pubsub = match PubsubClient::new(&ws_url).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(url = %ws_url, error = %e, "liquidations pubsub connect failed; retry");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };
            backoff = WSS_RETRY_INIT;

            let filter =
                RpcTransactionLogsFilter::Mentions(vec![PHOENIX_ETERNAL_PROGRAM_ID.to_string()]);
            let cfg = RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig::confirmed()),
            };

            let (mut stream, unsub) = match pubsub.logs_subscribe(filter, cfg).await {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = %e, "logsSubscribe failed; retry");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };

            while let Some(notif) = stream.next().await {
                let logs = &notif.value.logs;
                if notif.value.err.is_some() {
                    continue;
                }
                if !logs_hint_at_liquidation(logs) {
                    continue;
                }

                let signature = notif.value.signature.clone();
                if !remember_signature(&mut seen_signatures, &mut seen_order, signature.clone()) {
                    continue;
                }

                let rpc_clone = Arc::clone(&rpc);
                let configs_clone = configs_by_asset.clone();
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    fetch_and_emit(rpc_clone, signature, configs_clone, tx_clone).await;
                });
            }

            unsub().await;
            tokio::time::sleep(WSS_RETRY_INIT).await;
        }
    })
}

/// Cheap pre-filter: skip transactions whose log lines don't mention
/// "liquidat" (case-insensitive). Phoenix's instruction-name logs include the
/// instruction name, so a liquidation tx will always have a matching line.
/// This keeps us from issuing a `getTransaction` RPC for every order place /
/// cancel notification.
fn logs_hint_at_liquidation(logs: &[String]) -> bool {
    logs.iter().any(|line| {
        // Walk bytes lower-cased on the fly to skip the allocation a `to_lowercase`
        // would force.
        let bytes = line.as_bytes();
        if bytes.len() < "liquidat".len() {
            return false;
        }
        bytes
            .windows("liquidat".len())
            .any(|w| w.eq_ignore_ascii_case(b"liquidat"))
    })
}

fn remember_signature(
    set: &mut HashSet<String>,
    order: &mut std::collections::VecDeque<String>,
    sig: String,
) -> bool {
    if !set.insert(sig.clone()) {
        return false;
    }
    order.push_back(sig);
    while order.len() > SIGNATURE_DEDUP_CAP {
        if let Some(old) = order.pop_front() {
            set.remove(&old);
        }
    }
    true
}

async fn fetch_and_emit(
    rpc: Arc<RpcClient>,
    signature: String,
    configs_by_asset: HashMap<u32, SplineConfig>,
    tx: UnboundedSender<LiquidationEntry>,
) {
    let Ok(sig) = Signature::from_str(&signature) else {
        return;
    };
    let cfg = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };
    let fetch = rpc.get_transaction_with_config(&sig, cfg);
    let response = match tokio::time::timeout(GET_TX_TIMEOUT, fetch).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            warn!(error = %e, sig = %signature, "get_transaction failed for liquidation tx");
            return;
        }
        Err(_) => {
            warn!(sig = %signature, "get_transaction timed out for liquidation tx");
            return;
        }
    };

    let Some(meta) = response.transaction.meta else {
        return;
    };
    let inner_ixs: Vec<UiInnerInstructions> = match meta.inner_instructions {
        OptionSerializer::Some(v) => v,
        _ => return,
    };

    let account_keys = match extract_account_keys(&response.transaction.transaction) {
        Some(keys) => keys,
        None => return,
    };

    let parsed_ixs = match flatten_inner_instructions(&inner_ixs, &account_keys) {
        Some(p) => p,
        None => return,
    };

    let events = parse_events_from_inner_instructions_with_context(
        &PHOENIX_ETERNAL_PROGRAM_ID,
        &parsed_ixs,
    );

    // Build a quick `liquidated_trader` resolver: walk the event stream and
    // collect each Liquidation, then convert + emit.
    let block_time = response.block_time;
    for event in events {
        if let MarketEvent::Liquidation(e) = event {
            let received_at = block_time
                .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                .unwrap_or_else(Utc::now);
            let entry = build_entry(&e, received_at, &configs_by_asset);
            if tx.send(entry).is_err() {
                return;
            }
        }
    }
}

fn build_entry(
    e: &phoenix_eternal_types::LiquidationEvent,
    received_at: chrono::DateTime<Utc>,
    configs_by_asset: &HashMap<u32, SplineConfig>,
) -> LiquidationEntry {
    let cfg = configs_by_asset.get(&e.asset_id);
    let (symbol, price_decimals, size_decimals, size, mark_price) = match cfg {
        Some(c) => {
            let bld = c.base_lot_decimals;
            let size = base_lots_to_units(e.base_lots_filled.as_inner(), bld);
            let mark_price = ticks_to_price(e.mark_price.as_inner(), c.tick_size, bld);
            (
                c.symbol.clone(),
                c.price_decimals,
                c.size_decimals,
                size,
                mark_price,
            )
        }
        None => (String::new(), 4, 4, e.base_lots_filled.as_inner() as f64, 0.0),
    };

    let notional = size * mark_price;

    LiquidationEntry {
        received_at,
        symbol,
        asset_id: e.asset_id,
        liquidated_trader: pubkey_trader_prefix(&e.liquidated_trader),
        size,
        mark_price,
        notional,
        position_closed: e.position_closed,
        price_decimals,
        size_decimals,
    }
}

/// `lots / 10^bld`. `bld` may be negative for assets where one lot is greater
/// than one display unit, so we use `powi(i32)` with the integer exponent.
fn base_lots_to_units(lots: u64, bld: i8) -> f64 {
    let divisor = 10_f64.powi(bld as i32);
    if divisor == 0.0 {
        0.0
    } else {
        lots as f64 / divisor
    }
}

/// `ticks * tick_size * 10^bld / 10^6` — the inverse of the price encoding
/// used everywhere else in the repo (quote lot decimals are a fixed 6).
fn ticks_to_price(ticks: u64, tick_size: u64, bld: i8) -> f64 {
    ticks as f64 * tick_size as f64 * 10_f64.powi(bld as i32) / 1_000_000.0
}

fn extract_account_keys(encoded: &EncodedTransaction) -> Option<Vec<Pubkey>> {
    match encoded {
        EncodedTransaction::Json(ui_tx) => match &ui_tx.message {
            UiMessage::Raw(raw) => raw
                .account_keys
                .iter()
                .map(|s| Pubkey::from_str(s).ok())
                .collect(),
            UiMessage::Parsed(parsed) => parsed
                .account_keys
                .iter()
                .map(|k| Pubkey::from_str(&k.pubkey).ok())
                .collect(),
        },
        _ => None,
    }
}

fn flatten_inner_instructions(
    inner_ixs: &[UiInnerInstructions],
    account_keys: &[Pubkey],
) -> Option<Vec<(InnerInstructionContext, Pubkey, Vec<u8>)>> {
    let mut result = Vec::new();
    for group in inner_ixs {
        for ix in &group.instructions {
            if let UiInstruction::Compiled(compiled) = ix {
                let pid = *account_keys.get(compiled.program_id_index as usize)?;
                let data = bs58::decode(&compiled.data).into_vec().ok()?;
                let context = (group.index, compiled.stack_height);
                result.push((context, pid, data));
            }
        }
    }
    Some(result)
}
