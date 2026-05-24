//! Market order submission — builds, signs, and dispatches a Phoenix market
//! order on a background tokio task.

use std::sync::Arc;

use solana_keypair::Keypair;
use tokio::sync::oneshot;

use super::super::i18n::strings;
use super::super::state::{SliceOutcome, TxStatusMsg};
use super::super::trading::TradingSide;
use super::compute_budget::build_compute_budget_ixs;
use super::confirmation::{compile_and_sign, subscribe_send_confirm, ConfirmError};
use super::context::TxContext;
use super::error::{
    format_not_confirmed_error, log_tx_error, not_confirmed_is_onchain_execution_failure,
    parse_phoenix_tx_error,
};
use super::flight::wrap_order_ixs;
use super::isolated_margin::estimate_collateral_transfer;

/// Asynchronously constructs, signs, and dispatches a market order payload
/// onto the network.
///
/// If `outcome_tx` is supplied, the spawned task signals slice completion
/// (success / failure / unconfirmed) before it returns. The TWAP scheduler
/// uses this to avoid advancing `slices_submitted` on a failed broadcast.
///
/// If `silent_status` is true, suppresses every `TxStatusMsg::SetStatus` and
/// `TradeMarker` write. The TWAP scheduler passes true so each slice doesn't
/// clobber the user's manual-tx status line or spam the ledger modal with
/// per-slice signatures.
///
/// Returns the spawned task's `JoinHandle` so the caller can abort the
/// broadcast if the user stops/restarts/removes the bot mid-slice.
#[allow(clippy::too_many_arguments)]
pub fn submit_market_order(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    symbol: String,
    side: TradingSide,
    num_base_lots: u64,
    reduce_only: bool,
    // Human size for status messages (same units as the TUI order line).
    display_size: f64,
    subaccount_index: u8,
    isolated_only: bool,
    max_leverage: f64,
    reference_price_usd: f64,
    tx_status: tokio::sync::mpsc::UnboundedSender<TxStatusMsg>,
    outcome_tx: Option<oneshot::Sender<SliceOutcome>>,
    silent_status: bool,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Filter outbound `TxStatusMsg`s when running silent. TWAP slices set
        // `silent_status = true` so the noisy per-slice status-line + ledger
        // updates don't clobber manual-tx state. `TradeMarker` is the chart
        // pin (a buy/sell triangle on the price chart) and MUST always go
        // through — without it, TWAP fills wouldn't show on the chart.
        // `SetStatus` is the only variant silenced.
        let send_status = |msg: TxStatusMsg| match (&msg, silent_status) {
            (TxStatusMsg::SetStatus { .. }, true) => {}
            _ => {
                let _ = tx_status.send(msg);
            }
        };
        // Helper: send an outcome and return. `outcome_tx` is `Option` so the
        // non-TWAP call sites (manual orders) can pass `None`.
        let notify_failed = |sender: Option<oneshot::Sender<SliceOutcome>>, detail: String| {
            if let Some(tx) = sender {
                let _ = tx.send(SliceOutcome::Failed(detail));
            }
        };
        let notify_confirmed = |sender: Option<oneshot::Sender<SliceOutcome>>| {
            if let Some(tx) = sender {
                let _ = tx.send(SliceOutcome::Confirmed);
            }
        };
        let notify_unconfirmed = |sender: Option<oneshot::Sender<SliceOutcome>>, detail: String| {
            if let Some(tx) = sender {
                let _ = tx.send(SliceOutcome::Unknown(detail));
            }
        };
        // Take ownership of the sender so we can move it into the helpers
        // exactly once at the path that finally resolves.
        let mut outcome_tx = outcome_tx;
        use phoenix_rise::ix::{create_place_market_order_ix, MarketOrderParams, OrderFlags, Side};
        use phoenix_rise::PhoenixTxBuilder;

        let s = strings();
        let side_lbl = match side {
            TradingSide::Long => s.long_label,
            TradingSide::Short => s.short_label,
        };
        let order_summary = format!("{} {} {}", side_lbl, display_size, symbol);
        let order_summary = if reduce_only {
            format!("{} {}", order_summary, s.tx_reduce_only)
        } else {
            order_summary
        };

        let phx_side = match side {
            TradingSide::Long => Side::Bid,
            TradingSide::Short => Side::Ask,
        };
        let isolated_only = isolated_only || ctx.market_isolated_only(&symbol);
        let max_leverage = ctx
            .max_leverage_for_symbol(&symbol)
            .filter(|lev| lev.is_finite() && *lev > 0.0)
            .unwrap_or(max_leverage);

        if isolated_only && reduce_only && subaccount_index == 0 {
            let detail = "isolated reduce-only orders require an isolated subaccount".to_string();
            send_status(TxStatusMsg::SetStatus {
                title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                detail: detail.clone(),
            });
            notify_failed(outcome_tx.take(), detail);
            return;
        }

        if isolated_only && !reduce_only {
            let collateral =
                match estimate_collateral_transfer(display_size, reference_price_usd, max_leverage)
                {
                    Ok(collateral) => collateral,
                    Err(e) => {
                        send_status(TxStatusMsg::SetStatus {
                            title: format!("{} — {}", s.tx_failed_build_params, order_summary),
                            detail: e.clone(),
                        });
                        notify_failed(outcome_tx.take(), e);
                        return;
                    }
                };
            // Build locally via `PhoenixTxBuilder` so we don't round-trip
            // through the Phoenix HTTP API just to estimate a liquidation
            // price we discard anyway. The API path failed with "No mid price
            // available (insufficient liquidity)" when the server-side mid
            // couldn't be computed; the on-chain builder needs none of that.
            //
            // Require the trader-state WS to have hydrated before building.
            // Without this gate, the builder runs against an empty `Trader`
            // and creates a fresh isolated subaccount on top of any existing
            // one — silently doubling collateral on the next order.
            let trader_snapshot = match ctx.snapshot_trader() {
                Some(t) => t,
                None => {
                    let detail = s.twap_waiting_trader_sync.to_string();
                    send_status(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_failed_build_params, order_summary),
                        detail: detail.clone(),
                    });
                    notify_failed(outcome_tx.take(), detail);
                    return;
                }
            };
            let builder = PhoenixTxBuilder::new(&ctx.metadata);
            let mut ixs = match builder.build_isolated_market_order(
                &trader_snapshot,
                &symbol,
                phx_side,
                num_base_lots,
                Some(collateral),
                false,
                None,
            ) {
                Ok(ixs) => ixs,
                Err(e) => {
                    let detail = parse_phoenix_tx_error(&format!("{}", e));
                    send_status(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                        detail: detail.clone(),
                    });
                    notify_failed(outcome_tx.take(), detail);
                    return;
                }
            };
            ixs = match wrap_order_ixs(ixs, ctx.authority_v2) {
                Ok(ixs) => ixs,
                Err(e) => {
                    send_status(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                        detail: e.clone(),
                    });
                    notify_failed(outcome_tx.take(), e);
                    return;
                }
            };
            let cu_positions = ixs.len().max(1) as u32;
            ixs.extend(build_compute_budget_ixs(cu_positions));

            send_status(TxStatusMsg::SetStatus {
                title: format!("{} {}…", s.tx_broadcasting, order_summary),
                detail: String::new(),
            });

            let (tx, sig) = match compile_and_sign(&ctx, &keypair, &ixs).await {
                Ok(pair) => pair,
                Err(e) => {
                    send_status(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_failed_prepare, order_summary),
                        detail: e.clone(),
                    });
                    notify_failed(outcome_tx.take(), e);
                    return;
                }
            };
            let sig_str = sig.to_string();
            send_status(TxStatusMsg::SetStatus {
                title: format!("{} — {}…", s.tx_awaiting_confirm, order_summary),
                detail: sig_str.clone(),
            });

            match subscribe_send_confirm(&ctx, &tx, &sig).await {
                Ok(()) => {
                    send_status(TxStatusMsg::TradeMarker {
                        is_buy: matches!(side, TradingSide::Long),
                    });
                    send_status(TxStatusMsg::SetStatus {
                        title: format!("{} {}", s.tx_order_confirmed, order_summary),
                        detail: sig_str,
                    });
                    notify_confirmed(outcome_tx.take());
                }
                Err(ConfirmError::Rejected(e)) => {
                    log_tx_error(
                        None,
                        &format!("isolated market order rejected — {}", order_summary),
                        &e,
                    );
                    let detail = parse_phoenix_tx_error(&e);
                    send_status(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_tx_rejected, order_summary),
                        detail: detail.clone(),
                    });
                    notify_failed(outcome_tx.take(), detail);
                }
                Err(ConfirmError::NotConfirmed(e)) => {
                    log_tx_error(
                        Some(&sig_str),
                        &format!("isolated market order not confirmed — {}", order_summary),
                        &e,
                    );
                    let onchain_fail = not_confirmed_is_onchain_execution_failure(&e);
                    if onchain_fail {
                        let detail = parse_phoenix_tx_error(&e);
                        send_status(TxStatusMsg::SetStatus {
                            title: format!("{} — {}", s.tx_order_failed, order_summary),
                            detail: detail.clone(),
                        });
                        notify_failed(outcome_tx.take(), detail);
                    } else {
                        // Tx was broadcast but we never saw confirmation.
                        // Report Unknown rather than Confirmed — repeating
                        // the slice would risk double-execution if the
                        // original eventually landed, but the user must be
                        // able to tell apart "all confirmed" from "some
                        // unconfirmed" in the bot's tallies.
                        let detail = sig_str.clone();
                        send_status(TxStatusMsg::SetStatus {
                            title: format!(
                                "{} — {} ({})",
                                s.tx_order_not_confirmed,
                                order_summary,
                                format_not_confirmed_error(&e)
                            ),
                            detail: detail.clone(),
                        });
                        notify_unconfirmed(outcome_tx.take(), detail);
                    }
                }
            }
            return;
        }

        // ── Non-isolated branch ────────────────────────────────────────────
        // The non-isolated builder consumes `ctx.market_addrs.*`, which are
        // baked in for `ctx.active_symbol` only. Refuse to dispatch when the
        // caller-supplied symbol disagrees — otherwise the on-chain ix would
        // target the active market's orderbook with metadata for the wrong
        // symbol, which the runtime would reject (or silently misroute).
        if symbol != ctx.active_symbol {
            let detail = format!(
                "{} ({} != active {})",
                s.twap_waiting_active_market, symbol, ctx.active_symbol
            );
            send_status(TxStatusMsg::SetStatus {
                title: format!("{} — {}", s.tx_failed_build_params, order_summary),
                detail: detail.clone(),
            });
            notify_failed(outcome_tx.take(), detail);
            return;
        }

        let trader_account = ctx.trader_pda_for_subaccount(subaccount_index);

        let order_flags = if reduce_only {
            OrderFlags::ReduceOnly
        } else {
            OrderFlags::None
        };

        let params = match MarketOrderParams::builder()
            .trader(ctx.authority_v2)
            .trader_account(trader_account)
            .perp_asset_map(ctx.market_addrs.perp_asset_map)
            .orderbook(ctx.market_addrs.orderbook)
            .spline_collection(ctx.market_addrs.spline_collection)
            .global_trader_index(ctx.market_addrs.global_trader_index.clone())
            .active_trader_buffer(ctx.market_addrs.active_trader_buffer.clone())
            .symbol(&symbol)
            .side(phx_side)
            .num_base_lots(num_base_lots)
            .order_flags(order_flags)
            .subaccount_index(subaccount_index)
            .build()
        {
            Ok(p) => p,
            Err(e) => {
                let detail = format!("{}", e);
                send_status(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_build_params, order_summary),
                    detail: detail.clone(),
                });
                notify_failed(outcome_tx.take(), detail);
                return;
            }
        };

        let ixs = match create_place_market_order_ix(params) {
            Ok(ix) => vec![ix.into()],
            Err(e) => {
                let detail = format!("{}", e);
                send_status(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                    detail: detail.clone(),
                });
                notify_failed(outcome_tx.take(), detail);
                return;
            }
        };

        let ixs = match wrap_order_ixs(ixs, ctx.authority_v2) {
            Ok(ixs) => ixs,
            Err(e) => {
                send_status(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                    detail: e.clone(),
                });
                notify_failed(outcome_tx.take(), e);
                return;
            }
        };

        let mut mapped_ixs = ixs;
        mapped_ixs.extend(build_compute_budget_ixs(1));

        send_status(TxStatusMsg::SetStatus {
            title: format!("{} {}…", s.tx_broadcasting, order_summary),
            detail: String::new(),
        });

        let (tx, sig) = match compile_and_sign(&ctx, &keypair, &mapped_ixs).await {
            Ok(pair) => pair,
            Err(e) => {
                send_status(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_prepare, order_summary),
                    detail: e.clone(),
                });
                notify_failed(outcome_tx.take(), e);
                return;
            }
        };
        let sig_str = sig.to_string();
        send_status(TxStatusMsg::SetStatus {
            title: format!("{} — {}…", s.tx_awaiting_confirm, order_summary),
            detail: sig_str.clone(),
        });

        match subscribe_send_confirm(&ctx, &tx, &sig).await {
            Ok(()) => {
                send_status(TxStatusMsg::TradeMarker {
                    is_buy: matches!(side, TradingSide::Long),
                });
                send_status(TxStatusMsg::SetStatus {
                    title: format!("{} {}", s.tx_order_confirmed, order_summary),
                    detail: sig_str,
                });
                notify_confirmed(outcome_tx.take());
            }
            Err(ConfirmError::Rejected(e)) => {
                log_tx_error(
                    None,
                    &format!("market order rejected — {}", order_summary),
                    &e,
                );
                let detail = parse_phoenix_tx_error(&e);
                send_status(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_tx_rejected, order_summary),
                    detail: detail.clone(),
                });
                notify_failed(outcome_tx.take(), detail);
            }
            Err(ConfirmError::NotConfirmed(e)) => {
                log_tx_error(
                    Some(&sig_str),
                    &format!("market order not confirmed — {}", order_summary),
                    &e,
                );
                let onchain_fail = not_confirmed_is_onchain_execution_failure(&e);
                if onchain_fail {
                    let detail = parse_phoenix_tx_error(&e);
                    send_status(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_order_failed, order_summary),
                        detail: detail.clone(),
                    });
                    notify_failed(outcome_tx.take(), detail);
                } else {
                    let detail = sig_str.clone();
                    send_status(TxStatusMsg::SetStatus {
                        title: format!(
                            "{} — {} ({})",
                            s.tx_order_not_confirmed,
                            order_summary,
                            format_not_confirmed_error(&e)
                        ),
                        detail: detail.clone(),
                    });
                    notify_unconfirmed(outcome_tx.take(), detail);
                }
            }
        }
    })
}
