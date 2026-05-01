//! Frame, title, side toggle, and order-type controls.

use super::*;

pub(super) fn render_panel_frame(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
    symbol: &str,
) -> Vec<ratatui::layout::Rect> {
    let tp_s = strings();
    let wallet_title_right = if trading.wallet_loaded {
        Line::from(vec![
            Span::styled(
                format!(" {} ", tp_s.wallet),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                truncate_pubkey(&trading.wallet_label),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                " [w] ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Right)
    } else {
        Line::from(vec![
            Span::styled(
                format!(" {} ", tp_s.wallet),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("{} ", tp_s.not_loaded),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                "[w] ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Right)
    };

    // Build the block with an optional bottom-left notional label when a position
    // exists.
    let notional_bottom = trading.position.as_ref().map(|pos| {
        Line::from(vec![
            Span::styled(
                format!(" {} ", tp_s.notional),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("${} ", fmt_price(pos.notional, 2)),
                Style::default().fg(Color::White),
            ),
        ])
        .left_aligned()
    });

    let mut block = Block::default()
        .title(Line::from(vec![
            Span::styled(
                format!(" {} ", tp_s.trade),
                Style::default().fg(Color::White),
            ),
            Span::styled(symbol.to_owned(), Style::default().fg(Color::White)),
        ]))
        .title(wallet_title_right)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    if let Some(notional_line) = notional_bottom {
        block = block.title_bottom(notional_line);
    }

    let lev_bottom = trading.position.as_ref().and_then(|pos| {
        pos.leverage.map(|lev| {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", tp_s.lev),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{:.1}x ", lev), Style::default().fg(Color::White)),
            ])
            .right_aligned()
        })
    });
    if let Some(lev_line) = lev_bottom {
        block = block.title_bottom(lev_line);
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let (long_style, short_style) = match trading.side {
        TradingSide::Long => (
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Red),
        ),
        TradingSide::Short => (
            Style::default().fg(Color::Green),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
    };
    let is_limit = matches!(trading.order_kind, OrderKind::Limit { .. });
    let is_stop = matches!(trading.order_kind, OrderKind::StopMarket { .. });
    let is_market = matches!(trading.order_kind, OrderKind::Market);

    let active_style = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);
    let idle_style = Style::default().fg(Color::DarkGray);
    let market_style = if is_market { active_style } else { idle_style };
    let limit_style = if is_limit { active_style } else { idle_style };
    let stop_style = if is_stop { active_style } else { idle_style };

    // Two-column split: left column = side/qty, right column = type/price.
    // Fixed left width keeps BUY aligned with Qty, and MARKET/LIMIT aligned with
    // Price.
    const LEFT_COL_W: u16 = 28;
    let row0_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_COL_W), Constraint::Min(0)])
        .split(rows[0]);
    // Row 0 left: BUY/SELL toggle.
    let side_toggle = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(format!(" {} ", tp_s.buy), long_style),
        Span::styled(" / ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" {} ", tp_s.sell), short_style),
        Span::styled(
            "  [Tab]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(side_toggle), row0_cols[0]);

    // Row 0 right: MKT / LMT / STP toggle (same pattern as BUY / SELL + hotkey).
    let type_toggle = Line::from(vec![
        Span::styled(format!(" {} ", tp_s.mkt), market_style),
        Span::styled(" / ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" {} ", tp_s.lmt), limit_style),
        Span::styled(" / ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" {} ", tp_s.stp), stop_style),
        Span::styled(
            " [t]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(
        Paragraph::new(type_toggle).alignment(Alignment::Right),
        row0_cols[1],
    );

    rows.to_vec()
}
