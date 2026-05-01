//! Price and size form handlers.

use super::*;

pub(in crate::tui::runtime) fn handle_editing_price(
    code: KeyCode,
    trading: &mut TradingState,
) -> KeyAction {
    // The editor applies to whichever price-bearing kind is active. If we
    // somehow enter here while `Market` is selected, treat parsed input as a
    // limit price (the normal-mode [e] path seeds Limit for us, so this is a
    // safety net).
    let is_stop = matches!(trading.order_kind, OrderKind::StopMarket { .. });
    match code {
        KeyCode::Enter => {
            let trimmed = trading.input_buffer.trim();
            let s = strings();
            if trimmed.is_empty() {
                trading.order_kind = OrderKind::Market;
                trading.set_status_title(if is_stop {
                    s.st_stop_cleared
                } else {
                    s.st_lim_cleared
                });
            } else if let Ok(val) = trimmed.parse::<f64>() {
                if val.is_finite() && val > 0.0 {
                    if is_stop {
                        trading.order_kind = OrderKind::StopMarket { trigger: val };
                        trading.set_status_title(format!("{}{:.2}", s.st_stop_set, val));
                    } else {
                        trading.order_kind = OrderKind::Limit { price: val };
                        trading.set_status_title(format!("{}{:.2}", s.st_lim_set, val));
                    }
                } else {
                    trading.set_status_title(if is_stop {
                        s.st_stop_must_positive
                    } else {
                        s.st_lim_must_positive
                    });
                }
            } else {
                trading.set_status_title(s.st_invalid_price);
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
        KeyCode::Char('t') => {
            // Same as normal mode when leaving: fall back to market and exit the editor.
            trading.order_kind = OrderKind::Market;
            trading.input_buffer.clear();
            trading.input_mode = InputMode::Normal;
            trading.set_status_title(strings().st_market_mode);
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
