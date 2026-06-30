//! Stop-market order submission — builds, signs, and dispatches a Phoenix
//! **legacy stop-loss** (`PlaceStopLoss`) order on a background tokio task.
//!
//! Uses `phoenix_rise::core::PhoenixTxBuilder::build_stop_loss_orders`, the dedicated
//! stop-loss path that the Rise SDK exposes via `/v1/ix/place-stop-loss-order`:
//! for a long position the stop triggers on `LessThan`; for a short, on
//! `GreaterThan`. The trigger is reduce-only and closes the **full** position
//! at fire-time (the on-chain `PlaceStopLoss` instruction ignores trade size),
//! so no per-order size is encoded. The instruction creates its own stop-loss
//! PDA on demand, so no conditional-orders account needs to be created first.

use std::sync::Arc;

use solana_keypair::Keypair;

use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::super::trading::TradingSide;
use super::compute_budget::build_compute_budget_ixs;
use super::confirmation::{ConfirmError, compile_and_sign, subscribe_send_confirm};
use super::context::TxContext;
use super::error::{
    format_not_confirmed_error, log_tx_error, not_confirmed_is_onchain_execution_failure,
    parse_phoenix_tx_error,
};
use super::flight::wrap_order_ixs;

/// Asynchronously constructs, signs, and dispatches a stop-market order onto
/// the network.
pub fn submit_stop_market_order(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    symbol: String,
    side: TradingSide,
    // Retained for call-site compatibility. The legacy `PlaceStopLoss`
    // instruction always closes the full position at trigger, so a per-order
    // base-lot size is not encoded.
    _num_base_lots: u64,
    trigger_price_usd: f64,
    display_size: f64,
    subaccount_index: u8,
    isolated_only: bool,
    tx_status: tokio::sync::mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        use phoenix_rise::core::{BracketLeg, BracketLegOrders, PhoenixTxBuilder};
        use phoenix_rise::ix::types::Side;

        let s = strings();
        let side_lbl = match side {
            TradingSide::Long => s.long_label,
            TradingSide::Short => s.short_label,
        };
        let order_summary = format!(
            "{} {} {} {} @ ${:.2}",
            s.stp, side_lbl, display_size, symbol, trigger_price_usd
        );

        let position_side = match side {
            TradingSide::Long => Side::Bid,
            TradingSide::Short => Side::Ask,
        };

        let bracket = BracketLegOrders {
            stop_loss: Some(BracketLeg::new(trigger_price_usd)),
            take_profit: None,
        };

        let builder = PhoenixTxBuilder::new(&ctx.metadata);
        let isolated_only = isolated_only || ctx.market_isolated_only(&symbol);
        if isolated_only && subaccount_index == 0 {
            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                detail: "isolated stop orders require an existing isolated position".to_string(),
            });
            return;
        }
        let trader_account = ctx.trader_pda_for_subaccount(subaccount_index);

        // The legacy `PlaceStopLoss` instruction creates its own stop-loss PDA
        // on demand (the system program is wired into the ix), so there is no
        // conditional-orders account to provision beforehand.
        let ixs = match builder.build_stop_loss_orders(
            ctx.authority_v2,
            trader_account,
            &symbol,
            position_side,
            &bracket,
        ) {
            Ok(v) => v,
            Err(e) => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                    detail: format!("{}", e),
                });
                return;
            }
        };

        let ixs = match wrap_order_ixs(ixs, ctx.authority_v2) {
            Ok(ixs) => ixs,
            Err(e) => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                    detail: e,
                });
                return;
            }
        };

        let cu_mul = 1u32;
        let mut mapped_ixs = ixs;
        mapped_ixs.extend(build_compute_budget_ixs(cu_mul));

        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} {}…", s.tx_broadcasting, order_summary),
            detail: String::new(),
        });

        let (tx, sig) = match compile_and_sign(&ctx, &keypair, &mapped_ixs).await {
            Ok(pair) => pair,
            Err(e) => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_prepare, order_summary),
                    detail: e,
                });
                return;
            }
        };
        let sig_str = sig.to_string();
        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} — {}…", s.tx_awaiting_confirm, order_summary),
            detail: sig_str.clone(),
        });

        match subscribe_send_confirm(&ctx, &tx, &sig).await {
            Ok(()) => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} {}", s.tx_order_confirmed, order_summary),
                    detail: sig_str,
                });
            }
            Err(ConfirmError::Rejected(e)) => {
                log_tx_error(
                    None,
                    &format!("stop-market order rejected — {}", order_summary),
                    &e,
                );
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_tx_rejected, order_summary),
                    detail: parse_phoenix_tx_error(&e),
                });
            }
            Err(ConfirmError::NotConfirmed(e)) => {
                log_tx_error(
                    Some(&sig_str),
                    &format!("stop-market order not confirmed — {}", order_summary),
                    &e,
                );
                let onchain_fail = not_confirmed_is_onchain_execution_failure(&e);
                let (title, detail) = if onchain_fail {
                    (
                        format!("{} — {}", s.tx_order_failed, order_summary),
                        parse_phoenix_tx_error(&e),
                    )
                } else {
                    (
                        format!(
                            "{} — {} ({})",
                            s.tx_order_not_confirmed,
                            order_summary,
                            format_not_confirmed_error(&e)
                        ),
                        sig_str.clone(),
                    )
                };
                let _ = tx_status.send(TxStatusMsg::SetStatus { title, detail });
            }
        }
    });
}
