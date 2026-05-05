//! Config and ledger input handlers.

use super::*;

fn toggle_show_clob(trading: &mut TradingState) -> KeyAction {
    let prev = trading.config.show_clob;
    trading.config.show_clob = !prev;
    match save_user_config(&trading.config) {
        Ok(()) => {
            let s = strings();
            let state = if trading.config.show_clob {
                "On"
            } else {
                "Off"
            };
            trading.set_status_title(format!("{} {}", s.st_clob_set, state));
            KeyAction::ToggleClob
        }
        Err(e) => {
            trading.config.show_clob = prev;
            trading.set_status_title(format!("{} {}", strings().st_failed_save, e));
            KeyAction::Redraw
        }
    }
}

fn toggle_fanout_public_rpc(trading: &mut TradingState) -> KeyAction {
    let prev = trading.config.fanout_public_rpc;
    trading.config.fanout_public_rpc = !prev;
    match save_user_config(&trading.config) {
        Ok(()) => {
            let s = strings();
            let state = if trading.config.fanout_public_rpc {
                s.on
            } else {
                s.off
            };
            trading.set_status_title(format!("{} {}", s.st_fanout_set, state));
        }
        Err(e) => {
            trading.config.fanout_public_rpc = prev;
            trading.set_status_title(format!("{} {}", strings().st_failed_save, e));
        }
    }
    KeyAction::Redraw
}

fn toggle_skip_order_confirmation(trading: &mut TradingState) -> KeyAction {
    let prev = trading.config.skip_order_confirmation;
    trading.config.skip_order_confirmation = !prev;
    match save_user_config(&trading.config) {
        Ok(()) => {
            let s = strings();
            let state = if trading.config.skip_order_confirmation {
                s.on
            } else {
                s.off
            };
            trading.set_status_title(format!("{} {}", s.st_skip_order_confirmation_set, state));
        }
        Err(e) => {
            trading.config.skip_order_confirmation = prev;
            trading.set_status_title(format!("{} {}", strings().st_failed_save, e));
        }
    }
    KeyAction::Redraw
}

