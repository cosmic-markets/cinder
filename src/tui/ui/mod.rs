//! Ratatui widgets for the SOL spline TUI.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use solana_signer::Signer;

use super::config::SplineConfig;
use super::constants::TOP_N;
use super::state::TuiState;
use super::trading::InputMode;

mod chart;
mod modals;
mod orderbook;
mod status;
mod trade_panel;

const MODAL_BORDER: Color = Color::Rgb(80, 80, 140);
const MODAL_HIGHLIGHT_BG: Color = Color::Rgb(50, 50, 100);

/// Collapse a hostname down to `name.tld`, stripping any subdomains.
fn registered_domain(host: &str) -> String {
    let domain = match host.rmatch_indices('.').nth(1) {
        Some((i, _)) => &host[i + 1..],
        None => host,
    };
    domain.to_string()
}

/// Extract the hostname portion from an HTTP(S)/WS(S) URL or bare host string.
pub fn rpc_host_from_urlish(input: &str) -> String {
    let without_scheme = input
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(input);
    let host_port_path = without_scheme.split('/').next().unwrap_or(without_scheme);
    host_port_path
        .split('@')
        .next_back()
        .unwrap_or(host_port_path)
        .split(':')
        .next()
        .unwrap_or(host_port_path)
        .to_string()
}

/// True when `s` has the shape of a base58 Solana transaction signature.
pub(super) fn is_tx_signature_like(s: &str) -> bool {
    (80..=100).contains(&s.len())
        && !s.contains(' ')
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Build a highlighted confirmation prompt line (`<msg>  [Y]/[N]`).
pub(super) fn confirm_prompt_line(msg: String, bg: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            msg,
            Style::default()
                .fg(Color::White)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "[Y]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[N]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Render the entire TUI frame (order book, price chart, trade panel, funds, status tray, and active modal).
pub fn render_frame(f: &mut Frame, state: &TuiState, cfg: &SplineConfig, rpc_host: &str) {
    let area = f.area();
    let chart_data = state.chart_data();
    let (y_min, y_max) = state.price_bounds();
    let trading = &state.trading;

    // Pre-compute the user's 4-char authority prefix to highlight own CLOB rows.
    let user_trader_prefix: Option<String> = trading
        .keypair
        .as_ref()
        .map(|kp| kp.pubkey().to_string().chars().take(4).collect());

    let fixed_below = 6 + 4; // actions row + status tray
    let available_for_ob = area.height.saturating_sub(fixed_below);

    let data_symmetric = (state.merged_book.ask_rows.len().min(TOP_N) as u16)
        .min(state.merged_book.bid_rows.len().min(TOP_N) as u16);
    let max_rows_per_side = available_for_ob.saturating_sub(4).saturating_sub(6) / 2;
    let symmetric_count = data_symmetric.min(max_rows_per_side).max(1);

    let orderbook_height = 3 + (symmetric_count + 3) + 1 + (symmetric_count + 3);
    let vert_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(orderbook_height),
            Constraint::Length(6),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(area);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(vert_chunks[0]);

    orderbook::render_orderbook(
        f,
        main_chunks[0],
        cfg,
        &state.merged_book,
        state.last_slot,
        &state.market_stats,
        user_trader_prefix.as_deref(),
    );
    chart::render_price_chart(
        f,
        main_chunks[1],
        chart_data,
        y_min,
        y_max,
        &state.trade_markers,
        &state.orders_view.orders,
        &state.order_chart_markers,
        &cfg.symbol,
        cfg.price_decimals,
        &state.chart_clock_hms,
    );

    let actions_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(vert_chunks[1]);

    trade_panel::render_trading_panel(f, actions_row[0], trading, &cfg.symbol, cfg.price_decimals);
    status::render_funds_panel(f, actions_row[1], trading, &state.positions_view);
    status::render_status_tray(f, vert_chunks[2], trading, rpc_host);

    match trading.input_mode {
        InputMode::SelectingMarket => {
            modals::render_market_selector(f, area, &state.market_selector, &cfg.symbol);
        }
        InputMode::ViewingPositions => {
            modals::render_positions_modal(
                f,
                area,
                &state.positions_view,
                &cfg.symbol,
                &state.market_selector,
            );
        }
        InputMode::ViewingTopPositions => {
            modals::render_top_positions_modal(
                f,
                area,
                &state.top_positions_view,
                &state.market_selector,
            );
        }
        InputMode::ViewingLiquidations => {
            modals::render_liquidation_feed_modal(f, area, &state.liquidation_feed_view);
        }
        InputMode::EditingTwap => {
            modals::render_twap_modal(f, area, trading, &cfg.symbol);
        }
        InputMode::ViewingBots => {
            modals::render_bots_modal(f, area, &state.twaps_view, &cfg.symbol);
        }
        InputMode::ViewingOrders => {
            modals::render_orders_modal(f, area, &state.orders_view, &cfg.symbol);
        }
        InputMode::ViewingLedger => {
            modals::render_ledger_modal(f, area, trading);
        }
        InputMode::ConfirmQuit => {
            modals::render_quit_modal(f, area);
        }
        InputMode::ViewingConfig
        | InputMode::EditingRpcUrl
        | InputMode::EditingComputeUnitPrice
        | InputMode::EditingComputeUnitLimit => {
            modals::render_config_modal(f, area, trading);
        }
        InputMode::EditingWalletPath => {
            modals::render_wallet_path_modal(f, area, trading);
        }
        InputMode::ChoosingReferral => {
            modals::render_referral_choice_modal(f, area, trading);
        }
        InputMode::EditingReferralCode => {
            modals::render_referral_code_modal(f, area, trading);
        }
        InputMode::Normal
        | InputMode::EditingSize
        | InputMode::EditingPrice
        | InputMode::EditingDeposit
        | InputMode::EditingWithdraw
        | InputMode::Confirming(_) => {}
    }

    if let Some(sym) = &state.switching_to {
        modals::render_switching_modal(f, area, sym);
    }
}
