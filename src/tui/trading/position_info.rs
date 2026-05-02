//! Snapshot views of an open trader position.

use super::TradingSide;

/// Active-market position summary used by the trading panel and chart.
#[derive(Debug, Clone)]
pub struct PositionInfo {
    /// Phoenix trader subaccount index that owns this position.
    /// `0` is cross-margin; `1+` are isolated subaccounts.
    pub subaccount_index: u8,
    pub side: TradingSide,
    pub size: f64,
    /// Phoenix `position_size.value` / `position_size.decimals` for exact lot
    /// sizing on close.
    pub position_size_raw: Option<(i64, i8)>,
    pub entry_price: f64,
    pub unrealized_pnl: f64,
    pub liquidation_price: Option<f64>,
    pub notional: f64,
    pub leverage: Option<f64>,
}

/// Same shape as [`PositionInfo`] but carries the `symbol`, used by the
/// "all positions" modal where rows span multiple markets.
#[derive(Debug, Clone)]
pub struct FullPositionInfo {
    pub symbol: String,
    pub subaccount_index: u8,
    pub side: TradingSide,
    pub size: f64,
    pub position_size_raw: Option<(i64, i8)>,
    pub entry_price: f64,
    pub unrealized_pnl: f64,
    pub liquidation_price: Option<f64>,
    pub notional: f64,
    pub leverage: Option<f64>,
}
