//! Order cancellation — batches `CancelOrdersById` instructions per market
//! and stop-loss cancels per asset, staggering submission so a stack of
//! cancels lands without exceeding the per-tx instruction limit.

use std::sync::Arc;
use std::time::Duration;

use solana_keypair::Keypair;

use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::compute_budget::build_compute_budget_ixs;
use super::confirmation::{compile_and_sign, subscribe_send_confirm, ConfirmError};
use super::context::TxContext;
use super::error::{log_tx_error, parse_phoenix_tx_error};

/// One open order to cancel — the (`price_ticks`, `order_sequence_number`) pair
/// is the on-chain `CancelId`, `symbol` selects which market's IX it lives in.
///
/// When `is_stop_loss` is true, price_ticks/order_sequence_number are
/// ignored; the entry is routed through `cancel_stop_loss` instead and the
/// trigger is keyed by the market's `asset_id` + `stop_direction`.
pub struct CancelOrderEntry {
    pub symbol: String,
    pub price_ticks: u64,
    pub order_sequence_number: u64,
    pub is_stop_loss: bool,
    pub stop_direction: Option<phoenix_rise::Direction>,
}

/// One IX per symbol fits at most this many orders; mirrors
/// `MAX_CANCEL_ORDER_IDS` in `phoenix_rise::ix::cancel_orders`. We chunk per-symbol
/// orders into runs of this size.
const MAX_CANCELS_PER_IX: usize = 100;
/// Bundle this many cancel-orders IXs into a single transaction. Mirrors
/// `CLOSE_BATCH_SIZE` for closes; cancels touch the same write-set shape
/// (orderbook + spline collection + trader buffers), so the same conservative
/// budget applies.
const CANCEL_BATCH_SIZE: usize = 5;
const CANCEL_BATCH_STAGGER: Duration = Duration::from_millis(500);

