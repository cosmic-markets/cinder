//! Live liquidation feed modal state.
//!
//! Stores the most-recent liquidations parsed from Phoenix Eternal transactions
//! as a bounded ring buffer. New entries are inserted at the front so the modal
//! reads newest-first without resorting on every push.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use super::super::trading::TradingSide;

/// One row in the liquidation feed.
#[derive(Debug, Clone)]
pub struct LiquidationEntry {
    /// Block time of the underlying tx when the RPC returned one, otherwise
    /// the wall-clock time we decoded the event. Used both as the displayed
    /// timestamp and as the sort key that keeps the modal in chronological
    /// (newest-first) order regardless of arrival order.
    pub received_at: DateTime<Utc>,
    /// Resolved market symbol (e.g. "SOL"). Empty if the asset_id wasn't in
    /// the local config map at decode time.
    pub symbol: String,
    /// On-chain asset_id from the event — kept around so the row can render
    /// usefully even when symbol resolution failed.
    pub asset_id: u32,
    /// Side of the liquidated trader's position (the position being closed,
    /// not the liquidator's offsetting trade). `None` when the program log
    /// announcing the side wasn't present or couldn't be parsed; the modal
    /// renders that as a dash.
    pub side: Option<TradingSide>,
    /// 4-char prefix of the liquidated trader's pubkey, displayed in the
    /// modal's rightmost column.
    pub liquidated_trader: String,
    /// Base-asset units actually filled by the liquidation order.
    pub size: f64,
    /// Mark price (USD) used during liquidation.
    pub mark_price: f64,
    /// `size * mark_price` — convenience field for the table.
    pub notional: f64,
    /// Decimals used when formatting `mark_price` so the modal renders in the
    /// same precision the orderbook does.
    pub price_decimals: usize,
    /// Decimals for `size` in the same way.
    pub size_decimals: usize,
}

/// Fixed cap on the in-memory buffer. The modal is informational; we don't
/// need the entire history.
pub const LIQUIDATION_FEED_CAPACITY: usize = 200;

/// Channel payload carrying either a decoded liquidation row or the one-shot
/// signal that startup backfill has finished. The runtime forwards both to
/// `handle_liquidation_update`, which mutates `LiquidationFeedView`
/// accordingly.
#[derive(Debug, Clone)]
pub enum LiquidationFeedMsg {
    Entry(LiquidationEntry),
    /// Startup backfill is finished — flip the modal indicator from
    /// "backfilling…" to "live". Sent exactly once for the process lifetime.
    BackfillComplete,
}

pub struct LiquidationFeedView {
    /// Sorted by `received_at` descending — newest at index 0. The view is
    /// fed by both a live stream and an out-of-order startup backfill, so the
    /// invariant is maintained on insert rather than relying on push order.
    pub entries: VecDeque<LiquidationEntry>,
    pub selected_index: usize,
    /// True until the startup backfill task signals completion via
    /// `LiquidationFeedMsg::BackfillComplete`. Drives the modal header
    /// indicator ("backfilling…" vs "live").
    pub is_backfilling: bool,
}

impl LiquidationFeedView {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(LIQUIDATION_FEED_CAPACITY),
            selected_index: 0,
            is_backfilling: true,
        }
    }

    /// Insert in chronological position so the deque stays newest-first by
    /// `received_at`. Drops the oldest entry once the buffer is full so the
    /// deque never grows unbounded; if the new entry is itself the oldest,
    /// it's dropped immediately.
    pub fn push(&mut self, entry: LiquidationEntry) {
        let pos = self
            .entries
            .iter()
            .position(|e| entry.received_at > e.received_at)
            .unwrap_or(self.entries.len());
        self.entries.insert(pos, entry);
        while self.entries.len() > LIQUIDATION_FEED_CAPACITY {
            self.entries.pop_back();
        }
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

    fn make_entry_at(tag: &str, secs: i64) -> LiquidationEntry {
        LiquidationEntry {
            received_at: DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap(),
            symbol: tag.to_string(),
            asset_id: 0,
            side: None,
            liquidated_trader: String::new(),
            size: 1.0,
            mark_price: 100.0,
            notional: 100.0,
            price_decimals: 2,
            size_decimals: 2,
        }
    }

    #[test]
    fn push_orders_by_received_at_descending_regardless_of_arrival_order() {
        let mut v = LiquidationFeedView::new();
        v.push(make_entry_at("older", 0));
        v.push(make_entry_at("newer", 10));
        v.push(make_entry_at("middle", 5));
        let tags: Vec<&str> = v.entries.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(tags, vec!["newer", "middle", "older"]);
    }

    #[test]
    fn push_drops_oldest_at_capacity() {
        let mut v = LiquidationFeedView::new();
        for i in 0..(LIQUIDATION_FEED_CAPACITY + 5) {
            v.push(make_entry_at(&format!("{:04}", i), i as i64));
        }
        assert_eq!(v.entries.len(), LIQUIDATION_FEED_CAPACITY);
        // Largest i is newest by received_at, so it sits at the front.
        assert_eq!(
            v.entries.front().unwrap().symbol,
            format!("{:04}", LIQUIDATION_FEED_CAPACITY + 4)
        );
        // Five oldest entries (i = 0..=4) were evicted; back of the deque is
        // therefore i = 5.
        assert_eq!(v.entries.back().unwrap().symbol, format!("{:04}", 5));
    }

    #[test]
    fn push_drops_self_if_older_than_full_buffer() {
        let mut v = LiquidationFeedView::new();
        for i in 0..LIQUIDATION_FEED_CAPACITY {
            // i = 0 is oldest, so secs starts at 100 to leave room below.
            v.push(make_entry_at(&format!("{:04}", i), 100 + i as i64));
        }
        v.push(make_entry_at("ancient", 0));
        assert_eq!(v.entries.len(), LIQUIDATION_FEED_CAPACITY);
        assert!(v.entries.iter().all(|e| e.symbol != "ancient"));
    }

    #[test]
    fn move_clamped_to_bounds() {
        let mut v = LiquidationFeedView::new();
        v.push(make_entry_at("a", 0));
        v.push(make_entry_at("b", 1));
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
