//! Spline account subscription loop and TUI event handling.
//!
//! Supports runtime market switching via the [M] hotkey.

use std::io::Stdout;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures_util::StreamExt;
use phoenix_rise::PhoenixHttpClient;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client_types::config::RpcAccountInfoConfig;
use solana_signer::Signer;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tracing::{error, warn};

use super::config::{current_user_config, rpc_http_url_from_env, ws_url_from_env, SplineConfig};
use super::gti::{spawn_gti_loader, GtiHandle};
use super::i18n::strings;
use super::parse::{parse_spline_data, parse_spline_sequence};
use super::render;
use super::state::{
    make_status_timestamp, BalanceUpdate, L2BookStreamMsg, MarketInfo, MarketListUpdate,
    MarketStatUpdate, TuiState, TxStatusMsg,
};
use super::terminal::{restore_terminal, setup_terminal};
use super::trading::{InputMode, OrderInfo, TopPositionEntry, TradingSide};
use super::tx::TxContext;

mod input;
mod submit;
mod tasks;
mod wallet;

/// Full `terminal.draw` at most this often for stream + stats; state still
/// updates every message. Increase (e.g. 150–250ms) if CPU is still high;
/// decrease for snappier visuals.
const FEED_REDRAW_MIN_INTERVAL: Duration = Duration::from_millis(150);

/// Throttle for the Phoenix L2 producer task. Phoenix emits many book deltas
/// per second; anything faster than the TUI redraw cadence is coalesced away
/// downstream, so we cap emission here to avoid the per-delta allocation +
/// channel traffic that was spiking CPU.
pub(super) const L2_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(100);

/// Max L2 levels per side pushed to the TUI. The orderbook only renders `TOP_N`
/// rows; we keep a small cushion so spline/CLOB merge logic has some depth to
/// choose from.
pub(super) const L2_SNAPSHOT_DEPTH: usize = 20;

/// Initial and maximum retry delays for WSS reconnect backoff.
pub(super) const WSS_RETRY_INIT: Duration = Duration::from_secs(2);
pub(super) const WSS_RETRY_CAP: Duration = Duration::from_secs(30);

/// Delay until the next UTC wall-clock second boundary (chart clock string
/// updates on the 1s timer only).
fn duration_until_next_utc_second() -> Duration {
    let nanos = Utc::now().timestamp_subsec_nanos() as u64;
    Duration::from_nanos(1_000_000_000 - nanos)
}

pub(super) enum KeyAction {
    Nothing,
    Redraw,
    BreakInner,
    BreakOuter,
    /// User saved new RPC URL: tear down and rebuild WSS/RPC connections using
    /// the fresh URL.
    ReconnectRpc,
    /// User toggled CLOB orders on/off: start or abort the L2 websocket task
    /// accordingly.
    ToggleClob,
}

/// `TxContext` is built asynchronously; include `wallet` (authority pubkey) +
/// `symbol` so a late completion for a replaced wallet or an old market is
/// ignored rather than misapplied.
pub(super) type TxCtxMsg = (solana_pubkey::Pubkey, String, Arc<TxContext>);

pub(super) struct Channels {
    pub tx_status: UnboundedSender<TxStatusMsg>,
    pub balance_tx: UnboundedSender<BalanceUpdate>,
    pub wallet_usdc_tx: UnboundedSender<f64>,
    pub wallet_sol_tx: UnboundedSender<f64>,
    pub tx_ctx_tx: UnboundedSender<TxCtxMsg>,
    pub orders_tx: UnboundedSender<Vec<OrderInfo>>,
    pub top_positions_tx: UnboundedSender<Vec<TopPositionEntry>>,
}

fn redraw_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &TuiState,
    cfg: &SplineConfig,
    rpc_host: &str,
) {
    // Paint if either source has produced rows; merged_book also reflects CLOB-only
    // state.
    let has_rows = !state.merged_book.bid_rows.is_empty() || !state.merged_book.ask_rows.is_empty();
    if state.last_parsed.is_none() && !has_rows {
        return;
    }
    let chart_data = state.chart_data();
    let (y_min, y_max) = state.price_bounds();
    let _ = terminal.draw(|f| {
        render::render_frame(
            f,
            chart_data,
            y_min,
            y_max,
            cfg,
            &state.merged_book,
            state.last_slot,
            &state.market_stats,
            &state.chart_clock_hms,
            &state.trading,
            &state.trade_markers,
            &state.market_selector,
            &state.positions_view,
            &state.orders_view,
            &state.top_positions_view,
            &state.order_chart_markers,
            rpc_host,
            &state.switching_to,
        );
    });
}

