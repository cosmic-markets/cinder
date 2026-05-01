//! Live liquidation feed task.
//!
//! On startup, backfills until N recent liquidations are populated by paging
//! `getSignaturesForAddress` against Phoenix's sole liquidator and fetching
//! each non-failed transaction sequentially (RPC providers throttle parallel
//! `getTransaction` bursts, so there's no benefit to fanning out here). Then
//! subscribes (via `logsSubscribe`) for live updates. For each transaction we
//! fetch the full transaction, flatten the inner instructions, and parse
//! `MarketEvent::Liquidation`s out of the self-CPI event stream. Each
//! liquidation is converted to display units against the locally-cached
//! `SplineConfig` map and shipped to the TUI through `liquidation_tx`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
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
use solana_rpc_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_rpc_client_types::config::{
    RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_signature::Signature;
use solana_transaction_status_client_types::option_serializer::OptionSerializer;
use solana_transaction_status_client_types::{
    EncodedTransaction, UiInnerInstructions, UiInstruction, UiLoadedAddresses, UiMessage,
    UiTransactionEncoding,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Semaphore;
use tracing::warn;

use super::super::super::config::SplineConfig;
use super::super::super::constants::QUOTE_LOT_DECIMALS;
use super::super::super::format::pubkey_trader_prefix;
use super::super::super::math::{base_lots_to_units, ticks_to_price};
use super::super::super::state::{LiquidationEntry, LiquidationFeedMsg};
use super::super::super::trading::TradingSide;
use super::{WSS_RETRY_CAP, WSS_RETRY_INIT};

/// How many recently-seen tx signatures to remember so we don't re-fetch the
/// same tx if `logsSubscribe` re-emits it (some RPCs do under reconnect or
/// commitment-level transitions). Also dedups the small overlap window between
/// the startup backfill's first page and live notifications arriving for the
/// same fresh signatures.
const SIGNATURE_DEDUP_CAP: usize = 256;

/// Phoenix's current sole liquidator. Subscribing to this account is far
/// quieter than subscribing to every transaction mentioning the Phoenix Eternal
/// program.
const PHOENIX_SOLE_LIQUIDATOR: &str = "BP7sV1VFnbPMPyJX1tZNbXHbZkyLNFEaBWJhyMvkbxKz";

/// Cap on a single `get_transaction` RPC. Liquidation latency is tolerable;
/// hanging on a stalled RPC is not.
const GET_TX_TIMEOUT: Duration = Duration::from_secs(8);

/// Bound concurrent transaction fetches on the live path so a burst of
/// liquidation-like logs cannot create an unbounded pile of RPC requests.
/// Backfill uses its own [`BACKFILL_CONCURRENCY`] cap independently.
const MAX_CONCURRENT_GET_TX: usize = 8;

/// Concurrency for backfill `getTransaction` calls. Kept low because public
/// RPC providers throttle bursts; 2 in-flight halves wall-clock without
/// tripping rate limits on typical providers.
const BACKFILL_CONCURRENCY: usize = 2;

/// Hard cap on the number of `getTransaction` calls the startup backfill
/// makes — the most-recent successful signatures (per `getSignaturesForAddress`)
/// are fetched, in this count. Whatever decodes into `Liquidation` events
/// shows up in the modal. We do NOT keep paging until N events are found,
/// because (a) the RPC bottlenecks per-tx fetches and (b) paging deeper just
/// surfaces stale rows.
const BACKFILL_TX_FETCH_LIMIT: usize = 15;

/// Limit passed to the single `getSignaturesForAddress` call. Sized large so
/// the recent window almost always contains at least
/// [`BACKFILL_TX_FETCH_LIMIT`] successful signatures.
const BACKFILL_SIGNATURES_LIMIT: usize = 1000;

/// Cap on the single `getSignaturesForAddress` call.
const GET_SIGNATURES_TIMEOUT: Duration = Duration::from_secs(8);

/// Spawn the long-running liquidation subscription. Reconnects on its own with
/// exponential backoff capped at [`WSS_RETRY_CAP`]. The task lives for the
/// duration of the process; closing the modal does not stop it (so the buffer
/// stays warm).
pub(in crate::tui::runtime) fn spawn_liquidation_feed_task(
    ws_url: String,
    rpc_url: String,
    configs: HashMap<String, SplineConfig>,
    tx: UnboundedSender<LiquidationFeedMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Asset id → cloned config so the feed can resolve symbol/decimals on
        // the fly without locking shared state. Built once; we accept that
        // dynamically-added markets won't appear until the next process
        // restart — the alternative (a watch channel) is far more plumbing
        // than this informational feature warrants.
        let configs_by_asset: Arc<HashMap<u32, SplineConfig>> =
            Arc::new(configs.values().map(|c| (c.asset_id, c.clone())).collect());
        let get_tx_permits = Arc::new(Semaphore::new(MAX_CONCURRENT_GET_TX));

        let rpc = Arc::new(RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        ));

        let mut backoff = WSS_RETRY_INIT;
        // Shared between the live loop (single-threaded) and the backfill
        // task. `std::sync::Mutex` is fine here: the lock body is two
        // microsecond-scale set/queue ops and is never held across `.await`.
        let dedup = Arc::new(Mutex::new(SignatureDedup::new()));
        let mut backfill_started = false;

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
                RpcTransactionLogsFilter::Mentions(vec![PHOENIX_SOLE_LIQUIDATOR.to_string()]);
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

            // One-shot startup backfill, kicked off after the live subscription
            // is attached so any liquidations occurring during backfill
            // collection are still captured by the live stream (and deduped via
            // the shared `SignatureDedup`).
            if !backfill_started {
                backfill_started = true;
                let rpc_clone = Arc::clone(&rpc);
                let configs_clone = Arc::clone(&configs_by_asset);
                let tx_clone = tx.clone();
                let dedup_clone = Arc::clone(&dedup);
                tokio::spawn(async move {
                    backfill_recent_liquidations(rpc_clone, configs_clone, tx_clone, dedup_clone)
                        .await;
                });
            }

            while let Some(notif) = stream.next().await {
                if notif.value.err.is_some() {
                    continue;
                }

                let signature = notif.value.signature.clone();
                let new = dedup
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remember(signature.clone());
                if !new {
                    continue;
                }

                let Ok(permit) = Arc::clone(&get_tx_permits).acquire_owned().await else {
                    continue;
                };
                let rpc_clone = Arc::clone(&rpc);
                let configs_clone = Arc::clone(&configs_by_asset);
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    fetch_and_emit(rpc_clone, signature, configs_clone, tx_clone).await;
                });
            }

            unsub().await;
            tokio::time::sleep(WSS_RETRY_INIT).await;
        }
    })
}

