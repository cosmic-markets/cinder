//! Submit, close, and confirmation action row rendering.

use super::*;

pub(super) fn render_actions(
    f: &mut Frame,
    rows: &[ratatui::layout::Rect],
    trading: &TradingState,
) {
    let tp_s = strings();
    let is_limit = matches!(trading.order_kind, OrderKind::Limit { .. });
    let is_stop = matches!(trading.order_kind, OrderKind::StopMarket { .. });
    const LEFT_COL_W: u16 = 28;

    // Row 2: primary action (or confirm overlay).
    let row2_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_COL_W), Constraint::Min(0)])
        .split(rows[2]);

    match &trading.input_mode {
        InputMode::Confirming(PendingAction::PlaceOrder { side, size, kind }) => {
            let side_lbl = match side {
                TradingSide::Long => tp_s.long_label,
                TradingSide::Short => tp_s.short_label,
            };
            let kind_lbl = match kind {
                OrderKind::Market => tp_s.mkt,
                OrderKind::Limit { .. } => tp_s.lmt,
                OrderKind::StopMarket { .. } => tp_s.stp,
            };
            let msg = format!(" {} {} {} {}? ", tp_s.confirm, side_lbl, size, kind_lbl);
            let bg = side.color();
            let line = Line::from(vec![
                Span::styled(
                    msg,
                    Style::default()
                        .fg(Color::White)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  [Y]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("/", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "[N]",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]);
            f.render_widget(Paragraph::new(line), rows[2]);
        }
        InputMode::Confirming(PendingAction::ClosePosition) => {
            let line = Line::from(vec![
                Span::styled(
                    format!(" {}? ", tp_s.close_position),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  [Y]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("/", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "[N]",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]);
            f.render_widget(Paragraph::new(line), rows[2]);
        }
        InputMode::Confirming(PendingAction::ClosePositionBySymbol {
            symbol, side, size, ..
        }) => {
            let side_lbl = match side {
                TradingSide::Long => tp_s.long_label,
                TradingSide::Short => tp_s.short_label,
            };
            let msg = format!(" {} {} {} {}? ", tp_s.close, size, symbol, side_lbl);
            let line = Line::from(vec![
                Span::styled(
                    msg,
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  [Y]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("/", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "[N]",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]);
            f.render_widget(Paragraph::new(line), rows[2]);
        }
        InputMode::Confirming(PendingAction::CancelOrder {
            symbol,
            side,
            size,
            price_usd,
            ..
        }) => {
            let side_lbl = match side {
                TradingSide::Long => tp_s.long_label,
                TradingSide::Short => tp_s.short_label,
            };
            let msg = format!(
                " {} {} {} {} @ ${:.2}? ",
                tp_s.cancel, side_lbl, size, symbol, price_usd
            );
            let line = Line::from(vec![
                Span::styled(
                    msg,
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  [Y]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("/", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "[N]",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]);
            f.render_widget(Paragraph::new(line), rows[2]);
        }
        InputMode::Confirming(PendingAction::CancelAllOrders) => {
            let line = Line::from(vec![
                Span::styled(
                    format!(" {}? ", tp_s.cancel_all_orders),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  [Y]",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("/", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "[N]",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]);
            f.render_widget(Paragraph::new(line), rows[2]);
        }
        _ => {
            let verb = if is_limit {
                tp_s.limit_order
            } else if is_stop {
                tp_s.stop_order
            } else {
                tp_s.market_order
            };
            let action_left = Line::from(vec![
                Span::styled(
                    " [Enter]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}", verb), Style::default().fg(Color::DarkGray)),
            ]);
            f.render_widget(Paragraph::new(action_left), row2_cols[0]);

            if trading.position.is_some() {
                let action_right = Line::from(vec![
                    Span::styled(
                        "[x]",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {}", tp_s.close),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                f.render_widget(
                    Paragraph::new(action_right).alignment(Alignment::Right),
                    row2_cols[1],
                );
            }
        }
    };
}
