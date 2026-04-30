//! TuiState: the top-level mutable runtime state for the TUI.

use std::collections::{HashMap, VecDeque};

use chrono::Utc;

use super::super::constants::{MAX_PRICE_HISTORY, MIN_SOL_SPREAD_USD, SOL_SYMBOL};
use super::super::format::pubkey_trader_prefix;
use super::super::gti::GtiCache;
use super::super::parse::ParsedSplineData;
use super::book::{BookRow, ClobLevel, MergedBook, RowSource};
use super::markers::{OrderChartMarker, TradeMarker};
use super::market::{MarketInfo, MarketSelector};
use super::orders_view::OrdersView;
use super::positions_view::PositionsView;
use super::top_positions_view::TopPositionsView;
use super::trading::TradingState;

pub struct TuiState {
    pub price_history: VecDeque<f64>,
    pub market_stats: Option<phoenix_rise::MarketStatsUpdate>,
    /// Last Phoenix CLOB L2 snapshot (bids best-first, asks best-first) for the
    /// active market. Poller filters by symbol before writing these, so
    /// stale rows don't appear during market switches.
    pub clob_bids: Vec<ClobLevel>,
    pub clob_asks: Vec<ClobLevel>,
    /// Spline+CLOB merged view, rebuilt whenever either source updates. This is
    /// what the book table renders.
    pub merged_book: MergedBook,
    pub last_parsed: Option<ParsedSplineData>,
    pub last_slot: u64,
    /// Chart corner `HH:MM:SS`; updated only on the 1s timer in `poller` so
    /// seconds don't drift with feed FPS.
    pub chart_clock_hms: String,
    pub trading: TradingState,
    pub trade_markers: Vec<TradeMarker>,
    pub market_selector: MarketSelector,
    pub positions_view: PositionsView,
    pub orders_view: OrdersView,
    /// Top-N largest positions across the protocol (on-chain ActiveTraderBuffer
    /// scan).
    pub top_positions_view: TopPositionsView,
    /// One chart marker per active-market open order, keyed by `(symbol,
    /// order_sequence_number)`. Kept separate from `orders_view` because it
    /// tracks chart-geometry state (x-coordinate that scrolls with
    /// `price_history`), whereas `orders_view` is pure snapshot data.
    pub order_chart_markers: HashMap<(String, u64), OrderChartMarker>,
    /// Set to the target symbol while a market switch is in-flight. The old
    /// chart/book data stays visible until the first WSS payload arrives for
    /// the new market, at which point this is cleared.
    pub switching_to: Option<String>,
    // Rebuilt only when price_history mutates; avoids per-frame allocation/scan.
    chart_data_cache: Vec<(f64, f64)>,
    price_bounds_cache: (f64, f64),
    // Running min/max over `price_history` (no margin). Sentinel ±inf means "empty".
    // Lets `push_price` fold new samples in O(1) and rescan only when the popped sample
    // was an extremum.
    chart_min: f64,
    chart_max: f64,
}

impl TuiState {
    pub fn new(market_list: Vec<MarketInfo>) -> Self {
        Self {
            price_history: VecDeque::with_capacity(MAX_PRICE_HISTORY),
            market_stats: None,
            clob_bids: Vec::new(),
            clob_asks: Vec::new(),
            merged_book: MergedBook::default(),
            last_parsed: None,
            last_slot: 0,
            chart_clock_hms: Utc::now().format("%H:%M:%S").to_string(),
            trading: TradingState::new(),
            trade_markers: Vec::new(),
            market_selector: MarketSelector::new(market_list),
            positions_view: PositionsView::new(),
            orders_view: OrdersView::new(),
            top_positions_view: TopPositionsView::new(),
            order_chart_markers: HashMap::new(),
            switching_to: None,
            chart_data_cache: Vec::with_capacity(MAX_PRICE_HISTORY),
            price_bounds_cache: (0.0, 1.0),
            chart_min: f64::INFINITY,
            chart_max: f64::NEG_INFINITY,
        }
    }

