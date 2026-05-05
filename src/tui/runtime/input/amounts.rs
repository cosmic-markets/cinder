//! Wallet and transfer form handlers.

use tracing::warn;

use super::super::super::config::current_user_config;
use super::*;

fn parse_checked_usdc_amount(input: &str) -> Result<f64, &'static str> {
    let amount = input
        .trim()
        .parse::<f64>()
        .map_err(|_| "enter a number like 100.00")?;
    if !amount.is_finite() {
        return Err("amount must be a finite number");
    }
    if amount <= 0.0 {
        return Err("amount must be greater than zero");
    }
    if amount > MAX_USDC_TRANSFER_AMOUNT {
        return Err("amount is above the release safety limit");
    }

    let amount = truncate_balance(amount);
    if amount <= 0.0 {
        return Err("amount is below the minimum USDC precision");
    }

    Ok(amount)
}

/// Text editor for the RPC URL. Enter saves + triggers a live reconnect;
/// Esc discards.
pub(in crate::tui::runtime) fn handle_editing_rpc_url(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    match code {
        KeyCode::Enter => {
            let new_url = trading.input_buffer.trim().to_string();
            if new_url == trading.config.rpc_url {
                trading.input_mode = InputMode::ViewingConfig;
                trading.input_buffer.clear();
                trading.set_status_title(strings().st_rpc_unchanged);
                return KeyAction::Redraw;
            }
            let prev = trading.config.rpc_url.clone();
            trading.config.rpc_url = new_url.clone();
            match save_user_config(&trading.config) {
                Ok(()) => {
                    trading.input_mode = InputMode::Normal;
                    trading.input_buffer.clear();
                    if new_url.is_empty() {
                        trading.set_status_title(strings().st_rpc_cleared);
                    } else {
                        trading.set_status_title(format!(
                            "{} {}\u{2026}",
                            strings().st_reconnecting,
                            new_url
                        ));
                    }
                    KeyAction::ReconnectRpc
                }
                Err(e) => {
                    trading.config.rpc_url = prev;
                    trading.set_status_title(format!("{} {}", strings().st_failed_save, e));
                    KeyAction::Redraw
                }
            }
        }
        KeyCode::Esc => {
            trading.input_mode = InputMode::ViewingConfig;
            trading.input_buffer.clear();
            KeyAction::Redraw
        }
        KeyCode::Backspace => {
            trading.input_buffer.pop();
            KeyAction::Redraw
        }
        KeyCode::Char(c) if !c.is_control() => {
            trading.input_buffer.push(c);
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

/// "Load Wallet" modal opened by [w]. Edits a path string; Enter attempts
/// to load the keypair from disk. On success, runs the connect-wallet flow
/// and closes the modal. On failure, sets `wallet_path_error` and stays open
/// so the user can retry.
#[allow(clippy::too_many_arguments)]
pub(in crate::tui::runtime) fn handle_editing_wallet_path(
    code: KeyCode,
    state: &mut TuiState,
    cfg: &SplineConfig,
    configs: &std::collections::HashMap<String, SplineConfig>,
    channels: &Channels,
    ws_url: &str,
    http: Arc<PhoenixHttpClient>,
    wallet_wss_handle: &mut Option<tokio::task::JoinHandle<()>>,
    balance_fetch_handle: &mut Option<tokio::task::JoinHandle<()>>,
    trader_orders_handle: &mut Option<tokio::task::JoinHandle<()>>,
    tx_ctx_task: &mut Option<tokio::task::JoinHandle<()>>,
    awaiting_first_tx_ctx: &mut bool,
) -> KeyAction {
    match code {
        KeyCode::Enter => {
            let input = state.trading.wallet_path_buffer.trim().to_string();
            let result = resolve_wallet_modal_input(&input);
            match result {
                Ok(kp) => {
                    // Persist the path when the user loaded from a real file
                    // so the modal is pre-populated next time. Skip raw-base58
                    // and inline-JSON inputs (neither resolves to a real path).
                    if !input.starts_with('[') && std::path::Path::new(&input).is_file() {
                        let mut cfg_to_save = current_user_config();
                        if cfg_to_save.wallet_path != input {
                            cfg_to_save.wallet_path = input.clone();
                            if let Err(e) = save_user_config(&cfg_to_save) {
                                warn!(error = %e, "failed to persist wallet_path to user config");
                            }
                        }
                    }
                    let handles = connect_wallet_with_keypair(
                        state, kp, cfg, configs, channels, ws_url, http,
                    );
                    *wallet_wss_handle = Some(handles.wallet_wss);
                    *balance_fetch_handle = Some(handles.initial_balance);
                    *trader_orders_handle = Some(handles.trader_orders);
                    if let Some(h) = tx_ctx_task.replace(handles.tx_ctx) {
                        h.abort();
                    }
                    *awaiting_first_tx_ctx = true;
                    state.trading.input_mode = InputMode::Normal;
                    state.trading.wallet_path_buffer.clear();
                    state.trading.wallet_path_error = None;
                }
                Err(e) => {
                    state.trading.wallet_path_error = Some(e);
                }
            }
            KeyAction::Redraw
        }
        KeyCode::Esc => {
            state.trading.input_mode = InputMode::Normal;
            state.trading.wallet_path_buffer.clear();
            state.trading.wallet_path_error = None;
            KeyAction::Redraw
        }
        KeyCode::Backspace => {
            state.trading.wallet_path_buffer.pop();
            state.trading.wallet_path_error = None;
            KeyAction::Redraw
        }
        KeyCode::Char(c) if !c.is_control() => {
            state.trading.wallet_path_buffer.push(c);
            state.trading.wallet_path_error = None;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

pub(in crate::tui::runtime) fn handle_editing_size(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    match code {
        KeyCode::Enter => {
            match trading.input_buffer.parse::<f64>() {
                Ok(val) if val.is_finite() && val > 0.0 && val <= MAX_UI_ORDER_SIZE_UNITS => {
                    trading.custom_size = Some(val);
                    trading.set_status_title(format!("{} {}", strings().st_size_set, val));
                }
                Ok(val) if val > MAX_UI_ORDER_SIZE_UNITS => {
                    trading.set_status_title(format!(
                        "{} ({})",
                        strings().st_invalid_size,
                        LotConversionError::AboveUiLimit
                    ));
                    trading.custom_size = None;
                }
                Ok(_) => {
                    trading.set_status_title(format!(
                        "{} ({})",
                        strings().st_invalid_size,
                        LotConversionError::NonPositive
                    ));
                    trading.custom_size = None;
                }
                Err(_) => {
                    trading.set_status_title(strings().st_invalid_size);
                    trading.custom_size = None;
                }
            }
            trading.input_mode = InputMode::Normal;
            trading.input_buffer.clear();
            KeyAction::Redraw
        }
        KeyCode::Esc => {
            trading.input_mode = InputMode::Normal;
            trading.input_buffer.clear();
            KeyAction::Redraw
        }
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            trading.input_buffer.push(c);
            KeyAction::Redraw
        }
        KeyCode::Backspace => {
            trading.input_buffer.pop();
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

pub(in crate::tui::runtime) fn handle_editing_deposit(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    match code {
        KeyCode::Enter => {
            if let Ok(val) = parse_checked_usdc_amount(&trading.deposit_buffer) {
                trading.input_mode =
                    InputMode::Confirming(PendingAction::DepositFunds { amount: val });
                {
                    let s = strings();
                    trading.set_status_title(format!(
                        "{} {} USDC? {}",
                        s.st_confirm_deposit_st, val, s.st_yn
                    ));
                }
            } else {
                let detail = parse_checked_usdc_amount(&trading.deposit_buffer)
                    .err()
                    .unwrap_or("enter a number like 100.00");
                trading.set_status_title(format!("{} ({})", strings().st_invalid_amount, detail));
                trading.input_mode = InputMode::Normal;
            }
            trading.deposit_buffer.clear();
            KeyAction::Redraw
        }
        KeyCode::Esc => {
            trading.input_mode = InputMode::Normal;
            trading.deposit_buffer.clear();
            KeyAction::Redraw
        }
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            trading.deposit_buffer.push(c);
            KeyAction::Redraw
        }
        KeyCode::Backspace => {
            trading.deposit_buffer.pop();
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

pub(in crate::tui::runtime) fn handle_editing_withdraw(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    match code {
        KeyCode::Enter => {
            if let Ok(val) = parse_checked_usdc_amount(&trading.withdraw_buffer) {
                trading.input_mode =
                    InputMode::Confirming(PendingAction::WithdrawFunds { amount: val });
                {
                    let s = strings();
                    trading.set_status_title(format!(
                        "{} {} USDC? {}",
                        s.st_confirm_withdraw_st, val, s.st_yn
                    ));
                }
            } else {
                let detail = parse_checked_usdc_amount(&trading.withdraw_buffer)
                    .err()
                    .unwrap_or("enter a number like 100.00");
                trading.set_status_title(format!("{} ({})", strings().st_invalid_amount, detail));
                trading.input_mode = InputMode::Normal;
            }
            trading.withdraw_buffer.clear();
            KeyAction::Redraw
        }
        KeyCode::Esc => {
            trading.input_mode = InputMode::Normal;
            trading.withdraw_buffer.clear();
            KeyAction::Redraw
        }
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            trading.withdraw_buffer.push(c);
            KeyAction::Redraw
        }
        KeyCode::Backspace => {
            trading.withdraw_buffer.pop();
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}
