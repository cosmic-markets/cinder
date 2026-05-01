//! Market selector modal.

use super::*;

pub(in crate::tui::ui) fn render_market_selector(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    selector: &MarketSelector,
    active_symbol: &str,
) {
    let list_height = selector.markets.len() as u16 + 4;
    let max_width: u16 = 72;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = list_height.min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let ms_s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", ms_s.markets),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", selector.markets.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let total_vol: f64 = selector.markets.iter().map(|m| m.volume_24h).sum();
    let total_oi: f64 = selector.markets.iter().map(|m| m.open_interest_usd).sum();
    let vol_title = Line::from(vec![
        Span::styled(
            format!("{} ", ms_s.vol_24h),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("${} ", fmt_price(total_vol, 0)),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!(" {} ", ms_s.oi),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("${} ", fmt_price(total_oi, 0)),
            Style::default().fg(Color::White),
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
            format!("{}  ", ms_s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", ms_s.confirm),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", ms_s.back),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title(vol_title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let price_w: u16 = 12;
    let chg_w: u16 = 10;
    let lev_w: u16 = 10;
    let vol_w: u16 = 10;
    let oi_w: u16 = 10;

    let visible_slots = inner.height.saturating_sub(1) as usize; // -1 for header row
    let scroll_offset = if selector.selected_index >= visible_slots {
        selector.selected_index - visible_slots + 1
    } else {
        0
    };

    let table_rows: Vec<Row> = selector
        .markets
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, m)| {
            let is_selected = i == selector.selected_index;
            let is_active = m.symbol == active_symbol;

            let cursor_str = if is_selected { "▸" } else { " " };
            let sym_str = if is_active {
                format!("{} ●", m.symbol)
            } else {
                m.symbol.clone()
            };
            let price_str = if m.price > 0.0 {
                format!("${}", fmt_price(m.price, m.price_decimals))
            } else {
                "—".to_string()
            };
            let chg_str = if m.change_24h != 0.0 {
                format!("{:+.1}%", m.change_24h)
            } else {
                "—".to_string()
            };
            let chg_color = if is_selected {
                Color::White
            } else if m.change_24h > 0.0 {
                Color::LightGreen
            } else if m.change_24h < 0.0 {
                Color::LightRed
            } else {
                Color::DarkGray
            };
            let lev_str = format!("{:.0}x", m.max_leverage);
            let vol_str = if m.volume_24h > 0.0 {
                format!("${}", fmt_compact(m.volume_24h))
            } else {
                "—".to_string()
            };
            let oi_str = if m.open_interest_usd > 0.0 {
                format!("${}", fmt_compact(m.open_interest_usd))
            } else {
                "—".to_string()
            };

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

            // Per-cell colors only apply to unselected, non-active rows.
            // Selected rows stay all-white on the highlight bg; active rows stay cyan.
            let (price_cell_style, lev_cell_style, vol_cell_style, oi_cell_style) =
                if is_selected || is_active {
                    (
                        Style::default(),
                        Style::default().fg(FIRE_ORANGE),
                        Style::default(),
                        Style::default(),
                    )
                } else {
                    (
                        Style::default().fg(Color::White),
                        Style::default().fg(FIRE_ORANGE),
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::DarkGray),
                    )
                };

            Row::new(vec![
                Cell::from(cursor_str),
                Cell::from(sym_str).style(Style::default().fg(FIRE_ORANGE)),
                Cell::from(Line::from(price_str).alignment(Alignment::Right))
                    .style(price_cell_style),
                Cell::from(Line::from(chg_str).alignment(Alignment::Right))
                    .style(Style::default().fg(chg_color)),
                Cell::from(Line::from(lev_str).alignment(Alignment::Right)).style(lev_cell_style),
                Cell::from(Line::from(vol_str).alignment(Alignment::Right)).style(vol_cell_style),
                Cell::from(Line::from(oi_str).alignment(Alignment::Right)).style(oi_cell_style),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(ms_s.market),
        Cell::from(Line::from(ms_s.price).alignment(Alignment::Right)),
        Cell::from(Line::from(ms_s.pct_change).alignment(Alignment::Right)),
        Cell::from(Line::from(ms_s.leverage).alignment(Alignment::Right)),
        Cell::from(Line::from(ms_s.vol_24h).alignment(Alignment::Right)),
        Cell::from(Line::from(ms_s.oi).alignment(Alignment::Right)),
    ])
    .style(header_style);

    let widths = [
        Constraint::Length(1),
        Constraint::Max(10),
        Constraint::Length(price_w),
        Constraint::Length(chg_w),
        Constraint::Length(lev_w),
        Constraint::Length(vol_w),
        Constraint::Length(oi_w),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);

    f.render_widget(table, inner);
}
