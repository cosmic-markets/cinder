//! Live liquidation feed task.
//!
//! On startup, backfills a short recent window by polling recent successful
//! transactions mentioning Phoenix's current sole liquidator. Then subscribes
//! (via `logsSubscribe`) for live updates. For each transaction, we fetch the
//! full transaction, flatten the inner instructions, and parse
//! `MarketEvent::Liquidation`s out of the self-CPI event stream. Each
//! liquidation is converted to display units against the locally-cached
//! `SplineConfig` map and shipped to the TUI through `liquidation_tx`.

use std::collections::{HashMap, HashSet, VecDeque};
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
use super::super::super::state::LiquidationEntry;
use super::{WSS_RETRY_CAP, WSS_RETRY_INIT};

/// How many recently-seen tx signatures to remember so we don't re-fetch the
/// same tx if `logsSubscribe` re-emits it (some RPCs do under reconnect or
/// commitment-level transitions).
const SIGNATURE_DEDUP_CAP: usize = 256;

/// Phoenix's current sole liquidator. Subscribing to this account is far
/// quieter than subscribing to every transaction mentioning the Phoenix Eternal
/// program.
const PHOENIX_SOLE_LIQUIDATOR: &str = "BP7sV1VFnbPMPyJX1tZNbXHbZkyLNFEaBWJhyMvkbxKz";

/// Cap on a single `get_transaction` RPC. Liquidation latency is tolerable;
/// hanging on a stalled RPC is not.
const GET_TX_TIMEOUT: Duration = Duration::from_secs(8);

/// Bound concurrent transaction fetches so a burst of liquidation-like logs
/// cannot create an unbounded pile of RPC requests.
const MAX_CONCURRENT_GET_TX: usize = 8;

/// One-shot startup backfill depth (recent successful txs touching the
/// liquidator account). This warms the modal even before the first live event.
const STARTUP_BACKFILL_SIGNATURE_LIMIT: usize = 100;

/// Cap on `get_signatures_for_address_with_config`; used only during startup
/// backfill.
const GET_SIGNATURES_TIMEOUT: Duration = Duration::from_secs(8);

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
        let configs_by_asset: Arc<HashMap<u32, SplineConfig>> =
            Arc::new(configs.values().map(|c| (c.asset_id, c.clone())).collect());
        let get_tx_permits = Arc::new(Semaphore::new(MAX_CONCURRENT_GET_TX));

        let rpc = Arc::new(RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        ));

        let mut backoff = WSS_RETRY_INIT;
        let mut seen_signatures: HashSet<String> = HashSet::with_capacity(SIGNATURE_DEDUP_CAP);
        let mut seen_order: VecDeque<String> = VecDeque::with_capacity(SIGNATURE_DEDUP_CAP);

        // One-shot startup backfill so the modal has recent rows even if this
        // process came online after liquidations already happened. We only pull
        // signatures here; actual transaction fetch/decode starts after the
        // live subscription is established.
        let mut startup_backfill_sigs = collect_recent_liquidation_signatures(
            Arc::clone(&rpc),
            &mut seen_signatures,
            &mut seen_order,
        )
        .await;

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

            // Kick off startup backfill only once, after we already have the
            // live stream attached.
            if !startup_backfill_sigs.is_empty() {
                let signatures = std::mem::take(&mut startup_backfill_sigs);
                let rpc_clone = Arc::clone(&rpc);
                let configs_clone = Arc::clone(&configs_by_asset);
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    for signature in signatures {
                        fetch_and_emit(
                            Arc::clone(&rpc_clone),
                            signature,
                            Arc::clone(&configs_clone),
                            tx_clone.clone(),
                        )
                        .await;
                    }
                });
            }

            while let Some(notif) = stream.next().await {
                if notif.value.err.is_some() {
                    continue;
                }

                let signature = notif.value.signature.clone();
                if !remember_signature(&mut seen_signatures, &mut seen_order, signature.clone()) {
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

fn remember_signature(
    set: &mut HashSet<String>,
    order: &mut VecDeque<String>,
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

async fn collect_recent_liquidation_signatures(
    rpc: Arc<RpcClient>,
    seen_signatures: &mut HashSet<String>,
    seen_order: &mut VecDeque<String>,
) -> Vec<String> {
    let Ok(liquidator) = Pubkey::from_str(PHOENIX_SOLE_LIQUIDATOR) else {
        warn!("invalid liquidator pubkey; skipping liquidation backfill");
        return Vec::new();
    };

    let cfg = GetConfirmedSignaturesForAddress2Config {
        before: None,
        until: None,
        limit: Some(STARTUP_BACKFILL_SIGNATURE_LIMIT),
        commitment: Some(CommitmentConfig::confirmed()),
        // `get_signatures_for_address_with_config` uses this compatibility
        // config type and maps it to the JSON-RPC config under the hood.
    };
    let fetch = rpc.get_signatures_for_address_with_config(&liquidator, cfg);
    let signatures = match tokio::time::timeout(GET_SIGNATURES_TIMEOUT, fetch).await {
        Ok(Ok(rows)) => rows,
        Ok(Err(e)) => {
            warn!(error = %e, "liquidation startup backfill signatures RPC failed");
            return Vec::new();
        }
        Err(_) => {
            warn!("liquidation startup backfill signatures RPC timed out");
            return Vec::new();
        }
    };

    // RPC returns newest-first. Replay oldest->newest so the feed's push_front
    // ordering remains newest-first after the backfill is inserted.
    let mut ordered_sigs = Vec::with_capacity(signatures.len());
    for row in signatures {
        // `err: null` means success in `getSignaturesForAddress`. We backfill
        // only successful signatures and skip failures.
        if row.err.is_some() {
            continue;
        }
        let signature = row.signature;
        if remember_signature(seen_signatures, seen_order, signature.clone()) {
            ordered_sigs.push(signature);
        }
    }
    ordered_sigs.into_iter().rev().collect()
}

async fn fetch_and_emit(
    rpc: Arc<RpcClient>,
    signature: String,
    configs_by_asset: Arc<HashMap<u32, SplineConfig>>,
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

    let account_keys =
        match extract_account_keys(&response.transaction.transaction, &meta.loaded_addresses) {
            Some(keys) => keys,
            None => return,
        };

    let parsed_ixs = match flatten_inner_instructions(&inner_ixs, &account_keys) {
        Some(p) => p,
        None => return,
    };

    let events =
        parse_events_from_inner_instructions_with_context(&PHOENIX_ETERNAL_PROGRAM_ID, &parsed_ixs);

    // Build a quick `liquidated_trader` resolver: walk the event stream and
    // collect each Liquidation, then convert + emit.
    let block_time = response.block_time;
    for event in events {
        if let MarketEvent::Liquidation(e) = event {
            let received_at = block_time
                .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                .unwrap_or_else(Utc::now);
            let entry = build_entry(&e, received_at, configs_by_asset.as_ref());
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
        liquidated_trader: pubkey_trader_prefix(&e.liquidated_trader),
        size,
        mark_price,
        notional,
        position_closed: e.position_closed,
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

        let entry = build_entry(&event, Utc::now(), &configs);

        assert_eq!(entry.size, 3.0);
        assert_eq!(entry.mark_price, 2.0);
        assert_eq!(entry.notional, 7.5);
    }
}
