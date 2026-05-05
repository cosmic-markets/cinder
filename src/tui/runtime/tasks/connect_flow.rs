//! Initial wallet connection flow task spawner.

use super::*;

/// Number of `get_trader` attempts before we give up. A transient HTTP
/// failure on the first call would otherwise let the user trade through
/// without ever seeing the referral choice (and Cinder's funding
/// disclosure). Retries with a short backoff close that gap.
const GET_TRADER_RETRIES: usize = 4;
const GET_TRADER_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(750);

/// On wallet connect: check whether the authority already has a Phoenix
/// account; if not, prompt the first-run referral choice modal so the user
/// can decide between COSMIC, a custom code, or continuing without one. The
/// actual activation runs in a separate task spawned from the modal
/// handler. Then kick off the initial balance/position fetch in parallel —
/// the user can see balances even before they've finished the referral
/// choice.
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

        let mut final_err: Option<String> = None;
        for attempt in 0..GET_TRADER_RETRIES {
            match http.traders().get_trader(&authority_v2).await {
                Ok(traders) if traders.is_empty() => {
                    let _ = tx_status.send(TxStatusMsg::PromptReferralChoice);
                    final_err = None;
                    break;
                }
                Ok(_) => {
                    // Existing Phoenix account — no referral choice
                    // possible (attribution is permanent on Phoenix's
                    // side). Nothing to prompt.
                    final_err = None;
                    break;
                }
                Err(e) => {
                    final_err = Some(format!("{}", e));
                    if attempt + 1 < GET_TRADER_RETRIES {
                        tokio::time::sleep(GET_TRADER_RETRY_DELAY).await;
                    }
                }
            }
        }
        if let Some(err) = final_err {
            warn!(error = %err, "get_trader failed after retries; skipping referral check");
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
