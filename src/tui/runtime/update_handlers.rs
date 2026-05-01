//! Channel update handlers for the runtime event loop.

use std::collections::HashMap;
use std::io::Stdout;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use phoenix_rise::MarketStatsUpdate;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use solana_signer::Signer;

use super::super::config::{current_user_config, SplineConfig};
use super::super::data::position_leaderboard;
use super::super::data::GtiHandle;
use super::super::data::{parse_spline_data, parse_spline_sequence};
use super::super::format::pubkey_trader_short;
use super::super::i18n::strings;
use super::super::state::{
    BalanceUpdate, LiquidationFeedMsg, MarketListUpdate, TuiState, TxStatusMsg,
};
use super::super::trading::{InputMode, OrderInfo, TopPositionEntry, TradingSide};
use super::super::tx::TxContext;
use super::redraw::{redraw_tui, redraw_tui_force};
use super::{tasks, FEED_REDRAW_MIN_INTERVAL};

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_spline_account_update(
    wss_slot: u64,
    data: Vec<u8>,
    cfg: &SplineConfig,
    state: &mut TuiState,
    gti_cache: &GtiHandle,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
    last_seen_seq: &mut Option<(u64, u64)>,
    last_feed_paint: &mut Instant,
) {
    let Some(seq) = parse_spline_sequence(&data) else {
        tracing::warn!(slot = wss_slot, "failed to decode spline payload sequence");
        return;
    };
    if *last_seen_seq == Some(seq) {
        return;
    }
    *last_seen_seq = Some(seq);

    let Some(parsed) = parse_spline_data(&data, cfg.tick_size, cfg.base_lot_decimals) else {
        tracing::warn!(slot = wss_slot, "failed to parse spline payload");
        return;
    };

    if state.switching_to.is_some() {
        state.complete_market_switch();
    }
    if let (Some(bid), Some(ask)) = (parsed.best_bid, parsed.best_ask) {
        state.push_price((bid + ask) / 2.0);
    }
    state.last_parsed = Some(parsed);
    state.last_slot = wss_slot;
    reconcile_active_position_mark(state);

    if matches!(state.trading.input_mode, InputMode::ViewingPositions) {
        if let Some(stats) = state.market_stats.as_ref() {
            state.positions_view.apply_mark_price(stats);
        }
    }

    if last_feed_paint.elapsed() >= FEED_REDRAW_MIN_INTERVAL {
        let gti_guard = gti_cache.read().await;
        state.rebuild_merged_book(
            &cfg.symbol,
            current_user_config().show_clob,
            gti_guard.as_ref(),
        );
        drop(gti_guard);
        redraw_tui(terminal, state, cfg, rpc_host);
        *last_feed_paint = Instant::now();
    }
}

pub(super) fn handle_tx_status_update(
    msg: TxStatusMsg,
    state: &mut TuiState,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
    last_feed_paint: &mut Instant,
) {
    match msg {
        TxStatusMsg::TradeMarker { is_buy } => {
            state.add_trade_marker(is_buy);
        }
        TxStatusMsg::SetStatus { title, detail } => {
            state.trading.status_timestamp = super::super::state::make_status_timestamp();
            if super::super::ui::is_tx_signature_like(detail.as_str()) {
                state.trading.record_ledger(title.clone(), detail.clone());
            }
            state.trading.status_title = title;
            state.trading.status_detail = detail;
        }
    }
    if last_feed_paint.elapsed() >= FEED_REDRAW_MIN_INTERVAL {
        redraw_tui(terminal, state, cfg, rpc_host);
        *last_feed_paint = Instant::now();
    }
}

