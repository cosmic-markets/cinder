//! Close-all-positions submission — fans out reduce-only market orders for
//! every open position, batched and staggered so the network accepts the
//! whole burst.

use std::sync::Arc;
use std::time::Duration;

use solana_keypair::Keypair;

use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::super::trading::TradingSide;
use super::compute_budget::build_compute_budget_ixs;
use super::confirmation::{compile_and_sign, subscribe_send_confirm, ConfirmError};
use super::context::TxContext;
use super::error::{
    format_not_confirmed_error, log_tx_error, not_confirmed_is_onchain_execution_failure,
    parse_phoenix_tx_error,
};

/// Describes a single position to close, carrying everything needed to build
/// its reduce-only market order instruction.
pub struct ClosePositionEntry {
    pub symbol: String,
    pub subaccount_index: u8,
    pub close_side: TradingSide,
    pub num_base_lots: u64,
    pub display_size: f64,
}

pub(super) const CLOSE_BATCH_SIZE: usize = 5;
pub(super) const CLOSE_BATCH_STAGGER: Duration = Duration::from_millis(500);

pub fn submit_close_all_positions(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    entries: Vec<ClosePositionEntry>,
    active_symbol: String,
    tx_status: tokio::sync::mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        use phoenix_rise::ix::{create_place_market_order_ix, MarketOrderParams, OrderFlags, Side};

        let s = strings();
        let total = entries.len();
        if total == 0 {
            return;
        }
        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} {} {}…", s.tx_building_close_all, total, s.st_position_s),
            detail: String::new(),
        });

        // Reuse the caller's live TxContext — its Phoenix metadata covers every market,
        // the blockhash pool is already warm, and the RpcClient/WSS are reusable.

        let mut all_ixs: Vec<(solana_instruction::Instruction, String, String, TradingSide)> =
            Vec::new();
        for entry in &entries {
            let addrs = match ctx.market_addrs_for_symbol(&entry.symbol) {
                Some(addrs) => addrs,
                None => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} {} {}", s.market, entry.symbol, s.tx_not_found_skip),
                        detail: String::new(),
                    });
                    continue;
                }
            };
            let trader_account = ctx.trader_pda_for_subaccount(entry.subaccount_index);

            let phx_side = match entry.close_side {
                TradingSide::Long => Side::Bid,
                TradingSide::Short => Side::Ask,
            };
            let params = match MarketOrderParams::builder()
                .trader(ctx.authority_v2)
                .trader_account(trader_account)
                .perp_asset_map(addrs.perp_asset_map)
                .orderbook(addrs.orderbook)
                .spline_collection(addrs.spline_collection)
                .global_trader_index(addrs.global_trader_index)
                .active_trader_buffer(addrs.active_trader_buffer)
                .symbol(&entry.symbol)
                .side(phx_side)
                .num_base_lots(entry.num_base_lots)
                .order_flags(OrderFlags::ReduceOnly)
                .subaccount_index(entry.subaccount_index)
                .build()
            {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("Params error (close {}): {}", entry.symbol, e),
                        detail: String::new(),
                    });
                    continue;
                }
            };
            let phoenix_ixs = match create_place_market_order_ix(params) {
                Ok(ix) => vec![ix.into()],
                Err(e) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("Build error (close {}): {}", entry.symbol, e),
                        detail: String::new(),
                    });
                    continue;
                }
            };
            let mapped = phoenix_ixs;
            let label = format!(
                "{} {} {}",
                s.tx_close_label, entry.display_size, entry.symbol
            );
            for ix in mapped {
                all_ixs.push((ix, label.clone(), entry.symbol.clone(), entry.close_side));
            }
        }

        if all_ixs.is_empty() {
            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: s.tx_close_all_aborted.to_string(),
                detail: String::new(),
            });
            return;
        }

        let batches: Vec<Vec<(solana_instruction::Instruction, String, String, TradingSide)>> =
            all_ixs
                .chunks(CLOSE_BATCH_SIZE)
                .map(|c| c.to_vec())
                .collect();

        let num_batches = batches.len();
        let mut last_sig = String::new();
        for (batch_idx, batch) in batches.into_iter().enumerate() {
            let summary: Vec<String> = batch.iter().map(|(_, lbl, _, _)| lbl.clone()).collect();
            let summary_dedup: Vec<String> = {
                let mut seen = std::collections::HashSet::new();
                summary
                    .into_iter()
                    .filter(|s| seen.insert(s.clone()))
                    .collect()
            };
            let batch_label = summary_dedup.join(", ");
            let active_markers: Vec<bool> = {
                let mut seen = std::collections::HashSet::new();
                batch
                    .iter()
                    .filter_map(|(_, _, sym, side)| {
                        if sym == &active_symbol && seen.insert(sym.clone()) {
                            Some(matches!(side, TradingSide::Long))
                        } else {
                            None
                        }
                    })
                    .collect()
            };

            let mut ixs: Vec<solana_instruction::Instruction> =
                batch.into_iter().map(|(ix, _, _, _)| ix).collect();
            // One market-order IX per entry in the batch; scale CU with instruction count
            // (not deduped labels).
            let market_ops_in_batch = ixs.len().max(1) as u32;
            ixs.extend(build_compute_budget_ixs(market_ops_in_batch));

            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: format!(
                    "{} {}/{}: {}…",
                    s.tx_broadcasting_batch,
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
                            s.tx_failed_prepare_batch,
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
                    s.tx_confirming_batch,
                    batch_idx + 1,
                    num_batches,
                    batch_label
                ),
                detail: sig_str.clone(),
            });

            match subscribe_send_confirm(&ctx, &tx, &sig).await {
                Ok(()) => {
                    for is_buy in &active_markers {
                        let _ = tx_status.send(TxStatusMsg::TradeMarker { is_buy: *is_buy });
                    }
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!(
                            "✅ {} {}/{} {} {}",
                            s.tx_batch,
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
                        &format!("close-all batch {}/{} rejected", batch_idx + 1, num_batches),
                        &e,
                    );
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!(
                            "❌ {} {}/{} {}",
                            s.tx_batch,
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
                            "close-all batch {}/{} not confirmed",
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
                                s.tx_batch,
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
                                s.tx_batch,
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
                tokio::time::sleep(CLOSE_BATCH_STAGGER).await;
            }
        }

        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!(
                "{} ({} {})",
                s.tx_close_all_complete, total, s.st_position_s
            ),
            detail: last_sig,
        });
    });
}
