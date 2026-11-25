//! USDC ↔ Phoenix token deposit and withdraw using `phoenix_rise::core::PhoenixTxBuilder`.

use std::sync::Arc;

use phoenix_rise::core::PhoenixTxBuilder;
use solana_keypair::Keypair;
use tokio::sync::mpsc;

use super::super::format::truncate_balance;
use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::confirmation::{CuBudget, TxReport, execute_tx};
use super::context::TxContext;

/// Dispatches a deposit or withdraw transaction towards the user's margin vault.
pub fn submit_funds_transfer(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    amount: f64,
    is_deposit: bool,
    tx_status: mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        let s = strings();
        let amount = truncate_balance(amount);
        let flow = if is_deposit {
            s.tx_flow_deposit
        } else {
            s.tx_flow_withdraw
        };
        let fund_scope = format!("{amount:.2} USDC {flow}");

        let builder = PhoenixTxBuilder::new(&ctx.metadata);
        let build_res = if is_deposit {
            builder.build_deposit_funds(ctx.authority_v2, ctx.trader_pda_v2, amount)
        } else {
            builder.build_withdraw_funds(ctx.authority_v2, ctx.trader_pda_v2, amount)
        };
        let ixs = match build_res {
            Ok(instructions) => instructions,
            Err(e) => {
                let prefix = if is_deposit {
                    s.tx_failed_build_deposit
                } else {
                    s.tx_failed_build_withdrawal
                };
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{prefix} — {fund_scope}"),
                    detail: format!("{e}"),
                });
                return;
            }
        };

        let confirmed_prefix = if is_deposit {
            s.tx_deposit_confirmed
        } else {
            s.tx_withdrawal_confirmed
        };
        let report = TxReport {
            broadcasting_title: format!("{} {fund_scope}…", s.tx_broadcasting),
            prepare_failed_title: format!("{} — {fund_scope}", s.tx_failed_prepare),
            confirming_title: format!("{} — {fund_scope}…", s.tx_awaiting_confirm),
            confirmed_title: format!("{confirmed_prefix} {amount:.2} {}", s.tx_usdc_confirmed),
            rejected_title: format!("{} — {fund_scope}", s.tx_tx_rejected),
            exec_failed_title: format!("{} — {fund_scope}", s.tx_transfer_failed),
            not_confirmed_prefix: format!("{} — {fund_scope}", s.tx_transfer_not_confirmed),
            log_context: format!("funds transfer — {fund_scope}"),
            trade_markers: Vec::new(),
            silent_status: false,
            record_ledger_on_silent: false,
            outcome_tx: None,
        };

        execute_tx(
            &ctx,
            &keypair,
            ixs,
            CuBudget::Raw(100_000),
            &tx_status,
            report,
        )
        .await;
    });
}
