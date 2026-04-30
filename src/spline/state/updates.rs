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
