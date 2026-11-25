//! Close-all-positions submission — fans out reduce-only market orders for
//! every open position, batched and staggered.

use std::collections::HashSet;
use std::sync::Arc;

use phoenix_rise::ix::prelude::{MarketOrderParams, OrderFlags, create_place_market_order_ix};
use solana_keypair::Keypair;
use tokio::sync::mpsc;

use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::super::trading::TradingSide;
use super::confirmation::{BatchItem, BatchLabels, execute_batched_txs};
use super::context::TxContext;
use super::orders::wrap_order_ixs;

/// Describes a single position to close via a reduce-only market order.
pub struct ClosePositionEntry {
    pub symbol: String,
    pub subaccount_index: u8,
    pub close_side: TradingSide,
    pub num_base_lots: u64,
    pub display_size: f64,
}

/// Emit a single-line build warning onto `tx_status`.
fn send_build_err(tx_status: &mpsc::UnboundedSender<TxStatusMsg>, title: String) {
    let _ = tx_status.send(TxStatusMsg::SetStatus {
        title,
        detail: String::new(),
    });
}

/// Builds and submits batched reduce-only market orders to close `entries`.
pub fn submit_close_all_positions(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    entries: Vec<ClosePositionEntry>,
    positions_requested: usize,
    active_symbol: String,
    tx_status: mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        let s = strings();
        let total = entries.len();
        if total == 0 {
            return;
        }
        let mut built_symbols: HashSet<String> = HashSet::new();
        send_build_err(
            &tx_status,
            format!("{} {total} {}…", s.tx_building_close_all, s.st_position_s),
        );

        let mut items: Vec<BatchItem> = Vec::new();
        for entry in &entries {
            let Some(addrs) = ctx.market_addrs_for_symbol(&entry.symbol) else {
                send_build_err(
                    &tx_status,
                    format!("{} {} {}", s.market, entry.symbol, s.tx_not_found_skip),
                );
                continue;
            };
            let params = match MarketOrderParams::builder()
                .trader(ctx.authority_v2)
                .trader_account(ctx.trader_pda_for_subaccount(entry.subaccount_index))
                .perp_asset_map(addrs.perp_asset_map)
                .orderbook(addrs.orderbook)
                .spline_collection(addrs.spline_collection)
                .global_trader_index(addrs.global_trader_index)
                .active_trader_buffer(addrs.active_trader_buffer)
                .symbol(&entry.symbol)
                .side(entry.close_side.to_ix_side())
                .num_base_lots(entry.num_base_lots)
                .order_flags(OrderFlags::ReduceOnly)
                .subaccount_index(entry.subaccount_index)
                .build()
            {
                Ok(p) => p,
                Err(e) => {
                    send_build_err(
                        &tx_status,
                        format!("Params error (close {}): {e}", entry.symbol),
                    );
                    continue;
                }
            };
            let phoenix_ixs = match create_place_market_order_ix(params) {
                Ok(ix) => vec![ix.into()],
                Err(e) => {
                    send_build_err(
                        &tx_status,
                        format!("Build error (close {}): {e}", entry.symbol),
                    );
                    continue;
                }
            };
            let wrapped = match wrap_order_ixs(phoenix_ixs, ctx.authority_v2) {
                Ok(ixs) => ixs,
                Err(e) => {
                    send_build_err(
                        &tx_status,
                        format!("Flight wrap error (close {}): {e}", entry.symbol),
                    );
                    continue;
                }
            };

            let label = format!(
                "{} {} {}",
                s.tx_close_label, entry.display_size, entry.symbol
            );
            let trade_marker = (entry.symbol == active_symbol).then(|| {
                (
                    entry.symbol.clone(),
                    matches!(entry.close_side, TradingSide::Long),
                )
            });
            built_symbols.insert(entry.symbol.clone());
            for ix in wrapped {
                items.push(BatchItem {
                    ix,
                    label: label.clone(),
                    trade_marker: trade_marker.clone(),
                });
            }
        }

        if items.is_empty() {
            send_build_err(&tx_status, s.tx_close_all_aborted.to_string());
            return;
        }

        let last_sig = execute_batched_txs(
            &ctx,
            &keypair,
            items,
            BatchLabels {
                broadcasting: s.tx_broadcasting_batch,
                failed_prepare: s.tx_failed_prepare_batch,
                confirming: s.tx_confirming_batch,
                batch_noun: s.tx_batch,
                log_noun: "close-all",
            },
            &tx_status,
        )
        .await;

        let closed = built_symbols.len();
        let title = if closed < positions_requested {
            format!(
                "{} ({closed}/{positions_requested} {})",
                s.tx_close_all_complete, s.st_position_s
            )
        } else {
            format!("{} ({closed} {})", s.tx_close_all_complete, s.st_position_s)
        };
        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title,
            detail: last_sig,
        });
    });
}
