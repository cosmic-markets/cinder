//! Referral modal handlers — both the first-run choice modal
//! (`ChoosingReferral`) and the custom-code text input (`EditingReferralCode`)
//! it can lead into.
//!
//! Choice modal opens automatically when a wallet with no Phoenix account
//! connects. The user picks between the COSMIC referral (Cinder's funding
//! model — see README), a custom code they were given by someone else, or
//! skipping entirely. Skip / Esc just closes the modal — Phoenix account
//! creation is handled separately on Phoenix's side.

use std::str::FromStr;
use std::sync::Arc;

use phoenix_rise::PhoenixHttpClient;
use solana_keypair::Keypair;
use solana_signer::Signer;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

use super::super::super::constants::MAX_REFERRAL_CODE_LEN;
use super::super::super::state::TxStatusMsg;
use super::*;

/// Phoenix referral code Cinder is funded through. Activation happens in
/// `spawn_referral_activation` when the user picks "Use COSMIC" in the
/// choice modal.
const COSMIC_REFERRAL_CODE: &str = "COSMIC";

/// Number of options in the first-run referral choice modal.
const CHOICE_COUNT: usize = 3;
const CHOICE_COSMIC: usize = 0;
const CHOICE_CUSTOM: usize = 1;
/// Index 2 is the skip option — handled by the wildcard arm in the choice
/// match, since any out-of-range index also collapses to "skip" as the safe
/// default.
const _CHOICE_SKIP: usize = 2;

pub(in crate::tui::runtime) fn handle_choosing_referral(
    code: KeyCode,
    state: &mut TuiState,
    channels: &Channels,
    http: Arc<PhoenixHttpClient>,
) -> KeyAction {
    let s = strings();
    match code {
        KeyCode::Up => {
            if state.trading.referral_choice_index > 0 {
                state.trading.referral_choice_index -= 1;
            } else {
                state.trading.referral_choice_index = CHOICE_COUNT - 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Down | KeyCode::Tab => {
            state.trading.referral_choice_index =
                (state.trading.referral_choice_index + 1) % CHOICE_COUNT;
            KeyAction::Redraw
        }
        KeyCode::Enter => match state.trading.referral_choice_index {
            CHOICE_COSMIC => {
                let Some(kp) = state.trading.keypair.as_ref().cloned() else {
                    state.trading.input_mode = InputMode::Normal;
                    return KeyAction::Redraw;
                };
                spawn_referral_activation(
                    http,
                    kp,
                    COSMIC_REFERRAL_CODE.to_string(),
                    s.tx_registered_referral.to_string(),
                    channels.tx_status.clone(),
                );
                state.trading.set_status_title(s.tx_registering_referral);
                state.trading.input_mode = InputMode::Normal;
                KeyAction::Redraw
            }
            CHOICE_CUSTOM => {
                state.trading.referral_code_buffer.clear();
                state.trading.referral_code_error = None;
                state.trading.input_mode = InputMode::EditingReferralCode;
                KeyAction::Redraw
            }
            // CHOICE_SKIP and any out-of-range index fall here.
            _ => {
                close_with_connected_status(state);
                KeyAction::Redraw
            }
        },
        KeyCode::Esc => {
            // Esc = skip. Closes the modal without registering any
            // referral; the wallet stays in whatever state Phoenix already
            // has it in.
            close_with_connected_status(state);
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

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
                state.trading.referral_code_buffer.clear();
                state.trading.referral_code_error = None;
                close_with_connected_status(state);
                return KeyAction::Redraw;
            }
            // Spawn the activation task; the modal closes immediately so the
            // user isn't stuck staring at it while the HTTP call is in flight.
            // Status toasts (registering / registered / failed) flow back via
            // the same tx_status channel the connect flow uses.
            let Some(kp) = state.trading.keypair.as_ref().cloned() else {
                state.trading.referral_code_error = Some("wallet not loaded".to_string());
                return KeyAction::Redraw;
            };
            let success_title = format!("{} {}", s.tx_registered_custom_prefix, trimmed);
            spawn_referral_activation(
                http,
                kp,
                trimmed.clone(),
                success_title,
                channels.tx_status.clone(),
            );
            state
                .trading
                .set_status_title(format!("{} {}…", s.tx_registering_custom_prefix, trimmed));
            state.trading.input_mode = InputMode::Normal;
            state.trading.referral_code_buffer.clear();
            state.trading.referral_code_error = None;
            KeyAction::Redraw
        }
        KeyCode::Esc => {
            state.trading.referral_code_buffer.clear();
            state.trading.referral_code_error = None;
            close_with_connected_status(state);
            KeyAction::Redraw
        }
        KeyCode::Backspace => {
            state.trading.referral_code_buffer.pop();
            state.trading.referral_code_error = None;
            KeyAction::Redraw
        }
        KeyCode::Char(c) if !c.is_control() && !c.is_whitespace() => {
            if state.trading.referral_code_buffer.chars().count() >= MAX_REFERRAL_CODE_LEN {
                // Silently ignore further input rather than flashing an
                // error: the underline cursor stops advancing, which is the
                // visual signal users expect from a bounded text field.
                return KeyAction::Nothing;
            }
            state.trading.referral_code_buffer.push(c);
            state.trading.referral_code_error = None;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

/// Close the referral modal without registering any referral and restore
/// the standard "wallet connected" status line. Used for Skip and Esc.
fn close_with_connected_status(state: &mut TuiState) {
    state.trading.input_mode = InputMode::Normal;
    let s = strings();
    let pk = state.trading.wallet_label.clone();
    if pk.is_empty() {
        state.trading.set_status_title(s.st_wallet_connected);
    } else {
        state
            .trading
            .set_status_title(format!("{} {}", s.st_wallet_connected_as, pk));
    }
}

/// Spawn the Phoenix `activate-with-referral` call as a background task and
/// fan results back through the shared `tx_status` channel. `success_title`
/// is the toast shown on success — callers parameterize this so the COSMIC
/// path can show the discount-mentioning message while the custom-code path
/// shows a generic prefix + the typed code.
fn spawn_referral_activation(
    http: Arc<PhoenixHttpClient>,
    kp: Arc<Keypair>,
    referral_code: String,
    success_title: String,
    tx_status: UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        let s = strings();
        let authority = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(error = %e, "failed to convert wallet pubkey for referral activation");
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
                    title: success_title,
                    detail: String::new(),
                });
            }
            Err(e) => {
                warn!(error = %e, code = %referral_code, "activate_referral failed");
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: s.tx_registration_failed.to_string(),
                    detail: format!("{}", e),
                });
            }
        }
    });
}
