//! TuiState: the top-level mutable runtime state for the TUI.

use std::collections::{HashMap, VecDeque};

use chrono::Utc;

use super::super::constants::{MAX_PRICE_HISTORY, MIN_SOL_SPREAD_USD, SOL_SYMBOL};
use super::super::data::GtiCache;
use super::super::data::ParsedSplineData;
use super::super::format::pubkey_trader_prefix;
use super::book::{BookRow, ClobLevel, MergedBook, RowSource};
use super::liquidation_feed_view::LiquidationFeedView;
use super::markers::{OrderChartMarker, TradeMarker};
use super::market::{MarketInfo, MarketSelector};
use super::orders_view::OrdersView;
use super::position_leaderboard_view::TopPositionsView;
use super::positions_view::PositionsView;
use super::trade_panel::TradingState;

pub struct TuiState {
    pub price_history: VecDeque<f64>,
    pub market_stats: Option<phoenix_rise::MarketStatsUpdate>,
    /// Most-recent `MarketStatsUpdate` per symbol. Populated for every market
    /// the stats stream emits, not just the active one, so a market switch can
    /// seed the header instantly instead of flashing "Waiting for market
    /// data…" until the next push for the new market arrives.
    pub market_stats_cache: HashMap<String, phoenix_rise::MarketStatsUpdate>,
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
    /// Live liquidation feed: most-recent `LiquidationEvent`s decoded from
    /// inner instructions on Phoenix Eternal txs.
    pub liquidation_feed_view: LiquidationFeedView,
    /// One chart marker per active-market open order, keyed by `(symbol,
    /// subaccount_index, order_sequence_number)`. Kept separate from `orders_view` because it
    /// tracks chart-geometry state (x-coordinate that scrolls with
    /// `price_history`), whereas `orders_view` is pure snapshot data.
    pub order_chart_markers: HashMap<(String, u8, u64), OrderChartMarker>,
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
            market_stats_cache: HashMap::new(),
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
            liquidation_feed_view: LiquidationFeedView::new(),
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
    ///
    /// `merged_book` is intentionally not reset — the user has asked to keep
    /// the previous market's order book visible during the switch rather than
    /// flashing an empty book. `clob_*` are cleared so the next rebuild after
    /// the switch completes doesn't carry stale CLOB rows from the prior
    /// market alongside fresh spline rows; while `switching_to` is set,
    /// [`rebuild_merged_book`](Self::rebuild_merged_book) is a no-op so any
    /// CLOB writes that arrive mid-switch don't pollute the stale view.
    pub fn begin_market_switch(&mut self, target_symbol: &str) {
        self.switching_to = Some(target_symbol.to_string());
        self.clob_bids.clear();
        self.clob_asks.clear();
        // Seed the header from the cache so the user doesn't see the old
        // market's numbers (or "Waiting for market data…") under the new
        // symbol. Cache miss → None, which is the same fallback as before.
        self.market_stats = self.market_stats_cache.get(target_symbol).cloned();
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
        price_decimals: usize,
    ) {
        // Mid-switch: keep the prior merged book on screen. CLOB writes that
        // arrive before the first spline payload (or vice versa) would
        // otherwise produce a mixed-source book under the new symbol — visibly
        // wrong. `complete_market_switch` clears `switching_to`, after which
        // the next call here renders fresh data.
        if self.switching_to.is_some() {
            return;
        }
        let resolve_spline_trader = |pda: &solana_pubkey::Pubkey| -> String {
            let authority = gti_cache.and_then(|c| c.resolve_pda(pda));
            match authority {
                Some(auth) => pubkey_trader_prefix(&auth),
                None => pubkey_trader_prefix(pda),
            }
        };

        // Per-side raw quotes before grouping. Splines collapse to a single
        // point at their most aggressive price (price_start of the region) so
        // the rendered book reads like a normal CLOB. The trailing bool is
        // the spline iceberg-hit flag; CLOB rows don't have hidden fills so
        // they always carry `false`.
        let mut raw_bids: Vec<(f64, f64, String, RowSource, bool)> = Vec::new();
        let mut raw_asks: Vec<(f64, f64, String, RowSource, bool)> = Vec::new();

        if let Some(parsed) = self.last_parsed.as_ref() {
            for r in &parsed.bid_rows {
                raw_bids.push((r.1, r.2, resolve_spline_trader(&r.0), RowSource::Spline, r.3));
            }
            for r in &parsed.ask_rows {
                raw_asks.push((r.1, r.2, resolve_spline_trader(&r.0), RowSource::Spline, r.3));
            }
        }
        if show_clob {
            for (price, qty, trader) in &self.clob_bids {
                raw_bids.push((*price, *qty, trader.clone(), RowSource::Clob, false));
            }
            for (price, qty, trader) in &self.clob_asks {
                raw_asks.push((*price, *qty, trader.clone(), RowSource::Clob, false));
            }
        }

        let bid_rows = group_by_price(raw_bids, true, price_decimals);
        let ask_rows = group_by_price(raw_asks, false, price_decimals);

        let best_bid = bid_rows.first().map(|r| r.price);
        let best_ask = ask_rows.first().map(|r| r.price);
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
        // Note: `market_stats` is intentionally not cleared here — it was
        // already seeded from the cache in `begin_market_switch` and live
        // stat updates keep refreshing it. Clearing here would cause a
        // visible "Waiting for market data…" flash between the first spline
        // payload and the next stats push.
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
    /// `active_symbol`. New (symbol, subaccount, seq) keys are inserted at the current
    /// right-edge x; keys no longer present in the snapshot are removed
    /// (fill / cancel). Price is refreshed from the snapshot in
    /// case the order was amended.
    pub fn sync_order_chart_markers(&mut self, active_symbol: &str) {
        let current_x = self.price_history.len().saturating_sub(1) as f64;
        let mut seen = std::collections::HashSet::<(u8, u64)>::new();

        for o in self
            .orders_view
            .orders
            .iter()
            .filter(|o| o.symbol == active_symbol && o.price_usd > 0.0)
        {
            let marker_id = (o.subaccount_index, o.order_sequence_number);
            seen.insert(marker_id);
            let key = (
                o.symbol.clone(),
                o.subaccount_index,
                o.order_sequence_number,
            );
            self.order_chart_markers
                .entry(key)
                .and_modify(|m| m.price = o.price_usd)
                .or_insert(OrderChartMarker {
                    x: current_x,
                    price: o.price_usd,
                });
        }

        self.order_chart_markers
            .retain(|key, _| key.0 != active_symbol || seen.contains(&(key.1, key.2)));
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

/// Collapse `(price, size, trader, source)` quotes into one [`BookRow`] per
/// distinct price, summing sizes and concatenating traders. Sorted best-first
/// (descending for bids, ascending for asks).
///
/// Grouping key is the price rounded to `price_decimals` (the market's tick
/// precision). f64 bit equality isn't enough on its own: a spline price is
/// computed as `mid - ticks_to_price(offset)`, which can differ in the last
/// ULP from the same tick reached directly via `ticks_to_price`, so a CLOB
/// and a spline quote at the same tick would otherwise render as two rows.
fn group_by_price(
    raw: Vec<(f64, f64, String, RowSource, bool)>,
    is_bid: bool,
    price_decimals: usize,
) -> Vec<BookRow> {
    let scale = 10_f64.powi(price_decimals as i32);
    let mut by_price: Vec<(i64, BookRow)> = Vec::new();
    for (price, size, trader, source, has_hidden_fill) in raw {
        let key = (price * scale).round() as i64;
        match by_price.iter_mut().find(|(k, _)| *k == key) {
            Some((_, row)) => {
                row.size += size;
                row.traders.push((trader, source));
                row.has_hidden_fill |= has_hidden_fill;
            }
            None => {
                by_price.push((
                    key,
                    BookRow {
                        price,
                        size,
                        traders: vec![(trader, source)],
                        has_hidden_fill,
                    },
                ));
            }
        }
    }
    let mut rows: Vec<BookRow> = by_price.into_iter().map(|(_, r)| r).collect();
    if is_bid {
        rows.sort_by(|a, b| {
            b.price
                .partial_cmp(&a.price)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    } else {
        rows.sort_by(|a, b| {
            a.price
                .partial_cmp(&b.price)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    rows
}

#[cfg(test)]
#[path = "tui_tests.rs"]
mod tests;
