//! Order cancellation — batches `CancelOrdersById` instructions per market
//! and stop-loss cancels per asset, staggering submission so a stack of
//! cancels lands without exceeding the per-tx instruction limit.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use solana_keypair::Keypair;

use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::compute_budget::build_compute_budget_ixs;
use super::confirmation::{compile_and_sign, subscribe_send_confirm, ConfirmError};
use super::context::TxContext;
use super::error::{
    format_not_confirmed_error, log_tx_error, not_confirmed_is_onchain_execution_failure,
    parse_phoenix_tx_error,
};

/// One open order to cancel — the (`price_ticks`, `order_sequence_number`) pair
/// is the on-chain `CancelId`, `symbol` selects which market's IX it lives in.
///
/// When `is_stop_loss` is true, price_ticks/order_sequence_number are
/// ignored; the entry is routed through `cancel_stop_loss` instead and the
/// trigger is keyed by the market's `asset_id` + `stop_direction`.
pub struct CancelOrderEntry {
    pub symbol: String,
    pub subaccount_index: u8,
    pub price_ticks: u64,
    pub order_sequence_number: u64,
    pub is_stop_loss: bool,
    pub stop_direction: Option<phoenix_rise::Direction>,
    pub conditional_order_index: Option<u8>,
    pub conditional_trigger_direction: Option<phoenix_rise::Direction>,
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
        use phoenix_rise::ix::{
            create_cancel_conditional_order_ix, create_cancel_orders_by_id_ix,
            create_cancel_stop_loss_ix, CancelConditionalOrderParams, CancelOrdersByIdParams,
            CancelStopLossParams,
        };
        use phoenix_rise::CancelId;

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

        // Split conditional rows first: position conditional orders live in the
        // trader conditional-orders account and cancel by account index +
        // trigger direction. Legacy stop-loss rows still route through
        // `cancel_stop_loss`; plain limits batch through `cancel_orders_by_id`.
        let (conditional_entries, remaining_entries): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|e| e.conditional_order_index.is_some());
        let (stop_entries, limit_entries): (Vec<_>, Vec<_>) =
            remaining_entries.into_iter().partition(|e| e.is_stop_loss);

        // Group limit cancels by symbol: each symbol becomes (at most ceil(n/100))
        // cancel-orders IXs.
        let mut by_symbol: std::collections::BTreeMap<(String, u8), Vec<CancelId>> =
            std::collections::BTreeMap::new();
        for e in limit_entries.into_iter() {
            by_symbol
                .entry((e.symbol, e.subaccount_index))
                .or_default()
                .push(CancelId::new(e.price_ticks, e.order_sequence_number));
        }

        let mut all_ixs: Vec<(solana_instruction::Instruction, String)> = Vec::new();

        for e in conditional_entries.into_iter() {
            let Some(order_index) = e.conditional_order_index else {
                continue;
            };
            let Some(direction) = e.conditional_trigger_direction else {
                continue;
            };
            let Some(market) = ctx.metadata.get_market(&e.symbol) else {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!(
                        "Build error (cancel conditional {}): unknown market",
                        e.symbol
                    ),
                    detail: String::new(),
                });
                continue;
            };
            let orderbook = match solana_pubkey::Pubkey::from_str(&market.market_pubkey) {
                Ok(pk) => pk,
                Err(err) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("Build error (cancel conditional {}): {}", e.symbol, err),
                        detail: String::new(),
                    });
                    continue;
                }
            };
            let trader_account = ctx.trader_pda_for_subaccount(e.subaccount_index);
            let params = match CancelConditionalOrderParams::builder()
                .trader_account(trader_account)
                .position_authority(ctx.authority_v2)
                .orderbook(orderbook)
                .conditional_order_index(order_index)
                .disable_first(matches!(direction, phoenix_rise::Direction::GreaterThan))
                .disable_second(matches!(direction, phoenix_rise::Direction::LessThan))
                .build()
            {
                Ok(p) => p,
                Err(err) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("Build error (cancel conditional {}): {}", e.symbol, err),
                        detail: String::new(),
                    });
                    continue;
                }
            };
            let ix: solana_instruction::Instruction =
                match create_cancel_conditional_order_ix(params) {
                    Ok(i) => i.into(),
                    Err(err) => {
                        let _ = tx_status.send(TxStatusMsg::SetStatus {
                            title: format!(
                                "IX build error (cancel conditional {}): {}",
                                e.symbol, err
                            ),
                            detail: String::new(),
                        });
                        continue;
                    }
                };
            all_ixs.push((ix, format!("{} {} STP", s.tx_cancel_label, e.symbol)));
        }

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
                .trader_account(ctx.trader_pda_for_subaccount(e.subaccount_index))
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

        for ((symbol, subaccount_index), cancel_ids) in by_symbol.into_iter() {
            for chunk in cancel_ids.chunks(MAX_CANCELS_PER_IX) {
                let Some(addrs) = ctx.market_addrs_for_symbol(&symbol) else {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("Build error (cancel {}): unknown market", symbol),
                        detail: String::new(),
                    });
                    continue;
                };
                let trader_account = ctx.trader_pda_for_subaccount(subaccount_index);
                let params = match CancelOrdersByIdParams::builder()
                    .trader(ctx.authority_v2)
                    .trader_account(trader_account)
                    .perp_asset_map(addrs.perp_asset_map)
                    .orderbook(addrs.orderbook)
                    .spline_collection(addrs.spline_collection)
                    .global_trader_index(addrs.global_trader_index)
                    .active_trader_buffer(addrs.active_trader_buffer)
                    .order_ids(chunk.to_vec())
                    .build()
                {
                    Ok(params) => params,
                    Err(e) => {
                        let _ = tx_status.send(TxStatusMsg::SetStatus {
                            title: format!("Build error (cancel {}): {}", symbol, e),
                            detail: String::new(),
                        });
                        continue;
                    }
                };
                let ix: solana_instruction::Instruction =
                    match create_cancel_orders_by_id_ix(params) {
                        Ok(ix) => ix.into(),
                        Err(e) => {
                            let _ = tx_status.send(TxStatusMsg::SetStatus {
                                title: format!("IX build error (cancel {}): {}", symbol, e),
                                detail: String::new(),
                            });
                            continue;
                        }
                    };
                let label = format!("{} {}×{}", s.tx_cancel_label, symbol, chunk.len());
                all_ixs.push((ix, label));
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
                    let onchain_fail = not_confirmed_is_onchain_execution_failure(&e);
                    let mapped = format_not_confirmed_error(&e);
                    let (title, detail) = if onchain_fail {
                        (
                            format!(
                                "❌ {} {}/{} {}",
                                s.tx_cancel_batch,
                                batch_idx + 1,
                                num_batches,
                                s.tx_batch_exec_failed_suf
                            ),
                            parse_phoenix_tx_error(&e),
                        )
                    } else {
                        (
                            format!(
                                "{} {}/{} {} ({})",
                                s.tx_cancel_batch,
                                batch_idx + 1,
                                num_batches,
                                s.tx_batch_not_confirmed_suf,
                                mapped
                            ),
                            sig_str,
                        )
                    };
                    let _ = tx_status.send(TxStatusMsg::SetStatus { title, detail });
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
