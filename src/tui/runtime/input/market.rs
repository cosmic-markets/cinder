//! Market selector input handlers.

use super::*;

pub(in crate::tui::runtime) fn handle_market_select_key(
    code: KeyCode,
    state: &mut TuiState,
    cfg: &SplineConfig,
    pending_market_switch: &mut Option<String>,
) -> KeyAction {
    match code {
        KeyCode::Up => {
            state.market_selector.move_up();
            KeyAction::Redraw
        }
        KeyCode::Down => {
            state.market_selector.move_down();
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            if let Some(sym) = state.market_selector.selected_symbol() {
                let sym = sym.to_string();
                if sym != cfg.symbol {
                    *pending_market_switch = Some(sym);
                    state.trading.input_mode = InputMode::Normal;
                    state
                        .trading
                        .set_status_title(strings().st_switching_market);
                    return KeyAction::BreakInner;
                }
            }
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('m') | KeyCode::Char('q') => {
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}
