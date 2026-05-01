//! Active position context row rendering.

use super::*;

pub(super) fn render_position_context(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    rows: &[ratatui::layout::Rect],
    trading: &TradingState,
    symbol: &str,
    price_decimals: usize,
) {
    let tp_s = strings();
    let trade_field_label_style = Style::default().fg(Color::DarkGray);

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