/// Bounded LRU dedup over recently-seen tx signatures. Held behind a mutex so
/// the live stream and the startup backfill share a single set.
struct SignatureDedup {
    set: HashSet<String>,
    order: VecDeque<String>,
}

impl SignatureDedup {
    fn new() -> Self {
        Self {
            set: HashSet::with_capacity(SIGNATURE_DEDUP_CAP),
            order: VecDeque::with_capacity(SIGNATURE_DEDUP_CAP),
        }
    }

    /// Returns true if `sig` was newly inserted, false if already known.
    fn remember(&mut self, sig: String) -> bool {
        if !self.set.insert(sig.clone()) {
            return false;
        }
        self.order.push_back(sig);
        while self.order.len() > SIGNATURE_DEDUP_CAP {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        true
    }
}

/// Make a single `getSignaturesForAddress` call against the liquidator, take
/// the most-recent [`BACKFILL_TX_FETCH_LIMIT`] non-failed signatures, and
/// fetch them with [`BACKFILL_CONCURRENCY`]-way parallelism. Whatever decodes
/// into `Liquidation` events streams to the view; the rest are silently
/// dropped. Bounded above by 1 + [`BACKFILL_TX_FETCH_LIMIT`] RPC round trips
/// — predictably fast and immune to long bail-out streaks.
///
/// Each decoded entry is streamed as it arrives — the view sorts by
/// `received_at` on insert, so out-of-order arrival (both within backfill and
/// relative to the live stream) is fine. A `BackfillComplete` signal is
/// always sent on exit so the modal's "backfilling…" indicator clears
/// regardless of how the task finished.
async fn backfill_recent_liquidations(
    rpc: Arc<RpcClient>,
    configs_by_asset: Arc<HashMap<u32, SplineConfig>>,
    tx: UnboundedSender<LiquidationFeedMsg>,
    dedup: Arc<Mutex<SignatureDedup>>,
) {
    backfill_inner(&rpc, &configs_by_asset, &tx, &dedup).await;
    // Always notify the view that backfill is finished — target reached, RPC
    // error, channel closed, all converge here.
    let _ = tx.send(LiquidationFeedMsg::BackfillComplete);
}

async fn backfill_inner(
    rpc: &Arc<RpcClient>,
    configs_by_asset: &Arc<HashMap<u32, SplineConfig>>,
    tx: &UnboundedSender<LiquidationFeedMsg>,
    dedup: &Arc<Mutex<SignatureDedup>>,
) {
    let Ok(liquidator) = Pubkey::from_str(PHOENIX_SOLE_LIQUIDATOR) else {
        warn!("invalid liquidator pubkey; skipping liquidation backfill");
        return;
    };

    let cfg = GetConfirmedSignaturesForAddress2Config {
        before: None,
        until: None,
        limit: Some(BACKFILL_SIGNATURES_LIMIT),
        commitment: Some(CommitmentConfig::confirmed()),
    };
    let fetch = rpc.get_signatures_for_address_with_config(&liquidator, cfg);
    let rows = match tokio::time::timeout(GET_SIGNATURES_TIMEOUT, fetch).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            warn!(error = %e, "liquidation backfill signatures RPC failed");
            return;
        }
        Err(_) => {
            warn!("liquidation backfill signatures RPC timed out");
            return;
        }
    };

    // Take the most-recent N successful (and not-yet-seen) signatures.
    let candidates: Vec<String> = rows
        .into_iter()
        .filter_map(|row| {
            if row.err.is_some() {
                return None;
            }
            let sig = row.signature;
            let new = dedup
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remember(sig.clone());
            if new {
                Some(sig)
            } else {
                None
            }
        })
        .take(BACKFILL_TX_FETCH_LIMIT)
        .collect();

    let mut decoded = futures_util::stream::iter(candidates)
        .map(|sig| {
            let rpc_clone = Arc::clone(rpc);
            let cfg_clone = Arc::clone(configs_by_asset);
            async move { fetch_and_decode(rpc_clone, sig, cfg_clone).await }
        })
        .buffer_unordered(BACKFILL_CONCURRENCY);

    while let Some(entries) = decoded.next().await {
        for e in entries {
            if tx.send(LiquidationFeedMsg::Entry(e)).is_err() {
                return;
            }
        }
    }
}

