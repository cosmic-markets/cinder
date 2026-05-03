//! Status tray and funds panel widgets.

use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::super::format::{fmt_balance, fmt_price};
use super::super::i18n::strings;
use super::super::state::{PositionsView, TradingState};
use super::super::trading::{InputMode, PendingAction};
use super::is_tx_signature_like;

pub(super) fn render_status_tray(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
    rpc_host: &str,
) {
    let s = strings();
    let status_label = Line::from(vec![
        Span::styled(
            format!(" [Cinder {}] ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(format!("{} ", s.status), Style::default().fg(Color::White)),
    ])
    .left_aligned();

    // Ledger sits in the top-right of the status frame so the high-traffic
    // bottom hotkey row stays focused on modal toggles.
    let ledger_top_right = Line::from(vec![
        Span::styled(
            " [L]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} ", s.txid),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .right_aligned();

    let rpc_bottom_left = Line::from(vec![
        Span::styled(
            " [c] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", super::registered_domain(rpc_host)),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    // Hotkey row labels — full forms by default, with compact fallbacks for
    // narrow terminals (e.g. 80x24) where the full row would overflow and
    // truncate. Chinese labels are mostly already short; only the four-glyph
    // "top positions" needs squeezing. Russian uses short Cyrillic abbrevs.
    let full_labels: [&str; 6] = [
        s.orders,
        s.positions,
        s.top_positions_title,
        s.liquidations_title,
        s.markets,
        s.quit,
    ];
    let short_labels: [&str; 6] = match super::super::config::current_user_config().language {
        super::super::config::Language::Chinese => [
            s.orders,
            s.positions,
            "顶级",
            s.liquidations_title,
            s.markets,
            s.quit,
        ],
        super::super::config::Language::English => ["ord", "pos", "top", "liq", "mkt", "quit"],
        super::super::config::Language::Russian => ["орд", "поз", "топ", "лик", "рын", "вых"],
        super::super::config::Language::Spanish => ["ord", "pos", "top", "liq", "mer", "sal"],
    };

    let build_quit_hint = |labels: &[&str; 6]| -> Line<'static> {
        let key_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let label_style = Style::default().fg(Color::DarkGray);
        Line::from(vec![
            Span::styled(" [o]", key_style),
            Span::styled(format!(" {} ", labels[0]), label_style),
            Span::styled("[p]", key_style),
            Span::styled(format!(" {} ", labels[1]), label_style),
            Span::styled("[T]", key_style),
            Span::styled(format!(" {} ", labels[2]), label_style),
            Span::styled("[F]", key_style),
            Span::styled(format!(" {} ", labels[3]), label_style),
            Span::styled("[m]", key_style),
            Span::styled(format!(" {} ", labels[4]), label_style),
            Span::styled("[q]", key_style),
            Span::styled(format!(" {} ", labels[5]), label_style),
        ])
        .right_aligned()
    };

    let full_hint = build_quit_hint(&full_labels);
    // Reserve room for the two border columns plus the bottom-left RPC label.
    let reserved = (rpc_bottom_left.width() as u16).saturating_add(2);
    let quit_hint = if (full_hint.width() as u16).saturating_add(reserved) > area.width {
        build_quit_hint(&short_labels)
    } else {
        full_hint
    };

    let block = Block::default()
        .title_top(status_label)
        .title_top(ledger_top_right)
        .title_bottom(rpc_bottom_left)
        .title_bottom(quit_hint)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let body_rect = ratatui::layout::Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

    // Status message (timestamp + message, wrapped across the available body rows).
    let message = if trading.status_detail.is_empty()
        || is_tx_signature_like(trading.status_detail.as_str())
    {
        trading.status_title.clone()
    } else {
        format!("{} — {}", trading.status_title, trading.status_detail)
    };

    let status_line = Line::from(vec![
        Span::styled(
            format!("{} ", trading.status_timestamp),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(message, Style::default().fg(Color::White)),
    ]);

    if body_rect.height >= 1 {
        f.render_widget(
            Paragraph::new(status_line).wrap(ratatui::widgets::Wrap { trim: false }),
            body_rect,
        );
    }
}

pub(super) fn render_funds_panel(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
    positions_view: &PositionsView,
    _symbol: &str,
) {
    let s = strings();
    let sol_title_right = {
        let sol_text = match trading.sol_balance {
            Some(b) => format!("{:.2}", b),
            None => "—".to_string(),
        };
        Line::from(vec![
            Span::styled(format!(" {} ", sol_text), Style::default().fg(Color::White)),
            Span::styled("SOL ", Style::default().fg(Color::DarkGray)),
        ])
        .alignment(Alignment::Right)
    };

    let block = Block::default()
        .title(Line::from(vec![
            Span::raw(" 💵 "),
            Span::styled(format!("{} ", s.balance), Style::default().fg(Color::White)),
        ]))
        .title(sol_title_right)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

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

    // Label column + right-aligned amount + small inset from the panel edge.
    const FUNDS_LABEL_W: u16 = 12;
    const FUNDS_RIGHT_PAD: u16 = 2;
    let funds_row_constraints = [
        Constraint::Length(FUNDS_LABEL_W),
        Constraint::Min(0),
        Constraint::Length(FUNDS_RIGHT_PAD),
    ];

    let disconnected_value = || {
        Paragraph::new(Line::from(vec![
            Span::styled("$—", Style::default().fg(Color::DarkGray)),
            Span::styled(" USDC", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(Alignment::Right)
    };

    let wallet_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(funds_row_constraints)
        .split(rows[0]);
    let wallet_left = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {}", s.wallet),
        Style::default().fg(Color::DarkGray),
    )]));
    f.render_widget(wallet_left, wallet_cols[0]);
    let wallet_right = if trading.wallet_loaded {
        let wallet_bal_text = match trading.usdc_balance {
            Some(b) => fmt_balance(b),
            None => "—".to_string(),
        };
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("${}", wallet_bal_text),
                Style::default().fg(Color::White),
            ),
            Span::styled(" USDC", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(Alignment::Right)
    } else {
        disconnected_value()
    };
    f.render_widget(wallet_right, wallet_cols[1]);

    let phx_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(funds_row_constraints)
        .split(rows[1]);
    let phx_left = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {}", s.perps),
        Style::default().fg(Color::DarkGray),
    )]));
    f.render_widget(phx_left, phx_cols[0]);
    let phx_right = if trading.wallet_loaded {
        let phx_bal_text = match trading.phoenix_balance {
            Some(b) => fmt_balance(b),
            None => "—".to_string(),
        };
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("${}", phx_bal_text),
                Style::default().fg(Color::White),
            ),
            Span::styled(" USDC", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(Alignment::Right)
    } else {
        disconnected_value()
    };
    f.render_widget(phx_right, phx_cols[1]);

    let pnl_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(funds_row_constraints)
        .split(rows[2]);
    let pnl_left = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {}", s.upnl),
        Style::default().fg(Color::DarkGray),
    )]));
    f.render_widget(pnl_left, pnl_cols[0]);
    let pnl_right = if trading.wallet_loaded && trading.phoenix_balance.is_some() {
        if positions_view.positions.is_empty() {
            disconnected_value()
        } else {
            let agg_pnl = positions_view.aggregate_pnl();
            let (pnl_color, pnl_prefix) = if agg_pnl >= 0.0 {
                (Color::LightGreen, "+$")
            } else {
                (Color::LightRed, "-$")
            };
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{}{}", pnl_prefix, fmt_price(agg_pnl.abs(), 2)),
                    Style::default().fg(pnl_color),
                ),
                Span::styled(" USDC", Style::default().fg(Color::DarkGray)),
            ]))
            .alignment(Alignment::Right)
        }
    } else {
        disconnected_value()
    };
    f.render_widget(pnl_right, pnl_cols[1]);

    let actions_line = match &trading.input_mode {
        InputMode::EditingDeposit => Line::from(vec![
            Span::raw(" "),
            Span::styled(
                " [d] ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}: ", s.amt), Style::default().fg(Color::White)),
            Span::styled(
                format!("{}_", trading.deposit_buffer),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        ]),
        InputMode::EditingWithdraw => Line::from(vec![
            Span::raw(" "),
            Span::styled(
                " [D] ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}: ", s.amt), Style::default().fg(Color::White)),
            Span::styled(
                format!("{}_", trading.withdraw_buffer),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        ]),
        InputMode::Confirming(PendingAction::DepositFunds { amount }) => Line::from(vec![
            Span::styled(
                format!(" {} {}? ", s.deposit, amount),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " [Y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("[N]", Style::default().fg(Color::Red)),
        ]),
        InputMode::Confirming(PendingAction::WithdrawFunds { amount }) => Line::from(vec![
            Span::styled(
                format!(" {} {}? ", s.withdraw, amount),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " [Y]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("[N]", Style::default().fg(Color::Red)),
        ]),
        _ => Line::from(vec![
            Span::styled(
                " [d]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}", s.deposit), Style::default().fg(Color::White)),
            Span::styled(
                " [D]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}", s.withdraw),
                Style::default().fg(Color::White),
            ),
        ]),
    };
    f.render_widget(
        Paragraph::new(actions_line)
            .alignment(ratatui::layout::Alignment::Left)
            .wrap(ratatui::widgets::Wrap { trim: false }),
        rows[3],
    );
}
