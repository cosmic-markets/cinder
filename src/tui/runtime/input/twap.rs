//! Input handlers for the "New TWAP" modal (`EditingTwap`) and the bots
//! modal (`ViewingBots`).

use crossterm::event::KeyCode;

use super::super::super::config::SplineConfig;
use super::super::super::i18n::strings;
use super::super::super::math::{ui_size_to_num_base_lots, MAX_UI_ORDER_SIZE_UNITS};
use super::super::super::state::{TuiState, TwapBot, TwapDraft};
use super::super::super::trading::{InputMode, OrderKind, TradingSide};
use super::super::KeyAction;

/// Editor for the TWAP form. Field layout: 0 = market, 1 = side,
/// 2 = total size, 3 = total time hours, 4 = total time minutes.
/// ↑↓ moves between fields, Tab toggles side, ←→ cycles the market when
/// row 0 is focused, digits/`.` edit numeric fields, Enter advances the
/// cursor through the form (validates & spawns on the final row), Esc
/// discards.
pub(in crate::tui::runtime) fn handle_editing_twap_key(
    code: KeyCode,
    state: &mut TuiState,
    configs: &std::collections::HashMap<String, SplineConfig>,
) -> KeyAction {
    match code {
        KeyCode::Esc => {
            state.trading.input_mode = InputMode::Normal;
            // Keep TWAP as the active order_kind so the user can press [Enter]
            // again to reopen the modal — they came here intentionally and
            // didn't switch kind.
            KeyAction::Redraw
        }
        KeyCode::Up => {
            state.trading.twap_draft.move_field_up();
            state.trading.twap_draft.error = None;
            KeyAction::Redraw
        }
        KeyCode::Down => {
            state.trading.twap_draft.move_field_down();
            state.trading.twap_draft.error = None;
            KeyAction::Redraw
        }
        KeyCode::Tab => {
            // Tab from anywhere on the form toggles the side — saves the user
            // from having to navigate to the side row each time.
            state.trading.twap_draft.side = state.trading.twap_draft.side.toggle();
            state.trading.twap_draft.error = None;
            KeyAction::Redraw
        }
        KeyCode::Left if state.trading.twap_draft.selected_field == 0 => {
            let symbols: Vec<String> = state
                .market_selector
                .markets
                .iter()
                .map(|m| m.symbol.clone())
                .collect();
            state.trading.twap_draft.cycle_market(&symbols, -1);
            KeyAction::Redraw
        }
        KeyCode::Right if state.trading.twap_draft.selected_field == 0 => {
            let symbols: Vec<String> = state
                .market_selector
                .markets
                .iter()
                .map(|m| m.symbol.clone())
                .collect();
            state.trading.twap_draft.cycle_market(&symbols, 1);
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            // Enter advances through the form like Tab on a web form: on
            // any non-final row it moves the cursor down. On the last row
            // (minutes) it validates and spawns the bot.
            if state.trading.twap_draft.selected_field + 1 < TwapDraft::FIELD_COUNT {
                state.trading.twap_draft.move_field_down();
                state.trading.twap_draft.error = None;
                return KeyAction::Redraw;
            }
            if !state.trading.wallet_loaded {
                state.trading.twap_draft.error = Some(strings().twap_err_no_wallet.to_string());
                return KeyAction::Redraw;
            }
            match build_bot_from_draft(state, configs) {
                Ok(bot) => {
                    let total = bot.total_size;
                    let slices = bot.slice_count;
                    let symbol = bot.symbol.clone();
                    let side = bot.side;
                    state.twaps_view.push(bot);
                    state.trading.input_mode = InputMode::Normal;
                    // Drop back to Market mode so a follow-up Enter doesn't
                    // immediately reopen the TWAP modal — most users will
                    // want to do something else next.
                    state.trading.order_kind = OrderKind::Market;
                    let s = strings();
                    let side_lbl = match side {
                        TradingSide::Long => s.long_label,
                        TradingSide::Short => s.short_label,
                    };
                    state.trading.set_status_title(format!(
                        "{}: {} {} {} \u{2014} {} {}",
                        s.twap_started, side_lbl, total, symbol, slices, s.twap_unit_slices
                    ));
                    KeyAction::Redraw
                }
                Err(msg) => {
                    state.trading.twap_draft.error = Some(msg);
                    KeyAction::Redraw
                }
            }
        }
        KeyCode::Backspace => {
            if let Some(buf) = field_buffer_mut(state) {
                buf.pop();
            }
            state.trading.twap_draft.error = None;
            KeyAction::Redraw
        }
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            // Hours / minutes are integer-only; reject `.` on those rows.
            // Size accepts decimals.
            let allow = match state.trading.twap_draft.selected_field {
                3 | 4 => c != '.',
                _ => true,
            };
            if allow {
                if let Some(buf) = field_buffer_mut(state) {
                    buf.push(c);
                    state.trading.twap_draft.error = None;
                }
            }
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

fn field_buffer_mut(state: &mut TuiState) -> Option<&mut String> {
    let d = &mut state.trading.twap_draft;
    match d.selected_field {
        2 => Some(&mut d.size_buffer),
        3 => Some(&mut d.duration_hour_buffer),
        4 => Some(&mut d.duration_min_buffer),
        // Row 0 (market) and row 1 (side) have no text buffer — they're
        // toggled via ←→ and Tab respectively.
        _ => None,
    }
}

/// Validate the TWAP draft and assemble a `TwapBot`. Returns a localized
/// error message string on failure. Slice cadence is fixed at one market
/// slice per minute (`slice_count = total_minutes`), matching Binance.
fn build_bot_from_draft(
    state: &TuiState,
    configs: &std::collections::HashMap<String, SplineConfig>,
) -> Result<TwapBot, String> {
    let s = strings();
    let draft = &state.trading.twap_draft;

    let market_cfg = configs
        .get(&draft.market)
        .ok_or_else(|| format!("{} {}", s.st_market_switch_failed, draft.market))?;

    let size: f64 = draft
        .size_buffer
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_UI_ORDER_SIZE_UNITS)
        .ok_or_else(|| s.twap_err_size.to_string())?;

    let hours: u32 = if draft.duration_hour_buffer.is_empty() {
        0
    } else {
        draft
            .duration_hour_buffer
            .parse::<u32>()
            .map_err(|_| s.twap_err_duration.to_string())?
    };
    let mins: u32 = if draft.duration_min_buffer.is_empty() {
        0
    } else {
        draft
            .duration_min_buffer
            .parse::<u32>()
            .map_err(|_| s.twap_err_duration.to_string())?
    };
    if hours == 0 && mins == 0 {
        return Err(s.twap_err_duration.to_string());
    }
    let total_minutes: u32 = hours
        .checked_mul(60)
        .and_then(|h| h.checked_add(mins))
        .ok_or_else(|| s.twap_err_duration.to_string())?;
    if total_minutes < 1 {
        return Err(s.twap_err_too_short.to_string());
    }
    let total_seconds: u64 = (total_minutes as u64) * 60;

    // Cadence: 1 slice / minute.
    let slice_count: u32 = total_minutes;
    let slice_size = size / slice_count as f64;
    // Reject slice sizes that would round to zero base lots on this market —
    // the on-chain order would immediately fail. Surface the friendlier
    // "shorten Total Time or increase size" message instead.
    if ui_size_to_num_base_lots(slice_size, market_cfg.base_lot_decimals)
        .map(|n| n == 0)
        .unwrap_or(true)
    {
        return Err(s.twap_err_size_too_small.to_string());
    }

    Ok(TwapBot::new(
        market_cfg.symbol.clone(),
        draft.side,
        size,
        slice_count,
        total_seconds.max(1),
    ))
}

/// Bots modal — pause/resume, stop, restart, remove, scroll, switch market.
pub(in crate::tui::runtime) fn handle_bots_view_key(
    code: KeyCode,
    state: &mut TuiState,
    cfg: &SplineConfig,
    pending_market_switch: &mut Option<String>,
) -> KeyAction {
    match code {
        KeyCode::Up => {
            state.twaps_view.move_up();
            KeyAction::Redraw
        }
        KeyCode::Down => {
            state.twaps_view.move_down();
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            // Same pattern as the positions/liquidations modals: jump the
            // active market to the bot's market, then close the modal so
            // the user lands on the chart that the bot is trading.
            let target = state
                .twaps_view
                .bots
                .get(state.twaps_view.selected_index)
                .map(|b| b.symbol.clone());
            if let Some(sym) = target {
                if sym != cfg.symbol {
                    state.trading.input_mode = InputMode::Normal;
                    state.trading.set_status_title(format!(
                        "{} {}\u{2026}",
                        strings().st_switching_to,
                        sym
                    ));
                    *pending_market_switch = Some(sym);
                    return KeyAction::BreakInner;
                }
            }
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        KeyCode::Char('p') => {
            let msg = state.twaps_view.selected_mut().map(|bot| {
                use super::super::super::state::TwapStatus;
                let s = strings();
                match bot.status {
                    TwapStatus::Running => {
                        bot.pause();
                        s.bots_paused_status
                    }
                    TwapStatus::Paused => {
                        bot.resume();
                        s.bots_resumed_status
                    }
                    _ => "",
                }
            });
            if let Some(line) = msg.filter(|m| !m.is_empty()) {
                state.trading.set_status_title(line);
            }
            KeyAction::Redraw
        }
        KeyCode::Char('s') => {
            if let Some(bot) = state.twaps_view.selected_mut() {
                bot.stop();
                state
                    .trading
                    .set_status_title(strings().bots_stopped_status);
            }
            KeyAction::Redraw
        }
        KeyCode::Char('r') => {
            if let Some(bot) = state.twaps_view.selected_mut() {
                bot.restart();
                state
                    .trading
                    .set_status_title(strings().bots_restarted_status);
            }
            KeyAction::Redraw
        }
        KeyCode::Char('x') => {
            if state.twaps_view.remove_selected().is_some() {
                state
                    .trading
                    .set_status_title(strings().bots_removed_status);
            }
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('q') => {
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}
