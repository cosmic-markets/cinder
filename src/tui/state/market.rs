//! Market info and market selector state.

use phoenix_rise::MarketStatsUpdate;

use super::super::math::pct_change_24h;

#[derive(Clone)]
pub struct MarketInfo {
    pub symbol: String,
    pub price: f64,
    pub volume_24h: f64,
    pub open_interest_usd: f64,
    pub max_leverage: f64,
    pub change_24h: f64,
    pub price_decimals: usize,
    pub isolated_only: bool,
}

pub struct MarketSelector {
    pub markets: Vec<MarketInfo>,
    pub selected_index: usize,
}

impl MarketSelector {
    pub fn new(markets: Vec<MarketInfo>) -> Self {
        Self {
            markets,
            selected_index: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.markets.len() {
            self.selected_index += 1;
        }
    }

    pub fn selected_symbol(&self) -> Option<&str> {
        self.markets
            .get(self.selected_index)
            .map(|s| s.symbol.as_str())
    }

    pub fn focus_on(&mut self, symbol: &str) {
        if let Some(idx) = self.markets.iter().position(|m| m.symbol == symbol) {
            self.selected_index = idx;
        }
    }

    /// Display precision for `symbol`'s mark price (tick-derived), or a
    /// conservative default if the market list has not loaded that symbol
    /// yet.
    pub fn price_decimals_for_symbol(&self, symbol: &str) -> usize {
        const FALLBACK: usize = 8;
        self.markets
            .iter()
            .find(|m| m.symbol == symbol)
            .map(|m| m.price_decimals)
            .unwrap_or(FALLBACK)
    }

    /// Append newly discovered markets (skips symbols already present).
    pub fn add_markets(&mut self, new_markets: Vec<MarketInfo>) {
        for m in new_markets {
            if !self
                .markets
                .iter()
                .any(|existing| existing.symbol == m.symbol)
            {
                self.markets.push(m);
            }
        }
        self.sort_by_volume_desc();
    }

    /// Ensure `selected_index` stays in bounds after a list mutation.
    fn clamp_index(&mut self) {
        if !self.markets.is_empty() {
            self.selected_index = self.selected_index.min(self.markets.len() - 1);
        } else {
            self.selected_index = 0;
        }
    }

    /// Re-sort by 24h volume (highest first) and keep the same market selected.
    fn sort_by_volume_desc(&mut self) {
        let selected = self.selected_symbol().map(std::string::String::from);
        self.markets.sort_by(|a, b| {
            b.volume_24h
                .partial_cmp(&a.volume_24h)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(sym) = selected {
            self.focus_on(&sym);
        }
        self.clamp_index();
    }

    /// Update price/volume/change for a single market from a live stat event.
    pub fn update_stat(&mut self, update: &MarketStatsUpdate) {
        let found = if let Some(m) = self.markets.iter_mut().find(|m| m.symbol == update.symbol) {
            m.price = update.mark_price;
            m.volume_24h = update.day_volume_usd;
            m.open_interest_usd = update.open_interest * update.mark_price;
            m.change_24h = pct_change_24h(update.mark_price, update.prev_day_mark_price);
            true
        } else {
            false
        };
        if found {
            self.sort_by_volume_desc();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_market(symbol: &str, volume: f64, price_decimals: usize) -> MarketInfo {
        MarketInfo {
            symbol: symbol.to_string(),
            price: 100.0,
            volume_24h: volume,
            open_interest_usd: 0.0,
            max_leverage: 10.0,
            change_24h: 0.0,
            price_decimals,
            isolated_only: false,
        }
    }

    fn make_stat(symbol: &str, mark: f64, prev: f64, vol: f64, oi: f64) -> MarketStatsUpdate {
        MarketStatsUpdate {
            symbol: symbol.to_string(),
            open_interest: oi,
            mark_price: mark,
            mid_price: mark,
            oracle_price: mark,
            prev_day_mark_price: prev,
            day_volume_usd: vol,
            funding_rate: 0.0,
        }
    }

    #[test]
    fn move_up_at_top_is_a_no_op() {
        let mut s = MarketSelector::new(vec![make_market("SOL", 1.0, 2)]);
        s.move_up();
        assert_eq!(s.selected_index, 0);
    }

    #[test]
    fn move_down_stops_at_last_market() {
        let mut s =
            MarketSelector::new(vec![make_market("SOL", 2.0, 2), make_market("BTC", 1.0, 1)]);
        s.move_down();
        s.move_down();
        s.move_down();
        assert_eq!(s.selected_index, 1);
        assert_eq!(s.selected_symbol(), Some("BTC"));
    }

    #[test]
    fn focus_on_jumps_to_known_symbol() {
        let mut s =
            MarketSelector::new(vec![make_market("SOL", 2.0, 2), make_market("BTC", 1.0, 1)]);
        s.focus_on("BTC");
        assert_eq!(s.selected_symbol(), Some("BTC"));
    }

    #[test]
    fn focus_on_unknown_symbol_is_a_no_op() {
        let mut s = MarketSelector::new(vec![make_market("SOL", 2.0, 2)]);
        s.focus_on("DOES-NOT-EXIST");
        assert_eq!(s.selected_index, 0);
    }

    #[test]
    fn price_decimals_falls_back_when_symbol_missing() {
        let s = MarketSelector::new(vec![make_market("SOL", 1.0, 3)]);
        assert_eq!(s.price_decimals_for_symbol("SOL"), 3);
        assert_eq!(s.price_decimals_for_symbol("BTC"), 8);
    }

    #[test]
    fn add_markets_dedups_and_sorts_by_volume() {
        let mut s = MarketSelector::new(vec![make_market("SOL", 2.0, 2)]);
        s.add_markets(vec![
            make_market("SOL", 99.0, 2), // dup ignored
            make_market("BTC", 5.0, 1),
            make_market("ETH", 1.0, 1),
        ]);
        let symbols: Vec<&str> = s.markets.iter().map(|m| m.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["BTC", "SOL", "ETH"]);
    }

    #[test]
    fn add_markets_keeps_selection_on_same_symbol() {
        let mut s =
            MarketSelector::new(vec![make_market("SOL", 2.0, 2), make_market("ETH", 1.0, 1)]);
        s.focus_on("ETH");
        s.add_markets(vec![make_market("BTC", 99.0, 1)]);
        // BTC sorts first by volume; ETH selection should follow the symbol.
        assert_eq!(s.selected_symbol(), Some("ETH"));
    }

    #[test]
    fn update_stat_writes_fields_and_resorts() {
        let mut s =
            MarketSelector::new(vec![make_market("SOL", 1.0, 2), make_market("BTC", 2.0, 1)]);
        s.update_stat(&make_stat("SOL", 110.0, 100.0, 100.0, 5.0));
        let symbols: Vec<&str> = s.markets.iter().map(|m| m.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["SOL", "BTC"]);
        let sol = s.markets.iter().find(|m| m.symbol == "SOL").unwrap();
        assert_eq!(sol.price, 110.0);
        assert_eq!(sol.volume_24h, 100.0);
        assert!((sol.change_24h - 10.0).abs() < 1e-9);
        assert!((sol.open_interest_usd - 5.0 * 110.0).abs() < 1e-9);
    }

    #[test]
    fn update_stat_is_a_no_op_for_unknown_symbol() {
        let mut s = MarketSelector::new(vec![make_market("SOL", 1.0, 2)]);
        s.update_stat(&make_stat("BTC", 99.0, 100.0, 50.0, 1.0));
        assert_eq!(s.markets[0].price, 100.0);
    }
}
