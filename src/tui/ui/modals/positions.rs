//! Account positions modal.

use super::*;

pub(in crate::tui::ui) fn render_positions_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &PositionsView,
    active_symbol: &str,
    markets: &MarketSelector,
) {
    let row_count = view.positions.len().max(1) as u16;
    let max_width: u16 = 90;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (row_count + 6).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    // `fmt_price` with 2 decimals renders anything in (-0.005, 0) as "-0.00", which
    // produces "$-0.00" when there are no positions (and -0.0 can sneak in from
    // float accumulation). Snap sub-cent magnitudes to +0.0 so the header reads
    // "$0.00" cleanly.
    let snap_zero = |v: f64| if v.abs() < 0.005 { 0.0 } else { v };
    let agg_notional = snap_zero(view.aggregate_notional());
    let agg_pnl = snap_zero(view.aggregate_pnl());
    let pnl_color = if agg_pnl >= 0.0 {
        Color::LightGreen
    } else {
        Color::LightRed
    };
    let pnl_prefix = if agg_pnl >= 0.0 { "+$" } else { "-$" };

    let pm_s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", pm_s.positions),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", view.positions.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let pnl_title = Line::from(vec![
        Span::styled(
            format!(" {} ", pm_s.notional_col),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("${} ", fmt_price(agg_notional, 2)),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!(" {} ", pm_s.upnl),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("{}{} ", pnl_prefix, fmt_price(agg_pnl.abs(), 2)),
            Style::default().fg(pnl_color),
        ),
    ])
    .right_aligned();

    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", pm_s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", pm_s.view_market),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "x ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", pm_s.close_pos),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "u ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", pm_s.close_all),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", pm_s.back),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title(pnl_title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if view.positions.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", pm_s.no_open_positions),
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
            let is_active = p.symbol == active_symbol;
            let cursor_str = if is_selected { "▸" } else { " " };
            let sym_str = if is_active {
                format!("{} ●", p.symbol)
            } else {
                p.symbol.clone()
            };

            let price_decimals = markets.price_decimals_for_symbol(&p.symbol);

            let pnl_color = if p.unrealized_pnl >= 0.0 {
                Color::LightGreen
            } else {
                Color::LightRed
            };
            let pnl_prefix = if p.unrealized_pnl >= 0.0 { "+$" } else { "-$" };
            let pnl_str = format!("{}{}", pnl_prefix, fmt_price(p.unrealized_pnl.abs(), 2));

            let liq_str = p
                .liquidation_price
                .map(|l| format!("${}", fmt_price(l, price_decimals)))
                .unwrap_or_else(|| "—".to_string());

            let lev_str = p
                .leverage
                .map(|l| format!("{:.1}x", l))
                .unwrap_or_else(|| "—".to_string());

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
                Cell::from(cursor_str),
                Cell::from(sym_str).style(Style::default().fg(FIRE_ORANGE)),
                Cell::from(Span::styled(
                    match p.side {
                        TradingSide::Long => pm_s.long_label,
                        TradingSide::Short => pm_s.short_label,
                    },
                    Style::default()
                        .fg(p.side.color())
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Line::from(format!("{}", p.size)).alignment(Alignment::Right)),
                Cell::from(
                    Line::from(format!("${}", fmt_price(p.entry_price, price_decimals)))
                        .alignment(Alignment::Right),
                ),
                Cell::from(
                    Line::from(format!("${}", fmt_price(p.notional, 2)))
                        .alignment(Alignment::Right),
                ),
                Cell::from(Line::from(pnl_str).alignment(Alignment::Right))
                    .style(Style::default().fg(pnl_color)),
                Cell::from(Line::from(liq_str).alignment(Alignment::Right)),
                Cell::from(Line::from(lev_str).alignment(Alignment::Right))
                    .style(Style::default().fg(FIRE_ORANGE)),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(pm_s.market),
        Cell::from(pm_s.side),
        Cell::from(Line::from(pm_s.size).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.entry).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.notional_col).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.pnl).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.liq_col).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.lev_col).alignment(Alignment::Right)),
    ])
    .style(header_style);

    let widths = [
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(7),
        Constraint::Length(9),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Length(10),
        Constraint::Length(7),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);

    f.render_widget(table, inner);
}