async fn fetch_and_emit(
    rpc: Arc<RpcClient>,
    signature: String,
    configs_by_asset: Arc<HashMap<u32, SplineConfig>>,
    tx: UnboundedSender<LiquidationFeedMsg>,
) {
    for entry in fetch_and_decode(rpc, signature, configs_by_asset).await {
        if tx.send(LiquidationFeedMsg::Entry(entry)).is_err() {
            return;
        }
    }
}

/// Pull a single tx, decode any Phoenix Eternal `Liquidation` events, and
/// return them as display-ready entries. Returns empty on any RPC/decoding
/// failure (warns rather than propagating — backfill and live both treat empty
/// as "skip").
async fn fetch_and_decode(
    rpc: Arc<RpcClient>,
    signature: String,
    configs_by_asset: Arc<HashMap<u32, SplineConfig>>,
) -> Vec<LiquidationEntry> {
    let Ok(sig) = Signature::from_str(&signature) else {
        return Vec::new();
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
            return Vec::new();
        }
        Err(_) => {
            warn!(sig = %signature, "get_transaction timed out for liquidation tx");
            return Vec::new();
        }
    };

    let Some(meta) = response.transaction.meta else {
        return Vec::new();
    };
    let inner_ixs: Vec<UiInnerInstructions> = match meta.inner_instructions {
        OptionSerializer::Some(v) => v,
        _ => return Vec::new(),
    };

    let account_keys =
        match extract_account_keys(&response.transaction.transaction, &meta.loaded_addresses) {
            Some(keys) => keys,
            None => return Vec::new(),
        };

    // The `LiquidationEvent` doesn't include the position's side, so we read
    // it out of the program logs the runtime emits alongside each liquidation:
    //   `Program log: Position side for trader [b0, b1, …, b31] on asset N: Long`
    // (or `Short`). Build a (trader, asset_id) → side map so each event can
    // resolve its own side without re-scanning the log slice.
    let sides: HashMap<(Pubkey, u32), TradingSide> = match &meta.log_messages {
        OptionSerializer::Some(lines) => lines
            .iter()
            .filter_map(|l| parse_position_side_log(l))
            .map(|(trader, asset_id, side)| ((trader, asset_id), side))
            .collect(),
        _ => HashMap::new(),
    };

    let parsed_ixs = match flatten_inner_instructions(&inner_ixs, &account_keys) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let events =
        parse_events_from_inner_instructions_with_context(&PHOENIX_ETERNAL_PROGRAM_ID, &parsed_ixs);

    let block_time = response.block_time;
    let mut out = Vec::new();
    for event in events {
        if let MarketEvent::Liquidation(e) = event {
            let received_at = block_time
                .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                .unwrap_or_else(Utc::now);
            let side = sides.get(&(e.liquidated_trader, e.asset_id)).copied();
            out.push(build_entry(
                &e,
                received_at,
                configs_by_asset.as_ref(),
                side,
            ));
        }
    }
    out
}

