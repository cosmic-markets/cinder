//! Custom referral code modal — reached from the first-run choice modal
//! when the user picks "Enter a custom referral / invite code".

use super::super::super::constants::MAX_REFERRAL_CODE_LEN;
use super::*;

pub(in crate::tui::ui) fn render_referral_code_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
) {
    let s = strings();
    let has_error = trading.referral_code_error.is_some();
    let desired_h: u16 = if has_error { 9 } else { 8 };
    let popup_h: u16 = desired_h.min(area.height.saturating_sub(2));
    let popup_w: u16 = 80.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![
        Span::raw(" 🐦‍🔥 "),
        Span::styled(
            format!("{} ", s.referral_modal_title),
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
            format!("{}  ", s.referral_modal_action),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.referral_modal_skip),
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
        Constraint::Length(1), // help text
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
            format!(" {}", s.referral_modal_label),
            Style::default().fg(Color::DarkGray),
        ))),
        rows[0],
    );

    let used = trading.referral_code_buffer.chars().count();
    let counter_color = if used >= MAX_REFERRAL_CODE_LEN {
        // Cap reached — flag the counter in warm-amber so the user sees why
        // further keystrokes aren't echoing.
        Color::Rgb(220, 150, 60)
    } else {
        Color::DarkGray
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{}_", trading.referral_code_buffer),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{}/{}", used, MAX_REFERRAL_CODE_LEN),
                Style::default().fg(counter_color),
            ),
        ])),
        rows[1],
    );

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", s.referral_modal_help),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ))),
        rows[3],
    );

    if has_error {
        let err = trading.referral_code_error.as_deref().unwrap_or("");
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " ❌ ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(err.to_string(), Style::default().fg(Color::LightRed)),
            ])),
            rows[4],
        );
    }
}
