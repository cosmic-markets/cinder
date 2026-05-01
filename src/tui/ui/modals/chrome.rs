//! Lightweight modal overlays.

use super::*;

pub(in crate::tui::ui) fn render_switching_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    symbol: &str,
) {
    let label = format!("🐦‍🔥 Switching to {} market… ", symbol);
    let popup_w: u16 = (label.len() as u16 + 4).min(area.width.saturating_sub(4));
    let popup_h: u16 = 3.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let text = Line::from(vec![
        Span::styled("🐦‍🔥 Switching to ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            symbol,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" market…", Style::default().fg(Color::DarkGray)),
    ])
    .centered();

    f.render_widget(
        Paragraph::new(text).alignment(ratatui::layout::Alignment::Center),
        inner,
    );
}
