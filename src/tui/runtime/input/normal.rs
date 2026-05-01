//! Normal-mode keyboard shortcuts.

use super::*;

pub(in crate::tui::runtime) fn num_base_lots_for_close(
    market_cfg: &SplineConfig,
    size: f64,
    position_size_raw: Option<(i64, i8)>,
) -> Result<u64, LotConversionError> {
    use crate::tui::math::phoenix_decimal_to_num_base_lots;
    if let Some(raw_lots) = position_size_raw
        .and_then(|(v, d)| phoenix_decimal_to_num_base_lots(v, d, market_cfg.base_lot_decimals))
        .filter(|&n| n > 0)
    {
        return Ok(raw_lots);
    }

    ui_size_to_num_base_lots(size, market_cfg.base_lot_decimals)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::tui::runtime) fn handle_normal_key(
    key: &KeyEvent,
    state: &mut TuiState,
    cfg: &SplineConfig,
    wallet_wss_handle: &mut Option<tokio::task::JoinHandle<()>>,
    blockhash_refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    balance_fetch_handle: &mut Option<tokio::task::JoinHandle<()>>,
    trader_orders_handle: &mut Option<tokio::task::JoinHandle<()>>,
    tx_ctx_task: &mut Option<tokio::task::JoinHandle<()>>,
    awaiting_first_tx_ctx: &mut bool,
) -> KeyAction {
    match key.code {
        KeyCode::Char('q') => {
            state.trading.input_mode = InputMode::ConfirmQuit;
            KeyAction::Redraw
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            KeyAction::BreakOuter
        }
        KeyCode::Char('m') => {
            state.market_selector.focus_on(&cfg.symbol);
            state.trading.input_mode = InputMode::SelectingMarket;
            KeyAction::Redraw
        }
        KeyCode::Char('c') => {
            state.trading.config_selected_field = 0;
            state.trading.input_mode = InputMode::ViewingConfig;
            KeyAction::Redraw
        }
        KeyCode::Char('o') => {
            state.trading.input_mode = InputMode::ViewingOrders;
            KeyAction::Redraw
        }
        KeyCode::Char('p') => {
            state.positions_view.selected_index = 0;
            state.trading.input_mode = InputMode::ViewingPositions;
            KeyAction::Redraw
        }
        KeyCode::Tab => {
            state.trading.side = state.trading.side.toggle();
            KeyAction::Redraw
        }
        KeyCode::Char('t') => {
            // Cycle Market → Limit → StopMarket → Market. On transitions into
            // a price-bearing kind, seed the price from the current mark.
            let mark = state
                .market_stats
                .as_ref()
                .map(|s| s.mark_price)
                .unwrap_or(0.0);
            let s = strings();
            match state.trading.order_kind {
                OrderKind::Market => {
                    if mark.is_finite() && mark > 0.0 {
                        state.trading.order_kind = OrderKind::Limit { price: mark };
                        state.trading.set_status_title(format!(
                            "{}{} {}",
                            s.st_switched_limit,
                            fmt_size(mark, cfg.price_decimals),
                            s.st_switched_limit_hint
                        ));
                    } else {
                        state.trading.set_status_title(s.st_no_mark_price);
                    }
                }
                OrderKind::Limit { .. } => {
                    let seed = if mark.is_finite() && mark > 0.0 {
                        mark
                    } else {
                        state.trading.order_kind.price().unwrap_or(0.0)
                    };
                    if seed > 0.0 {
                        state.trading.order_kind = OrderKind::StopMarket { trigger: seed };
                        state.trading.set_status_title(format!(
                            "{}{}",
                            s.st_switched_stop,
                            fmt_size(seed, cfg.price_decimals)
                        ));
                    } else {
                        state.trading.set_status_title(s.st_no_mark_price);
                    }
                }
                OrderKind::StopMarket { .. } => {
                    state.trading.order_kind = OrderKind::Market;
                    state.trading.set_status_title(s.st_market_mode);
                }
            }
            KeyAction::Redraw
        }
        KeyCode::Char('e') => {
            // Edit the price of the active price-bearing kind. From Market,
            // seed a Limit from mark first (same intent as [t] + [e]).
            if matches!(state.trading.order_kind, OrderKind::Market) {
                let mark = state
                    .market_stats
                    .as_ref()
                    .map(|s| s.mark_price)
                    .unwrap_or(0.0);
                if mark.is_finite() && mark > 0.0 {
                    state.trading.order_kind = OrderKind::Limit { price: mark };
                }
            }
            state.trading.input_mode = InputMode::EditingPrice;
            state.trading.input_buffer.clear();
            let prompt = if matches!(state.trading.order_kind, OrderKind::StopMarket { .. }) {
                strings().st_enter_stop_price
            } else {
                strings().st_enter_price
            };
            state.trading.set_status_title(prompt);
            KeyAction::Redraw
        }
        KeyCode::Char('s') => {
            state.trading.input_mode = InputMode::EditingSize;
            state.trading.input_buffer.clear();
            state.trading.set_status_title(strings().st_enter_size);
            KeyAction::Redraw
        }
        KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up => {
            state.trading.increase_size();
            state.trading.custom_size = None;
            KeyAction::Redraw
        }
        KeyCode::Char('-') | KeyCode::Down => {
            state.trading.decrease_size();
            state.trading.custom_size = None;
            KeyAction::Redraw
        }
        KeyCode::Char('w') => {
            if !state.trading.wallet_loaded {
                state.trading.wallet_path_buffer = default_wallet_path();
                state.trading.wallet_path_error = None;
                state.trading.input_mode = InputMode::EditingWalletPath;
            } else {
                disconnect_wallet(
                    state,
                    wallet_wss_handle,
                    blockhash_refresh_handle,
                    balance_fetch_handle,
                    trader_orders_handle,
                    tx_ctx_task,
                    awaiting_first_tx_ctx,
                );
            }
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            if state.trading.wallet_loaded {
                let side = state.trading.side;
                let size = state.trading.order_size();
                if let Err(e) = ui_size_to_num_base_lots(size, cfg.base_lot_decimals) {
                    state.trading.set_status_title(format!(
                        "{} ({})",
                        strings().st_invalid_size,
                        e
                    ));
                    return KeyAction::Redraw;
                }
                // Drop any non-positive/non-finite price by falling back to Market.
                let kind = match state.trading.order_kind {
                    OrderKind::Limit { price } if price.is_finite() && price > 0.0 => {
                        OrderKind::Limit { price }
                    }
                    OrderKind::StopMarket { trigger } if trigger.is_finite() && trigger > 0.0 => {
                        OrderKind::StopMarket { trigger }
                    }
                    _ => OrderKind::Market,
                };
                state.trading.input_mode =
                    InputMode::Confirming(PendingAction::PlaceOrder { side, size, kind });
                let s = strings();
                let side_lbl = match side {
                    TradingSide::Long => s.long_label,
                    TradingSide::Short => s.short_label,
                };
                match kind {
                    OrderKind::Limit { price } => {
                        state.trading.set_status_title(format!(
                            "{} {} {} {} @ ${:.2}? {}",
                            s.st_confirm_limit, side_lbl, size, cfg.symbol, price, s.st_yn
                        ));
                    }
                    OrderKind::StopMarket { trigger } => {
                        state.trading.set_status_title(format!(
                            "{} {} {} {} @ ${:.2}? {}",
                            s.st_confirm_stop, side_lbl, size, cfg.symbol, trigger, s.st_yn
                        ));
                    }
                    OrderKind::Market => {
                        state.trading.set_status_title(format!(
                            "{} {} {} {}? {}",
                            s.st_confirm, side_lbl, size, cfg.symbol, s.st_yn
                        ));
                    }
                }
            } else {
                state
                    .trading
                    .set_status_title(strings().st_wallet_not_loaded);
            }
            KeyAction::Redraw
        }
        KeyCode::Char('x') => {
            if let Some(pos) = &state.trading.position {
                let s = strings();
                let side_lbl = match pos.side {
                    TradingSide::Long => s.long_label,
                    TradingSide::Short => s.short_label,
                };
                let close_msg = format!(
                    "{} {} {} {}? {}",
                    s.st_confirm_close, pos.size, cfg.symbol, side_lbl, s.st_yn
                );
                state.trading.input_mode = InputMode::Confirming(PendingAction::ClosePosition);
                state.trading.set_status_title(close_msg);
            } else {
                state
                    .trading
                    .set_status_title(strings().st_no_position_to_close);
            }
            KeyAction::Redraw
        }
        KeyCode::Char('d') => {
            if state.trading.wallet_loaded {
                state.trading.input_mode = InputMode::EditingDeposit;
                state.trading.deposit_buffer.clear();
                state.trading.set_status_title(strings().st_type_deposit);
            } else {
                state
                    .trading
                    .set_status_title(strings().st_wallet_not_loaded);
            }
            KeyAction::Redraw
        }
        KeyCode::Char('D') => {
            if state.trading.wallet_loaded {
                state.trading.input_mode = InputMode::EditingWithdraw;
                state.trading.withdraw_buffer.clear();
                state.trading.set_status_title(strings().st_type_withdraw);
            } else {
                state
                    .trading
                    .set_status_title(strings().st_wallet_not_loaded);
            }
            KeyAction::Redraw
        }
        KeyCode::Char('L') => {
            if state.trading.ledger_selected >= state.trading.ledger.len() {
                state.trading.ledger_selected = 0;
            }
            state.trading.input_mode = InputMode::ViewingLedger;
            KeyAction::Redraw
        }
        // Capital T opens the "top positions on Phoenix" modal. (Lowercase `t`
        // is taken — it cycles the order type.) The modal shows the top-N
        // largest active positions across every trader on the protocol.
        KeyCode::Char('T') => {
            state.top_positions_view.selected_index = 0;
            state.trading.input_mode = InputMode::ViewingTopPositions;
            KeyAction::Redraw
        }
        // [F] opens the live liquidation feed modal — recent on-chain
        // `LiquidationEvent`s parsed from Phoenix Eternal transactions. The
        // subscription task runs continuously in the background and keeps
        // pushing entries even while the modal is closed, so the buffer is
        // already warm on first open.
        KeyCode::Char('F') => {
            state.liquidation_feed_view.selected_index = 0;
            state.trading.input_mode = InputMode::ViewingLiquidations;
            KeyAction::Redraw
        }
        _ => KeyAction::Nothing,
    }
}
