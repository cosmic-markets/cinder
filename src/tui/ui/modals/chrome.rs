//! Shared modal chrome helpers and lightweight overlays (switching, quit).

use ratatui::layout::Rect;
use ratatui::widgets::Clear;

use super::*;

/// Compute a centered popup rect clamped to `area`, clear it, render `block`,
/// and return the block's inner rect.
pub(super) fn centered_modal(
    f: &mut Frame,
    area: Rect,
    desired_w: u16,
    desired_h: u16,
    block: Block<'_>,
) -> Rect {
    let popup_w = desired_w.min(area.width.saturating_sub(4));
    let popup_h = desired_h.min(area.height.saturating_sub(2));
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect::new(x, y, popup_w, popup_h);

    f.render_widget(Clear, popup_area);
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);
    inner
}

/// Build a left-aligned footer hint line from `(key, color, label)` tuples.
pub(super) fn footer_hints(items: &[(&str, Color, &str)]) -> Line<'static> {
    let mut spans = Vec::with_capacity(items.len() * 2);
    for (idx, (key, color, label)) in items.iter().enumerate() {
        let key_text = if idx == 0 {
            format!(" {key} ")
        } else {
            format!("{key} ")
        };
        let label_text = if idx + 1 < items.len() {
            format!("{label}  ")
        } else {
            format!("{label} ")
        };
        spans.push(Span::styled(
            key_text,
            Style::default().fg(*color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            label_text,
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans).left_aligned()
}

/// Render the "Switching to <symbol> market…" overlay.
pub(in crate::tui::ui) fn render_switching_modal(f: &mut Frame, area: Rect, symbol: &str) {
    let label = format!("🐦‍🔥 Switching to {symbol} market… ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));
    let inner = centered_modal(f, area, label.len() as u16 + 4, 3, block);

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

    f.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

/// Render the quit confirmation modal (`Quit? [Y]/[N]`).
pub(in crate::tui::ui) fn render_quit_modal(f: &mut Frame, area: Rect) {
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
    let inner = centered_modal(f, area, 32, 3, block);

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