/// Extract `(trader, asset_id, side)` from a runtime log line of the form
/// `Program log: Position side for trader [b0, b1, …, b31] on asset N: Long`.
/// Returns `None` for any line that doesn't match the shape — the parser is
/// strict about the marker but lenient about surrounding whitespace.
fn parse_position_side_log(line: &str) -> Option<(Pubkey, u32, TradingSide)> {
    const MARKER: &str = "Position side for trader [";
    let bytes_start = line.find(MARKER)? + MARKER.len();
    let close_offset = line[bytes_start..].find(']')?;
    let bytes_str = &line[bytes_start..bytes_start + close_offset];

    let mut arr = [0u8; 32];
    let mut count = 0usize;
    for tok in bytes_str.split(',') {
        if count >= 32 {
            return None;
        }
        arr[count] = tok.trim().parse().ok()?;
        count += 1;
    }
    if count != 32 {
        return None;
    }
    let trader = Pubkey::from(arr);

    let after = &line[bytes_start + close_offset + 1..];
    const ASSET_MARKER: &str = "on asset ";
    let asset_start = after.find(ASSET_MARKER)? + ASSET_MARKER.len();
    let colon = after[asset_start..].find(':')?;
    let asset_id: u32 = after[asset_start..asset_start + colon]
        .trim()
        .parse()
        .ok()?;
    let side_str = after[asset_start + colon + 1..].trim();
    let side = if side_str.starts_with("Long") {
        TradingSide::Long
    } else if side_str.starts_with("Short") {
        TradingSide::Short
    } else {
        return None;
    };

    Some((trader, asset_id, side))
}

fn build_entry(
    e: &phoenix_eternal_types::LiquidationEvent,
    received_at: chrono::DateTime<Utc>,
    configs_by_asset: &HashMap<u32, SplineConfig>,
    side: Option<TradingSide>,
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
        None => (
            String::new(),
            4,
            4,
            e.base_lots_filled.as_inner() as f64,
            0.0,
        ),
    };

    let notional = quote_lots_to_usd(e.quote_lots_filled.as_inner());

    LiquidationEntry {
        received_at,
        symbol,
        asset_id: e.asset_id,
        side,
        liquidated_trader: pubkey_trader_prefix(&e.liquidated_trader),
        size,
        mark_price,
        notional,
        price_decimals,
        size_decimals,
    }
}

fn quote_lots_to_usd(quote_lots: u64) -> f64 {
    quote_lots as f64 / 10_f64.powi(QUOTE_LOT_DECIMALS)
}

