//! Positions modal state — list of open positions across all markets plus
//! the cursor used for keyboard navigation.

use phoenix_rise::MarketStatsUpdate;

use super::super::trading::{FullPositionInfo, TradingSide};

pub struct PositionsView {
    pub positions: Vec<FullPositionInfo>,
    pub selected_index: usize,
}

impl PositionsView {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn aggregate_notional(&self) -> f64 {
        self.positions.iter().map(|p| p.notional).sum()
    }

    pub fn aggregate_pnl(&self) -> f64 {
        self.positions.iter().map(|p| p.unrealized_pnl).sum()
    }

    /// Refresh notional and unrealized PnL from the latest mark for rows
    /// matching `update.symbol`. Called from the market-stats feed so the
    /// Positions modal tracks live marks between HTTP polls. Returns true if
    /// any row was updated.
    pub fn apply_mark_price(&mut self, update: &MarketStatsUpdate) -> bool {
        let mark = update.mark_price;
        let mut any = false;
        for p in self.positions.iter_mut() {
            if p.symbol != update.symbol {
                continue;
            }
            any = true;
            p.notional = p.size * mark;
            p.unrealized_pnl = match p.side {
                TradingSide::Long => p.size * (mark - p.entry_price),
                TradingSide::Short => p.size * (p.entry_price - mark),
            };
        }
        any
    }

    pub fn clamp_index(&mut self) {
        if !self.positions.is_empty() {
            self.selected_index = self.selected_index.min(self.positions.len() - 1);
        } else {
            self.selected_index = 0;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.positions.len() {
            self.selected_index += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn selected_symbol(&self) -> Option<&str> {
        self.positions
            .get(self.selected_index)
            .map(|p| p.symbol.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_position(symbol: &str, side: TradingSide, size: f64, entry: f64) -> FullPositionInfo {
        FullPositionInfo {
            symbol: symbol.to_string(),
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

    fn make_view(positions: Vec<FullPositionInfo>) -> PositionsView {
        let mut v = PositionsView::new();
        v.positions = positions;
        v
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
    fn empty_view_has_no_selection() {
        let v = PositionsView::new();
        assert_eq!(v.selected_symbol(), None);
        assert_eq!(v.aggregate_pnl(), 0.0);
        assert_eq!(v.aggregate_notional(), 0.0);
    }

    #[test]
    fn move_up_at_top_is_a_no_op() {
        let mut v = make_view(vec![make_position("SOL", TradingSide::Long, 1.0, 100.0)]);
        v.move_up();
        assert_eq!(v.selected_index, 0);
    }

    #[test]
    fn move_down_stops_at_last_row() {
        let mut v = make_view(vec![
            make_position("SOL", TradingSide::Long, 1.0, 100.0),
            make_position("BTC", TradingSide::Short, 0.1, 50_000.0),
        ]);
        v.move_down();
        v.move_down();
        v.move_down();
        assert_eq!(v.selected_index, 1);
        assert_eq!(v.selected_symbol(), Some("BTC"));
    }

    #[test]
    fn clamp_index_recovers_from_shrink() {
        let mut v = make_view(vec![
            make_position("SOL", TradingSide::Long, 1.0, 100.0),
            make_position("BTC", TradingSide::Short, 0.1, 50_000.0),
        ]);
        v.selected_index = 1;
        v.positions.pop();
        v.clamp_index();
        assert_eq!(v.selected_index, 0);
    }

    #[test]
    fn clamp_index_resets_when_empty() {
        let mut v = PositionsView::new();
        v.selected_index = 5;
        v.clamp_index();
        assert_eq!(v.selected_index, 0);
    }

    #[test]
    fn aggregates_sum_across_rows() {
        let mut v = make_view(vec![
            make_position("SOL", TradingSide::Long, 2.0, 100.0),
            make_position("BTC", TradingSide::Short, 0.5, 50_000.0),
        ]);
        v.positions[0].unrealized_pnl = 10.0;
        v.positions[1].unrealized_pnl = -5.0;
        assert_eq!(v.aggregate_pnl(), 5.0);
        assert_eq!(v.aggregate_notional(), 2.0 * 100.0 + 0.5 * 50_000.0);
    }

    #[test]
    fn apply_mark_price_updates_long_pnl_and_notional() {
        let mut v = make_view(vec![make_position("SOL", TradingSide::Long, 2.0, 100.0)]);
        let updated = v.apply_mark_price(&make_stat("SOL", 110.0));
        assert!(updated);
        assert_eq!(v.positions[0].notional, 220.0);
        assert!((v.positions[0].unrealized_pnl - 20.0).abs() < 1e-9);
    }

    #[test]
    fn apply_mark_price_handles_short_pnl_sign() {
        let mut v = make_view(vec![make_position("SOL", TradingSide::Short, 2.0, 100.0)]);
        let updated = v.apply_mark_price(&make_stat("SOL", 90.0));
        assert!(updated);
        // Short profit when price falls.
        assert!((v.positions[0].unrealized_pnl - 20.0).abs() < 1e-9);
    }

    #[test]
    fn apply_mark_price_skips_unrelated_symbols() {
        let mut v = make_view(vec![make_position("SOL", TradingSide::Long, 1.0, 100.0)]);
        let updated = v.apply_mark_price(&make_stat("BTC", 60_000.0));
        assert!(!updated);
        // Original notional / pnl untouched.
        assert_eq!(v.positions[0].notional, 100.0);
        assert_eq!(v.positions[0].unrealized_pnl, 0.0);
    }
}
