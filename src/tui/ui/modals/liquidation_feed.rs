//! Live liquidation feed modal.
//!
//! Renders the most-recent on-chain liquidations as a scrollable table, with
//! a footer hint for navigation and a header counter for total entries seen
//! while the modal was alive.

use chrono::Utc;

use super::*;
use crate::tui::format::fmt_time_since_secs;
use crate::tui::state::LiquidationFeedView;
use crate::tui::trading::TradingSide;

/// Hard cap on the modal's data area so a buffer of 200 rows doesn't expand
/// the popup vertically — extra rows scroll under the cursor (same pattern
/// as the markets modal).
const VISIBLE_ROWS: u16 = 10;

pub(in crate::tui::ui) fn render_liquidation_feed_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &LiquidationFeedView,
) {
    // Width budget: cursor(1) + time(4) + market(8) + side(5) + notional(10)
    // + size(10) + price(11) + trader(6) + 7 column gaps + 2 borders = 64.
    // Trader values are only 4 chars but the header label "Trader" is 6.
    let max_width: u16 = 66;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    // Captured once per draw; the runtime force-redraws the TUI on each 1s
    // clock tick so age columns advance without per-row work.
    let now = Utc::now();
    // Height = 2 borders + 1 header + visible data rows. Capped above by the
    // available terminal height.
    let visible = (view.entries.len() as u16).min(VISIBLE_ROWS).max(1);
    let popup_h = (visible + 3).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let s = strings();
    let (status_label, status_color) = if view.is_backfilling {
        (s.liq_feed_backfilling, Color::Yellow)
    } else {
        (s.liq_feed_live, Color::LightGreen)
    };
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.liquidations_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", view.entries.len()),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("{} ", status_label),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.liq_feed_scroll),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.close),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if view.entries.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", s.liq_feed_waiting),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, inner);
        return;
    }

    let visible_slots = inner.height.saturating_sub(1) as usize;
    let scroll_offset = if view.selected_index >= visible_slots {
        view.selected_index - visible_slots + 1
    } else {
        0
    };

    let table_rows: Vec<Row> = view
        .entries
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, e)| {
            let is_selected = i == view.selected_index;
            let cursor_str = if is_selected { "▸" } else { " " };

            let age_secs = (now - e.received_at).num_seconds();
            let time_str = fmt_time_since_secs(age_secs);
            let market_str = if e.symbol.is_empty() {
                format!("#{}", e.asset_id)
            } else {
                e.symbol.clone()
            };
            let size_str = fmt_size(e.size, e.size_decimals.min(6));
            let price_str = format!("${}", fmt_price(e.mark_price, e.price_decimals));
            let notional_str = format!("${}", fmt_compact(e.notional));
            let (side_label, side_color) = match e.side {
                Some(TradingSide::Long) => ("LONG", TradingSide::Long.color()),
                Some(TradingSide::Short) => ("SHORT", TradingSide::Short.color()),
                None => ("—", Color::DarkGray),
            };

            let row_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(MODAL_HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            Row::new(vec![
                Cell::from(cursor_str),
                Cell::from(Line::from(time_str).alignment(Alignment::Right))
                    .style(Style::default().fg(Color::DarkGray)),
                Cell::from(market_str).style(Style::default().fg(FIRE_ORANGE)),
                Cell::from(Span::styled(
                    side_label,
                    Style::default()
                        .fg(side_color)
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Line::from(notional_str).alignment(Alignment::Right)),
                Cell::from(Line::from(size_str).alignment(Alignment::Right)),
                Cell::from(Line::from(price_str).alignment(Alignment::Right)),
                Cell::from(
                    Line::from(e.liquidated_trader.clone()).alignment(Alignment::Right),
                ),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(Line::from(s.ledger_col_time).alignment(Alignment::Right)),
        Cell::from(s.market),
        Cell::from(s.side),
        Cell::from(Line::from(s.notional_col).alignment(Alignment::Right)),
        Cell::from(Line::from(s.size).alignment(Alignment::Right)),
        Cell::from(Line::from(s.price).alignment(Alignment::Right)),
        Cell::from(Line::from(s.trader).alignment(Alignment::Right)),
    ])
    .style(header_style);

    let widths = [
        Constraint::Length(1),
        Constraint::Length(4),
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(11),
        Constraint::Length(6),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);
    f.render_widget(table, inner);
}
