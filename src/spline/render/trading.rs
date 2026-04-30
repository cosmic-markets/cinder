//! Trading panel widget (order side/type/size, position summary).

use phoenix_rise::MarketStatsUpdate;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::super::format::{fmt_pnl_compact, fmt_price, truncate_pubkey};
use super::super::i18n::strings;
use super::super::state::TradingState;
use super::super::trading::{InputMode, OrderKind, PendingAction, TradingSide};

pub(super) fn render_trading_panel(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
    symbol: &str,
    _market_stats: &Option<MarketStatsUpdate>,
    price_decimals: usize,
    _market_pubkey: &str,
) {
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

    // Muted labels (Size:, Price:, etc.) so values read as primary.
    let trade_field_label_style = Style::default().fg(Color::DarkGray);

    // Two-column split: left column = side/qty, right column = type/price.
    // Fixed left width keeps BUY aligned with Qty, and MARKET/LIMIT aligned with
    // Price.
    const LEFT_COL_W: u16 = 28;
    let row0_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_COL_W), Constraint::Min(0)])
        .split(rows[0]);
    let row1_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_COL_W), Constraint::Min(0)])
        .split(rows[1]);

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
            let qty_line = Line::from(vec![
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
            ]);
            f.render_widget(Paragraph::new(qty_line), row1_cols[0]);

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

    // Row 3: position context. Both widgets share the full row — the left-aligned
    // position text can use the whole width so the entry price is never clipped by
    // an arbitrary column boundary, and the right-aligned PnL/Liq renders on top.
    if let Some(pos) = &trading.position {
        let pnl_color = if pos.unrealized_pnl >= 0.0 {
            Color::LightGreen
        } else {
            Color::LightRed
        };
        let pnl_prefix = if pos.unrealized_pnl >= 0.0 {
            "+$"
        } else {
            "-$"
        };

        // Show symbol next to side label when the terminal is wide enough.
        let show_symbol = area.width >= 50;
        let side_label = match pos.side {
            TradingSide::Long => tp_s.long_label,
            TradingSide::Short => tp_s.short_label,
        };
        let mut pos_spans = vec![Span::styled(
            format!(" {}", side_label),
            Style::default()
                .fg(pos.side.color())
                .add_modifier(Modifier::BOLD),
        )];
        if show_symbol {
            pos_spans.push(Span::styled(
                format!(" {}", symbol),
                Style::default().fg(Color::White),
            ));
        }
        pos_spans.push(Span::styled(
            format!(
                " {} @ ${}",
                pos.size,
                fmt_price(pos.entry_price, price_decimals)
            ),
            Style::default().fg(Color::White),
        ));
        let pos_left = Line::from(pos_spans);
        f.render_widget(Paragraph::new(pos_left), rows[3]);

        let mut pnl_spans = vec![Span::styled(
            format!(
                "{}{}",
                pnl_prefix,
                fmt_pnl_compact(pos.unrealized_pnl.abs())
            ),
            Style::default().fg(pnl_color),
        )];

        pnl_spans.push(Span::styled(
            format!(" {}", tp_s.liq),
            trade_field_label_style,
        ));
        match pos.liquidation_price {
            Some(liq) => {
                pnl_spans.push(Span::styled(
                    format!(" ${}", fmt_price(liq, price_decimals)),
                    Style::default().fg(Color::White),
                ));
            }
            None => {
                pnl_spans.push(Span::styled(" N/A", Style::default().fg(Color::White)));
            }
        }

        f.render_widget(
            Paragraph::new(Line::from(pnl_spans)).alignment(Alignment::Right),
            rows[3],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!(" {}", tp_s.no_position),
                Style::default().fg(Color::DarkGray),
            )])),
            rows[3],
        );
    }
}
