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

/// Modal-level navigation for the config view (before entering edit mode).
/// Row 0 = RPC URL, Row 1 = language, Row 2 = CLOB orders, Row 3 = public-RPC
/// fan-out.
pub(in crate::tui::runtime) fn handle_config_view_key(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    match code {
        KeyCode::Up => {
            if trading.config_selected_field > 0 {
                trading.config_selected_field -= 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Down => {
            if trading.config_selected_field < 3 {
                trading.config_selected_field += 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            if trading.config_selected_field == 0 {
                trading.input_buffer = trading.config.rpc_url.clone();
                trading.input_mode = InputMode::EditingRpcUrl;
            } else {
                // Non-text rows close the modal on Enter; Left/Right toggles them.
                trading.input_mode = InputMode::Normal;
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
