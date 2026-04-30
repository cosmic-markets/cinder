//! Open-order row as displayed in the orders modal.

use super::TradingSide;

#[derive(Debug, Clone)]
pub struct OrderInfo {
    pub symbol: String,
    /// Phoenix order sequence number — unique per market; used as the map key
    /// for per-order chart markers so their x-coordinate can advance
    /// independently across snapshots.
    pub order_sequence_number: u64,
    pub side: TradingSide,
    pub order_type: String,
    pub price_usd: f64,
    /// Phoenix `price_ticks` from the trader-state snapshot. Required to
    /// construct a `CancelId` — the on-chain cancel matches by
    /// `(price_in_ticks, order_sequence_number)`.
    pub price_ticks: u64,
    /// Size in base units (UI). Derived from raw lots + market
    /// `base_lot_decimals`; falls back to raw lots when the market isn't in
    /// the local `configs` map.
    pub size_remaining: f64,
    pub initial_size: f64,
    pub reduce_only: bool,
    pub is_stop_loss: bool,
}
