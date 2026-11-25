//! Scrollable table modal views: open positions, open orders, and protocol leaderboard.

use phoenix_rise::types::market::MarketStatsUpdate;

use super::super::trading::{
    FullPositionInfo, OrderInfo, TopPositionEntry, position_notional_and_pnl,
};
use super::{clamp_list_index, move_list_down, move_list_up};

/// Open-orders modal state across all markets.
pub struct OrdersView {
    pub orders: Vec<OrderInfo>,
    pub selected_index: usize,
}

impl OrdersView {
    /// Create an empty open-orders view.
    pub fn new() -> Self {
        Self {
            orders: Vec::new(),
            selected_index: 0,
        }
    }

    /// Keep `selected_index` within the current orders list.
    pub fn clamp_index(&mut self) {
        clamp_list_index(&mut self.selected_index, self.orders.len());
    }

    /// Move selection down by one order.
    pub fn move_down(&mut self) {
        move_list_down(&mut self.selected_index, self.orders.len());
    }

    /// Move selection up by one order.
    pub fn move_up(&mut self) {
        move_list_up(&mut self.selected_index);
    }
}

/// Protocol-wide top-positions leaderboard modal state.
pub struct TopPositionsView {
    pub positions: Vec<TopPositionEntry>,
    pub selected_index: usize,
    /// `false` until the first refresh completes (shows loading hint on cold open).
    pub loaded: bool,
}

impl TopPositionsView {
    /// Create an unloaded, empty leaderboard view.
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            selected_index: 0,
            loaded: false,
        }
    }

    /// Keep `selected_index` within the current leaderboard rows.
    pub fn clamp_index(&mut self) {
        clamp_list_index(&mut self.selected_index, self.positions.len());
    }

    /// Move selection down by one row.
    pub fn move_down(&mut self) {
        move_list_down(&mut self.selected_index, self.positions.len());
    }

    /// Move selection up by one row.
    pub fn move_up(&mut self) {
        move_list_up(&mut self.selected_index);
    }
}

/// Open-positions modal state across all markets.
pub struct PositionsView {
    pub positions: Vec<FullPositionInfo>,
    pub selected_index: usize,
}

impl PositionsView {
    /// Create an empty positions view.
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            selected_index: 0,
        }
    }

    /// Sum notional USD across all open positions.
    pub fn aggregate_notional(&self) -> f64 {
        self.positions.iter().map(|p| p.notional).sum()
    }

    /// Sum unrealized PnL USD across all open positions.
    pub fn aggregate_pnl(&self) -> f64 {
        self.positions.iter().map(|p| p.unrealized_pnl).sum()
    }

    /// Refresh notional and unrealized PnL from the latest mark for rows matching `update.symbol`.
    pub fn apply_mark_price(&mut self, update: &MarketStatsUpdate) -> bool {
        let mut any = false;
        for p in &mut self.positions {
            if p.symbol != update.symbol {
                continue;
            }
            any = true;
            let (notional, pnl) =
                position_notional_and_pnl(p.size, p.entry_price, update.mark_price, p.side);
            p.notional = notional;
            p.unrealized_pnl = pnl;
        }
        any
    }

    /// Keep `selected_index` within the current positions list.
    pub fn clamp_index(&mut self) {
        clamp_list_index(&mut self.selected_index, self.positions.len());
    }

    /// Move selection down by one position.
    pub fn move_down(&mut self) {
        move_list_down(&mut self.selected_index, self.positions.len());
    }

    /// Move selection up by one position.
    pub fn move_up(&mut self) {
        move_list_up(&mut self.selected_index);
    }

    /// Symbol of the currently highlighted position, if any.
    pub fn selected_symbol(&self) -> Option<&str> {
        self.positions
            .get(self.selected_index)
            .map(|p| p.symbol.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::trading::TradingSide;
    use super::*;

    fn make_position(symbol: &str, side: TradingSide, size: f64, entry: f64) -> FullPositionInfo {
        FullPositionInfo {
            symbol: symbol.to_string(),
            subaccount_index: 0,
            side,
            size,
            position_size_raw: None,
            entry_price: entry,
            unrealized_pnl: 0.0,
            liquidation_price: None,
            notional: size * entry,
            leverage: None,
        }
    }

    fn make_stat(symbol: &str, mark: f64) -> MarketStatsUpdate {
        MarketStatsUpdate {
            symbol: symbol.to_string(),
            open_interest: 0.0,
            mark_price: mark,
            mid_price: mark,
            oracle_price: mark,
            prev_day_mark_price: mark,
            day_volume_usd: 0.0,
            funding_rate: 0.0,
        }
    }

    #[test]
    fn positions_view_aggregates_and_updates_marks() {
        let mut v = PositionsView::new();
        assert_eq!(v.selected_symbol(), None);
        v.positions = vec![
            make_position("SOL", TradingSide::Long, 2.0, 100.0),
            make_position("BTC", TradingSide::Short, 2.0, 100.0),
        ];

        assert!(v.apply_mark_price(&make_stat("SOL", 110.0)));
        assert!(v.apply_mark_price(&make_stat("BTC", 90.0)));
        assert!(!v.apply_mark_price(&make_stat("ETH", 3000.0)));

        assert!((v.aggregate_notional() - 400.0).abs() < 1e-9);
        assert!((v.aggregate_pnl() - 40.0).abs() < 1e-9);

        v.move_down();
        assert_eq!(v.selected_symbol(), Some("BTC"));
    }

    #[test]
    fn top_positions_starts_unloaded_and_empty() {
        let v = TopPositionsView::new();
        assert!(!v.loaded);
        assert!(v.positions.is_empty());
        assert_eq!(v.selected_index, 0);
    }
}
