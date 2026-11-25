//! Order cancellation — batches `CancelOrdersById` instructions per market
//! and stop-loss / conditional cancels per asset.

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use phoenix_rise::ix::prelude::{
    CancelConditionalOrderParams, CancelId, CancelOrdersByIdParams, CancelStopLossParams,
    Direction, create_cancel_conditional_order_ix, create_cancel_orders_by_id_ix,
    create_cancel_stop_loss_ix,
};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use tokio::sync::mpsc;

use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::confirmation::{BatchItem, BatchLabels, execute_batched_txs};
use super::context::TxContext;

/// One open order to cancel.
pub struct CancelOrderEntry {
    pub symbol: String,
    pub subaccount_index: u8,
    pub price_ticks: u64,
    pub order_sequence_number: u64,
    pub is_stop_loss: bool,
    pub stop_direction: Option<Direction>,
    pub conditional_order_index: Option<u8>,
    pub conditional_trigger_direction: Option<Direction>,
}

/// Maximum order IDs per `CancelOrdersById` instruction.
const MAX_CANCELS_PER_IX: usize = 100;

/// Emit a single-line build error onto `tx_status`.
fn send_build_err(tx_status: &mpsc::UnboundedSender<TxStatusMsg>, title: String) {
    let _ = tx_status.send(TxStatusMsg::SetStatus {
        title,
        detail: String::new(),
    });
}

