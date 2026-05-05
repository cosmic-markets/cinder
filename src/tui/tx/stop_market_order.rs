//! Stop-market order submission — builds, signs, and dispatches a Phoenix
//! stop-loss as a **position conditional** order (bracket leg) on a background
//! tokio task.
//!
//! Uses `phoenix_rise::PhoenixTxBuilder::build_bracket_leg_orders`: for a long
//! position the stop triggers on `LessThan`; for a short, on `GreaterThan`.
//! If the trader has no conditional-orders account yet, a create-account
//! instruction is prepended (same flow as the Rise SDK bracket helpers).

use std::sync::Arc;

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

/// Asynchronously constructs, signs, and dispatches a stop-market order onto
/// the network.
pub fn submit_stop_market_order(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    symbol: String,
    side: TradingSide,
    num_base_lots: u64,
    trigger_price_usd: f64,
    display_size: f64,
    subaccount_index: u8,
    isolated_only: bool,
    tx_status: tokio::sync::mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        use phoenix_rise::{
            get_conditional_orders_address, BracketLeg, BracketLegOrders, BracketLegSize,
            PhoenixTxBuilder, Side,
        };

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
            stop_loss: Some(
                BracketLeg::new(trigger_price_usd)
                    .with_size(BracketLegSize::BaseLots(num_base_lots)),
            ),
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

        let mut prepended_conditional_create = false;
        let mut ixs: Vec<solana_instruction::Instruction> = Vec::new();

        let cond_pda = get_conditional_orders_address(&trader_account);
        match ctx.rpc_client.get_account(&cond_pda).await {
            Ok(acc) if !acc.data.is_empty() => {}
            _ => {
                match builder.build_create_conditional_orders_account(
                    ctx.authority_v2,
                    ctx.authority_v2,
                    trader_account,
                    8,
                ) {
                    Ok(mut create_ixs) => {
                        ixs.append(&mut create_ixs);
                        prepended_conditional_create = true;
                    }
                    Err(e) => {
                        let _ = tx_status.send(TxStatusMsg::SetStatus {
                            title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                            detail: format!("{}", e),
                        });
                        return;
                    }
                }
            }
        }

        let mut bracket_ixs = match builder.build_bracket_leg_orders(
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
        ixs.append(&mut bracket_ixs);

        let mut cu_mul = 1u32;
        if prepended_conditional_create {
            cu_mul += 1;
        }
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
