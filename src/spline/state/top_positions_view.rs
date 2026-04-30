//! "Top positions on Phoenix" modal state — snapshot of the top-N largest
//! active positions across the protocol, plus UI cursor.

use super::super::trading::TopPositionEntry;

pub struct TopPositionsView {
    pub positions: Vec<TopPositionEntry>,
    pub selected_index: usize,
    /// `true` until the first refresh completes — lets the modal show a
    /// "loading" hint instead of "empty" on cold open.
    pub loaded: bool,
}

impl TopPositionsView {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            selected_index: 0,
            loaded: false,
        }
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
}

#[cfg(test)]
mod tests {
    use super::super::super::trading::TradingSide;
    use super::*;

    fn make_entry() -> TopPositionEntry {
        TopPositionEntry {
            symbol: "SOL".to_string(),
            trader: None,
            trader_display: "abcd…".to_string(),
            side: TradingSide::Long,
            size: 1.0,
            entry_price: 100.0,
            notional: 100.0,
            unrealized_pnl: 0.0,
        }
    }

    #[test]
    fn new_starts_unloaded_and_empty() {
        let v = TopPositionsView::new();
        assert!(!v.loaded);
        assert!(v.positions.is_empty());
        assert_eq!(v.selected_index, 0);
    }

    #[test]
    fn move_down_stops_at_last_row() {
        let mut v = TopPositionsView::new();
        v.positions = vec![make_entry(), make_entry()];
        v.move_down();
        v.move_down();
        v.move_down();
        assert_eq!(v.selected_index, 1);
    }

    #[test]
    fn clamp_index_recovers_from_shrink() {
        let mut v = TopPositionsView::new();
        v.positions = vec![make_entry(), make_entry()];
        v.selected_index = 1;
        v.positions.pop();
        v.clamp_index();
        assert_eq!(v.selected_index, 0);
    }
}
