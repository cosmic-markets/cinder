//! Wallet path editor modal.

use super::*;

/// "Load Wallet" modal — single-line editable path with a hint row and an
/// optional error row when a previous Enter failed.
pub(in crate::tui::ui) fn render_wallet_path_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
) {
    let s = strings();
    let has_error = trading.wallet_path_error.is_some();
    let desired_h: u16 = if has_error { 7 } else { 6 };
    let popup_h: u16 = desired_h.min(area.height.saturating_sub(2));
    let popup_w: u16 = 80.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![
        Span::raw(" 🐦‍🔥 "),
        Span::styled(
            format!("{} ", s.st_load_wallet_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let footer = Line::from(vec![
        Span::styled(
            " Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.st_load_wallet_action),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.cancel),
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

    let mut constraints = vec![
        Constraint::Length(1), // label
        Constraint::Length(1), // input
        Constraint::Length(1), // spacer
    ];
    if has_error {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(0));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", s.st_wallet_path_label),
            Style::default().fg(Color::DarkGray),
        ))),
        rows[0],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{}_", trading.wallet_path_buffer),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        ])),
        rows[1],
    );

    if has_error {
        let err = trading.wallet_path_error.as_deref().unwrap_or("");
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", s.st_wallet_load_failed),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(err.to_string(), Style::default().fg(Color::LightRed)),
            ])),
            rows[3],
        );
    }
}
