//! Modal list navigation handlers.

use super::*;

pub(in crate::tui::runtime) fn handle_orders_view_key(
    code: KeyCode,
    state: &mut TuiState,
) -> KeyAction {
    match code {
        KeyCode::Up => {
            state.orders_view.move_up();
            KeyAction::Redraw
        }
        KeyCode::Down => {
            state.orders_view.move_down();
            KeyAction::Redraw
        }
        KeyCode::Char('x') => {
            if let Some(o) = state
                .orders_view
                .orders
                .get(state.orders_view.selected_index)
            {
                let symbol = o.symbol.clone();
                let subaccount_index = o.subaccount_index;
                let side = o.side;
                let size = o.size_remaining;
                let price_usd = o.price_usd;
                let price_ticks = o.price_ticks;
                let order_sequence_number = o.order_sequence_number;
                let is_stop_loss = o.is_stop_loss;
                let conditional_order_index = o.conditional_order_index;
                let conditional_trigger_direction = o.conditional_trigger_direction;
                state.trading.input_mode = InputMode::Confirming(PendingAction::CancelOrder {
                    symbol: symbol.clone(),
                    subaccount_index,
                    side,
                    size,
                    price_usd,
                    price_ticks,
                    order_sequence_number,
                    is_stop_loss,
                    conditional_order_index,
                    conditional_trigger_direction,
                });
                {
                    let s = strings();
                    let side_lbl = match side {
                        TradingSide::Long => s.long_label,
                        TradingSide::Short => s.short_label,
                    };
                    state.trading.set_status_title(format!(
                        "{} {} {} {} @ ${:.2}? {}",
                        s.st_cancel_order_yn, side_lbl, size, symbol, price_usd, s.st_yn
                    ));
                }
            }
            KeyAction::Redraw
        }
        KeyCode::Char('u') => {
            if !state.orders_view.orders.is_empty() {
                state.trading.input_mode = InputMode::Confirming(PendingAction::CancelAllOrders);
                {
                    let s = strings();
                    state.trading.set_status_title(format!(
                        "{} {} {} {}",
                        s.st_cancel_all_yn,
                        state.orders_view.orders.len(),
                        s.st_open_orders_yn,
                        s.st_yn
                    ));
                }
            }
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('q') => {
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

/// Live liquidation feed modal — Up/Down navigate; Enter switches to the
/// market where the selected liquidation occurred (no-op if the row's symbol
/// failed to resolve at decode time, or already matches the active market);
/// Esc/F/q close.
pub(in crate::tui::runtime) fn handle_liquidation_feed_view_key(
    code: KeyCode,
    state: &mut TuiState,
    cfg: &SplineConfig,
    pending_market_switch: &mut Option<String>,
) -> KeyAction {
    match code {
        KeyCode::Up => {
            state.liquidation_feed_view.move_up();
            KeyAction::Redraw
        }
        KeyCode::Down => {
            state.liquidation_feed_view.move_down();
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            let sym = state
                .liquidation_feed_view
                .entries
                .get(state.liquidation_feed_view.selected_index)
                .map(|e| e.symbol.clone())
                .unwrap_or_default();
            if !sym.is_empty() && sym != cfg.symbol {
                state.trading.input_mode = InputMode::Normal;
                state.trading.set_status_title(format!(
                    "{} {}\u{2026}",
                    strings().st_switching_to,
                    sym
                ));
                *pending_market_switch = Some(sym);
                return KeyAction::BreakInner;
            }
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('F') | KeyCode::Char('q') => {
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

/// "Top positions on Phoenix" modal — Up/Down navigate; Enter copies the
/// selected row's trader pubkey to the clipboard (closes the modal so the
/// status line confirms the copy); Esc/T/q close.
pub(in crate::tui::runtime) fn handle_top_positions_view_key(
    code: KeyCode,
    state: &mut TuiState,
) -> KeyAction {
    match code {
        KeyCode::Up => {
            state.top_positions_view.move_up();
            KeyAction::Redraw
        }
        KeyCode::Down => {
            state.top_positions_view.move_down();
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            let s = strings();
            let entry = state
                .top_positions_view
                .positions
                .get(state.top_positions_view.selected_index)
                .cloned();
            if let Some(entry) = entry {
                match entry.trader.as_deref() {
                    Some(pk) => match copy_to_clipboard(pk) {
                        Ok(()) => state
                            .trading
                            .set_status_title(format!("{} {}", s.ledger_copied, pk)),
                        Err(e) => state
                            .trading
                            .set_status_title(format!("{}: {}", s.ledger_copy_failed, e)),
                    },
                    // Trader not yet resolved by the GTI cache — nothing
                    // useful to put on the clipboard. Surface a status hint
                    // and leave the modal open so the user can retry on the
                    // next refresh tick.
                    None => {
                        state.trading.set_status_title(s.top_positions_no_trader);
                        return KeyAction::Redraw;
                    }
                }
            }
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('T') | KeyCode::Char('q') => {
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}

pub(in crate::tui::runtime) fn handle_positions_view_key(
    code: KeyCode,
    state: &mut TuiState,
    cfg: &SplineConfig,
    pending_market_switch: &mut Option<String>,
) -> KeyAction {
    match code {
        KeyCode::Up => {
            state.positions_view.move_up();
            KeyAction::Redraw
        }
        KeyCode::Down => {
            state.positions_view.move_down();
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            if let Some(sym) = state.positions_view.selected_symbol() {
                let sym = sym.to_string();
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
        KeyCode::Char('x') | KeyCode::Char('c') => {
            if let Some(pos) = state
                .positions_view
                .positions
                .get(state.positions_view.selected_index)
            {
                let symbol = pos.symbol.clone();
                let subaccount_index = pos.subaccount_index;
                let side = pos.side;
                let size = pos.size;
                state.trading.input_mode =
                    InputMode::Confirming(PendingAction::ClosePositionBySymbol {
                        symbol: symbol.clone(),
                        subaccount_index,
                        side,
                        size,
                        position_size_raw: pos.position_size_raw,
                    });
                {
                    let s = strings();
                    let side_lbl = match side {
                        TradingSide::Long => s.long_label,
                        TradingSide::Short => s.short_label,
                    };
                    state.trading.set_status_title(format!(
                        "{} {} {} {}? {}",
                        s.st_close_by_sym_yn, size, symbol, side_lbl, s.st_yn
                    ));
                }
            }
            KeyAction::Redraw
        }
        KeyCode::Char('u') => {
            if !state.positions_view.positions.is_empty() {
                state.trading.input_mode = InputMode::Confirming(PendingAction::CloseAllPositions);
                state.trading.set_status_title(strings().st_close_all_yn);
            }
            KeyAction::Redraw
        }
        KeyCode::Esc | KeyCode::Char('p') | KeyCode::Char('q') => {
            state.trading.input_mode = InputMode::Normal;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}
