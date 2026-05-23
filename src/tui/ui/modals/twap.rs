//! "New TWAP" and "Bots" modals.
//!
//! `render_twap_modal` collects side / total size / total time
//! (hours + minutes) from the user; slice cadence is fixed at one market
//! slice per minute so total time fully determines the schedule.
//! `render_bots_modal` lists every TWAP bot tracked by [`TwapsView`] with
//! status and lifecycle hotkeys.

use std::time::Instant;

use super::*;

use super::super::super::state::{TwapBot, TwapStatus, TwapsView};
use super::super::super::trading::TradingSide;

/// "Phoenix TWAP" modal.
pub(in crate::tui::ui) fn render_twap_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
    _symbol: &str,
) {
    let s = strings();
    let draft = &trading.twap_draft;
    let has_error = draft.error.is_some();
    // Header block (intro + summary) + spacer + 6 form rows, plus an
    // optional 2-row error tail. +2 for the top/bottom border.
    let desired_h: u16 = if has_error { 13 } else { 11 };
    let popup_h: u16 = desired_h.min(area.height.saturating_sub(2));
    let popup_w: u16 = 72.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "🐦‍🔥 ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.twap_modal_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
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
            format!("{}  ", s.twap_nav_field),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "←→ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.twap_cycle_market),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Tab ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.twap_toggle_side),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Enter ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.twap_start),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
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

    // Header (intro + schedule preview) at the top, form below.
    let mut constraints = vec![
        Constraint::Length(1), // intro
        Constraint::Length(1), // derived summary
        Constraint::Length(1), // spacer
        Constraint::Length(1), // market
        Constraint::Length(1), // side
        Constraint::Length(1), // total size
        Constraint::Length(1), // total time header
        Constraint::Length(1), // hours
        Constraint::Length(1), // minutes
    ];
    if has_error {
        constraints.push(Constraint::Length(1)); // spacer before error
        constraints.push(Constraint::Length(1)); // error
    }
    constraints.push(Constraint::Min(0));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // — Top header: intro + schedule preview —
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", s.twap_modal_intro),
            Style::default().fg(Color::DarkGray),
        ))),
        rows[0],
    );
    let summary = derive_summary(draft);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(summary, Style::default().fg(Color::DarkGray)),
        ])),
        rows[1],
    );

    // Field 0 — Market: shown with ←→ hint when focused so the user knows
    // it's a cyclable picker rather than a text input.
    let market_value = if draft.selected_field == 0 {
        format!("{}  [←→]", draft.market)
    } else {
        draft.market.clone()
    };
    render_form_row(
        f,
        rows[3],
        s.twap_field_market,
        Span::styled(
            market_value,
            Style::default()
                .fg(if draft.selected_field == 0 {
                    Color::Cyan
                } else {
                    Color::White
                })
                .add_modifier(Modifier::BOLD),
        ),
        draft.selected_field == 0,
    );

    let side_value: String = match draft.side {
        TradingSide::Long => format!("{}  [Tab]", s.long_label),
        TradingSide::Short => format!("{}  [Tab]", s.short_label),
    };
    let side_color = draft.side.color();
    render_form_row(
        f,
        rows[4],
        s.twap_field_side,
        Span::styled(
            side_value,
            Style::default().fg(side_color).add_modifier(Modifier::BOLD),
        ),
        draft.selected_field == 1,
    );
    render_form_row(
        f,
        rows[5],
        s.twap_field_size,
        editable_value_span(&draft.size_buffer, draft.selected_field == 2, &draft.market),
        draft.selected_field == 2,
    );

    // Total Time header (label-only).
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  {}", s.twap_field_total_time),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ))),
        rows[6],
    );

    render_form_row(
        f,
        rows[7],
        s.twap_field_hours,
        editable_value_span(
            &draft.duration_hour_buffer,
            draft.selected_field == 3,
            s.twap_unit_hr,
        ),
        draft.selected_field == 3,
    );
    render_form_row(
        f,
        rows[8],
        s.twap_field_mins,
        editable_value_span(
            &draft.duration_min_buffer,
            draft.selected_field == 4,
            s.twap_unit_min,
        ),
        draft.selected_field == 4,
    );

    if has_error {
        let err = draft.error.as_deref().unwrap_or("");
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " ✗ ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(err.to_string(), Style::default().fg(Color::LightRed)),
            ])),
            rows[10],
        );
    }
}

