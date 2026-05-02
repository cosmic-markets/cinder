//! Channel message types for async balance / market-list / market-stat updates.

use std::collections::HashMap;

use phoenix_rise::MarketStatsUpdate;

use super::super::config::SplineConfig;
use super::super::trading::{FullPositionInfo, PositionInfo};
use super::market::MarketInfo;

pub struct BalanceUpdate {
    pub phoenix_collateral: f64,
    pub position: Option<PositionInfo>,
    pub all_positions: Vec<FullPositionInfo>,
}

pub struct MarketListUpdate {
    pub markets: Vec<MarketInfo>,
    pub configs: HashMap<String, SplineConfig>,
}

pub type MarketStatUpdate = MarketStatsUpdate;

/// Snapshot of a spline collection account fetched once at market-switch time.
///
/// Solana's `accountSubscribe` only pushes when the account changes, so an
/// idle market would leave the "Switching to … market…" modal stuck until
/// the next on-chain spline write. This message carries the current account
/// state (HTTP `getAccountInfo`) into the same handler the WSS path uses.
pub struct SplineBootstrapMsg {
    pub symbol: String,
    pub slot: u64,
    pub data: Vec<u8>,
}
