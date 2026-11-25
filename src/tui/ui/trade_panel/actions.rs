//! Submit, close, and confirmation action row rendering.

use super::*;
use crate::tui::ui::confirm_prompt_line;

/// Render row 2 of the trading panel: submit/close hints or inline `[Y]/[N]` confirm prompt.
pub(super) fn render_actions(
    f: &mut Frame,
    rows: &[ratatui::layout::Rect],
    trading: &TradingState,
) {
    let tp_s = strings();
    let row2_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_COL_W), Constraint::Min(0)])
        .split(rows[2]);

    let confirm_spec: Option<(String, Color)> = match &trading.input_mode {
        InputMode::Confirming(PendingAction::PlaceOrder { side, size, kind }) => {
            let kind_lbl = match kind {
                OrderKind::Market => tp_s.mkt,
                OrderKind::Limit { .. } => tp_s.lmt,
                OrderKind::StopMarket { .. } => tp_s.stp,
                OrderKind::Twap => tp_s.twap,
            };
            Some((
                format!(" {} {} {size} {kind_lbl}? ", tp_s.confirm, side.label(tp_s)),
                side.color(),
            ))
        }
        InputMode::Confirming(PendingAction::ClosePosition) => {
            Some((format!(" {}? ", tp_s.close_position), Color::Red))
        }
        InputMode::Confirming(PendingAction::ClosePositionBySymbol {
            symbol, side, size, ..
        }) => Some((
            format!(" {} {size} {symbol} {}? ", tp_s.close, side.label(tp_s)),
            Color::Red,
        )),
        InputMode::Confirming(PendingAction::CancelOrder {
            symbol,
            side,
            size,
            price_usd,
            ..
        }) => Some((
            format!(
                " {} {} {size} {symbol} @ ${price_usd:.2}? ",
                tp_s.cancel,
                side.label(tp_s)
            ),
            Color::Red,
        )),
        InputMode::Confirming(PendingAction::CancelAllOrders) => {
            Some((format!(" {}? ", tp_s.cancel_all_orders), Color::Red))
        }
        _ => None,
    };

    if let Some((msg, bg)) = confirm_spec {
        f.render_widget(Paragraph::new(confirm_prompt_line(msg, bg)), rows[2]);
        return;
    }

    let verb = match trading.order_kind {
        OrderKind::Market => tp_s.market_order,
        OrderKind::Limit { .. } => tp_s.limit_order,
        OrderKind::StopMarket { .. } => tp_s.stop_order,
        OrderKind::Twap => tp_s.twap_order,
    };
    let action_left = Line::from(vec![
        Span::styled(
            " [Enter]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {verb}"), Style::default().fg(Color::DarkGray)),
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
