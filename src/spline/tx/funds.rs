//! USDC ↔ Phoenix token deposit and withdraw using `PhoenixTxBuilder` fund
//! builders (`build_deposit_funds` / `build_withdraw_funds`).

use std::sync::Arc;

use solana_keypair::Keypair;

use super::super::format::truncate_balance;
use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::compute_budget::build_compute_budget_ixs_raw;
use super::confirmation::{compile_and_sign, subscribe_send_confirm, ConfirmError};
use super::context::TxContext;
use super::error::{log_tx_error, parse_phoenix_tx_error};

/// Dispatches a multi-instruction deposit or withdraw transaction towards the
/// user's margin vault.
pub fn submit_funds_transfer(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    amount: f64,
    is_deposit: bool,
    tx_status: tokio::sync::mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        use phoenix_rise::PhoenixTxBuilder;

        let s = strings();
        let amount = truncate_balance(amount);
        let flow = if is_deposit {
            s.tx_flow_deposit
        } else {
            s.tx_flow_withdraw
        };
        let fund_scope = format!("{:.2} USDC {}", amount, flow);

        let builder = PhoenixTxBuilder::new(&ctx.metadata);

        let mut ixs = if is_deposit {
            match builder.build_deposit_funds(ctx.authority_v2, ctx.trader_pda_v2, amount) {
                Ok(instructions) => instructions,
                Err(e) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_failed_build_deposit, fund_scope),
                        detail: format!("{}", e),
                    });
                    return;
                }
            }
        } else {
            match builder.build_withdraw_funds(ctx.authority_v2, ctx.trader_pda_v2, amount) {
                Ok(instructions) => instructions,
                Err(e) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_failed_build_withdrawal, fund_scope),
                        detail: format!("{}", e),
                    });
                    return;
                }
            }
        };

        let mut includes_register = false;
        if is_deposit
            && !ctx
                .trader_registered
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            match ctx.rpc_client.get_account(&ctx.trader_pda_v2).await {
                Ok(acc) if !acc.data.is_empty() => {
                    ctx.trader_registered
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                _ => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} ({})…", s.tx_registering_trader, fund_scope),
                        detail: String::new(),
                    });
                    match builder.build_register_trader(ctx.authority_v2, 0, 0) {
                        Ok(mut reg_ixs) => {
                            reg_ixs.extend(ixs);
                            ixs = reg_ixs;
                            includes_register = true;
                        }
                        Err(e) => {
                            let _ = tx_status.send(TxStatusMsg::SetStatus {
                                title: format!("{} — {}", s.tx_failed_build_reg, fund_scope),
                                detail: format!("{}", e),
                            });
                            return;
                        }
                    }
                }
            }
        }

        let mapped_ixs = ixs;

        let base_cu: u32 = if includes_register { 300_000 } else { 100_000 };
        let mut final_ixs = mapped_ixs;
        final_ixs.extend(build_compute_budget_ixs_raw(base_cu));
        let mapped_ixs = final_ixs;

        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} {}…", s.tx_broadcasting, fund_scope),
            detail: String::new(),
        });

        let (tx, sig) = match compile_and_sign(&ctx, &keypair, &mapped_ixs).await {
            Ok(pair) => pair,
            Err(e) => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_prepare, fund_scope),
                    detail: e,
                });
                return;
            }
        };
        let sig_str = sig.to_string();
        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} — {}…", s.tx_awaiting_confirm, fund_scope),
            detail: sig_str.clone(),
        });

        match subscribe_send_confirm(&ctx, &tx, &sig).await {
            Ok(()) => {
                ctx.trader_registered
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                let confirmed_prefix = if is_deposit {
                    s.tx_deposit_confirmed
                } else {
                    s.tx_withdrawal_confirmed
                };
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} {:.2} {}", confirmed_prefix, amount, s.tx_usdc_confirmed),
                    detail: sig_str,
                });
            }
            Err(ConfirmError::Rejected(e)) => {
                log_tx_error(
                    None,
                    &format!("funds transfer rejected — {}", fund_scope),
                    &e,
                );
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_tx_rejected, fund_scope),
                    detail: parse_phoenix_tx_error(&e),
                });
            }
            Err(ConfirmError::NotConfirmed(e)) => {
                log_tx_error(
                    Some(&sig_str),
                    &format!("funds transfer not confirmed — {}", fund_scope),
                    &e,
                );
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {} ({})", s.tx_transfer_not_confirmed, fund_scope, e),
                    detail: sig_str,
                });
            }
        }
    });
}
