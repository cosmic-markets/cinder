//! Config and ledger input handlers.

use super::*;

/// Flip a boolean setting in `trading.config`, persist it (rolling back on error),
/// and return `on_success` when saved.
fn toggle_bool_config(
    trading: &mut TradingState,
    field: impl Fn(&mut crate::tui::config::UserConfig) -> &mut bool,
    status_prefix: &'static str,
    on_success: KeyAction,
) -> KeyAction {
    let prev = *field(&mut trading.config);
    *field(&mut trading.config) = !prev;
    match save_user_config(&trading.config) {
        Ok(()) => {
            let s = strings();
            let state = if !prev { s.on } else { s.off };
            trading.set_status_title(format!("{status_prefix} {state}"));
            on_success
        }
        Err(e) => {
            *field(&mut trading.config) = prev;
            trading.set_status_title(format!("{} {e}", strings().st_failed_save));
            KeyAction::Redraw
        }
    }
}

fn toggle_show_clob(trading: &mut TradingState) -> KeyAction {
    toggle_bool_config(
        trading,
        |c| &mut c.show_clob,
        strings().st_clob_set,
        KeyAction::ToggleClob,
    )
}

fn toggle_fanout_public_rpc(trading: &mut TradingState) -> KeyAction {
    toggle_bool_config(
        trading,
        |c| &mut c.fanout_public_rpc,
        strings().st_fanout_set,
        KeyAction::Redraw,
    )
}

fn toggle_skip_order_confirmation(trading: &mut TradingState) -> KeyAction {
    toggle_bool_config(
        trading,
        |c| &mut c.skip_order_confirmation,
        strings().st_skip_order_confirmation_set,
        KeyAction::Redraw,
    )
}

fn toggle_skip_preflight(trading: &mut TradingState) -> KeyAction {
    toggle_bool_config(
        trading,
        |c| &mut c.skip_preflight,
        strings().st_skip_preflight_set,
        KeyAction::Redraw,
    )
}

/// Modal-level navigation for the config view (before entering edit mode).
/// Row 0 = RPC URL, Row 1 = language, Row 2 = CLOB orders, Row 3 = public-RPC
/// fan-out, Row 4 = skip order confirmation, Row 5 = skip preflight, Row 6 =
/// CU price, Row 7 = CU limit per position.
pub(in crate::tui::runtime) fn handle_config_view_key(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    const LAST_FIELD: usize = 7;
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
                6 => {
                    trading.input_buffer = trading
                        .config
                        .compute_unit_price_micro_lamports
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    trading.input_mode = InputMode::EditingComputeUnitPrice;
                }
                7 => {
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
            } else if trading.config_selected_field == 5 {
                return toggle_skip_preflight(trading);
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

/// Shared numeric editor for optional `UserConfig` compute-unit overrides.
fn handle_editing_cu_field<T>(
    code: KeyCode,
    trading: &mut TradingState,
    max_digits: usize,
    set_status_prefix: &'static str,
    field: impl Fn(&mut crate::tui::config::UserConfig) -> &mut Option<T>,
    parse: impl Fn(&str) -> Option<T>,
) -> KeyAction
where
    T: Copy + std::fmt::Display,
{
    match code {
        KeyCode::Enter => {
            let trimmed = trading.input_buffer.trim();
            let s = strings();
            let prev = *field(&mut trading.config);
            if trimmed.is_empty() {
                *field(&mut trading.config) = None;
                match save_user_config(&trading.config) {
                    Ok(()) => trading.set_status_title(s.st_cu_cleared),
                    Err(e) => {
                        *field(&mut trading.config) = prev;
                        trading.set_status_title(format!("{} {e}", s.st_failed_save));
                    }
                }
                trading.input_mode = InputMode::ViewingConfig;
            } else if let Some(val) = parse(trimmed) {
                *field(&mut trading.config) = Some(val);
                match save_user_config(&trading.config) {
                    Ok(()) => trading.set_status_title(format!("{set_status_prefix} {val}")),
                    Err(e) => {
                        *field(&mut trading.config) = prev;
                        trading.set_status_title(format!("{} {e}", s.st_failed_save));
                    }
                }
                trading.input_mode = InputMode::ViewingConfig;
            } else {
                trading.set_status_title(s.st_cu_invalid);
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
            if trading.input_buffer.len() < max_digits {
                trading.input_buffer.push(c);
                KeyAction::Redraw
            } else {
                KeyAction::Nothing
            }
        }
        _ => KeyAction::Nothing,
    }
}

/// Text editor for the `SetComputeUnitPrice` override (microlamports per CU).
pub(in crate::tui::runtime) fn handle_editing_compute_unit_price(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    handle_editing_cu_field(
        code,
        trading,
        MAX_CU_PRICE_DIGITS,
        strings().st_cu_price_set,
        |c| &mut c.compute_unit_price_micro_lamports,
        |s| s.parse::<u64>().ok(),
    )
}

/// Text editor for the `SetComputeUnitLimit` per-position override.
pub(in crate::tui::runtime) fn handle_editing_compute_unit_limit(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    handle_editing_cu_field(
        code,
        trading,
        MAX_CU_LIMIT_DIGITS,
        strings().st_cu_limit_set,
        |c| &mut c.compute_unit_limit_per_position,
        |s| s.parse::<u32>().ok().filter(|&v| v > 0),
    )
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
