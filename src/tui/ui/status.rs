//! Status tray and funds panel widgets.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::super::config::Language;
use super::super::format::{fmt_balance, fmt_price};
use super::super::i18n::strings;
use super::super::state::{PositionsView, TradingState};
use super::super::trading::{InputMode, PendingAction};
use super::{confirm_prompt_line, is_tx_signature_like};

const FUNDS_LABEL_W: u16 = 12;
const FUNDS_RIGHT_PAD: u16 = 2;

/// Render one label + right-aligned value row inside the funds panel.
fn render_funds_row(f: &mut Frame, row: Rect, label: &str, value: Line<'_>) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(FUNDS_LABEL_W),
            Constraint::Min(0),
            Constraint::Length(FUNDS_RIGHT_PAD),
        ])
        .split(row);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {label}"),
            Style::default().fg(Color::DarkGray),
        ))),
        cols[0],
    );
    f.render_widget(Paragraph::new(value).alignment(Alignment::Right), cols[1]);
}

/// Build a right-aligned `$amount USDC` line (or `$— USDC` when disconnected/absent).
fn usdc_balance_line(wallet_loaded: bool, balance: Option<f64>) -> Line<'static> {
    if !wallet_loaded {
        return Line::from(vec![
            Span::styled("$—", Style::default().fg(Color::DarkGray)),
            Span::styled(" USDC", Style::default().fg(Color::DarkGray)),
        ]);
    }
    let text = balance.map_or_else(|| "—".to_string(), fmt_balance);
    Line::from(vec![
        Span::styled(format!("${text}"), Style::default().fg(Color::White)),
        Span::styled(" USDC", Style::default().fg(Color::DarkGray)),
    ])
}

/// Render the bottom status tray (version, RPC host, hotkey bar, latest status message).
pub(super) fn render_status_tray(
    f: &mut Frame,
    area: Rect,
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

    let labels: [&str; 7] = match trading.config.language {
        Language::Chinese => [
            s.orders,
            s.positions,
            "顶级",
            "强平",
            s.bots_title,
            s.markets,
            s.quit,
        ],
        Language::English => ["orders", "pos", "top", "liqs", "bots", "mkts", "quit"],
        Language::Russian => ["орд", "поз", "топ", "лик", "боты", "рын", "вых"],
        Language::Spanish => ["ord", "pos", "top", "liqs", "bots", "mer", "sal"],
    };

    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(Color::DarkGray);
    let keys = [" [o]", "[p]", "[T]", "[F]", "[b]", "[m]", "[q]"];
    let mut hint_spans = Vec::with_capacity(keys.len() * 2);
    for (k, lbl) in keys.iter().zip(labels.iter()) {
        hint_spans.push(Span::styled(*k, key_style));
        hint_spans.push(Span::styled(format!(" {lbl} "), label_style));
    }
    let quit_hint = Line::from(hint_spans).right_aligned();

    let block = Block::default()
        .title_top(status_label)
        .title_top(ledger_top_right)
        .title_bottom(rpc_bottom_left)
        .title_bottom(quit_hint)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let body_rect = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

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
            Paragraph::new(status_line).wrap(Wrap { trim: false }),
            body_rect,
        );
    }
}

/// Render the funds panel (wallet USDC, Phoenix perps collateral, aggregate uPnL, deposit/withdraw).
pub(super) fn render_funds_panel(
    f: &mut Frame,
    area: Rect,
    trading: &TradingState,
    positions_view: &PositionsView,
) {
    let s = strings();
    let sol_text = trading
        .sol_balance
        .map_or_else(|| "—".to_string(), |b| format!("{b:.2}"));
    let sol_title_right = Line::from(vec![
        Span::styled(format!(" {sol_text} "), Style::default().fg(Color::White)),
        Span::styled("SOL ", Style::default().fg(Color::DarkGray)),
    ])
    .alignment(Alignment::Right);

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
        .constraints([Constraint::Length(1); 4])
        .split(inner);

    render_funds_row(
        f,
        rows[0],
        s.wallet,
        usdc_balance_line(trading.wallet_loaded, trading.usdc_balance),
    );
    render_funds_row(
        f,
        rows[1],
        s.perps,
        usdc_balance_line(trading.wallet_loaded, trading.phoenix_balance),
    );

    let pnl_line = if trading.wallet_loaded
        && trading.phoenix_balance.is_some()
        && !positions_view.positions.is_empty()
    {
        let agg_pnl = positions_view.aggregate_pnl();
        let (pnl_color, pnl_prefix) = if agg_pnl >= 0.0 {
            (Color::LightGreen, "+$")
        } else {
            (Color::LightRed, "-$")
        };
        Line::from(vec![
            Span::styled(
                format!("{pnl_prefix}{}", fmt_price(agg_pnl.abs(), 2)),
                Style::default().fg(pnl_color),
            ),
            Span::styled(" USDC", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        usdc_balance_line(false, None)
    };
    render_funds_row(f, rows[2], s.upnl, pnl_line);

    let edit_transfer_line = |badge: &'static str, bg: Color, buf: &str| {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                badge,
                Style::default()
                    .fg(Color::Black)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}: ", s.amt), Style::default().fg(Color::White)),
            Span::styled(
                format!("{buf}_"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        ])
    };

    let actions_line = match &trading.input_mode {
        InputMode::EditingDeposit => {
            edit_transfer_line(" [d] ", Color::Green, &trading.deposit_buffer)
        }
        InputMode::EditingWithdraw => {
            edit_transfer_line(" [D] ", Color::Red, &trading.withdraw_buffer)
        }
        InputMode::Confirming(PendingAction::DepositFunds { amount }) => {
            confirm_prompt_line(format!(" {} {amount}? ", s.deposit), Color::Green)
        }
        InputMode::Confirming(PendingAction::WithdrawFunds { amount }) => {
            confirm_prompt_line(format!(" {} {amount}? ", s.withdraw), Color::Red)
        }
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
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        rows[3],
    );
}
