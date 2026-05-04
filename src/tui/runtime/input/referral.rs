//! "Custom referral code" modal handler. Opens automatically when a wallet
//! with no Phoenix account connects while `CINDER_SKIP_REFERRAL` is set.
//!
//! Empty input + Enter (or Esc at any time) skips activation; trading will
//! fail until the user self-registers at app.phoenix.trade. A non-empty code
//! spawns a background task that calls Phoenix's `activate-with-referral`
//! endpoint with the typed code.

use std::str::FromStr;
use std::sync::Arc;

use phoenix_rise::PhoenixHttpClient;
use solana_keypair::Keypair;
use solana_signer::Signer;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

use super::super::super::state::TxStatusMsg;
use super::*;

pub(in crate::tui::runtime) fn handle_editing_referral_code(
    code: KeyCode,
    state: &mut TuiState,
    channels: &Channels,
    http: Arc<PhoenixHttpClient>,
) -> KeyAction {
    let s = strings();
    match code {
        KeyCode::Enter => {
            let trimmed = state.trading.referral_code_buffer.trim().to_string();
            if trimmed.is_empty() {
                state.trading.input_mode = InputMode::Normal;
                state.trading.referral_code_buffer.clear();
                state.trading.referral_code_error = None;
                state
                    .trading
                    .set_status_title(s.tx_referral_skipped);
                return KeyAction::Redraw;
            }
            // Spawn the activation task; the modal closes immediately so the
            // user isn't stuck staring at it while the HTTP call is in flight.
            // Status toasts (registering / registered / failed) flow back via
            // the same tx_status channel the connect flow uses.
            let Some(kp) = state.trading.keypair.as_ref().cloned() else {
                state.trading.referral_code_error =
                    Some("wallet not loaded".to_string());
                return KeyAction::Redraw;
            };
            spawn_referral_activation(http, kp, trimmed.clone(), channels.tx_status.clone());
            state
                .trading
                .set_status_title(format!("{} {}…", s.tx_registering_custom_prefix, trimmed));
            state.trading.input_mode = InputMode::Normal;
            state.trading.referral_code_buffer.clear();
            state.trading.referral_code_error = None;
            KeyAction::Redraw
        }
        KeyCode::Esc => {
            state.trading.input_mode = InputMode::Normal;
            state.trading.referral_code_buffer.clear();
            state.trading.referral_code_error = None;
            state
                .trading
                .set_status_title(s.tx_referral_skipped);
            KeyAction::Redraw
        }
        KeyCode::Backspace => {
            state.trading.referral_code_buffer.pop();
            state.trading.referral_code_error = None;
            KeyAction::Redraw
        }
        KeyCode::Char(c) if !c.is_control() && !c.is_whitespace() => {
            state.trading.referral_code_buffer.push(c);
            state.trading.referral_code_error = None;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

fn spawn_referral_activation(
    http: Arc<PhoenixHttpClient>,
    kp: Arc<Keypair>,
    referral_code: String,
    tx_status: UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        let s = strings();
        let authority = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(error = %e, "failed to convert wallet pubkey for custom referral activation");
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: s.tx_registration_failed.to_string(),
                    detail: format!("{}", e),
                });
                return;
            }
        };
        match http
            .invite()
            .activate_referral(&authority, &referral_code)
            .await
        {
            Ok(_) => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} {}", s.tx_registered_custom_prefix, referral_code),
                    detail: String::new(),
                });
            }
            Err(e) => {
                warn!(error = %e, code = %referral_code, "custom activate_referral failed");
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: s.tx_registration_failed.to_string(),
                    detail: format!("{}", e),
                });
            }
        }
    });
}