/// Builds and submits cancel-orders transactions for `entries`. Orders are
/// grouped by symbol (one IX per symbol, capped at `MAX_CANCELS_PER_IX`), then
/// those IXs are batched into transactions of `CANCEL_BATCH_SIZE`.
/// `summary` is a short label for the status line, e.g. "1 order on SOL" or "3
/// order(s)".
pub fn submit_cancel_orders(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    entries: Vec<CancelOrderEntry>,
    summary: String,
    tx_status: tokio::sync::mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        use phoenix_rise::ix::{create_cancel_stop_loss_ix, CancelStopLossParams};
        use phoenix_rise::{CancelId, PhoenixTxBuilder};

        let s = strings();

        if entries.is_empty() {
            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: s.st_no_orders.to_string(),
                detail: String::new(),
            });
            return;
        }

        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} {}…", s.tx_building_cancel, summary),
            detail: String::new(),
        });

        let builder = PhoenixTxBuilder::new(&ctx.metadata);

        // Split stop cancels out — they don't batch via cancel_orders_by_id.
        let (stop_entries, limit_entries): (Vec<_>, Vec<_>) =
            entries.into_iter().partition(|e| e.is_stop_loss);

        // Group limit cancels by symbol: each symbol becomes (at most ceil(n/100))
        // cancel-orders IXs.
        let mut by_symbol: std::collections::BTreeMap<String, Vec<CancelId>> =
            std::collections::BTreeMap::new();
        for e in limit_entries.into_iter() {
            by_symbol
                .entry(e.symbol)
                .or_default()
                .push(CancelId::new(e.price_ticks, e.order_sequence_number));
        }

        let mut all_ixs: Vec<(solana_instruction::Instruction, String)> = Vec::new();

        // One `cancel_stop_loss` IX per (symbol, direction). Pending stops are
        // keyed on the (trader_account, asset_id, direction) triple so we never
        // need more than two per symbol (LessThan + GreaterThan).
        for e in stop_entries.into_iter() {
            let Some(direction) = e.stop_direction else {
                continue;
            };
            let Some(market) = ctx.metadata.get_market(&e.symbol) else {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("Build error (cancel stop {}): unknown market", e.symbol),
                    detail: String::new(),
                });
                continue;
            };
            let asset_id = market.asset_id as u64;
            let params = match CancelStopLossParams::builder()
                .funder(ctx.authority_v2)
                .trader_account(ctx.trader_pda_v2)
                .position_authority(ctx.authority_v2)
                .asset_id(asset_id)
                .execution_direction(direction)
                .build()
            {
                Ok(p) => p,
                Err(err) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("Build error (cancel stop {}): {}", e.symbol, err),
                        detail: String::new(),
                    });
                    continue;
                }
            };
            let ix: solana_instruction::Instruction = match create_cancel_stop_loss_ix(params) {
                Ok(i) => i.into(),
                Err(err) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("IX build error (cancel stop {}): {}", e.symbol, err),
                        detail: String::new(),
                    });
                    continue;
                }
            };
            let mapped = vec![ix];
            let label = format!("{} {} STP", s.tx_cancel_label, e.symbol);
            for ix in mapped {
                all_ixs.push((ix, label.clone()));
            }
        }

        for (symbol, cancel_ids) in by_symbol.into_iter() {
            for chunk in cancel_ids.chunks(MAX_CANCELS_PER_IX) {
                let phoenix_ixs = match builder.build_cancel_orders(
                    ctx.authority_v2,
                    ctx.trader_pda_v2,
                    &symbol,
                    chunk.to_vec(),
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = tx_status.send(TxStatusMsg::SetStatus {
                            title: format!("Build error (cancel {}): {}", symbol, e),
                            detail: String::new(),
                        });
                        continue;
                    }
                };
                let mapped = phoenix_ixs;
                let label = format!("{} {}×{}", s.tx_cancel_label, symbol, chunk.len());
                for ix in mapped {
                    all_ixs.push((ix, label.clone()));
                }
            }
        }

        if all_ixs.is_empty() {
            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: s.tx_cancel_aborted.to_string(),
                detail: String::new(),
            });
            return;
        }

        let batches: Vec<Vec<(solana_instruction::Instruction, String)>> = all_ixs
            .chunks(CANCEL_BATCH_SIZE)
            .map(|c| c.to_vec())
            .collect();

        let num_batches = batches.len();
        let mut last_sig = String::new();
        for (batch_idx, batch) in batches.into_iter().enumerate() {
            let labels: Vec<String> = {
                let mut seen = std::collections::HashSet::new();
                batch
                    .iter()
                    .filter_map(|(_, lbl)| {
                        if seen.insert(lbl.clone()) {
                            Some(lbl.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            let batch_label = labels.join(", ");

            let mut ixs: Vec<solana_instruction::Instruction> =
                batch.into_iter().map(|(ix, _)| ix).collect();
            // One cancel-orders IX per item in the batch — scale CU the same way as closes.
            let ops_in_batch = ixs.len().max(1) as u32;
            ixs.extend(build_compute_budget_ixs(ops_in_batch));

            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: format!(
                    "{} {}/{}: {}…",
                    s.tx_broadcasting_cancel_batch,
                    batch_idx + 1,
                    num_batches,
                    batch_label
                ),
                detail: String::new(),
            });

            let (tx, sig) = match compile_and_sign(&ctx, &keypair, &ixs).await {
                Ok(pair) => pair,
                Err(e) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!(
                            "{} {}/{}",
                            s.tx_failed_prepare_cancel_batch,
                            batch_idx + 1,
                            num_batches
                        ),
                        detail: e,
                    });
                    break;
                }
            };
            let sig_str = sig.to_string();
            last_sig = sig_str.clone();
            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: format!(
                    "{} {}/{}: {}…",
                    s.tx_confirming_cancel_batch,
                    batch_idx + 1,
                    num_batches,
                    batch_label
                ),
                detail: sig_str.clone(),
            });

            match subscribe_send_confirm(&ctx, &tx, &sig).await {
                Ok(()) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!(
                            "✅ {} {}/{} {} {}",
                            s.tx_cancel_batch,
                            batch_idx + 1,
                            num_batches,
                            s.tx_batch_confirmed_suf,
                            batch_label
                        ),
                        detail: sig_str,
                    });
                }
                Err(ConfirmError::Rejected(e)) => {
                    log_tx_error(
                        None,
                        &format!("cancel batch {}/{} rejected", batch_idx + 1, num_batches),
                        &e,
                    );
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!(
                            "❌ {} {}/{} {}",
                            s.tx_cancel_batch,
                            batch_idx + 1,
                            num_batches,
                            s.tx_batch_rejected_suf
                        ),
                        detail: parse_phoenix_tx_error(&e),
                    });
                }
                Err(ConfirmError::NotConfirmed(e)) => {
                    log_tx_error(
                        Some(&sig_str),
                        &format!(
                            "cancel batch {}/{} not confirmed",
                            batch_idx + 1,
                            num_batches
                        ),
                        &e,
                    );
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!(
                            "{} {}/{} {} ({})",
                            s.tx_cancel_batch,
                            batch_idx + 1,
                            num_batches,
                            s.tx_batch_not_confirmed_suf,
                            e
                        ),
                        detail: sig_str,
                    });
                }
            }

            if batch_idx + 1 < num_batches {
                tokio::time::sleep(CANCEL_BATCH_STAGGER).await;
            }
        }

        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} ({})", s.tx_cancel_complete, summary),
            detail: last_sig,
        });
    });
}
