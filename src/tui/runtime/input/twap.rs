//! Input handlers for the "New TWAP" modal (`EditingTwap`) and the bots
//! modal (`ViewingBots`).

use crossterm::event::KeyCode;

use super::super::super::config::{current_user_config, SplineConfig};
use super::super::super::i18n::strings;
use super::super::super::math::{ui_size_to_num_base_lots, MAX_UI_ORDER_SIZE_UNITS};
use super::super::super::state::{TuiState, TwapBot, TwapBotConfirm, TwapDraft};
use super::super::super::trading::{InputMode, OrderKind, TradingSide};
use super::super::twap_scheduler;
use super::super::KeyAction;

/// Editor for the TWAP form. Field layout: 0 = market, 1 = side,
/// 2 = total size, 3 = total time hours, 4 = total time minutes,
/// 5 = total time seconds. ↑↓ moves between fields, Tab toggles side, ←→
/// cycles the market when row 0 is focused, digits/`.` edit numeric fields,
/// Enter advances the cursor through the form (validates & opens confirm
/// prompt on the final row), Esc discards.
pub(in crate::tui::runtime) fn handle_editing_twap_key(
    code: KeyCode,
    state: &mut TuiState,
    configs: &std::collections::HashMap<String, SplineConfig>,
) -> KeyAction {
    // When the confirmation prompt is showing, route keys through a
    // separate Y/N handler so the user can't keep editing fields underneath.
    if state.trading.twap_draft.pending_confirm {
        return handle_twap_confirm_key(code, state, configs);
    }

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
            // (minutes) it validates and either opens the confirm prompt
            // or — if the user enabled `skip_order_confirmation` — pushes
            // the bot immediately.
            if state.trading.twap_draft.selected_field + 1 < TwapDraft::FIELD_COUNT {
                state.trading.twap_draft.move_field_down();
                state.trading.twap_draft.error = None;
                return KeyAction::Redraw;
            }
            if !state.trading.wallet_loaded {
                state.trading.twap_draft.error = Some(strings().twap_err_no_wallet.to_string());
                return KeyAction::Redraw;
            }
            // Validate now so the user sees the error before reaching the
            // confirm prompt.
            if let Err(msg) = build_bot_from_draft(state, configs) {
                state.trading.twap_draft.error = Some(msg);
                return KeyAction::Redraw;
            }
            if current_user_config().skip_order_confirmation {
                submit_pending_twap(state, configs);
            } else {
                state.trading.twap_draft.pending_confirm = true;
            }
            KeyAction::Redraw
        }
        KeyCode::Backspace => {
            if let Some(buf) = field_buffer_mut(state) {
                buf.pop();
            }
            state.trading.twap_draft.error = None;
            KeyAction::Redraw
        }
        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
            // Hours / minutes / seconds are integer-only; reject `.` on
            // those rows. Size accepts decimals.
            let allow = match state.trading.twap_draft.selected_field {
                3 | 4 | 5 => c != '.',
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

/// Y/N gate shown after the user passes form validation on the final row.
/// Y / Enter pushes the bot; N / Esc cancels back to editing.
fn handle_twap_confirm_key(
    code: KeyCode,
    state: &mut TuiState,
    configs: &std::collections::HashMap<String, SplineConfig>,
) -> KeyAction {
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            submit_pending_twap(state, configs);
            KeyAction::Redraw
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            state.trading.twap_draft.pending_confirm = false;
            state.trading.twap_draft.error = None;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

/// Build the bot from the validated draft and push it onto the running list.
/// Called from both the immediate-submit path (skip_order_confirmation=true)
/// and the Y branch of the confirm prompt.
fn submit_pending_twap(
    state: &mut TuiState,
    configs: &std::collections::HashMap<String, SplineConfig>,
) {
    let s = strings();
    match build_bot_from_draft(state, configs) {
        Ok(bot) => {
            let total = bot.total_size;
            let slices = bot.slice_count;
            let symbol = bot.symbol.clone();
            let side = bot.side;
            state.twaps_view.push(bot);
            state.trading.twap_draft.pending_confirm = false;
            state.trading.input_mode = InputMode::Normal;
            // Drop back to Market mode so a follow-up Enter doesn't
            // immediately reopen the TWAP modal — most users will
            // want to do something else next.
            state.trading.order_kind = OrderKind::Market;
            let side_lbl = match side {
                TradingSide::Long => s.long_label,
                TradingSide::Short => s.short_label,
            };
            state.trading.set_status_title(format!(
                "{}: {} {} {} \u{2014} {} {}",
                s.twap_started, side_lbl, total, symbol, slices, s.twap_unit_slices
            ));
        }
        Err(msg) => {
            // Validation regressed between original validation and now (e.g.
            // wallet disconnected). Drop confirmation, surface the error.
            state.trading.twap_draft.pending_confirm = false;
            state.trading.twap_draft.error = Some(msg);
        }
    }
}

fn field_buffer_mut(state: &mut TuiState) -> Option<&mut String> {
    let d = &mut state.trading.twap_draft;
    match d.selected_field {
        2 => Some(&mut d.size_buffer),
        3 => Some(&mut d.duration_hour_buffer),
        4 => Some(&mut d.duration_min_buffer),
        5 => Some(&mut d.duration_sec_buffer),
        // Row 0 (market) and row 1 (side) have no text buffer — they're
        // toggled via ←→ and Tab respectively.
        _ => None,
    }
}

/// Validate the TWAP draft and assemble a `TwapBot`. Returns a localized
/// error message string on failure.
///
/// Cadence rule (must stay in lockstep with `derive_summary` in the modal
/// renderer): if the Seconds field is non-zero OR the total is sub-minute,
/// fire one slice per second; otherwise fall back to one slice per minute
/// (matches Binance's default). This lets the user dial in either coarse
/// hour-long schedules or rapid sub-minute drips with the same form.
///
/// Re-checks `wallet_loaded` so the submit path can't create a bot that
/// would never fire (e.g. wallet disconnected between Enter-validate and
/// Y-confirm) — the bot needs the wallet's authority to bind its identity.
fn build_bot_from_draft(
    state: &TuiState,
    configs: &std::collections::HashMap<String, SplineConfig>,
) -> Result<TwapBot, String> {
    let s = strings();
    let draft = &state.trading.twap_draft;

    let market_cfg = configs
        .get(&draft.market)
        .ok_or_else(|| format!("{} {}", s.st_market_switch_failed, draft.market))?;

    // Resolve the wallet authority now — bind the bot to this wallet so a
    // later wallet-swap can't redirect its slices.
    let authority: solana_pubkey::Pubkey = match state.trading.keypair.as_ref() {
        Some(kp) => {
            use solana_signer::Signer;
            kp.pubkey()
        }
        None => return Err(s.twap_err_no_wallet.to_string()),
    };

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
    let secs: u32 = if draft.duration_sec_buffer.is_empty() {
        0
    } else {
        draft
            .duration_sec_buffer
            .parse::<u32>()
            .map_err(|_| s.twap_err_duration.to_string())?
    };
    if hours == 0 && mins == 0 && secs == 0 {
        return Err(s.twap_err_duration.to_string());
    }
    // u64 totals so a maximum-u32 hours field can't overflow (3600 *
    // 4_294_967_295 ≫ u32::MAX; would panic in debug, wrap in release).
    let total_seconds: u64 = (hours as u64)
        .checked_mul(3600)
        .and_then(|h| h.checked_add((mins as u64).checked_mul(60)?))
        .and_then(|hm| hm.checked_add(secs as u64))
        .ok_or_else(|| s.twap_err_duration.to_string())?;
    if total_seconds < 1 {
        return Err(s.twap_err_too_short.to_string());
    }

    // Cadence: 1 / sec when seconds is set (or total is sub-minute),
    // otherwise 1 / min. Keep this rule identical to `derive_summary` so
    // the preview matches what actually fires.
    let total_minutes = hours
        .checked_mul(60)
        .and_then(|h| h.checked_add(mins))
        .unwrap_or(u32::MAX);
    let slice_count: u32 = if secs > 0 || total_minutes == 0 {
        // Total fits in u32 here because secs/mins/hours are u32 and the
        // total_seconds u64 above didn't overflow → < u32::MAX.
        (total_seconds as u32).max(1)
    } else {
        total_minutes
    };
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
        authority,
    ))
}

