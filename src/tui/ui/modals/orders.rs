//! Open orders modal.

use super::*;

pub(in crate::tui::ui) fn render_orders_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &OrdersView,
    active_symbol: &str,
) {
    let row_count = view.orders.len().max(1) as u16;
    let max_width: u16 = 96;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (row_count + 6).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let om_s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", om_s.orders),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", view.orders.len()),
            Style::default().fg(Color::DarkGray),
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
            format!("{}  ", om_s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "x ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", om_s.cxl_order),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "u ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", om_s.cxl_all),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", om_s.back),
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

    if view.orders.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", om_s.no_open_orders),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, inner);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled(
            format!("  {}", om_s.market),
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.side,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.order_type,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.price,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.size,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.filled,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.flags,
            Style::default().fg(Color::DarkGray),
        )),
    ]);

    let visible_slots = inner.height.saturating_sub(1) as usize;
    let scroll_offset = if view.selected_index >= visible_slots {
        view.selected_index - visible_slots + 1
    } else {
        0
    };

    let table_rows: Vec<Row> = view
        .orders
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, o)| order_row(o, active_symbol, i == view.selected_index))
        .collect();

    let widths = [
        Constraint::Length(13),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Min(0),
    ];
    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);
    f.render_widget(table, inner);
}

fn order_row<'a>(o: &'a OrderInfo, active_symbol: &str, is_selected: bool) -> Row<'a> {
    let is_active = o.symbol == active_symbol;
    let cursor_str = if is_selected { "▸" } else { " " };
    let sym_str = if is_active {
        format!("{} {} ●", cursor_str, o.symbol)
    } else {
        format!("{} {}", cursor_str, o.symbol)
    };

    let side_color = o.side.color();
    let or_s = strings();
    let side_label = match o.side {
        TradingSide::Long => or_s.buy,
        TradingSide::Short => or_s.sell,
    };

    let filled = if o.initial_size > 0.0 {
        100.0 * (1.0 - (o.size_remaining / o.initial_size).clamp(0.0, 1.0))
    } else {
        0.0
    };

    let mut flag_parts: Vec<&str> = Vec::new();
    if o.reduce_only {
        flag_parts.push("RO");
    }
    if o.is_stop_loss {
        flag_parts.push("SL");
    }
    let flags = flag_parts.join(" ");

    let row_style = if is_selected {
        Style::default()
            .fg(Color::White)
            .bg(MODAL_HIGHLIGHT_BG)
            .add_modifier(Modifier::BOLD)
    } else if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };

    Row::new(vec![
        Cell::from(Span::styled(
            sym_str,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            side_label,
            Style::default().fg(side_color).add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            if o.is_stop_loss {
                or_s.stp.to_string()
            } else {
                // The SDK returns "Limit" / "Market" / etc. Map the two we
                // know to the 3-letter abbreviation used elsewhere in the UI;
                // anything else (future order types) renders uppercased.
                match o.order_type.as_str() {
                    "Limit" => or_s.lmt.to_string(),
                    "Market" => or_s.mkt.to_string(),
                    other => other.to_uppercase(),
                }
            },
            Style::default().fg(Color::White),
        )),
        Cell::from(Span::styled(
            format!("${}", fmt_price(o.price_usd, 4)),
            Style::default().fg(Color::White),
        )),
        Cell::from(Span::styled(
            fmt_size(o.size_remaining, 4),
            Style::default().fg(Color::White),
        )),
        Cell::from(Span::styled(
            format!("{:.1}%", filled),
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            flags,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
    ])
    .style(row_style)
}
