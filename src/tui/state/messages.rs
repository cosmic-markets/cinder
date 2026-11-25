//! Channel message payloads, chart markers, and ledger entries.

use std::collections::HashMap;

use phoenix_rise::types::market::MarketStatsUpdate;

use super::super::config::SplineConfig;
use super::super::trading::{FullPositionInfo, PositionInfo};
use super::market::MarketInfo;

/// Executed-trade pin rendered on the rolling price chart.
#[derive(Clone)]
pub struct TradeMarker {
    pub x: f64,
    pub y: f64,
    pub is_buy: bool,
}

/// Resting-order marker whose `x` scrolls left alongside the rolling price chart.
#[derive(Clone)]
pub struct OrderChartMarker {
    pub x: f64,
    pub price: f64,
}

/// Out-of-band status, chart-marker, ledger, and modal notifications from tx tasks.
#[derive(Clone, Debug)]
pub enum TxStatusMsg {
    TradeMarker {
        symbol: String,
        is_buy: bool,
    },
    SetStatus {
        title: String,
        detail: String,
    },
    /// Record a confirmed tx in the Ledger ring without touching the status line.
    LedgerOnly {
        title: String,
        signature: String,
    },
    /// Open the first-run referral choice modal for unregistered wallets.
    PromptReferralChoice,
}

impl TxStatusMsg {
    /// True if this message is per-slice status chatter silenced by TWAP slices.
    pub fn is_per_slice_chatter(&self) -> bool {
        match self {
            TxStatusMsg::SetStatus { .. } => true,
            TxStatusMsg::TradeMarker { .. }
            | TxStatusMsg::LedgerOnly { .. }
            | TxStatusMsg::PromptReferralChoice => false,
        }
    }
}

/// One row in the ledger modal (newest-first, capped at 50 entries).
#[derive(Clone, Debug)]
pub struct LedgerEntry {
    pub timestamp: String,
    pub title: String,
    pub txid: String,
}

/// Periodic HTTP balance + positions snapshot pushed into the event loop.
pub struct BalanceUpdate {
    pub phoenix_collateral: f64,
    pub position: Option<PositionInfo>,
    pub all_positions: Vec<FullPositionInfo>,
}

/// Newly discovered markets from the 60-second HTTP market poller.
pub struct MarketListUpdate {
    pub markets: Vec<MarketInfo>,
    pub configs: HashMap<String, SplineConfig>,
}

/// Live market-stats update from the Phoenix WebSocket stream.
pub type MarketStatUpdate = MarketStatsUpdate;

/// One-shot spline account snapshot fetched at market-switch time so idle
/// markets render immediately without waiting for the next WSS account write.
pub struct SplineBootstrapMsg {
    pub symbol: String,
    pub slot: u64,
    pub data: Vec<u8>,
}