fn extract_account_keys(
    encoded: &EncodedTransaction,
    loaded_addresses: &OptionSerializer<UiLoadedAddresses>,
) -> Option<Vec<Pubkey>> {
    let mut keys: Vec<Pubkey> = match encoded {
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
    }?;

    if let OptionSerializer::Some(loaded) = loaded_addresses {
        keys.extend(
            loaded
                .writable
                .iter()
                .chain(loaded.readonly.iter())
                .map(|s| Pubkey::from_str(s).ok())
                .collect::<Option<Vec<_>>>()?,
        );
    }

    Some(keys)
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

#[cfg(test)]
mod tests {
    use super::*;
    use phoenix_eternal_types::{BaseLots, LiquidationEvent, QuoteLots, Ticks};
    use solana_message::MessageHeader;
    use solana_transaction_status_client_types::{UiRawMessage, UiTransaction};

    #[test]
    fn extract_account_keys_appends_loaded_addresses_for_v0_transactions() {
        let static_key = Pubkey::from([1u8; 32]);
        let writable_loaded_key = Pubkey::from([2u8; 32]);
        let readonly_loaded_key = Pubkey::from([3u8; 32]);
        let encoded = EncodedTransaction::Json(UiTransaction {
            signatures: vec![],
            message: UiMessage::Raw(UiRawMessage {
                header: MessageHeader {
                    num_required_signatures: 0,
                    num_readonly_signed_accounts: 0,
                    num_readonly_unsigned_accounts: 0,
                },
                account_keys: vec![static_key.to_string()],
                recent_blockhash: String::new(),
                instructions: vec![],
                address_table_lookups: None,
            }),
        });
        let loaded = OptionSerializer::Some(UiLoadedAddresses {
            writable: vec![writable_loaded_key.to_string()],
            readonly: vec![readonly_loaded_key.to_string()],
        });

        let keys = extract_account_keys(&encoded, &loaded).expect("valid pubkeys");

        assert_eq!(
            keys,
            vec![static_key, writable_loaded_key, readonly_loaded_key]
        );
    }

    #[test]
    fn build_entry_uses_executed_quote_lots_for_notional() {
        let mut configs = HashMap::new();
        configs.insert(
            7,
            SplineConfig {
                tick_size: 1,
                base_lot_decimals: 0,
                spline_collection: String::new(),
                market_pubkey: String::new(),
                symbol: "SOL".to_string(),
                asset_id: 7,
                price_decimals: 2,
                size_decimals: 0,
            },
        );
        let event = LiquidationEvent {
            liquidator: Pubkey::from([1u8; 32]),
            liquidated_trader: Pubkey::from([2u8; 32]),
            asset_id: 7,
            liquidation_size: BaseLots::new(3),
            mark_price: Ticks::new(2_000_000),
            base_lots_filled: BaseLots::new(3),
            quote_lots_filled: QuoteLots::new(7_500_000),
            position_closed: false,
        };

        let entry = build_entry(&event, Utc::now(), &configs, Some(TradingSide::Long));

        assert_eq!(entry.size, 3.0);
        assert_eq!(entry.mark_price, 2.0);
        assert_eq!(entry.notional, 7.5);
        assert_eq!(entry.side, Some(TradingSide::Long));
    }

    #[test]
    fn parse_position_side_log_extracts_trader_asset_and_side() {
        let bytes = (0u8..32)
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let line = format!(
            "Program log: Position side for trader [{}] on asset 7: Short",
            bytes
        );
        let (trader, asset_id, side) = parse_position_side_log(&line).expect("parses");
        assert_eq!(
            trader,
            Pubkey::from(std::array::from_fn::<u8, 32, _>(|i| i as u8))
        );
        assert_eq!(asset_id, 7);
        assert_eq!(side, TradingSide::Short);
    }

    #[test]
    fn parse_position_side_log_rejects_unrelated_lines() {
        assert!(parse_position_side_log("Program log: instruction Liquidate").is_none());
        assert!(parse_position_side_log(
            "Program log: Position side for trader [1, 2] on asset 0: Long"
        )
        .is_none());
    }

    #[test]
    fn signature_dedup_rejects_duplicate_and_evicts_oldest() {
        let mut d = SignatureDedup::new();
        assert!(d.remember("a".to_string()));
        assert!(!d.remember("a".to_string()));
        for i in 0..SIGNATURE_DEDUP_CAP {
            d.remember(format!("k{i}"));
        }
        // "a" was inserted before the SIGNATURE_DEDUP_CAP fresh entries, so it
        // has been evicted and re-insertion now succeeds.
        assert!(d.remember("a".to_string()));
    }
}
