//! First-run referral choice modal — three rows (COSMIC / custom / skip)
//! shown automatically when a wallet with no Phoenix account connects.
//! Picking a row determines whether the wallet is registered with the
//! COSMIC referral, a user-supplied code, or not at all.

use super::*;

const ROWS: usize = 3;

pub(in crate::tui::ui) fn render_referral_choice_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
) {
    let s = strings();
    let popup_h: u16 = 16.min(area.height.saturating_sub(2));
    let popup_w: u16 = 84.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    // "Phoenix" is rendered in FIRE_ORANGE between the localized prefix
    // and suffix. Languages where the proper noun comes first (EN, ZH)
    // leave prefix empty; languages where it comes last (ES, RU) leave
    // suffix empty.
    let bold_white = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let bold_orange = Style::default()
        .fg(FIRE_ORANGE)
        .add_modifier(Modifier::BOLD);
    let title = Line::from(vec![
        Span::raw(" 🐦‍🔥 "),
        Span::styled(s.referral_choice_title_prefix, bold_white),
        Span::styled("Phoenix", bold_orange),
        Span::styled(format!("{} ", s.referral_choice_title_suffix), bold_white),
    ]);

    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.referral_choice_nav),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.referral_choice_action),
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

    let constraints = [
        Constraint::Length(2), // intro (with trailing blank)
        Constraint::Length(1), // option 0
        Constraint::Length(1), // cosmic note
        Constraint::Length(1), // spacer
        Constraint::Length(1), // option 1
        Constraint::Length(1), // spacer
        Constraint::Length(1), // option 2
        Constraint::Length(1), // spacer
        Constraint::Length(1), // sticky attribution note
        Constraint::Min(0),
    ];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", s.referral_choice_intro),
            Style::default().fg(Color::Gray),
        ))),
        rows[0],
    );

    let labels: [&str; ROWS] = [
        s.referral_choice_cosmic,
        s.referral_choice_custom,
        s.referral_choice_skip,
    ];
    let row_indices = [1usize, 4, 6];

    let selected = trading.referral_choice_index.min(ROWS - 1);

    for (i, label) in labels.iter().enumerate() {
        let is_selected = i == selected;
        let marker = if is_selected { "▶ " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(Color::White)
                .bg(MODAL_HIGHLIGHT_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(marker, style),
                Span::styled(label.to_string(), style),
            ])),
            rows[row_indices[i]],
        );
    }

    // Helper note under option 0 making the funding share explicit even when
    // COSMIC is not the active row.
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("    {}", s.referral_choice_cosmic_note),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ))),
        rows[2],
    );

    // Sticky-attribution disclosure pinned to the bottom.
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", s.referral_choice_sticky_note),
            Style::default()
                .fg(Color::Rgb(180, 130, 60))
                .add_modifier(Modifier::ITALIC),
        ))),
        rows[8],
    );
}
