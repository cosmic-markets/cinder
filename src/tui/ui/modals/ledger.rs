//! Transaction ledger modal.

use super::*;

pub(in crate::tui::ui) fn render_ledger_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
) {
    let lg_s = strings();
    // Each entry takes 2 lines (header row + signature row), plus one header line,
    // plus borders/padding.
    let body_lines = (trading.ledger.len().max(1) as u16) * 2 + 1;
    let max_width: u16 = 110;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (body_lines + 4).min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", lg_s.ledger_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", trading.ledger.len()),
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
            format!("{}  ", lg_s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", lg_s.close),
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

    if trading.ledger.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", lg_s.ledger_empty),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, inner);
        return;
    }

    // Header line: column labels for the first (summary) row of each entry.
    // The second row per entry holds only the txid and spans the full width,
    // so it doesn't get a column label.
    let header_line = Line::from(vec![
        Span::styled(
            format!("  {:<11}", lg_s.ledger_col_time),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(lg_s.ledger_col_action, Style::default().fg(Color::DarkGray)),
    ]);
    let header_rect = ratatui::layout::Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(header_line), header_rect);

    let body_rect = ratatui::layout::Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    let visible_entries = (body_rect.height / 2) as usize;
    if visible_entries == 0 {
        return;
    }
    let selected = trading
        .ledger_selected
        .min(trading.ledger.len().saturating_sub(1));
    let scroll_offset = if selected >= visible_entries {
        selected - visible_entries + 1
    } else {
        0
    };

    for (slot, (i, entry)) in trading
        .ledger
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_entries)
        .enumerate()
    {
        let is_selected = i == selected;
        let cursor_str = if is_selected { "▸" } else { " " };
        let entry_style = if is_selected {
            Style::default()
                .bg(MODAL_HIGHLIGHT_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let summary_line = Line::from(vec![
            Span::styled(
                format!(" {}", cursor_str),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<11}", entry.timestamp),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(entry.title.clone(), Style::default().fg(Color::White)),
        ]);
        let sig_line = Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(entry.txid.clone(), Style::default().fg(Color::Cyan)),
        ]);

        let entry_rect = ratatui::layout::Rect {
            x: body_rect.x,
            y: body_rect.y + (slot as u16) * 2,
            width: body_rect.width,
            height: 2,
        };
        let para = Paragraph::new(vec![summary_line, sig_line]).style(entry_style);
        f.render_widget(para, entry_rect);
    }
}