/// Bots modal — pause/resume, stop, restart, remove, scroll, switch market.
///
/// `[s]` (stop), `[r]` (restart), and `[x]` (remove) all open a Y/N
/// confirmation overlay rather than firing immediately, because each one
/// either moves real capital (restart) or cancels in-flight execution (stop,
/// remove). `[p]` (pause/resume) is unguarded because it's reversible.
pub(in crate::tui::runtime) fn handle_bots_view_key(
    code: KeyCode,
    state: &mut TuiState,
    cfg: &SplineConfig,
    pending_market_switch: &mut Option<String>,
) -> KeyAction {
    // Y/N gate for the destructive actions.
    if let Some(pending) = state.twaps_view.pending_confirm {
        return handle_bots_confirm_key(code, state, pending);
    }

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
            // Guard: only confirm if there's actually a Running/Paused bot
            // to stop — pressing [s] on a Completed or Stopped row is a no-op.
            let can_stop = state
                .twaps_view
                .bots
                .get(state.twaps_view.selected_index)
                .map(|b| !b.status.is_terminal())
                .unwrap_or(false);
            if can_stop {
                state.twaps_view.pending_confirm =
                    Some(TwapBotConfirm::Stop(state.twaps_view.selected_index));
            }
            KeyAction::Redraw
        }
        KeyCode::Char('r') => {
            if state
                .twaps_view
                .bots
                .get(state.twaps_view.selected_index)
                .is_some()
            {
                state.twaps_view.pending_confirm =
                    Some(TwapBotConfirm::Restart(state.twaps_view.selected_index));
            }
            KeyAction::Redraw
        }
        KeyCode::Char('x') => {
            if state
                .twaps_view
                .bots
                .get(state.twaps_view.selected_index)
                .is_some()
            {
                state.twaps_view.pending_confirm =
                    Some(TwapBotConfirm::Remove(state.twaps_view.selected_index));
            }
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('q') => {
            // Drop any stale confirmation prompt on close so a re-open
            // doesn't surface a destructive prompt the user never armed.
            state.twaps_view.pending_confirm = None;
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

/// Y/N gate for the bots modal's destructive actions (stop / restart /
/// remove). Executes the action on Y, cancels on N/Esc. Other keys ignored
/// so the user can't accidentally scroll or open another modal mid-prompt.
fn handle_bots_confirm_key(
    code: KeyCode,
    state: &mut TuiState,
    pending: TwapBotConfirm,
) -> KeyAction {
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            let s = strings();
            // Cache live authority so Restart can refuse to fire against the
            // wrong wallet without re-reading state inside the match arm.
            let live_authority: Option<solana_pubkey::Pubkey> =
                state.trading.keypair.as_ref().map(|kp| {
                    use solana_signer::Signer;
                    kp.pubkey()
                });
            match pending {
                TwapBotConfirm::Stop(i) => {
                    // Re-check terminal status — the bot may have completed
                    // between arm and confirm, in which case stop() is a
                    // no-op and we shouldn't lie to the user about "stopped".
                    let was_active = state
                        .twaps_view
                        .bots
                        .get(i)
                        .map(|b| !b.status.is_terminal())
                        .unwrap_or(false);
                    if was_active {
                        if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                            twap_scheduler::settle_in_flight_for_interrupt(
                                bot,
                                std::time::Instant::now(),
                                s.bots_stopped_status,
                            );
                            bot.stop();
                            state.trading.set_status_title(s.bots_stopped_status);
                        }
                    } else {
                        // No-op: bot already terminal. Don't claim it stopped.
                    }
                }
                TwapBotConfirm::Restart(i) => {
                    // Refuse restart when no wallet is connected or the
                    // connected wallet isn't the bot's original wallet —
                    // restart re-deploys capital and must not silently
                    // redirect to a different wallet's funds.
                    let bot_authority = state.twaps_view.bots.get(i).map(|b| b.authority);
                    match (bot_authority, live_authority) {
                        (Some(bot_auth), Some(live)) if bot_auth == live => {
                            if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                                if bot.in_flight.is_some() {
                                    twap_scheduler::settle_in_flight_for_interrupt(
                                        bot,
                                        std::time::Instant::now(),
                                        s.bots_stopped_status,
                                    );
                                    bot.stop();
                                    state.trading.set_status_title(s.bots_stopped_status);
                                } else {
                                    bot.restart();
                                    state.trading.set_status_title(s.bots_restarted_status);
                                }
                            }
                        }
                        (Some(_), None) => {
                            state.trading.set_status_title(s.twap_err_no_wallet);
                        }
                        (Some(_), Some(_)) => {
                            state
                                .trading
                                .set_status_title(s.twap_restart_wallet_mismatch);
                        }
                        _ => {}
                    }
                }
                TwapBotConfirm::Remove(i) => {
                    if i == state.twaps_view.selected_index {
                        let has_in_flight = state
                            .twaps_view
                            .bots
                            .get(i)
                            .map(|bot| bot.in_flight.is_some())
                            .unwrap_or(false);
                        if has_in_flight {
                            if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                                twap_scheduler::settle_in_flight_for_interrupt(
                                    bot,
                                    std::time::Instant::now(),
                                    s.bots_stopped_status,
                                );
                                bot.stop();
                            }
                            state.trading.set_status_title(s.bots_stopped_status);
                        } else if state.twaps_view.remove_selected().is_some() {
                            state.trading.set_status_title(s.bots_removed_status);
                        }
                    }
                }
            }
            state.twaps_view.pending_confirm = None;
            KeyAction::Redraw
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            state.twaps_view.pending_confirm = None;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}
