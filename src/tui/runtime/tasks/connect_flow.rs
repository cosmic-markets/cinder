//! Initial wallet connection flow task spawner.

use super::*;

/// On wallet connect: check whether the authority already has a Phoenix
/// account; if not, prompt the first-run referral choice modal so the user
/// can decide between COSMIC, a custom code, or skipping. The actual
/// activation runs in a separate task spawned from the modal handler. Then
/// kick off the initial balance/position fetch in parallel — the user can
/// see balances even before they've finished the referral choice.
pub(in crate::tui::runtime) fn spawn_initial_connect_flow(
    http: Arc<PhoenixHttpClient>,
    kp: Arc<Keypair>,
    symbol: String,
    balance_tx: UnboundedSender<BalanceUpdate>,
    tx_status: UnboundedSender<TxStatusMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let authority_v2 = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(error = %e, "failed to convert wallet pubkey for initial connect flow");
                return;
            }
        };

        match http.traders().get_trader(&authority_v2).await {
            Ok(traders) if traders.is_empty() => {
                let _ = tx_status.send(TxStatusMsg::PromptReferralChoice);
            }
            Ok(_) => { /* account already present — nothing to do */ }
            Err(e) => {
                // Don't block the initial balance fetch on a transient get_trader error;
                // the 1.1s poll will retry anyway.
                warn!(error = %e, "initial get_trader failed; skipping referral check");
            }
        }

        let Ok((phoenix_bal, position, all_positions)) = tokio::time::timeout(
            BALANCE_FETCH_TIMEOUT,
            fetch_phoenix_balance_and_position(&http, &authority_v2, &symbol),
        )
        .await
        else {
            warn!(symbol = %symbol, "initial phoenix balance fetch timed out");
            return;
        };
        let _ = balance_tx.send(BalanceUpdate {
            phoenix_collateral: phoenix_bal,
            position,
            all_positions,
        });
    })
}