pub(super) fn handle_position_leaderboard_update(
    mut entries: Vec<TopPositionEntry>,
    state: &mut TuiState,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
) {
    let marks: HashMap<String, f64> = state
        .market_selector
        .markets
        .iter()
        .filter(|m| m.price > 0.0)
        .map(|m| (m.symbol.clone(), m.price))
        .collect();
    for e in entries.iter_mut() {
        if let Some(&mark) = marks.get(&e.symbol) {
            if mark > 0.0 {
                e.notional = e.size * mark;
                e.unrealized_pnl = match e.side {
                    TradingSide::Long => e.size * (mark - e.entry_price),
                    TradingSide::Short => e.size * (e.entry_price - mark),
                };
            }
        }
    }

    if state.trading.wallet_loaded {
        merge_wallet_positions_into_leaderboard(&mut entries, state, &marks);
    }

    entries.sort_by(|a, b| {
        b.notional
            .partial_cmp(&a.notional)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entries.truncate(position_leaderboard::TOP_N_POSITIONS);
    state.top_positions_view.positions = entries;
    state.top_positions_view.loaded = true;
    state.top_positions_view.clamp_index();
    if matches!(state.trading.input_mode, InputMode::ViewingTopPositions) {
        redraw_tui_force(terminal, state, cfg, rpc_host);
    }
}

/// Apply a fresh `LiquidationFeedMsg` to the in-memory feed, then redraw if
/// the modal is currently open. Background pushes (modal closed) just update
/// state silently — the next opening of the modal renders them. Handles both
/// row arrivals and the one-shot backfill-complete signal.
pub(super) fn handle_liquidation_update(
    msg: LiquidationFeedMsg,
    state: &mut TuiState,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
) {
    match msg {
        LiquidationFeedMsg::Entry(entry) => state.liquidation_feed_view.push(entry),
        LiquidationFeedMsg::BackfillComplete => {
            state.liquidation_feed_view.is_backfilling = false;
        }
    }
    if matches!(state.trading.input_mode, InputMode::ViewingLiquidations) {
        redraw_tui_force(terminal, state, cfg, rpc_host);
    }
}

fn merge_wallet_positions_into_leaderboard(
    entries: &mut Vec<TopPositionEntry>,
    state: &TuiState,
    marks: &HashMap<String, f64>,
) {
    let user_authority = state
        .trading
        .keypair
        .as_ref()
        .map(|kp| kp.pubkey().to_string());
    let Some(auth_str) = user_authority else {
        return;
    };

    let user_display = solana_pubkey::Pubkey::from_str(&auth_str)
        .ok()
        .map(|pk| pubkey_trader_short(&pk))
        .unwrap_or_else(|| auth_str.clone());
    entries.retain(|e| e.trader.as_deref() != Some(auth_str.as_str()));

    for p in &state.positions_view.positions {
        let mark = marks.get(&p.symbol).copied().unwrap_or(0.0);
        let (notional, pnl) = if mark > 0.0 {
            (
                p.size * mark,
                match p.side {
                    TradingSide::Long => p.size * (mark - p.entry_price),
                    TradingSide::Short => p.size * (p.entry_price - mark),
                },
            )
        } else {
            (p.notional, p.unrealized_pnl)
        };
        entries.push(TopPositionEntry {
            symbol: p.symbol.clone(),
            trader: Some(auth_str.clone()),
            trader_display: user_display.clone(),
            side: p.side,
            size: p.size,
            entry_price: p.entry_price,
            notional,
            unrealized_pnl: pnl,
        });
    }
}

pub(super) fn handle_wallet_usdc_update(
    bal: f64,
    state: &mut TuiState,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
) {
    if state.trading.wallet_loaded && state.trading.usdc_balance != Some(bal) {
        state.trading.usdc_balance = Some(bal);
        redraw_tui(terminal, state, cfg, rpc_host);
    }
}

pub(super) fn handle_wallet_sol_update(
    bal: f64,
    state: &mut TuiState,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
) {
    if state.trading.wallet_loaded && state.trading.sol_balance != Some(bal) {
        state.trading.sol_balance = Some(bal);
        redraw_tui(terminal, state, cfg, rpc_host);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_tx_context_update(
    wallet: solana_pubkey::Pubkey,
    sym: String,
    ctx: Arc<TxContext>,
    state: &mut TuiState,
    cfg: &SplineConfig,
    blockhash_refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    awaiting_first_tx_ctx: &mut bool,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
) {
    if !state.trading.wallet_loaded || sym != cfg.symbol {
        return;
    }
    let current_wallet = state
        .trading
        .keypair
        .as_ref()
        .and_then(|k| solana_pubkey::Pubkey::from_str(&k.pubkey().to_string()).ok());
    if current_wallet != Some(wallet) {
        return;
    }
    if let Some(h) = blockhash_refresh_handle.take() {
        h.abort();
    }
    *blockhash_refresh_handle = Some(tasks::spawn_blockhash_refresh_task(Arc::clone(&ctx)));
    state.trading.tx_context = Some(ctx);
    if *awaiting_first_tx_ctx {
        *awaiting_first_tx_ctx = false;
        let pk = state.trading.wallet_label.clone();
        if pk.is_empty() {
            state
                .trading
                .set_status_title(strings().st_wallet_connected);
        } else {
            let s = strings();
            state
                .trading
                .set_status_title(format!("{} {}", s.st_wallet_connected_as, pk));
        }
    }
    redraw_tui(terminal, state, cfg, rpc_host);
}

pub(super) fn handle_balance_update(
    update: BalanceUpdate,
    state: &mut TuiState,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
) {
    if !state.trading.wallet_loaded {
        return;
    }
    state.trading.phoenix_balance = Some(update.phoenix_collateral);
    state.trading.position = update.position;

    let market_order: HashMap<&str, usize> = state
        .market_selector
        .markets
        .iter()
        .enumerate()
        .map(|(i, m)| (m.symbol.as_str(), i))
        .collect();
    let mut sorted_positions = update.all_positions;
    sorted_positions.sort_by_key(|p| {
        market_order
            .get(p.symbol.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    state.positions_view.positions = sorted_positions;
    state.positions_view.clamp_index();

    let mark_by_symbol: HashMap<&str, f64> = state
        .market_selector
        .markets
        .iter()
        .filter(|m| m.price > 0.0)
        .map(|m| (m.symbol.as_str(), m.price))
        .collect();
    for p in state.positions_view.positions.iter_mut() {
        if let Some(&mark) = mark_by_symbol.get(p.symbol.as_str()) {
            p.notional = p.size * mark;
            p.unrealized_pnl = match p.side {
                TradingSide::Long => p.size * (mark - p.entry_price),
                TradingSide::Short => p.size * (p.entry_price - mark),
            };
        }
    }
    reconcile_active_position_mark(state);
    redraw_tui(terminal, state, cfg, rpc_host);
}

pub(super) fn handle_orders_update(
    mut orders: Vec<OrderInfo>,
    state: &mut TuiState,
    configs: &HashMap<String, SplineConfig>,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
) {
    if !state.trading.wallet_loaded {
        return;
    }
    for o in orders.iter_mut() {
        if let Some(cfg) = configs.get(&o.symbol) {
            let scale = 10_f64.powi(cfg.base_lot_decimals as i32);
            if scale > 0.0 {
                o.size_remaining /= scale;
                o.initial_size /= scale;
            }
            if o.is_stop_loss && o.price_usd == 0.0 && o.price_ticks > 0 {
                o.price_usd = o.price_ticks as f64
                    * cfg.tick_size as f64
                    * 10_f64.powi(cfg.base_lot_decimals as i32)
                    / 1_000_000.0;
            }
        }
    }

    let market_order: HashMap<&str, usize> = state
        .market_selector
        .markets
        .iter()
        .enumerate()
        .map(|(i, m)| (m.symbol.as_str(), i))
        .collect();
    orders.sort_by_key(|o| {
        let mi = market_order
            .get(o.symbol.as_str())
            .copied()
            .unwrap_or(usize::MAX);
        let sr = match o.side {
            TradingSide::Long => 0u8,
            TradingSide::Short => 1u8,
        };
        (
            mi,
            sr,
            std::cmp::Reverse(o.price_ticks),
            o.order_sequence_number,
        )
    });
    state.orders_view.orders = orders;
    state.orders_view.clamp_index();
    state.sync_order_chart_markers(&cfg.symbol);
    redraw_tui(terminal, state, cfg, rpc_host);
}

pub(super) fn handle_market_list_update(
    update: MarketListUpdate,
    state: &mut TuiState,
    configs: &mut HashMap<String, SplineConfig>,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
) {
    state.market_selector.add_markets(update.markets);
    configs.extend(update.configs);
    if matches!(state.trading.input_mode, InputMode::SelectingMarket) {
        redraw_tui_force(terminal, state, cfg, rpc_host);
    }
}

pub(super) fn handle_stat_update(
    update: MarketStatsUpdate,
    state: &mut TuiState,
    cfg: &SplineConfig,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    rpc_host: &str,
    last_feed_paint: &mut Instant,
) {
    state.market_selector.update_stat(&update);

    let is_active_market = update.symbol == cfg.symbol;
    let pv_touched = state.positions_view.apply_mark_price(&update);
    if is_active_market {
        state.market_stats = Some(update);
        reconcile_active_position_mark(state);
    } else if matches!(state.trading.input_mode, InputMode::SelectingMarket) {
        redraw_tui_force(terminal, state, cfg, rpc_host);
    }

    let should_redraw_feed = is_active_market
        || (pv_touched && matches!(state.trading.input_mode, InputMode::ViewingPositions));
    if should_redraw_feed && last_feed_paint.elapsed() >= FEED_REDRAW_MIN_INTERVAL {
        redraw_tui(terminal, state, cfg, rpc_host);
        *last_feed_paint = Instant::now();
    }
}

fn reconcile_active_position_mark(state: &mut TuiState) {
    if let Some(pos) = &mut state.trading.position {
        if let Some(mark) = state
            .market_stats
            .as_ref()
            .map(|s| s.mark_price)
            .filter(|m| *m > 0.0)
        {
            pos.notional = pos.size * mark;
            pos.unrealized_pnl = match pos.side {
                TradingSide::Long => pos.size * (mark - pos.entry_price),
                TradingSide::Short => pos.size * (pos.entry_price - mark),
            };
        }
    }
}
