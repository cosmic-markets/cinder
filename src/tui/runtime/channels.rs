//! Channel payloads shared by runtime tasks and key handlers.

use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::super::state::{BalanceUpdate, LiquidationFeedMsg, SplineBootstrapMsg, TxStatusMsg};
use super::super::trading::{OrderInfo, TopPositionEntry};
use super::super::tx::TxContext;

pub(in crate::tui::runtime) enum KeyAction {
    Nothing,
    Redraw,
    BreakInner,
    BreakOuter,
    /// User saved new RPC URL: tear down and rebuild WSS/RPC connections using
    /// the fresh URL.
    ReconnectRpc,
    /// User toggled CLOB orders on/off: start or abort the L2 websocket task
    /// accordingly.
    ToggleClob,
}

/// `TxContext` is built asynchronously; include `wallet` (authority pubkey) +
/// `symbol` so a late completion for a replaced wallet or an old market is
/// ignored rather than misapplied.
pub(in crate::tui::runtime) type TxCtxMsg = (solana_pubkey::Pubkey, String, Arc<TxContext>);

pub(in crate::tui::runtime) struct Channels {
    pub tx_status: UnboundedSender<TxStatusMsg>,
    pub balance_tx: UnboundedSender<BalanceUpdate>,
    pub wallet_usdc_tx: UnboundedSender<f64>,
    pub wallet_sol_tx: UnboundedSender<f64>,
    pub tx_ctx_tx: UnboundedSender<TxCtxMsg>,
    pub orders_tx: UnboundedSender<Vec<OrderInfo>>,
    pub top_positions_tx: UnboundedSender<Vec<TopPositionEntry>>,
    pub liquidation_tx: UnboundedSender<LiquidationFeedMsg>,
    pub spline_bootstrap_tx: UnboundedSender<SplineBootstrapMsg>,
}

pub(in crate::tui::runtime) struct Receivers {
    pub rx_status: UnboundedReceiver<TxStatusMsg>,
    pub balance_rx: UnboundedReceiver<BalanceUpdate>,
    pub wallet_usdc_rx: UnboundedReceiver<f64>,
    pub wallet_sol_rx: UnboundedReceiver<f64>,
    pub tx_ctx_rx: UnboundedReceiver<TxCtxMsg>,
    pub orders_rx: UnboundedReceiver<Vec<OrderInfo>>,
    pub top_positions_rx: UnboundedReceiver<Vec<TopPositionEntry>>,
    pub liquidation_rx: UnboundedReceiver<LiquidationFeedMsg>,
    pub spline_bootstrap_rx: UnboundedReceiver<SplineBootstrapMsg>,
}

pub(in crate::tui::runtime) fn new_channels() -> (Channels, Receivers) {
    let (tx_status, rx_status) = tokio::sync::mpsc::unbounded_channel::<TxStatusMsg>();
    let (balance_tx, balance_rx) = tokio::sync::mpsc::unbounded_channel::<BalanceUpdate>();
    let (wallet_usdc_tx, wallet_usdc_rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
    let (wallet_sol_tx, wallet_sol_rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
    let (tx_ctx_tx, tx_ctx_rx) = tokio::sync::mpsc::unbounded_channel::<TxCtxMsg>();
    let (orders_tx, orders_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<OrderInfo>>();
    let (top_positions_tx, top_positions_rx) =
        tokio::sync::mpsc::unbounded_channel::<Vec<TopPositionEntry>>();
    let (liquidation_tx, liquidation_rx) =
        tokio::sync::mpsc::unbounded_channel::<LiquidationFeedMsg>();
    let (spline_bootstrap_tx, spline_bootstrap_rx) =
        tokio::sync::mpsc::unbounded_channel::<SplineBootstrapMsg>();

    (
        Channels {
            tx_status,
            balance_tx,
            wallet_usdc_tx,
            wallet_sol_tx,
            tx_ctx_tx,
            orders_tx,
            top_positions_tx,
            liquidation_tx,
            spline_bootstrap_tx,
        },
        Receivers {
            rx_status,
            balance_rx,
            wallet_usdc_rx,
            wallet_sol_rx,
            tx_ctx_rx,
            orders_rx,
            top_positions_rx,
            liquidation_rx,
            spline_bootstrap_rx,
        },
    )
}
