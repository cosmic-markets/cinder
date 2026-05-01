//! Rendering helpers used by the runtime loop.

use std::io::Stdout;

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use super::super::config::SplineConfig;
use super::super::state::TuiState;
use super::super::ui;

pub(super) fn redraw_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &TuiState,
    cfg: &SplineConfig,
    rpc_host: &str,
) {
    // Paint if either source has produced rows; merged_book also reflects CLOB-only
    // state.
    let has_rows = !state.merged_book.bid_rows.is_empty() || !state.merged_book.ask_rows.is_empty();
    if state.last_parsed.is_none() && !has_rows {
        return;
    }
    let chart_data = state.chart_data();
    let (y_min, y_max) = state.price_bounds();
    let _ = terminal.draw(|f| {
        ui::render_frame(
            f,
            chart_data,
            y_min,
            y_max,
            cfg,
            &state.merged_book,
            state.last_slot,
            &state.market_stats,
            &state.chart_clock_hms,
            &state.trading,
            &state.trade_markers,
            &state.market_selector,
            &state.positions_view,
            &state.orders_view,
            &state.top_positions_view,
            &state.liquidation_feed_view,
            &state.order_chart_markers,
            rpc_host,
            &state.switching_to,
        );
    });
}

pub(super) fn redraw_tui_force(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &TuiState,
    cfg: &SplineConfig,
    rpc_host: &str,
) {
    let chart_data = state.chart_data();
    let (y_min, y_max) = state.price_bounds();
    let _ = terminal.draw(|f| {
        ui::render_frame(
            f,
            chart_data,
            y_min,
            y_max,
            cfg,
            &state.merged_book,
            state.last_slot,
            &state.market_stats,
            &state.chart_clock_hms,
            &state.trading,
            &state.trade_markers,
            &state.market_selector,
            &state.positions_view,
            &state.orders_view,
            &state.top_positions_view,
            &state.liquidation_feed_view,
            &state.order_chart_markers,
            rpc_host,
            &state.switching_to,
        );
    });
}
