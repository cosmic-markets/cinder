//! Ratatui widgets for the SOL spline TUI.

use phoenix_rise::MarketStatsUpdate;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Color;
use ratatui::Frame;
use solana_signer::Signer;

use super::config::SplineConfig;
use super::state::{
    LiquidationFeedView, MarketSelector, MergedBook, OrderChartMarker, OrdersView, PositionsView,
    TopPositionsView, TradeMarker, TradingState,
};
use super::trading::InputMode;

mod chart;
mod modals;
mod orderbook;
mod status;
mod trade_panel;

const MODAL_BORDER: Color = Color::Rgb(80, 80, 140);
const MODAL_HIGHLIGHT_BG: Color = Color::Rgb(50, 50, 100);

/// Collapse a hostname down to `name.tld`, stripping any subdomains.
/// e.g. `api.mainnet-beta.solana.com` → `solana.com`.
fn registered_domain(host: &str) -> String {
    let domain = match host.rmatch_indices('.').nth(1) {
        Some((i, _)) => &host[i + 1..],
        None => host,
    };
    domain.to_string()
}

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

pub(super) fn is_tx_signature_like(s: &str) -> bool {
    let len = s.len();
    if !(80..=100).contains(&len) {
        return false;
    }
    if s.contains(' ') {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric())
}

pub fn render_frame(
    f: &mut Frame,
    chart_data: &[(f64, f64)],
    y_min: f64,
    y_max: f64,
    cfg: &SplineConfig,
    merged: &MergedBook,
    wss_slot: u64,
    market_stats: &Option<MarketStatsUpdate>,
    chart_clock_hms: &str,
    trading: &TradingState,
    trade_markers: &[TradeMarker],
    market_selector: &MarketSelector,
    positions_view: &PositionsView,
    orders_view: &OrdersView,
    top_positions_view: &TopPositionsView,
    liquidation_feed_view: &LiquidationFeedView,
    order_chart_markers: &std::collections::HashMap<(String, u8, u64), OrderChartMarker>,
    rpc_host: &str,
    switching_to: &Option<String>,
) {
    use super::constants::TOP_N;

    let area = f.area();

    // Pre-compute the user's 4-char authority prefix so the book-table renderer can
    // mark the user's own CLOB rows with the ">" arrow by comparing trader
    // identity instead of price — which would be ambiguous when multiple
    // traders quote the same tick.
    let user_trader_prefix: Option<String> = trading
        .keypair
        .as_ref()
        .map(|kp| kp.pubkey().to_string().chars().take(4).collect());

    let fixed_below = 6 + 4; // actions row + status tray (two body lines for wrap)
    let available_for_ob = area.height.saturating_sub(fixed_below);

    let data_symmetric =
        (merged.ask_rows.len().min(TOP_N) as u16).min(merged.bid_rows.len().min(TOP_N) as u16);
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
        merged,
        wss_slot,
        market_stats,
        chart_data,
        user_trader_prefix.as_deref(),
    );
    chart::render_price_chart(
        f,
        main_chunks[1],
        chart_data,
        y_min,
        y_max,
        trade_markers,
        &orders_view.orders,
        order_chart_markers,
        &cfg.symbol,
        cfg.price_decimals,
        chart_clock_hms,
    );

    let actions_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(vert_chunks[1]);

    trade_panel::render_trading_panel(
        f,
        actions_row[0],
        trading,
        &cfg.symbol,
        market_stats,
        cfg.price_decimals,
        &cfg.market_pubkey,
    );
    status::render_funds_panel(f, actions_row[1], trading, positions_view, "USDC");
    status::render_status_tray(f, vert_chunks[2], trading, rpc_host);

    if trading.input_mode == InputMode::SelectingMarket {
        modals::render_market_selector(f, area, market_selector, &cfg.symbol);
    }

    if trading.input_mode == InputMode::ViewingPositions {
        modals::render_positions_modal(f, area, positions_view, &cfg.symbol, market_selector);
    }

    if trading.input_mode == InputMode::ViewingTopPositions {
        modals::render_top_positions_modal(f, area, top_positions_view, market_selector);
    }

    if trading.input_mode == InputMode::ViewingLiquidations {
        modals::render_liquidation_feed_modal(f, area, liquidation_feed_view);
    }

    if trading.input_mode == InputMode::ViewingOrders {
        modals::render_orders_modal(f, area, orders_view, &cfg.symbol);
    }

    if trading.input_mode == InputMode::ViewingLedger {
        modals::render_ledger_modal(f, area, trading);
    }

    if trading.input_mode == InputMode::ConfirmQuit {
        modals::render_quit_modal(f, area);
    }

    if matches!(
        trading.input_mode,
        InputMode::ViewingConfig | InputMode::EditingRpcUrl
    ) {
        modals::render_config_modal(f, area, trading);
    }

    if trading.input_mode == InputMode::EditingWalletPath {
        modals::render_wallet_path_modal(f, area, trading);
    }

    if trading.input_mode == InputMode::ChoosingReferral {
        modals::render_referral_choice_modal(f, area, trading);
    }

    if trading.input_mode == InputMode::EditingReferralCode {
        modals::render_referral_code_modal(f, area, trading);
    }

    if let Some(sym) = switching_to {
        modals::render_switching_modal(f, area, sym);
    }
}
