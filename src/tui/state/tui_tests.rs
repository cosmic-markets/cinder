//! Tests for top-level TUI state behavior.

use solana_pubkey::Pubkey as PhoenixPubkey;

use super::super::super::data::spline_book::SplineRow;
use super::super::super::data::ParsedSplineData;
use super::super::super::format::pubkey_trader_prefix;
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

fn spline_row(tag: u8, price: f64, _price_end_unused: f64, size: f64) -> SplineRow {
    // These tests don't exercise the crossed-book trim heuristic, so the
    // region-level remaining depth (4th field) doesn't matter — populate it
    // with `size` so each row models a one-tick region.
    (pda_with_prefix(tag), price, size, size)
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
        bid_iceberg_markers: vec![],
        ask_iceberg_markers: vec![],
        best_bid: Some(100.0),
        best_ask: Some(101.0),
        best_bid_size: Some(2.0),
        best_ask_size: Some(4.0),
    });
    s.rebuild_merged_book("BTC", false, None, 2);
    let bids: Vec<f64> = s.merged_book.bid_rows.iter().map(|r| r.price).collect();
    let asks: Vec<f64> = s.merged_book.ask_rows.iter().map(|r| r.price).collect();
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
        bid_iceberg_markers: vec![],
        ask_iceberg_markers: vec![],
        best_bid: Some(100.0),
        best_ask: None,
        best_bid_size: Some(1.0),
        best_ask_size: None,
    });
    s.clob_bids = vec![(100.5, 1.0, "Z".to_string())];
    s.rebuild_merged_book("BTC", false, None, 2);
    assert_eq!(s.merged_book.bid_rows.len(), 1);
    assert!(s
        .merged_book
        .bid_rows
        .iter()
        .all(|r| r.traders.iter().all(|(t, _)| t != "Z")));
}

#[test]
fn rebuild_merged_book_includes_clob_when_show_clob_is_true() {
    let mut s = empty_state();
    s.clob_bids = vec![(100.0, 1.0, "Z".to_string())];
    s.clob_asks = vec![(101.0, 1.0, "Y".to_string())];
    s.rebuild_merged_book("BTC", true, None, 2);
    assert_eq!(s.merged_book.bid_rows.len(), 1);
    assert_eq!(s.merged_book.ask_rows.len(), 1);
    assert_eq!(s.merged_book.bid_rows[0].traders[0].0, "Z");
}

#[test]
fn rebuild_merged_book_combines_traders_at_same_price_level() {
    let mut s = empty_state();
    s.clob_bids = vec![
        (100.0, 1.0, "mmmb".to_string()),
        (100.0, 2.0, "xxxy".to_string()),
        (99.0, 5.0, "qqqq".to_string()),
    ];
    s.rebuild_merged_book("BTC", true, None, 2);
    assert_eq!(s.merged_book.bid_rows.len(), 2);
    let top = &s.merged_book.bid_rows[0];
    assert_eq!(top.price, 100.0);
    assert_eq!(top.size, 3.0);
    let traders: Vec<&str> = top.traders.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(traders, vec!["mmmb", "xxxy"]);
}

#[test]
fn rebuild_merged_book_abstracts_spline_range_to_point_quote() {
    let mut s = empty_state();
    s.last_parsed = Some(ParsedSplineData {
        bid_rows: vec![spline_row(0xA1, 100.0, 95.0, 50.0)],
        ask_rows: vec![],
        bid_iceberg_markers: vec![],
        ask_iceberg_markers: vec![],
        best_bid: Some(100.0),
        best_ask: None,
        best_bid_size: Some(50.0),
        best_ask_size: None,
    });
    s.rebuild_merged_book("BTC", false, None, 2);
    assert_eq!(s.merged_book.bid_rows.len(), 1);
    let row = &s.merged_book.bid_rows[0];
    assert_eq!(row.price, 100.0);
    assert_eq!(row.size, 50.0);
    assert_eq!(row.traders.len(), 1);
    assert_eq!(row.traders[0].1, RowSource::Spline);
}

#[test]
fn rebuild_merged_book_paints_iceberg_marker_on_outer_adjacent_row() {
    // Two ASK regions, both with hidden_take. Region A's worst tick is $80.01
    // (so its marker price is $80.02); region B occupies $80.02 and carries
    // its own marker price $80.03. The expected outcome: $80.01 has NO
    // marker, $80.02 has a marker (from region A), $80.03 is orphaned and
    // silently drops since no row exists at that price.
    let mut s = empty_state();
    s.last_parsed = Some(ParsedSplineData {
        bid_rows: vec![],
        ask_rows: vec![
            spline_row(0xA1, 80.01, 0.0, 5.0),
            spline_row(0xA2, 80.02, 0.0, 7.0),
        ],
        bid_iceberg_markers: vec![],
        ask_iceberg_markers: vec![
            (80.02, pda_with_prefix(0xA1)),
            (80.03, pda_with_prefix(0xA2)),
        ],
        best_bid: None,
        best_ask: Some(80.01),
        best_bid_size: None,
        best_ask_size: Some(5.0),
    });
    s.rebuild_merged_book("BTC", false, None, 2);
    let asks = &s.merged_book.ask_rows;
    let row = |p: f64| asks.iter().find(|r| (r.price - p).abs() < 1e-9).unwrap();
    assert!(!row(80.01).has_hidden_fill);
    assert!(row(80.02).has_hidden_fill);
    // Marker at $80.02 came from the spline owned by 0xA1, so the row picks
    // up that owner's prefix for blue-letter highlighting in the renderer.
    assert_eq!(
        row(80.02).iceberg_trader_prefix.as_deref(),
        Some(pubkey_trader_prefix(&pda_with_prefix(0xA1)).as_str())
    );
    assert_eq!(asks.len(), 2);
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
        subaccount_index: 0,
        order_sequence_number: 1,
        side: TradingSide::Long,
        order_type: "LMT".to_string(),
        price_usd: 99.0,
        price_ticks: 99,
        size_remaining: 1.0,
        initial_size: 1.0,
        reduce_only: false,
        is_stop_loss: false,
        conditional_order_index: None,
        conditional_trigger_direction: None,
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