fn redraw_tui_force(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &TuiState,
    cfg: &SplineConfig,
    rpc_host: &str,
) {
    let chart_data = state.chart_data();
    let (y_min, y_max) = state.price_bounds();
    let _ = terminal.draw(|f| {
        render::render_frame(
            f,
            chart_data,
            y_min,
            y_max,
            cfg,
            &state.merged_book,
            state.last_slot,
            &state.market_stats,
            &state.chart_clock_hms,
            &state.trading,
            &state.trade_markers,
            &state.market_selector,
            &state.positions_view,
            &state.orders_view,
            &state.top_positions_view,
            &state.order_chart_markers,
            rpc_host,
            &state.switching_to,
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn handle_key_press(
    key: &crossterm::event::KeyEvent,
    state: &mut TuiState,
    cfg: &SplineConfig,
    configs: &std::collections::HashMap<String, SplineConfig>,
    channels: &Channels,
    ws_url: &str,
    http: Arc<PhoenixHttpClient>,
    wallet_wss_handle: &mut Option<tokio::task::JoinHandle<()>>,
    blockhash_refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    balance_fetch_handle: &mut Option<tokio::task::JoinHandle<()>>,
    trader_orders_handle: &mut Option<tokio::task::JoinHandle<()>>,
    tx_ctx_task: &mut Option<tokio::task::JoinHandle<()>>,
    awaiting_first_tx_ctx: &mut bool,
    pending_market_switch: &mut Option<String>,
) -> KeyAction {
    let mode = state.trading.input_mode.clone();
    match mode {
        InputMode::SelectingMarket => {
            input::handle_market_select_key(key.code, state, cfg, pending_market_switch)
        }
        InputMode::ViewingPositions => {
            input::handle_positions_view_key(key.code, state, cfg, pending_market_switch)
        }
        InputMode::ViewingTopPositions => input::handle_top_positions_view_key(key.code, state),
        InputMode::EditingSize => input::handle_editing_size(key.code, &mut state.trading),
        InputMode::EditingPrice => input::handle_editing_price(key.code, &mut state.trading),
        InputMode::EditingDeposit => input::handle_editing_deposit(key.code, &mut state.trading),
        InputMode::EditingWithdraw => input::handle_editing_withdraw(key.code, &mut state.trading),
        InputMode::ViewingConfig => input::handle_config_view_key(key.code, &mut state.trading),
        InputMode::EditingRpcUrl => input::handle_editing_rpc_url(key.code, &mut state.trading),
        InputMode::EditingWalletPath => input::handle_editing_wallet_path(
            key.code,
            state,
            cfg,
            channels,
            ws_url,
            http,
            wallet_wss_handle,
            balance_fetch_handle,
            trader_orders_handle,
            tx_ctx_task,
            awaiting_first_tx_ctx,
        ),
        InputMode::Confirming(action) => {
            // handle confirming inline
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    submit::execute_confirmed_action(
                        &action,
                        state,
                        cfg,
                        configs,
                        &channels.tx_status,
                    );
                    state.trading.input_mode = InputMode::Normal;
                    KeyAction::Redraw
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    state.trading.input_mode = InputMode::Normal;
                    state
                        .trading
                        .set_status_title(submit::cancel_message(&action, state, cfg));
                    KeyAction::Redraw
                }
                _ => KeyAction::Nothing,
            }
        }
        InputMode::Normal => input::handle_normal_key(
            key,
            state,
            cfg,
            wallet_wss_handle,
            blockhash_refresh_handle,
            balance_fetch_handle,
            trader_orders_handle,
            tx_ctx_task,
            awaiting_first_tx_ctx,
        ),
        InputMode::ViewingOrders => input::handle_orders_view_key(key.code, state),
        InputMode::ViewingLedger => input::handle_ledger_view_key(key.code, &mut state.trading),
        InputMode::ConfirmQuit => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => KeyAction::BreakOuter,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                state.trading.input_mode = InputMode::Normal;
                KeyAction::Redraw
            }
            _ => KeyAction::Nothing,
        },
    }
}

