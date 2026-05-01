//! Live liquidation feed modal.
//!
//! Renders the most-recent on-chain liquidations as a scrollable table, with
//! a footer hint for navigation and a header counter for total entries seen
//! while the modal was alive.

use super::*;
use crate::tui::state::LiquidationFeedView;

pub(in crate::tui::ui) fn render_liquidation_feed_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &LiquidationFeedView,
) {
    let row_count = view.entries.len().max(1) as u16;
    let max_width: u16 = 96;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (row_count + 6).min(area.height.saturating_sub(2));

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

            let time_str = e.received_at.format("%H:%M:%S").to_string();
            let market_str = if e.symbol.is_empty() {
                format!("#{}", e.asset_id)
            } else {
                e.symbol.clone()
            };
            let size_str = fmt_size(e.size, e.size_decimals.min(6));
            let mark_str = format!("${}", fmt_price(e.mark_price, e.price_decimals));
            let notional_str = format!("${}", fmt_compact(e.notional));
            let status_label = if e.position_closed {
                s.liq_feed_status_closed
            } else {
                s.liq_feed_status_partial
            };
            let status_color = if e.position_closed {
                Color::LightRed
            } else {
                Color::Yellow
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
                Cell::from(time_str).style(Style::default().fg(Color::DarkGray)),
                Cell::from(market_str).style(Style::default().fg(FIRE_ORANGE)),
                Cell::from(e.liquidated_trader.clone()).style(Style::default().fg(Color::Cyan)),
                Cell::from(Line::from(size_str).alignment(Alignment::Right)),
                Cell::from(Line::from(mark_str).alignment(Alignment::Right)),
                Cell::from(Line::from(notional_str).alignment(Alignment::Right)),
                Cell::from(Span::styled(
                    status_label,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                )),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(s.ledger_col_time),
        Cell::from(s.market),
        Cell::from(s.trader),
        Cell::from(Line::from(s.size).alignment(Alignment::Right)),
        Cell::from(Line::from(s.mark).alignment(Alignment::Right)),
        Cell::from(Line::from(s.notional_col).alignment(Alignment::Right)),
        Cell::from(s.status),
    ])
    .style(header_style);

    let widths = [
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(11),
        Constraint::Length(10),
        Constraint::Length(7),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);

    f.render_widget(table, inner);
}
