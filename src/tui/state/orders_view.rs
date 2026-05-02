//! Open-orders modal state — list of working orders across all markets plus
//! the cursor used for keyboard navigation.

use super::super::trading::OrderInfo;

pub struct OrdersView {
    pub orders: Vec<OrderInfo>,
    pub selected_index: usize,
}

impl OrdersView {
    pub fn new() -> Self {
        Self {
            orders: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn clamp_index(&mut self) {
        if !self.orders.is_empty() {
            self.selected_index = self.selected_index.min(self.orders.len() - 1);
        } else {
            self.selected_index = 0;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.orders.len() {
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

    fn make_order(symbol: &str) -> OrderInfo {
        OrderInfo {
            symbol: symbol.to_string(),
            subaccount_index: 0,
            order_sequence_number: 1,
            side: TradingSide::Long,
            order_type: "LMT".to_string(),
            price_usd: 100.0,
            price_ticks: 100,
            size_remaining: 1.0,
            initial_size: 1.0,
            reduce_only: false,
            is_stop_loss: false,
            conditional_order_index: None,
            conditional_trigger_direction: None,
        }
    }

    #[test]
    fn empty_view_navigation_is_a_no_op() {
        let mut v = OrdersView::new();
        v.move_up();
        v.move_down();
        assert_eq!(v.selected_index, 0);
    }

    #[test]
    fn move_down_stops_at_last_row() {
        let mut v = OrdersView::new();
        v.orders = vec![make_order("SOL"), make_order("BTC")];
        v.move_down();
        v.move_down();
        v.move_down();
        assert_eq!(v.selected_index, 1);
    }

    #[test]
    fn clamp_index_recovers_from_shrink() {
        let mut v = OrdersView::new();
        v.orders = vec![make_order("SOL"), make_order("BTC")];
        v.selected_index = 1;
        v.orders.pop();
        v.clamp_index();
        assert_eq!(v.selected_index, 0);
    }

    #[test]
    fn clamp_index_resets_when_empty() {
        let mut v = OrdersView::new();
        v.selected_index = 4;
        v.clamp_index();
        assert_eq!(v.selected_index, 0);
    }
}
