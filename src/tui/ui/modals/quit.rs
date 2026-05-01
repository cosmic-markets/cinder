//! Quit confirmation modal.

use super::*;

pub(in crate::tui::ui) fn render_quit_modal(f: &mut Frame, area: ratatui::layout::Rect) {
    let popup_w: u16 = 32.min(area.width.saturating_sub(4));
    let popup_h: u16 = 3.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![
        Span::raw("🐦‍🔥 "),
        Span::styled(
            "Phoenix",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let q_s = strings();
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", q_s.quit_confirm),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            "[Y]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[N]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    ])
    .centered();

    f.render_widget(Paragraph::new(line), inner);
}