/// Modal-level navigation for the config view (before entering edit mode).
/// Row 0 = RPC URL, Row 1 = language, Row 2 = CLOB orders, Row 3 = public-RPC
/// fan-out, Row 4 = skip order confirmation, Row 5 = CU price, Row 6 = CU
/// limit per position.
pub(in crate::tui::runtime) fn handle_config_view_key(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    const LAST_FIELD: usize = 6;
    match code {
        KeyCode::Up => {
            if trading.config_selected_field > 0 {
                trading.config_selected_field -= 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Down => {
            if trading.config_selected_field < LAST_FIELD {
                trading.config_selected_field += 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            match trading.config_selected_field {
                0 => {
                    trading.input_buffer = trading.config.rpc_url.clone();
                    trading.input_mode = InputMode::EditingRpcUrl;
                }
                5 => {
                    trading.input_buffer = trading
                        .config
                        .compute_unit_price_micro_lamports
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    trading.input_mode = InputMode::EditingComputeUnitPrice;
                }
                6 => {
                    trading.input_buffer = trading
                        .config
                        .compute_unit_limit_per_position
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    trading.input_mode = InputMode::EditingComputeUnitLimit;
                }
                _ => {
                    // Non-text rows close the modal on Enter; Left/Right toggles them.
                    trading.input_mode = InputMode::Normal;
                }
            }
            KeyAction::Redraw
        }
        KeyCode::Left | KeyCode::Right => {
            if trading.config_selected_field == 1 {
                let prev = trading.config.language;
                trading.config.language = prev.toggle();
                match save_user_config(&trading.config) {
                    Ok(()) => {
                        let s = strings();
                        trading.set_status_title(format!(
                            "{} {}",
                            s.st_language_set,
                            trading.config.language.label()
                        ));
                    }
                    Err(e) => {
                        trading.config.language = prev;
                        trading.set_status_title(format!("{} {}", strings().st_failed_save, e));
                    }
                }
            } else if trading.config_selected_field == 2 {
                return toggle_show_clob(trading);
            } else if trading.config_selected_field == 3 {
                return toggle_fanout_public_rpc(trading);
            } else if trading.config_selected_field == 4 {
                return toggle_skip_order_confirmation(trading);
            }
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('q') => {
            trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

/// Maximum digits accepted in the CU-price text editor. 8 digits keeps the
/// value within sane priority-fee bounds while still fitting in `u64`.
const MAX_CU_PRICE_DIGITS: usize = 8;
/// Maximum digits accepted in the CU-limit text editor. 10 digits is the
/// largest value that still fits in `u32` (max 4_294_967_295).
const MAX_CU_LIMIT_DIGITS: usize = 10;

/// Text editor for the `SetComputeUnitPrice` override (microlamports per CU).
/// Empty + Enter clears the override and falls back to env / default.
pub(in crate::tui::runtime) fn handle_editing_compute_unit_price(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    match code {
        KeyCode::Enter => {
            let trimmed = trading.input_buffer.trim();
            let s = strings();
            let prev = trading.config.compute_unit_price_micro_lamports;
            if trimmed.is_empty() {
                trading.config.compute_unit_price_micro_lamports = None;
                match save_user_config(&trading.config) {
                    Ok(()) => trading.set_status_title(s.st_cu_cleared),
                    Err(e) => {
                        trading.config.compute_unit_price_micro_lamports = prev;
                        trading.set_status_title(format!("{} {}", s.st_failed_save, e));
                    }
                }
                trading.input_mode = InputMode::ViewingConfig;
                trading.input_buffer.clear();
                return KeyAction::Redraw;
            }
            match trimmed.parse::<u64>() {
                Ok(val) => {
                    trading.config.compute_unit_price_micro_lamports = Some(val);
                    match save_user_config(&trading.config) {
                        Ok(()) => {
                            trading.set_status_title(format!("{} {}", s.st_cu_price_set, val))
                        }
                        Err(e) => {
                            trading.config.compute_unit_price_micro_lamports = prev;
                            trading.set_status_title(format!("{} {}", s.st_failed_save, e));
                        }
                    }
                    trading.input_mode = InputMode::ViewingConfig;
                }
                Err(_) => {
                    trading.set_status_title(s.st_cu_invalid);
                }
            }
            trading.input_buffer.clear();
            KeyAction::Redraw
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
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if trading.input_buffer.len() < MAX_CU_PRICE_DIGITS {
                trading.input_buffer.push(c);
                KeyAction::Redraw
            } else {
                KeyAction::Nothing
            }
        }
        _ => KeyAction::Nothing,
    }
}

/// Text editor for the `SetComputeUnitLimit` per-position override (CUs per
/// trader position). Empty + Enter clears the override and falls back to env
/// / default.
pub(in crate::tui::runtime) fn handle_editing_compute_unit_limit(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    match code {
        KeyCode::Enter => {
            let trimmed = trading.input_buffer.trim();
            let s = strings();
            let prev = trading.config.compute_unit_limit_per_position;
            if trimmed.is_empty() {
                trading.config.compute_unit_limit_per_position = None;
                match save_user_config(&trading.config) {
                    Ok(()) => trading.set_status_title(s.st_cu_cleared),
                    Err(e) => {
                        trading.config.compute_unit_limit_per_position = prev;
                        trading.set_status_title(format!("{} {}", s.st_failed_save, e));
                    }
                }
                trading.input_mode = InputMode::ViewingConfig;
                trading.input_buffer.clear();
                return KeyAction::Redraw;
            }
            match trimmed.parse::<u32>() {
                Ok(val) if val > 0 => {
                    trading.config.compute_unit_limit_per_position = Some(val);
                    match save_user_config(&trading.config) {
                        Ok(()) => {
                            trading.set_status_title(format!("{} {}", s.st_cu_limit_set, val))
                        }
                        Err(e) => {
                            trading.config.compute_unit_limit_per_position = prev;
                            trading.set_status_title(format!("{} {}", s.st_failed_save, e));
                        }
                    }
                    trading.input_mode = InputMode::ViewingConfig;
                }
                _ => {
                    trading.set_status_title(s.st_cu_invalid);
                }
            }
            trading.input_buffer.clear();
            KeyAction::Redraw
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
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if trading.input_buffer.len() < MAX_CU_LIMIT_DIGITS {
                trading.input_buffer.push(c);
                KeyAction::Redraw
            } else {
                KeyAction::Nothing
            }
        }
        _ => KeyAction::Nothing,
    }
}

/// Ledger modal — arrow keys scroll; Enter copies the selected txid to the
/// system clipboard (closes the modal so the status line confirms); Esc/L/q
/// close.
pub(in crate::tui::runtime) fn handle_ledger_view_key(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    match code {
        KeyCode::Up => {
            if trading.ledger_selected > 0 {
                trading.ledger_selected -= 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Down => {
            let last = trading.ledger.len().saturating_sub(1);
            if trading.ledger_selected < last {
                trading.ledger_selected += 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            let s = strings();
            if let Some(entry) = trading.ledger.get(trading.ledger_selected).cloned() {
                match copy_to_clipboard(&entry.txid) {
                    Ok(()) => {
                        trading.set_status_title(format!("{} {}", s.ledger_copied, entry.txid))
                    }
                    Err(e) => trading.set_status_title(format!("{}: {}", s.ledger_copy_failed, e)),
                }
            }
            trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('L') | KeyCode::Char('q') => {
            trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}
