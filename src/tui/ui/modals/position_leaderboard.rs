//! Protocol top positions modal.

use super::*;

/// "Top positions on Phoenix" modal. Table columns: rank, market, trader,
/// side, size, entry, notional, PnL. Sized similar to the positions modal
/// but wider to fit the trader column.
pub(in crate::tui::ui) fn render_top_positions_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &TopPositionsView,
    markets: &MarketSelector,
) {
    // Cap visible rows so the popup doesn't grow to fit the full top-N (50).
    // Anything beyond `MAX_VISIBLE_ROWS` becomes scrollable via the existing
    // selected_index → scroll_offset path below.
    const MAX_VISIBLE_ROWS: u16 = 15;
    let row_count = (view.positions.len() as u16).clamp(1, MAX_VISIBLE_ROWS);
    // Sum of column widths + column spacing + borders. Picked tightly so the
    // modal doesn't sprawl on wide terminals.
    let max_width: u16 = 78;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (row_count + 6).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.top_positions_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", view.positions.len()),
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
            format!("{}  ", s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.top_positions_copy_hint),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{} ", s.back), Style::default().fg(Color::DarkGray)),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if view.positions.is_empty() {
        // Cold-open placeholder vs genuine empty list — the task flips
        // `loaded` after the first refresh completes.
        let msg = if view.loaded {
            s.top_positions_empty
        } else {
            s.top_positions_loading
        };
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", msg),
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
        .positions
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, p)| {
            let is_selected = i == view.selected_index;
            let cursor_str = if is_selected { "▸" } else { " " };
            let rank_str = format!("{}", i + 1);

            let price_decimals = markets.price_decimals_for_symbol(&p.symbol);

            let pnl_color = if p.unrealized_pnl >= 0.0 {
                Color::LightGreen
            } else {
                Color::LightRed
            };
            let pnl_prefix = if p.unrealized_pnl >= 0.0 { "+$" } else { "-$" };
            let pnl_str = format!("{}{}", pnl_prefix, fmt_price(p.unrealized_pnl.abs(), 2));

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
                Cell::from(Line::from(rank_str).alignment(Alignment::Right))
                    .style(Style::default().fg(Color::DarkGray)),
                Cell::from(p.symbol.clone()).style(Style::default().fg(FIRE_ORANGE)),
                Cell::from(p.trader_display.clone()).style(Style::default().fg(Color::White)),
                Cell::from(Span::styled(
                    match p.side {
                        TradingSide::Long => s.long_label,
                        TradingSide::Short => s.short_label,
                    },
                    Style::default()
                        .fg(p.side.color())
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Line::from(fmt_compact(p.size)).alignment(Alignment::Right)),
                Cell::from(
                    Line::from(format!("${}", fmt_price(p.entry_price, price_decimals)))
                        .alignment(Alignment::Right),
                ),
                Cell::from(
                    Line::from(format!("${}", fmt_compact(p.notional))).alignment(Alignment::Right),
                ),
                Cell::from(Line::from(pnl_str).alignment(Alignment::Right))
                    .style(Style::default().fg(pnl_color)),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(Line::from(s.top_positions_rank).alignment(Alignment::Right)),
        Cell::from(s.market),
        Cell::from(s.top_positions_trader),
        Cell::from(s.side),
        Cell::from(Line::from(s.size).alignment(Alignment::Right)),
        Cell::from(Line::from(s.entry).alignment(Alignment::Right)),
        Cell::from(Line::from(s.notional_col).alignment(Alignment::Right)),
        Cell::from(Line::from(s.pnl).alignment(Alignment::Right)),
    ])
    .style(header_style);

    let widths = [
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(8),
        // Trader column: 4 + ellipsis + 4 = 9 visible chars, plus a small
        // cushion so the cell isn't flush with its neighbour.
        Constraint::Length(10),
        Constraint::Length(5),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);

    f.render_widget(table, inner);
}
