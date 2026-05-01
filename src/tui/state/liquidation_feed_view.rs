//! Live liquidation feed modal state.
//!
//! Stores the most-recent liquidations parsed from Phoenix Eternal transactions
//! as a bounded ring buffer. New entries are inserted at the front so the modal
//! reads newest-first without resorting on every push.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

/// One row in the liquidation feed.
#[derive(Debug, Clone)]
pub struct LiquidationEntry {
    /// Wall-clock time the entry was decoded (used as the displayed timestamp;
    /// we don't have block-time without an extra RPC, so this is "received at"
    /// rather than "occurred at").
    pub received_at: DateTime<Utc>,
    /// Resolved market symbol (e.g. "SOL"). Empty if the asset_id wasn't in
    /// the local config map at decode time.
    pub symbol: String,
    /// On-chain asset_id from the event — kept around so the row can render
    /// usefully even when symbol resolution failed.
    pub asset_id: u32,
    /// 4-char prefix of the liquidated trader's PDA pubkey.
    pub liquidated_trader: String,
    /// Base-asset units actually filled by the liquidation order.
    pub size: f64,
    /// Mark price (USD) used during liquidation.
    pub mark_price: f64,
    /// `size * mark_price` — convenience field for the table.
    pub notional: f64,
    /// True if the liquidated position became fully closed on this market.
    pub position_closed: bool,
    /// Decimals used when formatting `mark_price` so the modal renders in the
    /// same precision the orderbook does.
    pub price_decimals: usize,
    /// Decimals for `size` in the same way.
    pub size_decimals: usize,
}

/// Fixed cap on the in-memory buffer. The modal is informational; we don't
/// need the entire history.
pub const LIQUIDATION_FEED_CAPACITY: usize = 200;

pub struct LiquidationFeedView {
    /// Newest first.
    pub entries: VecDeque<LiquidationEntry>,
    pub selected_index: usize,
}

impl LiquidationFeedView {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(LIQUIDATION_FEED_CAPACITY),
            selected_index: 0,
        }
    }

    /// Insert at the front. Drops the oldest entry once the buffer is full so
    /// the deque never grows unbounded.
    pub fn push(&mut self, entry: LiquidationEntry) {
        if self.entries.len() == LIQUIDATION_FEED_CAPACITY {
            self.entries.pop_back();
        }
        self.entries.push_front(entry);
        self.clamp_index();
    }

    pub fn clamp_index(&mut self) {
        if self.entries.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.entries.len() - 1);
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.entries.len() {
            self.selected_index += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }
}

impl Default for LiquidationFeedView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(trader: &str) -> LiquidationEntry {
        LiquidationEntry {
            received_at: Utc::now(),
            symbol: "SOL".to_string(),
            asset_id: 0,
            liquidated_trader: trader.to_string(),
            size: 1.0,
            mark_price: 100.0,
            notional: 100.0,
            position_closed: false,
            price_decimals: 2,
            size_decimals: 2,
        }
    }

    #[test]
    fn push_inserts_at_front() {
        let mut v = LiquidationFeedView::new();
        v.push(make_entry("aaaa"));
        v.push(make_entry("bbbb"));
        assert_eq!(v.entries.front().unwrap().liquidated_trader, "bbbb");
    }

    #[test]
    fn push_drops_oldest_at_capacity() {
        let mut v = LiquidationFeedView::new();
        for i in 0..(LIQUIDATION_FEED_CAPACITY + 5) {
            v.push(make_entry(&format!("{:04}", i)));
        }
        assert_eq!(v.entries.len(), LIQUIDATION_FEED_CAPACITY);
        // Newest entry sits at the front; oldest five rolled off the back.
        assert_eq!(
            v.entries.front().unwrap().liquidated_trader,
            format!("{:04}", LIQUIDATION_FEED_CAPACITY + 4)
        );
    }

    #[test]
    fn move_clamped_to_bounds() {
        let mut v = LiquidationFeedView::new();
        v.push(make_entry("a"));
        v.push(make_entry("b"));
        v.move_down();
        v.move_down();
        v.move_down();
        assert_eq!(v.selected_index, 1);
        v.move_up();
        v.move_up();
        v.move_up();
        assert_eq!(v.selected_index, 0);
    }
}