pub async fn spawn_spline_poller(
    _ws: &Arc<phoenix_rise::PhoenixClient>,
    market_list: Vec<MarketInfo>,
    configs: std::collections::HashMap<String, SplineConfig>,
    mut market_rx: tokio::sync::mpsc::Receiver<MarketListUpdate>,
    mut stat_rx: tokio::sync::mpsc::Receiver<MarketStatUpdate>,
    balance_http: Arc<PhoenixHttpClient>,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let initial_symbol = market_list
        .iter()
        .find(|m| m.symbol == "SOL")
        .or_else(|| market_list.first())
        .map(|m| m.symbol.clone())
        .unwrap_or_else(|| "SOL".to_string());
    let mut cfg = configs
        .get(&initial_symbol)
        .cloned()
        .ok_or_else(|| format!("no spline config for initial market {}", initial_symbol))?;
    let account_config = RpcAccountInfoConfig {
        encoding: Some(UiAccountEncoding::Base64),
        commitment: Some(CommitmentConfig::processed()),
        ..Default::default()
    };

    let handle = tokio::spawn(async move {
        let mut terminal = match setup_terminal() {
            Ok(t) => t,
            Err(e) => {
                error!(error = %e, "TUI setup failed");
                return;
            }
        };

        let mut configs = configs;
        let mut state = TuiState::new(market_list);
        let (l2_cfg_tx, l2_cfg_rx) = watch::channel(cfg.clone());
        let (l2_book_tx, mut l2_book_rx) =
            tokio::sync::mpsc::unbounded_channel::<L2BookStreamMsg>();
        let gti_cache: GtiHandle = Arc::new(tokio::sync::RwLock::new(None));
        let gti_refresh = Arc::new(tokio::sync::Notify::new());
        let gti_loader_task = spawn_gti_loader(
            Arc::clone(&gti_cache),
            Arc::clone(&gti_refresh),
            rpc_http_url_from_env,
        );
        let mut l2_book_task = if current_user_config().show_clob {
            tasks::spawn_phoenix_l2_book_rpc(
                l2_cfg_rx,
                l2_book_tx.clone(),
                Arc::clone(&gti_cache),
                Arc::clone(&gti_refresh),
            )
        } else {
            tokio::spawn(async {})
        };
        // Re-read per outer iteration so RPC URL changes picked up live via [c] take
        // effect.
        let mut ws_url = ws_url_from_env();
        let mut rpc_host = render::rpc_host_from_urlish(&rpc_http_url_from_env());
        let mut events = EventStream::new();
        let (tx_status, mut rx_status) = tokio::sync::mpsc::unbounded_channel::<TxStatusMsg>();
        let (balance_tx, mut balance_rx) = tokio::sync::mpsc::unbounded_channel::<BalanceUpdate>();
        let (wallet_usdc_tx, mut wallet_usdc_rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
        let (wallet_sol_tx, mut wallet_sol_rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
        let (tx_ctx_tx, mut tx_ctx_rx) = tokio::sync::mpsc::unbounded_channel::<TxCtxMsg>();
        let (orders_tx, mut orders_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<OrderInfo>>();
        let (top_positions_tx, mut top_positions_rx) =
            tokio::sync::mpsc::unbounded_channel::<Vec<TopPositionEntry>>();

        let channels = Channels {
            tx_status,
            balance_tx,
            wallet_usdc_tx,
            wallet_sol_tx,
            tx_ctx_tx,
            orders_tx,
            top_positions_tx,
        };

        let mut balance_interval = tokio::time::interval(Duration::from_millis(1100));
        balance_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Top-positions fetch: on-chain scan is heavier than the per-wallet HTTP
        // balance poll, so run it less often. The modal is non-critical UI;
        // a stale-by-5s snapshot is fine for a leaderboard view.
        let mut top_positions_interval = tokio::time::interval(Duration::from_secs(5));
        top_positions_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut top_positions_handle: Option<tokio::task::JoinHandle<()>> = None;
        // `interval(1s)` fires immediately then every ~1s monotonic tick — bad for
        // HH:MM:SS display. Start on the next UTC second, then 1s period so the
        // chart clock tracks wall seconds.
        let clock_start = tokio::time::Instant::now() + duration_until_next_utc_second();
        let mut clock_interval = tokio::time::interval_at(clock_start, Duration::from_secs(1));
        clock_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut wallet_wss_handle: Option<tokio::task::JoinHandle<()>> = None;
        let mut balance_fetch_handle: Option<tokio::task::JoinHandle<()>> = None;
        let mut blockhash_refresh_handle: Option<tokio::task::JoinHandle<()>> = None;
        let mut trader_orders_handle: Option<tokio::task::JoinHandle<()>> = None;
        // Previous in-flight TxContext loader. Respawns (market switch / RPC reconnect
        // / wallet reconnect) abort it before spawning a new one so rapid
        // toggling doesn't accumulate HTTP-bound background tasks whose results
        // would be discarded anyway.
        let mut tx_ctx_task: Option<tokio::task::JoinHandle<()>> = None;
        let mut pending_market_switch: Option<String> = None;
        let mut pending_full_reconnect = false;
        let mut awaiting_first_tx_ctx = false;

        'outer: loop {
            // (Re)connect the spline WSS. One connection is shared across market switches;
            // only a genuine stream drop breaks back here to reconnect.
            let pubsub = match PubsubClient::new(&ws_url).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(url = %ws_url, error = %e, "spline WSS connect failed; retry in 5s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            // Subscription loop: reuse the same pubsub connection, re-subscribing when
            // the market changes rather than opening a new TCP connection each time.
            'sub: loop {
                let spline_pk = match Pubkey::from_str(&cfg.spline_collection) {
                    Ok(pk) => pk,
                    Err(e) => {
                        warn!(error = %e, "invalid spline pubkey; retry in 5s");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        break 'sub;
                    }
                };

                let (mut stream, unsub) = match pubsub
                    .account_subscribe(&spline_pk, Some(account_config.clone()))
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(error = %e, "spline account_subscribe failed; reconnecting");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        break 'sub;
                    }
                };

                let mut last_seen_seq: Option<(u64, u64)> = None;
                // Avoid painting on every account/stats tick (can be 50+/s); clock still
                // refreshes 1Hz.
                let mut last_feed_paint = Instant::now()
                    .checked_sub(FEED_REDRAW_MIN_INTERVAL)
                    .unwrap_or_else(Instant::now);
                let mut stream_closed = false;
                // Set to request a full exit from the outer loop. We break the inner loop first
                // so the unsub at the bottom of 'sub runs before we leave 'outer.
                let mut break_outer_requested = false;

                loop {
                    // `biased`: clock first, then keyboard, then feed — so keys stay responsive
                    // under heavy WSS traffic.
                    tokio::select! {
                        biased;

                        _ = clock_interval.tick() => {
                            state.chart_clock_hms = Utc::now().format("%H:%M:%S").to_string();
                            redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                            last_feed_paint = Instant::now();
                        }

                        event = events.next() => {
                            if let Some(Ok(Event::Key(key))) = event {
                                if key.kind == KeyEventKind::Press {
                                    let action = handle_key_press(
                                        &key,
                                        &mut state,
                                        &cfg,
                                        &configs,
                                        &channels,
                                        &ws_url,
                                        Arc::clone(&balance_http),
                                        &mut wallet_wss_handle,
                                        &mut blockhash_refresh_handle,
                                        &mut balance_fetch_handle,
                                        &mut trader_orders_handle,
                                        &mut tx_ctx_task,
                                        &mut awaiting_first_tx_ctx,
                                        &mut pending_market_switch,
                                    );
                                    match action {
                                        KeyAction::Redraw => {
                                            redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                                        }
                                        KeyAction::BreakInner => {
                                            redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                                            break;
                                        }
                                        KeyAction::BreakOuter => {
                                            // Defer actual exit until after unsub() below so the
                                            // server-side spline subscription is torn down cleanly.
                                            break_outer_requested = true;
                                            break;
                                        }
                                        KeyAction::ReconnectRpc => {
                                            pending_full_reconnect = true;
                                            redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                                            break;
                                        }
                                        KeyAction::ToggleClob => {
                                            if current_user_config().show_clob {
                                                l2_book_task = tasks::spawn_phoenix_l2_book_rpc(
                                                    l2_cfg_tx.subscribe(),
                                                    l2_book_tx.clone(),
                                                    Arc::clone(&gti_cache),
                                                    Arc::clone(&gti_refresh),
                                                );
                                            } else {
                                                l2_book_task.abort();
                                                l2_book_task = tokio::spawn(async {});
                                                state.clob_bids.clear();
                                                state.clob_asks.clear();
                                                let gti_guard = gti_cache.read().await;
                                                state.rebuild_merged_book(
                                                    &cfg.symbol,
                                                    false,
                                                    gti_guard.as_ref(),
                                                );
                                                drop(gti_guard);
                                            }
                                            redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                                        }
                                        KeyAction::Nothing => {}
                                    }
                                }
                            }
                        }

                        response = stream.next() => {
                            let Some(response) = response else {
                                stream_closed = true;
                                break;
                            };
                            let wss_slot = response.context.slot;
                            let Some(data) = response.value.data.decode() else {
                                continue;
                            };

                            let Some(seq) = parse_spline_sequence(&data) else {
                                warn!(slot = wss_slot, "failed to decode spline payload sequence");
                                continue;
                            };
                            if last_seen_seq == Some(seq) {
                                continue;
                            }
                            last_seen_seq = Some(seq);

                            let Some(parsed) = parse_spline_data(
                                &data,
                                cfg.tick_size,
                                cfg.base_lot_decimals,
                            ) else {
                                warn!(slot = wss_slot, "failed to parse spline payload");
                                continue;
                            };

                            // First payload after a market switch: flush stale data.
                            if state.switching_to.is_some() {
                                state.complete_market_switch();
                            }

                            if let (Some(bid), Some(ask)) = (parsed.best_bid, parsed.best_ask) {
                                state.push_price((bid + ask) / 2.0);
                            }

                            state.last_parsed = Some(parsed);
                            state.last_slot = wss_slot;

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

                            // Keep the Positions modal row for the active market in sync with the header
                            // position line (both use `market_stats`); stats messages may arrive less often than book.
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
                                redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                                last_feed_paint = Instant::now();
                            }
                        }



                        status_update = rx_status.recv() => {
                            if let Some(msg) = status_update {
                                match msg {
                                    TxStatusMsg::TradeMarker { is_buy } => {
                                        state.add_trade_marker(is_buy);
                                    }
                                    TxStatusMsg::SetStatus { title, detail } => {
                                        state.trading.status_timestamp = make_status_timestamp();
                                        if render::is_tx_signature_like(detail.as_str()) {
                                            state.trading.record_ledger(title.clone(), detail.clone());
                                        }
                                        state.trading.status_title = title;
                                        state.trading.status_detail = detail;
                                    }
                                }
                                if last_feed_paint.elapsed() >= FEED_REDRAW_MIN_INTERVAL {
                                    redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                                    last_feed_paint = Instant::now();
                                }
                            }
                        }

                        _ = balance_interval.tick() => {
                            if let Some(kp) = &state.trading.keypair {
                                let in_flight = balance_fetch_handle
                                    .as_ref()
                                    .is_some_and(|h| !h.is_finished());
                                if !in_flight {
                                    balance_fetch_handle = Some(tasks::spawn_balance_fetch(
                                        Arc::clone(&balance_http),
                                        Arc::clone(kp),
                                        cfg.symbol.clone(),
                                        channels.balance_tx.clone(),
                                    ));
                                }
                            }
                        }

                        _ = top_positions_interval.tick() => {
                            let in_flight = top_positions_handle
                                .as_ref()
                                .is_some_and(|h| !h.is_finished());
                            if !in_flight {
                                // Snapshot the configs + current marks so the task
                                // doesn't need shared-state locks. `market_selector.markets`
                                // carries the latest mark for every market it's tracking.
                                let marks: std::collections::HashMap<String, f64> = state
                                    .market_selector
                                    .markets
                                    .iter()
                                    .filter(|m| m.price > 0.0)
                                    .map(|m| (m.symbol.clone(), m.price))
                                    .collect();
                                top_positions_handle = Some(tasks::spawn_top_positions_refresh(
                                    rpc_http_url_from_env(),
                                    configs.clone(),
                                    marks,
                                    Arc::clone(&gti_cache),
                                    Arc::clone(&gti_refresh),
                                    channels.top_positions_tx.clone(),
                                ));
                            }
                        }

                        top_update = top_positions_rx.recv() => {
                            if let Some(mut entries) = top_update {
                                // Apply live marks here (not in the task) so that
                                // every candidate is ranked by the latest price,
                                // not by whatever the task saw at spawn time.
                                let marks: std::collections::HashMap<String, f64> = state
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

                                // Inject the connected wallet's own positions from
                                // the HTTP snapshot. `ActiveTraderBuffer` only holds
                                // entries for GTI-indexed (active) traders — a trader
                                // who isn't in GTI is called "cold" in Phoenix's own
                                // reference tooling and their positions live only in
                                // their trader account, never in ATB. The HTTP balance
                                // fetch always reads from the trader account, so it
                                // catches both cases. Dedupe on (trader, symbol) so
                                // the user isn't listed twice when they're also in
                                // the ATB scan.
                                if state.trading.wallet_loaded {
                                    use super::format::pubkey_trader_short;
                                    use super::trading::TopPositionEntry;
                                    let user_authority = state
                                        .trading
                                        .keypair
                                        .as_ref()
                                        .map(|kp| {
                                            use solana_signer::Signer;
                                            kp.pubkey().to_string()
                                        });
                                    if let Some(auth_str) = user_authority {
                                        let user_display = solana_pubkey::Pubkey::from_str(&auth_str)
                                            .ok()
                                            .map(|pk| pubkey_trader_short(&pk))
                                            .unwrap_or_else(|| auth_str.clone());
                                        // Drop any ATB-derived duplicates for this
                                        // wallet before merging in the HTTP rows.
                                        entries.retain(|e| {
                                            e.trader.as_deref() != Some(auth_str.as_str())
                                        });
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
                                }

                                entries.sort_by(|a, b| {
                                    b.notional
                                        .partial_cmp(&a.notional)
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                });
                                entries.truncate(super::top_positions::TOP_N_POSITIONS);
                                state.top_positions_view.positions = entries;
                                state.top_positions_view.loaded = true;
                                state.top_positions_view.clamp_index();
                                if matches!(state.trading.input_mode, InputMode::ViewingTopPositions) {
                                    redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                                }
                            }
                        }

                        wallet_bal = wallet_usdc_rx.recv() => {
                            if let Some(bal) = wallet_bal {
                                if !state.trading.wallet_loaded { continue; }
                                if state.trading.usdc_balance != Some(bal) {
                                    state.trading.usdc_balance = Some(bal);
                                    redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                                }
                            }
                        }

                        sol_bal = wallet_sol_rx.recv() => {
                            if let Some(bal) = sol_bal {
                                if !state.trading.wallet_loaded { continue; }
                                if state.trading.sol_balance != Some(bal) {
                                    state.trading.sol_balance = Some(bal);
                                    redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                                }
                            }
                        }

                        ctx = tx_ctx_rx.recv() => {
                            if let Some((wallet, sym, ctx)) = ctx {
                                if !state.trading.wallet_loaded {
                                    continue;
                                }
                                if sym != cfg.symbol {
                                    continue;
                                }
                                // Drop TxContexts built for a prior wallet — rapid disconnect→reconnect
                                // could otherwise install a context signed against a replaced keypair.
                                let current_wallet = state
                                    .trading
                                    .keypair
                                    .as_ref()
                                    .and_then(|k| solana_pubkey::Pubkey::from_str(&k.pubkey().to_string()).ok());
                                if current_wallet != Some(wallet) {
                                    continue;
                                }
                                if let Some(h) = blockhash_refresh_handle.take() {
                                    h.abort();
                                }
                                blockhash_refresh_handle = Some(tasks::spawn_blockhash_refresh_task(Arc::clone(&ctx)));
                                state.trading.tx_context = Some(ctx);
                                if awaiting_first_tx_ctx {
                                    awaiting_first_tx_ctx = false;
                                    let pk = state.trading.wallet_label.clone();
                                    if pk.is_empty() {
                                        state.trading.set_status_title(strings().st_wallet_connected);
                                    } else {
                                        let s = strings();
                                        state.trading.set_status_title(
                                            format!("{} {}", s.st_wallet_connected_as, pk),
                                        );
                                    }
                                }
                                redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                            }
                        }

                        bal_update = balance_rx.recv() => {
                            if let Some(update) = bal_update {
                                if !state.trading.wallet_loaded { continue; }
                                state.trading.phoenix_balance = Some(update.phoenix_collateral);
                                state.trading.position = update.position;
                                let market_order: std::collections::HashMap<&str, usize> = state
                                    .market_selector
                                    .markets
                                    .iter()
                                    .enumerate()
                                    .map(|(i, m)| (m.symbol.as_str(), i))
                                    .collect();
                                let mut sorted_positions = update.all_positions;
                                sorted_positions.sort_by_key(|p| {
                                    market_order.get(p.symbol.as_str()).copied().unwrap_or(usize::MAX)
                                });
                                state.positions_view.positions = sorted_positions;
                                state.positions_view.clamp_index();
                                // HTTP returns notional = size*entry and a server-side uPnL snapshot; the stat/spline
                                // feeds then recompute them from the live mark. Left as-is those two values flip-flop
                                // (HTTP → stat → HTTP …) and the UI flickers. Reconcile to the mark-derived values
                                // here so every source converges on the same numbers.
                                let mark_by_symbol: std::collections::HashMap<&str, f64> = state
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
                                // Always repaint: snapshot may change PnL / rows even when collateral is unchanged.
                                redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                            }
                        }

                        orders_update = orders_rx.recv() => {
                            if let Some(mut orders) = orders_update {
                                if !state.trading.wallet_loaded { continue; }
                                // The WS task carries raw `size_remaining_lots` / `initial_size_lots`
                                // in the f64 fields since it doesn't own `configs`. Convert to base
                                // units now by dividing by 10^base_lot_decimals.
                                for o in orders.iter_mut() {
                                    if let Some(cfg) = configs.get(&o.symbol) {
                                        let scale = 10_f64.powi(cfg.base_lot_decimals as i32);
                                        if scale > 0.0 {
                                            o.size_remaining /= scale;
                                            o.initial_size /= scale;
                                        }
                                        // Synthetic stop-trigger rows arrive with
                                        // `price_usd == 0` — ticks → USD uses the
                                        // same formula as `compute_price_decimals`
                                        // (QUOTE_LOT_DECIMALS = 6).
                                        if o.is_stop_loss && o.price_usd == 0.0 && o.price_ticks > 0 {
                                            o.price_usd = o.price_ticks as f64
                                                * cfg.tick_size as f64
                                                * 10_f64.powi(cfg.base_lot_decimals as i32)
                                                / 1_000_000.0;
                                        }
                                    }
                                }
                                // `Trader.all_orders()` walks a HashMap, so row order between WS
                                // snapshots is non-deterministic without an explicit sort. Key on:
                                //   1. market index (active market first, then market_selector order)
                                //   2. side (BUY before SELL)
                                //   3. price descending (highest price at the top of each side group)
                                //   4. order_sequence_number ascending (unique per market — guarantees
                                //      a total order, so equal-price orders never swap places).
                                let market_order: std::collections::HashMap<&str, usize> = state
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
                                    (mi, sr, std::cmp::Reverse(o.price_ticks), o.order_sequence_number)
                                });
                                state.orders_view.orders = orders;
                                state.orders_view.clamp_index();
                                // Add markers for newly-seen orders at the current right-edge x,
                                // refresh price on existing ones, and drop ones that fell off the
                                // book (fill / cancel). Markers then scroll left via `push_price`.
                                state.sync_order_chart_markers(&cfg.symbol);
                                // Repaint when the chart is visible (markers on the chart changed)
                                // as well as when the Orders modal is open.
                                redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                            }
                        }

                        market_update = market_rx.recv() => {
                            if let Some(update) = market_update {
                                state.market_selector.add_markets(update.markets);
                                configs.extend(update.configs);
                                if matches!(state.trading.input_mode, InputMode::SelectingMarket) {
                                    redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                                }
                            }
                        }

                        stat = stat_rx.recv() => {
                            if let Some(update) = stat {
                                // Update market selector stats for all markets
                                state.market_selector.update_stat(&update);

                                let is_active_market = update.symbol == cfg.symbol;
                                let pv_touched =
                                    state.positions_view.apply_mark_price(&update);

                                // If this update is for the currently viewed market,
                                // also update the TUI's market_stats (replaces the old
                                // duplicate per-market stats subscription)
                                if is_active_market {
                                    state.market_stats = Some(update);
                                    // Refresh active-position notional + uPnL with the latest mark so the
                                    // header row stays consistent with the Positions modal (which already
                                    // recomputes uPnL via `apply_mark_price`).
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
                                } else if matches!(state.trading.input_mode, InputMode::SelectingMarket) {
                                    redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                                }

                                let should_redraw_feed = is_active_market
                                    || (pv_touched
                                        && matches!(
                                            state.trading.input_mode,
                                            InputMode::ViewingPositions
                                        ));
                                if should_redraw_feed
                                    && last_feed_paint.elapsed() >= FEED_REDRAW_MIN_INTERVAL
                                {
                                    redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                                    last_feed_paint = Instant::now();
                                }
                            }
                        }

                        l2_msg = l2_book_rx.recv() => {
                            let Some(mut msg) = l2_msg else { continue };
                            while let Ok(next) = l2_book_rx.try_recv() {
                                msg = next;
                            }
                            if msg.symbol == cfg.symbol {
                                state.clob_bids = msg.bids;
                                state.clob_asks = msg.asks;
                                if last_feed_paint.elapsed() >= FEED_REDRAW_MIN_INTERVAL {
                                    let gti_guard = gti_cache.read().await;
                                    state.rebuild_merged_book(
                                        &cfg.symbol,
                                        current_user_config().show_clob,
                                        gti_guard.as_ref(),
                                    );
                                    drop(gti_guard);
                                    if state.last_parsed.is_some() {
                                        redraw_tui(&mut terminal, &state, &cfg, &rpc_host);
                                    } else {
                                        redraw_tui_force(&mut terminal, &state, &cfg, &rpc_host);
                                    }
                                    last_feed_paint = Instant::now();
                                }
                            }
                        }
                    }
                }
                unsub().await;

                if break_outer_requested {
                    break 'outer;
                }

                // User-saved a new RPC URL: tear down wallet + spline WSS and rebuild
                // using the freshly-read `ws_url_from_env()`. Breaks out of 'sub so
                // the outer loop rebuilds the pubsub client on the new URL.
                if pending_full_reconnect {
                    pending_full_reconnect = false;
                    ws_url = ws_url_from_env();
                    rpc_host = render::rpc_host_from_urlish(&rpc_http_url_from_env());
                    // The L2 book task holds its own pubsub connection; bounce it so the next
                    // connect reads the new `ws_url_from_env()`.
                    l2_book_task.abort();
                    l2_book_task = if current_user_config().show_clob {
                        tasks::spawn_phoenix_l2_book_rpc(
                            l2_cfg_tx.subscribe(),
                            l2_book_tx.clone(),
                            Arc::clone(&gti_cache),
                            Arc::clone(&gti_refresh),
                        )
                    } else {
                        tokio::spawn(async {})
                    };
                    // Force a GTI refresh against the new RPC URL on next notify.
                    gti_refresh.notify_one();
                    if let Some(h) = wallet_wss_handle.take() {
                        h.abort();
                    }
                    if let Some(h) = blockhash_refresh_handle.take() {
                        h.abort();
                    }
                    if let Some(kp) = state.trading.keypair.clone() {
                        state.trading.tx_context = None;
                        let pk_bytes = kp.pubkey().to_bytes();
                        wallet_wss_handle = Some(tasks::spawn_wallet_wss(
                            pk_bytes,
                            ws_url.clone(),
                            channels.wallet_usdc_tx.clone(),
                            channels.wallet_sol_tx.clone(),
                        ));
                        let new_tx_ctx = tasks::spawn_tx_context_task(
                            kp,
                            cfg.symbol.clone(),
                            Arc::clone(&balance_http),
                            channels.tx_ctx_tx.clone(),
                            channels.tx_status.clone(),
                        );
                        if let Some(h) = tx_ctx_task.replace(new_tx_ctx) {
                            h.abort();
                        }
                    }
                    break 'sub;
                }

                if stream_closed {
                    // Feed stream dropped unexpectedly — reconnect the WSS.
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    break 'sub;
                }

                // User-initiated market switch: re-subscribe on the same connection.
                if let Some(new_symbol) = pending_market_switch.take() {
                    if let Some(new_cfg) = configs.get(&new_symbol).cloned() {
                        state.begin_market_switch(&new_cfg.symbol);
                        cfg = new_cfg;
                        let _ = l2_cfg_tx.send(cfg.clone());
                        state.trading.set_status_title(format!(
                            "{} {}",
                            strings().st_switched_to,
                            cfg.symbol
                        ));

                        if let Some(kp) = &state.trading.keypair {
                            state.trading.tx_context = None;
                            let new_tx_ctx = tasks::spawn_tx_context_task(
                                Arc::clone(kp),
                                cfg.symbol.clone(),
                                Arc::clone(&balance_http),
                                channels.tx_ctx_tx.clone(),
                                channels.tx_status.clone(),
                            );
                            if let Some(h) = tx_ctx_task.replace(new_tx_ctx) {
                                h.abort();
                            }
                        }
                        continue 'sub;
                    } else {
                        {
                            let s = strings();
                            state.trading.set_status_title(format!(
                                "{} {}{}",
                                s.st_market_switch_failed,
                                new_symbol,
                                s.st_market_switch_failed_suf
                            ));
                        }
                    }
                }

                tokio::time::sleep(Duration::from_secs(2)).await;
                break 'sub;
            } // end 'sub loop
        }

        if let Some(h) = wallet_wss_handle.take() {
            h.abort();
        }
        if let Some(h) = balance_fetch_handle.take() {
            h.abort();
        }
        if let Some(h) = blockhash_refresh_handle.take() {
            h.abort();
        }
        if let Some(h) = trader_orders_handle.take() {
            h.abort();
        }
        if let Some(h) = tx_ctx_task.take() {
            h.abort();
        }
        if let Some(h) = top_positions_handle.take() {
            h.abort();
        }
        l2_book_task.abort();
        gti_loader_task.abort();
        restore_terminal(&mut terminal);
    });

    Ok(handle)
}
