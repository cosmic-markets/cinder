//! Size and price entry row rendering.

use super::*;

/// Render a full-width inline numeric editor for size or limit/stop price.
fn render_inline_editor(
    f: &mut Frame,
    row: ratatui::layout::Rect,
    badge: &str,
    unit: &str,
    buf: &str,
) {
    let tp_s = strings();
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!(" {badge} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {unit} "), Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{buf}_"),
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
    f.render_widget(Paragraph::new(line), row);
}

/// Render row 1 of the trading panel: quantity and price (or inline editor).
pub(super) fn render_order_entry(
    f: &mut Frame,
    rows: &[ratatui::layout::Rect],
    trading: &TradingState,
    symbol: &str,
    price_decimals: usize,
) {
    let tp_s = strings();
    let label_style = Style::default().fg(Color::DarkGray);

    match &trading.input_mode {
        InputMode::EditingSize => {
            render_inline_editor(f, rows[1], tp_s.size, symbol, &trading.input_buffer);
            return;
        }
        InputMode::EditingPrice => {
            render_inline_editor(f, rows[1], tp_s.px, "USD", &trading.input_buffer);
            return;
        }
        _ => {}
    }

    let row1_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_COL_W), Constraint::Min(0)])
        .split(rows[1]);

    let qty_line = if matches!(trading.order_kind, OrderKind::Twap) {
        Line::from(vec![
            Span::styled(format!(" {}:", tp_s.size), label_style),
            Span::raw(" "),
            Span::styled(
                tp_s.twap,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::UNDERLINED),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(format!(" {}:", tp_s.size), label_style),
            Span::raw(" "),
            Span::styled(
                format!("{}", trading.order_size()),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled(format!(" {symbol} "), Style::default().fg(Color::White)),
            Span::styled(
                "[s]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };
    f.render_widget(Paragraph::new(qty_line), row1_cols[0]);

    if matches!(trading.order_kind, OrderKind::Twap) {
        return;
    }

    let mut price_spans = vec![
        Span::styled(format!("{}:", tp_s.px), label_style),
        Span::raw(" "),
    ];
    let underlined_white = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::UNDERLINED);
    let edit_hint = Span::styled(
        " [e]",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    match trading.order_kind {
        OrderKind::Limit { price } if price.is_finite() && price > 0.0 => {
            price_spans.push(Span::styled(
                format!("${}", fmt_price(price, price_decimals)),
                underlined_white,
            ));
            price_spans.push(edit_hint);
        }
        OrderKind::StopMarket { trigger } if trigger.is_finite() && trigger > 0.0 => {
            price_spans.push(Span::styled(
                format!("{} ${}", tp_s.stp, fmt_price(trigger, price_decimals)),
                underlined_white,
            ));
            price_spans.push(edit_hint);
        }
        _ => {
            price_spans.push(Span::styled(tp_s.mkt, underlined_white));
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(price_spans)).alignment(Alignment::Right),
        row1_cols[1],
    );
}
