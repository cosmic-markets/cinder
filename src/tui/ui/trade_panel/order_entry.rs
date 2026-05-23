//! Size and price entry row rendering.

use super::*;

pub(super) fn render_order_entry(
    f: &mut Frame,
    rows: &[ratatui::layout::Rect],
    trading: &TradingState,
    symbol: &str,
    price_decimals: usize,
) {
    let tp_s = strings();
    let trade_field_label_style = Style::default().fg(Color::DarkGray);
    const LEFT_COL_W: u16 = 28;
    let row1_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_COL_W), Constraint::Min(0)])
        .split(rows[1]);

    // Row 1: qty (left) + price (right), or full-width editor overlay.
    match &trading.input_mode {
        InputMode::EditingSize => {
            let line = Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    format!(" {} ", tp_s.size),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {} ", symbol),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{}_", trading.input_buffer),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "  [Enter]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}  ", tp_s.set),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    "[Esc]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", tp_s.cancel),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            f.render_widget(Paragraph::new(line), rows[1]);
        }
        InputMode::EditingPrice => {
            let line = Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    format!(" {} ", tp_s.px),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" USD ", trade_field_label_style),
                Span::styled(
                    format!("{}_", trading.input_buffer),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "  [Enter]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}  ", tp_s.set),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    "[Esc]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", tp_s.cancel),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            f.render_widget(Paragraph::new(line), rows[1]);
        }
        _ => {
            // TWAP doesn't have a single size — its total size, duration, and
            // slice cadence are all collected in the modal. Show "TWAP"
            // underlined in the size slot as a visual hint that the normal
            // size entry doesn't apply.
            let qty_line = if matches!(trading.order_kind, OrderKind::Twap) {
                Line::from(vec![
                    Span::styled(format!(" {}:", tp_s.size), trade_field_label_style),
                    Span::styled(" ", Style::default()),
                    Span::styled(
                        tp_s.twap,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!(" {}:", tp_s.size), trade_field_label_style),
                    Span::styled(" ", Style::default()),
                    Span::styled(
                        format!("{}", trading.order_size()),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                    Span::styled(format!(" {} ", symbol), Style::default().fg(Color::White)),
                    Span::styled(
                        "[s]",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            };
            f.render_widget(Paragraph::new(qty_line), row1_cols[0]);

            // TWAP has no single price; the left column already shows
            // "TWAP" as the type indicator, so leave the price column blank
            // rather than printing a misleading "Px: TWAP".
            if matches!(trading.order_kind, OrderKind::Twap) {
                return;
            }
            let mut price_spans = vec![Span::styled(
                format!("{}:", tp_s.px),
                trade_field_label_style,
            )];
            match trading.order_kind {
                OrderKind::Limit { price } if price.is_finite() && price > 0.0 => {
                    price_spans.push(Span::styled(" ", Style::default().fg(Color::White)));
                    price_spans.push(Span::styled(
                        format!("${}", fmt_price(price, price_decimals)),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                    price_spans.push(Span::styled(
                        " [e]",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                OrderKind::StopMarket { trigger } if trigger.is_finite() && trigger > 0.0 => {
                    price_spans.push(Span::styled(" ", Style::default().fg(Color::White)));
                    price_spans.push(Span::styled(
                        format!("{} ${}", tp_s.stp, fmt_price(trigger, price_decimals)),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                    price_spans.push(Span::styled(
                        " [e]",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                // Twap is short-circuited above; this match never sees it.
                OrderKind::Twap => unreachable!(),
                _ => {
                    price_spans.push(Span::styled(" ", Style::default().fg(Color::White)));
                    price_spans.push(Span::styled(
                        tp_s.mkt,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                }
            }
            f.render_widget(
                Paragraph::new(Line::from(price_spans)).alignment(Alignment::Right),
                row1_cols[1],
            );
        }
    }
}
