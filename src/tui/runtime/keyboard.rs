//! High-level keyboard dispatcher for runtime input modes.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent};
use phoenix_rise::PhoenixHttpClient;

use super::super::config::SplineConfig;
use super::super::state::TuiState;
use super::super::trading::InputMode;
use super::{input, submit, Channels, KeyAction};

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_key_press(
    key: &KeyEvent,
    state: &mut TuiState,
    cfg: &SplineConfig,
    configs: &std::collections::HashMap<String, SplineConfig>,
    channels: &Channels,
    ws_url: &str,
    http: Arc<PhoenixHttpClient>,
    wallet_wss_handle: &mut Option<tokio::task::JoinHandle<()>>,
    blockhash_refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    balance_fetch_handle: &mut Option<tokio::task::JoinHandle<()>>,
    trader_orders_handle: &mut Option<tokio::task::JoinHandle<()>>,
    tx_ctx_task: &mut Option<tokio::task::JoinHandle<()>>,
    awaiting_first_tx_ctx: &mut bool,
    pending_market_switch: &mut Option<String>,
) -> KeyAction {
    let mode = state.trading.input_mode.clone();
    match mode {
        InputMode::SelectingMarket => {
            input::handle_market_select_key(key.code, state, cfg, pending_market_switch)
        }
        InputMode::ViewingPositions => {
            input::handle_positions_view_key(key.code, state, cfg, pending_market_switch)
        }
        InputMode::ViewingTopPositions => input::handle_top_positions_view_key(key.code, state),
        InputMode::EditingSize => input::handle_editing_size(key.code, &mut state.trading),
        InputMode::EditingPrice => input::handle_editing_price(key.code, &mut state.trading),
        InputMode::EditingDeposit => input::handle_editing_deposit(key.code, &mut state.trading),
        InputMode::EditingWithdraw => input::handle_editing_withdraw(key.code, &mut state.trading),
        InputMode::ViewingConfig => input::handle_config_view_key(key.code, &mut state.trading),
        InputMode::EditingRpcUrl => input::handle_editing_rpc_url(key.code, &mut state.trading),
        InputMode::EditingWalletPath => input::handle_editing_wallet_path(
            key.code,
            state,
            cfg,
            configs,
            channels,
            ws_url,
            http,
            wallet_wss_handle,
            balance_fetch_handle,
            trader_orders_handle,
            tx_ctx_task,
            awaiting_first_tx_ctx,
        ),
        InputMode::Confirming(action) => {
            // handle confirming inline
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    submit::execute_confirmed_action(
                        &action,
                        state,
                        cfg,
                        configs,
                        &channels.tx_status,
                    );
                    state.trading.input_mode = InputMode::Normal;
                    KeyAction::Redraw
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    state.trading.input_mode = InputMode::Normal;
                    state
                        .trading
                        .set_status_title(submit::cancel_message(&action, state, cfg));
                    KeyAction::Redraw
                }
                _ => KeyAction::Nothing,
            }
        }
        InputMode::Normal => input::handle_normal_key(
            key,
            state,
            cfg,
            wallet_wss_handle,
            blockhash_refresh_handle,
            balance_fetch_handle,
            trader_orders_handle,
            tx_ctx_task,
            awaiting_first_tx_ctx,
        ),
        InputMode::ViewingOrders => input::handle_orders_view_key(key.code, state),
        InputMode::ViewingLedger => input::handle_ledger_view_key(key.code, &mut state.trading),
        InputMode::ConfirmQuit => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => KeyAction::BreakOuter,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.trading.input_mode = InputMode::Normal;
                KeyAction::Redraw
            }
            _ => KeyAction::Nothing,
        },
    }
}