fn editable_value_span<'a>(buffer: &'a str, is_selected: bool, unit: &'a str) -> Span<'a> {
    let cursor = if is_selected { "_" } else { "" };
    if buffer.is_empty() && !is_selected {
        Span::styled(format!("— {}", unit), Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(
            format!("{}{}  {}", buffer, cursor, unit),
            Style::default()
                .fg(if is_selected {
                    Color::Cyan
                } else {
                    Color::White
                })
                .add_modifier(if is_selected {
                    Modifier::BOLD | Modifier::UNDERLINED
                } else {
                    Modifier::BOLD
                }),
        )
    }
}

fn render_form_row(
    f: &mut Frame,
    rect: ratatui::layout::Rect,
    label: &str,
    value: Span<'_>,
    is_selected: bool,
) {
    let cursor = if is_selected { "▸" } else { " " };
    let label_style = if is_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let line = Line::from(vec![
        Span::styled(format!(" {} ", cursor), label_style),
        Span::styled(format!("{:<12}", label), label_style),
        Span::raw("  "),
        value,
    ]);
    f.render_widget(Paragraph::new(line), rect);
}

fn derive_summary(draft: &super::super::super::state::TwapDraft) -> String {
    let s = strings();
    let size: Option<f64> = draft.size_buffer.parse::<f64>().ok().filter(|v| *v > 0.0);
    let hours: u32 = draft.duration_hour_buffer.parse::<u32>().unwrap_or(0);
    let mins: u32 = draft.duration_min_buffer.parse::<u32>().unwrap_or(0);
    let total_minutes = hours * 60 + mins;
    if let (Some(size), true) = (size, total_minutes >= 1) {
        let slice_count = total_minutes;
        let slice_size = size / slice_count as f64;
        let interval_sec = (total_minutes as f64 * 60.0) / slice_count as f64;
        format!(
            "{} {} {} {} ({:.4} {}/{}, {:.0}{} {})",
            s.twap_summary_prefix,
            slice_count,
            s.twap_summary_slices_of,
            size,
            slice_size,
            draft.market,
            s.twap_summary_per_slice_suf,
            interval_sec,
            s.twap_unit_sec,
            s.twap_summary_interval_suf,
        )
    } else {
        s.twap_summary_placeholder.to_string()
    }
}

/// "Bots" modal (toggled with [b]).
pub(in crate::tui::ui) fn render_bots_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &TwapsView,
    active_symbol: &str,
) {
    let row_count = view.bots.len().max(1) as u16;
    let max_width: u16 = 110;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (row_count + 6).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.bots_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", view.bots.len()),
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
            "p ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.bots_pause_resume),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "s ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.bots_stop),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "r ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.bots_restart),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "x ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.bots_remove),
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

    if view.bots.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", s.bots_empty),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, inner);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled(
            format!("  {}", s.market),
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            s.bots_kind,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(s.side, Style::default().fg(Color::DarkGray))),
        Cell::from(Span::styled(
            s.bots_progress,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(s.size, Style::default().fg(Color::DarkGray))),
        Cell::from(Span::styled(
            s.bots_interval,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            s.bots_next,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            s.bots_state,
            Style::default().fg(Color::DarkGray),
        )),
    ]);

    let visible_slots = inner.height.saturating_sub(1) as usize;
    let scroll_offset = if view.selected_index >= visible_slots {
        view.selected_index - visible_slots + 1
    } else {
        0
    };

    // Snap once per frame so all rows in this redraw share the same
    // reference clock — keeps the countdown column monotonic across rows.
    let now = Instant::now();
    let table_rows: Vec<Row> = view
        .bots
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, b)| bot_row(b, active_symbol, i == view.selected_index, now))
        .collect();

    let widths = [
        Constraint::Length(13),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Min(0),
    ];
    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);
    f.render_widget(table, inner);
}

fn bot_row<'a>(b: &'a TwapBot, active_symbol: &str, is_selected: bool, now: Instant) -> Row<'a> {
    let s = strings();
    let cursor_str = if is_selected { "▸" } else { " " };
    let is_active_market = b.symbol == active_symbol;
    let sym_str = if is_active_market {
        format!("{} {} ●", cursor_str, b.symbol)
    } else {
        format!("{} {}", cursor_str, b.symbol)
    };

    let side_color = b.side.color();
    let side_label = match b.side {
        TradingSide::Long => s.buy,
        TradingSide::Short => s.sell,
    };

    let status_label = match b.status {
        TwapStatus::Running => s.bots_status_running,
        TwapStatus::Paused => s.bots_status_paused,
        TwapStatus::Stopped => s.bots_status_stopped,
        TwapStatus::Completed => s.bots_status_completed,
    };
    let status_color = match b.status {
        TwapStatus::Running => Color::Green,
        TwapStatus::Paused => Color::Yellow,
        TwapStatus::Stopped => Color::Red,
        TwapStatus::Completed => Color::DarkGray,
    };

    let interval_secs = b.slice_interval.as_secs();
    let interval_str = if interval_secs == 0 {
        "instant".to_string()
    } else {
        format!("{}s", interval_secs)
    };

    // Countdown to the next scheduler tick that would fire a slice. For a
    // running bot this is `(last_slice_at + slice_interval) - now`, clamped
    // at zero. The scheduler tick still runs at 1Hz so the value updates as
    // the modal redraws.
    let next_str = match b.status {
        TwapStatus::Running => match b.last_slice_at {
            None => "now".to_string(),
            Some(prev) => {
                let target = prev + b.slice_interval;
                if target <= now {
                    "now".to_string()
                } else {
                    let remaining = target.saturating_duration_since(now);
                    let secs = remaining.as_secs();
                    if secs >= 60 {
                        format!("{}m{:02}s", secs / 60, secs % 60)
                    } else {
                        format!("{}s", secs)
                    }
                }
            }
        },
        TwapStatus::Paused | TwapStatus::Stopped | TwapStatus::Completed => "—".to_string(),
    };
    let next_color = match b.status {
        TwapStatus::Running => Color::White,
        _ => Color::DarkGray,
    };

    let row_style = if is_selected {
        Style::default()
            .fg(Color::White)
            .bg(MODAL_HIGHLIGHT_BG)
            .add_modifier(Modifier::BOLD)
    } else if is_active_market {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };

    Row::new(vec![
        Cell::from(Span::styled(
            sym_str,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled("TWAP", Style::default().fg(Color::White))),
        Cell::from(Span::styled(
            side_label,
            Style::default().fg(side_color).add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            format!("{}/{}", b.slices_submitted, b.slice_count),
            Style::default().fg(Color::White),
        )),
        Cell::from(Span::styled(
            fmt_size(b.total_size, 4),
            Style::default().fg(Color::White),
        )),
        Cell::from(Span::styled(
            interval_str,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(next_str, Style::default().fg(next_color))),
        Cell::from(Span::styled(
            status_label,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        )),
    ])
    .style(row_style)
}