    /// Marks a market switch in-flight. Old chart/book data stays visible
    /// until [`complete_market_switch`](Self::complete_market_switch) is called
    /// when the first WSS payload for the new market arrives.
    pub fn begin_market_switch(&mut self, target_symbol: &str) {
        self.switching_to = Some(target_symbol.to_string());
        self.clob_bids.clear();
        self.clob_asks.clear();
        self.merged_book = MergedBook::default();
        // Clear per-market trading state that shouldn't carry over.
        self.trading.position = None;
        self.trading.order_kind = super::super::trading::OrderKind::Market;
    }

    /// Rebuild [`merged_book`](Self::merged_book) from `last_parsed` (splines)
    /// and `clob_*` (CLOB L2). Call after either source changes. Rows are
    /// sorted best-first on each side. When `show_clob` is false, CLOB rows
    /// are omitted and only spline rows are included.
    ///
    /// `gti_cache` resolves each spline's trader PDA to its wallet authority
    /// prefix so spline rows share the same identity namespace as CLOB rows
    /// (where the user recognises their wallet). Rows whose PDA isn't in the
    /// cache yet fall back to the PDA prefix until the next refresh.
    pub fn rebuild_merged_book(
        &mut self,
        symbol: &str,
        show_clob: bool,
        gti_cache: Option<&GtiCache>,
    ) {
        let mut bid_rows: Vec<BookRow> = Vec::new();
        let mut ask_rows: Vec<BookRow> = Vec::new();

        let resolve_spline_trader = |pda: &solana_pubkey::Pubkey| -> String {
            let authority = gti_cache.and_then(|c| c.resolve_pda(pda));
            match authority {
                Some(auth) => pubkey_trader_prefix(&auth),
                None => pubkey_trader_prefix(pda),
            }
        };

        if let Some(parsed) = self.last_parsed.as_ref() {
            for r in &parsed.bid_rows {
                bid_rows.push(BookRow {
                    source: RowSource::Spline,
                    trader: resolve_spline_trader(&r.0),
                    price_start: r.1,
                    price_end: r.2,
                    size: r.5,
                });
            }
            for r in &parsed.ask_rows {
                ask_rows.push(BookRow {
                    source: RowSource::Spline,
                    trader: resolve_spline_trader(&r.0),
                    price_start: r.1,
                    price_end: r.2,
                    size: r.5,
                });
            }
        }
        if show_clob {
            for (price, qty, trader) in &self.clob_bids {
                bid_rows.push(BookRow {
                    source: RowSource::Clob,
                    trader: trader.clone(),
                    price_start: *price,
                    price_end: *price,
                    size: *qty,
                });
            }
            for (price, qty, trader) in &self.clob_asks {
                ask_rows.push(BookRow {
                    source: RowSource::Clob,
                    trader: trader.clone(),
                    price_start: *price,
                    price_end: *price,
                    size: *qty,
                });
            }
        }

        bid_rows.sort_by(|a, b| {
            b.price_start
                .partial_cmp(&a.price_start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ask_rows.sort_by(|a, b| {
            a.price_start
                .partial_cmp(&b.price_start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best_bid = bid_rows.first().map(|r| r.price_start);
        let best_ask = ask_rows.first().map(|r| r.price_start);
        let spread = match (best_bid, best_ask) {
            (Some(b), Some(a)) => {
                let raw = (a - b).max(0.0);
                Some(if symbol == SOL_SYMBOL {
                    raw.max(MIN_SOL_SPREAD_USD)
                } else {
                    raw
                })
            }
            _ => None,
        };

        self.merged_book = MergedBook {
            bid_rows,
            ask_rows,
            best_bid,
            best_ask,
            spread,
        };
    }

    /// Called once the first new-market data arrives. Flushes stale chart
    /// data so the new market starts with a clean slate.
    pub fn complete_market_switch(&mut self) {
        self.switching_to = None;
        self.price_history.clear();
        self.trade_markers.clear();
        // Old market's order markers don't belong on the new chart (and their x would
        // be stale).
        self.order_chart_markers.clear();
        self.last_parsed = None;
        self.market_stats = None;
        self.last_slot = 0;
        self.chart_clock_hms = Utc::now().format("%H:%M:%S").to_string();
        self.chart_data_cache.clear();
        self.price_bounds_cache = (0.0, 1.0);
        self.chart_min = f64::INFINITY;
        self.chart_max = f64::NEG_INFINITY;
    }

    pub fn push_price(&mut self, mid: f64) {
        let popped = if self.price_history.len() >= MAX_PRICE_HISTORY {
            self.price_history.pop_front()
        } else {
            None
        };
        self.price_history.push_back(mid);

        if popped.is_some() {
            for m in &mut self.trade_markers {
                m.x -= 1.0;
            }
            self.trade_markers.retain(|m| m.x >= 0.0);
            // Order markers scroll with the chart too, but we do NOT prune them when x < 0:
            // the order is still live on the book and needs its square/letter re-rendered
            // if the y-range shifts back into view. Chart widget clips
            // out-of-bound x internally.
            for marker in self.order_chart_markers.values_mut() {
                marker.x -= 1.0;
            }
            // Rebuild from price_history (already contains new price) rather than
            // shifting all y-values left in place.
            self.chart_data_cache.clear();
            self.chart_data_cache.extend(
                self.price_history
                    .iter()
                    .enumerate()
                    .map(|(i, &y)| (i as f64, y)),
            );
        } else {
            let new_x = self.price_history.len().saturating_sub(1) as f64;
            self.chart_data_cache.push((new_x, mid));
        }

        // Rescan only when the popped sample was an extremum (removing it may have
        // widened the band) or when running bounds are uninitialized. Otherwise
        // fold `mid` in O(1).
        let rescan = matches!(popped, Some(p) if p <= self.chart_min || p >= self.chart_max)
            || !self.chart_min.is_finite()
            || !self.chart_max.is_finite();
        if rescan {
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            for &p in &self.price_history {
                if p < min {
                    min = p;
                }
                if p > max {
                    max = p;
                }
            }
            self.chart_min = min;
            self.chart_max = max;
        } else {
            if mid < self.chart_min {
                self.chart_min = mid;
            }
            if mid > self.chart_max {
                self.chart_max = mid;
            }
        }

        if self.price_history.is_empty() {
            self.price_bounds_cache = (0.0, 1.0);
        } else {
            let range = self.chart_max - self.chart_min;
            let mid_val = (self.chart_min + self.chart_max) / 2.0;
            let margin = if range > 0.0 {
                range * 0.05
            } else {
                mid_val.abs() * 0.0005
            };
            self.price_bounds_cache = (self.chart_min - margin, self.chart_max + margin);
        }
    }

    /// Reconcile `order_chart_markers` against the current WS snapshot for
    /// `active_symbol`. New (symbol, seq) keys are inserted at the current
    /// right-edge x; keys no longer present in the snapshot are removed
    /// (fill / cancel). Price is refreshed from the snapshot in
    /// case the order was amended.
    pub fn sync_order_chart_markers(&mut self, active_symbol: &str) {
        let current_x = self.price_history.len().saturating_sub(1) as f64;
        let mut seen = std::collections::HashSet::<u64>::new();

        for o in self
            .orders_view
            .orders
            .iter()
            .filter(|o| o.symbol == active_symbol && o.price_usd > 0.0)
        {
            seen.insert(o.order_sequence_number);
            let key = (o.symbol.clone(), o.order_sequence_number);
            self.order_chart_markers
                .entry(key)
                .and_modify(|m| m.price = o.price_usd)
                .or_insert(OrderChartMarker {
                    x: current_x,
                    price: o.price_usd,
                });
        }

        self.order_chart_markers
            .retain(|key, _| key.0 != active_symbol || seen.contains(&key.1));
    }

    pub fn add_trade_marker(&mut self, is_buy: bool) {
        let x = self.price_history.len().saturating_sub(1) as f64;
        if let Some(&y) = self.price_history.back() {
            self.trade_markers.push(TradeMarker { x, y, is_buy });
        }
    }

    pub fn chart_data(&self) -> &[(f64, f64)] {
        &self.chart_data_cache
    }

    pub fn price_bounds(&self) -> (f64, f64) {
        self.price_bounds_cache
    }
}

#[cfg(test)]
mod tests {
    use solana_pubkey::Pubkey as PhoenixPubkey;

    use super::super::super::parse::{ParsedSplineData, SplineRow};
    use super::super::super::trading::{OrderInfo, OrderKind, TradingSide};
    use super::*;

    fn empty_state() -> TuiState {
        TuiState::new(Vec::new())
    }

    // Build a PDA pubkey whose base58 encoding starts with `tag` so tests can
    // assert on the displayed prefix without standing up a GTI cache.
    fn pda_with_prefix(tag: u8) -> PhoenixPubkey {
        PhoenixPubkey::from([tag; 32])
    }

    fn spline_row(tag: u8, price_start: f64, price_end: f64, size: f64) -> SplineRow {
        (pda_with_prefix(tag), price_start, price_end, 0.0, 0.0, size)
    }

    #[test]
    fn push_price_grows_history_and_updates_bounds() {
        let mut s = empty_state();
        s.push_price(100.0);
        s.push_price(110.0);
        s.push_price(105.0);
        assert_eq!(s.chart_data().len(), 3);
        let (lo, hi) = s.price_bounds();
        assert!(
            lo < 100.0 && hi > 110.0,
            "bounds {lo}..{hi} should bracket samples with margin"
        );
    }

    #[test]
    fn push_price_caches_xy_pairs_with_zero_based_x() {
        let mut s = empty_state();
        s.push_price(50.0);
        s.push_price(60.0);
        let data = s.chart_data();
        assert_eq!(data[0], (0.0, 50.0));
        assert_eq!(data[1], (1.0, 60.0));
    }

    #[test]
    fn push_price_evicts_oldest_when_history_is_full() {
        let mut s = empty_state();
        for i in 0..MAX_PRICE_HISTORY {
            s.push_price(100.0 + i as f64);
        }
        assert_eq!(s.price_history.len(), MAX_PRICE_HISTORY);
        // One past the cap: the lowest sample falls off.
        s.push_price(99_999.0);
        assert_eq!(s.price_history.len(), MAX_PRICE_HISTORY);
        assert_eq!(s.chart_data().len(), MAX_PRICE_HISTORY);
        // The evicted minimum (100.0) is gone from `price_history`.
        assert_eq!(s.price_history.front().copied(), Some(101.0));
        // Bounds bracket the new max with positive margin.
        let (_, hi) = s.price_bounds();
        assert!(hi > 99_999.0, "hi {hi} should bracket new max with margin");
    }

    #[test]
    fn push_price_scrolls_trade_markers_left_and_drops_off_screen() {
        let mut s = empty_state();
        s.push_price(100.0);
        s.add_trade_marker(true);
        // Fill exactly to the cap so the next push evicts the marker's column.
        for i in 1..MAX_PRICE_HISTORY {
            s.push_price(100.0 + i as f64);
        }
        s.push_price(200.0);
        assert!(
            s.trade_markers.is_empty(),
            "marker at x=0 should drop after first eviction"
        );
    }

    #[test]
    fn rebuild_merged_book_sorts_each_side_best_first() {
        let mut s = empty_state();
        s.last_parsed = Some(ParsedSplineData {
            bid_rows: vec![
                spline_row(0xA1, 99.0, 99.5, 1.0),
                spline_row(0xA2, 100.0, 100.5, 2.0),
            ],
            ask_rows: vec![
                spline_row(0xA3, 102.0, 102.5, 3.0),
                spline_row(0xA4, 101.0, 101.5, 4.0),
            ],
            best_bid: Some(100.0),
            best_ask: Some(101.0),
        });
        s.rebuild_merged_book("BTC", false, None);
        let bids: Vec<f64> = s
            .merged_book
            .bid_rows
            .iter()
            .map(|r| r.price_start)
            .collect();
        let asks: Vec<f64> = s
            .merged_book
            .ask_rows
            .iter()
            .map(|r| r.price_start)
            .collect();
        assert_eq!(bids, vec![100.0, 99.0]);
        assert_eq!(asks, vec![101.0, 102.0]);
        assert_eq!(s.merged_book.best_bid, Some(100.0));
        assert_eq!(s.merged_book.best_ask, Some(101.0));
        assert!(s.merged_book.spread.unwrap() > 0.0);
    }

    #[test]
    fn rebuild_merged_book_omits_clob_when_show_clob_is_false() {
        let mut s = empty_state();
        s.last_parsed = Some(ParsedSplineData {
            bid_rows: vec![spline_row(0xA1, 100.0, 100.5, 1.0)],
            ask_rows: vec![],
            best_bid: Some(100.0),
            best_ask: None,
        });
        s.clob_bids = vec![(100.5, 1.0, "Z".to_string())];
        s.rebuild_merged_book("BTC", false, None);
        assert_eq!(s.merged_book.bid_rows.len(), 1);
        assert!(s.merged_book.bid_rows.iter().all(|r| r.trader != "Z"));
    }

    #[test]
    fn rebuild_merged_book_includes_clob_when_show_clob_is_true() {
        let mut s = empty_state();
        s.clob_bids = vec![(100.0, 1.0, "Z".to_string())];
        s.clob_asks = vec![(101.0, 1.0, "Y".to_string())];
        s.rebuild_merged_book("BTC", true, None);
        assert_eq!(s.merged_book.bid_rows.len(), 1);
        assert_eq!(s.merged_book.ask_rows.len(), 1);
        assert_eq!(s.merged_book.bid_rows[0].trader, "Z");
    }

    #[test]
    fn begin_market_switch_sets_pending_and_clears_book() {
        let mut s = empty_state();
        s.clob_bids = vec![(1.0, 1.0, "T".to_string())];
        s.trading.order_kind = OrderKind::Limit { price: 1.0 };
        s.begin_market_switch("BTC");
        assert_eq!(s.switching_to.as_deref(), Some("BTC"));
        assert!(s.clob_bids.is_empty());
        assert!(matches!(s.trading.order_kind, OrderKind::Market));
    }

    #[test]
    fn complete_market_switch_clears_chart_state() {
        let mut s = empty_state();
        s.push_price(100.0);
        s.push_price(110.0);
        s.add_trade_marker(true);
        s.begin_market_switch("BTC");
        s.complete_market_switch();
        assert_eq!(s.switching_to, None);
        assert!(s.price_history.is_empty());
        assert!(s.trade_markers.is_empty());
        assert!(s.chart_data().is_empty());
    }

    #[test]
    fn sync_order_chart_markers_inserts_new_and_drops_filled() {
        let mut s = empty_state();
        s.push_price(100.0);
        s.orders_view.orders = vec![OrderInfo {
            symbol: "SOL".to_string(),
            order_sequence_number: 1,
            side: TradingSide::Long,
            order_type: "LMT".to_string(),
            price_usd: 99.0,
            price_ticks: 99,
            size_remaining: 1.0,
            initial_size: 1.0,
            reduce_only: false,
            is_stop_loss: false,
        }];
        s.sync_order_chart_markers("SOL");
        assert_eq!(s.order_chart_markers.len(), 1);
        let marker = s.order_chart_markers.values().next().unwrap();
        assert_eq!(marker.price, 99.0);

        // Order is gone (filled / cancelled) → marker is removed for the active symbol.
        s.orders_view.orders.clear();
        s.sync_order_chart_markers("SOL");
        assert!(s.order_chart_markers.is_empty());
    }
}