/// Builds and submits batched cancel transactions for `entries`.
pub fn submit_cancel_orders(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    entries: Vec<CancelOrderEntry>,
    summary: String,
    tx_status: mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        let s = strings();
        if entries.is_empty() {
            send_build_err(&tx_status, s.st_no_orders.to_string());
            return;
        }

        send_build_err(&tx_status, format!("{} {summary}…", s.tx_building_cancel));

        let (conditional_entries, remaining): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|e| e.conditional_order_index.is_some());
        let (stop_entries, limit_entries): (Vec<_>, Vec<_>) =
            remaining.into_iter().partition(|e| e.is_stop_loss);

        let mut by_symbol: BTreeMap<(String, u8), Vec<CancelId>> = BTreeMap::new();
        for e in limit_entries {
            by_symbol
                .entry((e.symbol, e.subaccount_index))
                .or_default()
                .push(CancelId::new(e.price_ticks, e.order_sequence_number));
        }

        let mut items: Vec<BatchItem> = Vec::new();

        // Conditional position orders (indexed by slot + trigger direction).
        for e in conditional_entries {
            let (Some(order_index), Some(direction)) =
                (e.conditional_order_index, e.conditional_trigger_direction)
            else {
                continue;
            };
            let Some(market) = ctx.metadata.get_market(&e.symbol) else {
                send_build_err(
                    &tx_status,
                    format!(
                        "Build error (cancel conditional {}): unknown market",
                        e.symbol
                    ),
                );
                continue;
            };
            let orderbook = match Pubkey::from_str(&market.market_pubkey) {
                Ok(pk) => pk,
                Err(err) => {
                    send_build_err(
                        &tx_status,
                        format!("Build error (cancel conditional {}): {err}", e.symbol),
                    );
                    continue;
                }
            };
            let params = match CancelConditionalOrderParams::builder()
                .trader_account(ctx.trader_pda_for_subaccount(e.subaccount_index))
                .position_authority(ctx.authority_v2)
                .orderbook(orderbook)
                .conditional_order_index(order_index)
                .disable_first(matches!(direction, Direction::GreaterThan))
                .disable_second(matches!(direction, Direction::LessThan))
                .build()
            {
                Ok(p) => p,
                Err(err) => {
                    send_build_err(
                        &tx_status,
                        format!("Build error (cancel conditional {}): {err}", e.symbol),
                    );
                    continue;
                }
            };
            match create_cancel_conditional_order_ix(params) {
                Ok(ix) => items.push(BatchItem {
                    ix: ix.into(),
                    label: format!("{} {} STP", s.tx_cancel_label, e.symbol),
                    trade_marker: None,
                }),
                Err(err) => send_build_err(
                    &tx_status,
                    format!("IX build error (cancel conditional {}): {err}", e.symbol),
                ),
            }
        }

        // Legacy stop-loss orders (keyed by asset_id + direction).
        for e in stop_entries {
            let Some(direction) = e.stop_direction else {
                continue;
            };
            let Some(market) = ctx.metadata.get_market(&e.symbol) else {
                send_build_err(
                    &tx_status,
                    format!("Build error (cancel stop {}): unknown market", e.symbol),
                );
                continue;
            };
            let params = match CancelStopLossParams::builder()
                .funder(ctx.authority_v2)
                .trader_account(ctx.trader_pda_for_subaccount(e.subaccount_index))
                .position_authority(ctx.authority_v2)
                .asset_id(market.asset_id as u64)
                .execution_direction(direction)
                .build()
            {
                Ok(p) => p,
                Err(err) => {
                    send_build_err(
                        &tx_status,
                        format!("Build error (cancel stop {}): {err}", e.symbol),
                    );
                    continue;
                }
            };
            match create_cancel_stop_loss_ix(params) {
                Ok(ix) => items.push(BatchItem {
                    ix: ix.into(),
                    label: format!("{} {} STP", s.tx_cancel_label, e.symbol),
                    trade_marker: None,
                }),
                Err(err) => send_build_err(
                    &tx_status,
                    format!("IX build error (cancel stop {}): {err}", e.symbol),
                ),
            }
        }

        // Regular limit orders grouped by (symbol, subaccount_index).
        for ((symbol, subaccount_index), cancel_ids) in by_symbol {
            for chunk in cancel_ids.chunks(MAX_CANCELS_PER_IX) {
                let Some(addrs) = ctx.market_addrs_for_symbol(&symbol) else {
                    send_build_err(
                        &tx_status,
                        format!("Build error (cancel {symbol}): unknown market"),
                    );
                    continue;
                };
                let params = match CancelOrdersByIdParams::builder()
                    .trader(ctx.authority_v2)
                    .trader_account(ctx.trader_pda_for_subaccount(subaccount_index))
                    .perp_asset_map(addrs.perp_asset_map)
                    .orderbook(addrs.orderbook)
                    .spline_collection(addrs.spline_collection)
                    .global_trader_index(addrs.global_trader_index)
                    .active_trader_buffer(addrs.active_trader_buffer)
                    .order_ids(chunk.to_vec())
                    .build()
                {
                    Ok(p) => p,
                    Err(e) => {
                        send_build_err(&tx_status, format!("Build error (cancel {symbol}): {e}"));
                        continue;
                    }
                };
                match create_cancel_orders_by_id_ix(params) {
                    Ok(ix) => items.push(BatchItem {
                        ix: ix.into(),
                        label: format!("{} {symbol}×{}", s.tx_cancel_label, chunk.len()),
                        trade_marker: None,
                    }),
                    Err(e) => {
                        send_build_err(&tx_status, format!("IX build error (cancel {symbol}): {e}"))
                    }
                }
            }
        }

        if items.is_empty() {
            send_build_err(&tx_status, s.tx_cancel_aborted.to_string());
            return;
        }

        let last_sig = execute_batched_txs(
            &ctx,
            &keypair,
            items,
            BatchLabels {
                broadcasting: s.tx_broadcasting_cancel_batch,
                failed_prepare: s.tx_failed_prepare_cancel_batch,
                confirming: s.tx_confirming_cancel_batch,
                batch_noun: s.tx_cancel_batch,
                log_noun: "cancel",
            },
            &tx_status,
        )
        .await;

        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} ({summary})", s.tx_cancel_complete),
            detail: last_sig,
        });
    });
}
