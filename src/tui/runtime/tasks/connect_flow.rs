//! Initial wallet connection flow task spawner.

use super::super::super::i18n::strings;
use super::*;

/// Referral code activated for brand-new Phoenix accounts on first wallet
/// connect. Cinder is partially funded through Phoenix's referral program;
/// new users receive a 10% fee discount per Phoenix's program terms (see
/// README → "Referral Funding"). Set `CINDER_SKIP_REFERRAL=1` to skip the
/// auto-registration and register manually at app.phoenix.trade instead.
const REFERRAL_CODE: &str = "COSMIC";
const SKIP_REFERRAL_ENV: &str = "CINDER_SKIP_REFERRAL";

fn skip_referral_requested() -> bool {
    std::env::var(SKIP_REFERRAL_ENV)
        .map(|v| {
            let v = v.trim();
            !v.is_empty() && !v.eq_ignore_ascii_case("0") && !v.eq_ignore_ascii_case("false")
        })
        .unwrap_or(false)
}

/// On wallet connect: check whether the authority already has a Phoenix
/// account; if not, activate the `COSMIC` referral via the invite API so
/// subsequent trading calls succeed (unless the user has set
/// `CINDER_SKIP_REFERRAL`, in which case the activation is skipped and the
/// user is told to self-register). Then kick off the initial balance/position
/// fetch.
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
                if skip_referral_requested() {
                    // Open the custom-referral modal so the user can type any
                    // invite code they were given (or leave it blank to skip
                    // and self-register at app.phoenix.trade). Keeping the
                    // toast text alongside the prompt gives them context for
                    // why the modal appeared.
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: strings().tx_referral_skipped.to_string(),
                        detail: String::new(),
                    });
                    let _ = tx_status.send(TxStatusMsg::PromptReferralCode);
                } else {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: strings().tx_registering_referral.to_string(),
                        detail: String::new(),
                    });
                    match http
                        .invite()
                        .activate_referral(&authority_v2, REFERRAL_CODE)
                        .await
                    {
                        Ok(_) => {
                            let _ = tx_status.send(TxStatusMsg::SetStatus {
                                title: strings().tx_registered_referral.to_string(),
                                detail: String::new(),
                            });
                        }
                        Err(e) => {
                            warn!(error = %e, "activate_referral failed");
                            let _ = tx_status.send(TxStatusMsg::SetStatus {
                                title: strings().tx_registration_failed.to_string(),
                                detail: format!("{}", e),
                            });
                        }
                    }
                }
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
