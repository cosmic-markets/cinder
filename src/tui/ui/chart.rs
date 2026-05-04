//! Price chart widget with trade markers and order level overlays.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph};
use ratatui::{symbols, Frame};

use super::super::format::{fmt_price, fmt_size};
use super::super::i18n::strings;
use super::super::state::{OrderChartMarker, TradeMarker};
use super::super::trading::{OrderInfo, TradingSide};

pub(super) fn render_price_chart(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    chart_data: &[(f64, f64)],
    y_min: f64,
    y_max: f64,
    trade_markers: &[TradeMarker],
    orders: &[OrderInfo],
    order_chart_markers: &std::collections::HashMap<(String, u8, u64), OrderChartMarker>,
    symbol: &str,
    price_decimals: usize,
    chart_clock_hms: &str,
) {
    let current_price = chart_data
        .last()
        .map(|&(_, p)| format!(" ${} ", fmt_price(p, price_decimals)))
        .unwrap_or_default();

    let title_line = Line::from(vec![
        Span::styled(
            format!(" {}", strings().microprice_ema),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(current_price, Style::default().fg(Color::White)),
    ]);
    let clock_title = Line::from(vec![Span::styled(
        format!(" {} ", chart_clock_hms),
        Style::default().fg(Color::DarkGray),
    )])
    .alignment(Alignment::Right);

    let x_max = (chart_data.len().max(1) as f64 - 1.0).max(1.0);
    let y_range = y_max - y_min;
    let marker_offset = y_range * 0.04;

    let chart_height = area.height.saturating_sub(2) as usize;
    let label_count = (chart_height / 3).clamp(2, 5);
    let y_labels: Vec<Line> = (0..label_count)
        .map(|i| {
            let frac = i as f64 / (label_count - 1) as f64;
            let val = y_min + frac * y_range;
            Line::from(format!("${}", fmt_price(val, price_decimals)))
        })
        .collect();

    let buy_marker_data: Vec<(f64, f64)> = trade_markers
        .iter()
        .filter(|m| m.is_buy)
        .map(|m| (m.x, m.y - marker_offset))
        .collect();
    let sell_marker_data: Vec<(f64, f64)> = trade_markers
        .iter()
        .filter(|m| !m.is_buy)
        .map(|m| (m.x, m.y + marker_offset))
        .collect();

    // One white square per open limit order on the active market.
    // `order_chart_markers` owns the x-coordinate — it's captured when the
    // order first appears in a WS snapshot and decremented
    // on each `push_price`, so the square scrolls left with the rest of the chart
    // and the Chart widget clips it once it leaves the visible range.
    let order_markers: Vec<(f64, f64)> = order_chart_markers
        .iter()
        .filter(|((sym, _, _), m)| sym == symbol && m.price > 0.0)
        .map(|(_, m)| (m.x, m.price))
        .collect();

    let mut datasets = vec![Dataset::default()
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Cyan))
        .data(chart_data)];

    if !order_markers.is_empty() {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Block)
                .graph_type(GraphType::Scatter)
                .style(Style::default().fg(Color::DarkGray))
                .data(&order_markers),
        );
    }

    if !buy_marker_data.is_empty() {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Block)
                .graph_type(GraphType::Scatter)
                .style(Style::default().fg(Color::LightGreen))
                .data(&buy_marker_data),
        );
    }
    if !sell_marker_data.is_empty() {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Block)
                .graph_type(GraphType::Scatter)
                .style(Style::default().fg(Color::LightRed))
                .data(&sell_marker_data),
        );
    }

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(title_line)
                .title(clock_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .x_axis(
            Axis::default()
                .bounds([0.0, x_max])
                .style(Style::default().fg(Color::DarkGray)),
        )
        .y_axis(
            Axis::default()
                .bounds([y_min, y_max])
                .labels(y_labels)
                .style(Style::default().fg(Color::DarkGray)),
        );

    f.render_widget(chart, area);

    // Draw "B <LIMIT_PRICE>" / "S <LIMIT_PRICE>" in the order's side color at the
    // right edge of each order row. ratatui's `Chart` doesn't expose its plot
    // rect, so positions are derived from the known layout: outer border on
    // each side, 2 rows reserved at the bottom for the x-axis line + label gap
    // (reserving just 1 lands the text on the axis line row itself, where it's
    // painted over by ratatui's axis render).
    if y_range > 0.0 {
        const BORDER: u16 = 1;
        const X_AXIS_ROWS: u16 = 2;
        let plot_top = area.y + BORDER;
        let plot_bottom_exclusive = area.y + area.height.saturating_sub(BORDER + X_AXIS_ROWS);
        let plot_right_col = area.x + area.width.saturating_sub(BORDER + 1);

        if plot_bottom_exclusive > plot_top && plot_right_col >= area.x {
            let plot_h = plot_bottom_exclusive - plot_top;
            let last_row = plot_bottom_exclusive - 1;
            // Small tolerance so an order priced exactly at the axis floor (where f64
            // rounding can push the value a hair below `y_min`) still passes
            // the visibility filter.
            let band_eps = y_range * 0.001;
            let min_col = area.x + BORDER;
            for order in orders.iter().filter(|o| {
                o.symbol == symbol
                    && o.price_usd > 0.0
                    && o.price_usd >= y_min - band_eps
                    && o.price_usd <= y_max + band_eps
            }) {
                let norm = ((order.price_usd - y_min) / y_range).clamp(0.0, 1.0);
                let offset = (norm * (plot_h.saturating_sub(1)) as f64) as u16;
                let row = last_row.saturating_sub(offset).max(plot_top);

                let (color, side_tag) = match order.side {
                    TradingSide::Long => (Color::LightGreen, "B"),
                    TradingSide::Short => (Color::LightRed, "S"),
                };
                let text = format!("{} {}", side_tag, fmt_size(order.price_usd, price_decimals),);
                let text_len = text.len() as u16;
                // Right-anchor: final char lands on `plot_right_col`. Clamp to the inner border
                // column so very long labels don't spill into the y-axis gutter.
                let start_col = plot_right_col
                    .saturating_add(1)
                    .saturating_sub(text_len)
                    .max(min_col);
                let rect_width = plot_right_col.saturating_add(1).saturating_sub(start_col);
                let label = Paragraph::new(Span::styled(
                    text,
                    Style::default()
                        .fg(color)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ));
                f.render_widget(label, Rect::new(start_col, row, rect_width, 1));
            }
        }
    }
}
