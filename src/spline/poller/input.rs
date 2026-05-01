//! Keyboard input handlers for each input mode.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use phoenix_rise::PhoenixHttpClient;

use super::super::config::{
    default_wallet_path, resolve_wallet_modal_input, save_user_config, SplineConfig,
};
use super::super::format::{fmt_size, truncate_balance};
use super::super::i18n::strings;
use super::super::math::{ui_size_to_num_base_lots, LotConversionError, MAX_UI_ORDER_SIZE_UNITS};
use super::super::state::{TradingState, TuiState};
use super::super::trading::{InputMode, OrderKind, PendingAction, TradingSide};
use super::wallet::{connect_wallet_with_keypair, disconnect_wallet};
use super::{Channels, KeyAction};

const MAX_USDC_TRANSFER_AMOUNT: f64 = 1_000_000_000.0;

pub(super) fn handle_market_select_key(
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

pub(super) fn handle_orders_view_key(code: KeyCode, state: &mut TuiState) -> KeyAction {
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

/// "Top positions on Phoenix" modal — Up/Down navigate; Enter copies the
/// selected row's trader pubkey to the clipboard (closes the modal so the
/// status line confirms the copy); Esc/T/q close.
pub(super) fn handle_top_positions_view_key(code: KeyCode, state: &mut TuiState) -> KeyAction {
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

pub(super) fn handle_positions_view_key(
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
                let side = pos.side;
                let size = pos.size;
                state.trading.input_mode =
                    InputMode::Confirming(PendingAction::ClosePositionBySymbol {
                        symbol: symbol.clone(),
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

pub(super) fn handle_editing_price(code: KeyCode, trading: &mut TradingState) -> KeyAction {
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

/// Modal-level navigation for the config view (before entering edit mode).
/// Row 0 = RPC URL, Row 1 = language, Row 2 = CLOB orders.
pub(super) fn handle_config_view_key(code: KeyCode, trading: &mut TradingState) -> KeyAction {
    match code {
        KeyCode::Up => {
            if trading.config_selected_field > 0 {
                trading.config_selected_field -= 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Down => {
            if trading.config_selected_field < 2 {
                trading.config_selected_field += 1;
            }
            KeyAction::Redraw
        }
        KeyCode::Enter => {
            if trading.config_selected_field == 0 {
                trading.input_buffer = trading.config.rpc_url.clone();
                trading.input_mode = InputMode::EditingRpcUrl;
            } else if trading.config_selected_field == 1 {
                // Language: Enter closes the modal (Left/Right to toggle).
                trading.input_mode = InputMode::Normal;
            } else {
                // CLOB orders: Enter closes the modal (Left/Right to toggle).
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
pub(super) fn handle_ledger_view_key(code: KeyCode, trading: &mut TradingState) -> KeyAction {
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

/// Writes `text` to the OS clipboard. Returns a short error string on failure
/// (e.g. no display server) — callers surface it in the status line.
pub(super) fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}

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
pub(super) fn handle_editing_rpc_url(code: KeyCode, trading: &mut TradingState) -> KeyAction {
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
pub(super) fn handle_editing_wallet_path(
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

pub(super) fn handle_editing_size(code: KeyCode, trading: &mut TradingState) -> KeyAction {
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

pub(super) fn handle_editing_deposit(code: KeyCode, trading: &mut TradingState) -> KeyAction {
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

pub(super) fn handle_editing_withdraw(code: KeyCode, trading: &mut TradingState) -> KeyAction {
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

pub(super) fn num_base_lots_for_close(
    market_cfg: &SplineConfig,
    size: f64,
    position_size_raw: Option<(i64, i8)>,
) -> Result<u64, LotConversionError> {
    use super::super::math::phoenix_decimal_to_num_base_lots;
    if let Some(raw_lots) = position_size_raw
        .and_then(|(v, d)| phoenix_decimal_to_num_base_lots(v, d, market_cfg.base_lot_decimals))
        .filter(|&n| n > 0)
    {
        return Ok(raw_lots);
    }

    ui_size_to_num_base_lots(size, market_cfg.base_lot_decimals)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_normal_key(
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
        _ => KeyAction::Nothing,
    }
}
