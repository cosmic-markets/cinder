//! Spline account subscription loop and TUI event handling.
//!
//! Supports runtime market switching via the [M] hotkey.

use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures_util::StreamExt;
use phoenix_rise::PhoenixHttpClient;
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client_types::config::RpcAccountInfoConfig;
use tokio::sync::watch;
use tracing::warn;

use super::super::config::{
    current_user_config, rpc_http_url_from_env, ws_url_from_env, SplineConfig,
};
use super::super::data::{spawn_gti_loader, GtiHandle};
use super::super::state::{
    L2BookStreamMsg, MarketInfo, MarketListUpdate, MarketStatUpdate, TuiState,
};
use super::super::terminal::TuiTerminal;
use super::super::ui;
use super::{
    connection, keyboard::handle_key_press, new_channels, redraw::redraw_tui_force, tasks,
    update_handlers, KeyAction, FEED_REDRAW_MIN_INTERVAL,
};

pub async fn spawn_spline_poller(
    terminal: TuiTerminal,
    _ws: &Arc<phoenix_rise::PhoenixClient>,
    market_list: Vec<MarketInfo>,
    configs: std::collections::HashMap<String, SplineConfig>,
    mut market_rx: tokio::sync::mpsc::Receiver<MarketListUpdate>,
    mut stat_rx: tokio::sync::mpsc::Receiver<MarketStatUpdate>,
    balance_http: Arc<PhoenixHttpClient>,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let mut cfg = connection::initial_config(&market_list, &configs)?;
    let account_config = RpcAccountInfoConfig {
        encoding: Some(UiAccountEncoding::Base64),
        commitment: Some(CommitmentConfig::processed()),
        ..Default::default()
    };

    let handle = tokio::spawn(async move {
        let mut terminal = terminal;

        let mut configs = configs;
        let mut state = TuiState::new(market_list);
        let (l2_cfg_tx, l2_cfg_rx) = watch::channel(cfg.clone());
        let (l2_book_tx, mut l2_book_rx) =
            tokio::sync::mpsc::unbounded_channel::<L2BookStreamMsg>();
        let gti_cache: GtiHandle = Arc::new(tokio::sync::RwLock::new(None));
        let gti_refresh = Arc::new(tokio::sync::Notify::new());
        let mut gti_loader_task = spawn_gti_loader(
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
        let mut ws_url = ws_url_from_env();
        let mut rpc_host = ui::rpc_host_from_urlish(&rpc_http_url_from_env());
        let mut events = EventStream::new();
        let (channels, receivers) = new_channels();
        let mut rx_status = receivers.rx_status;
        let mut balance_rx = receivers.balance_rx;
        let mut wallet_usdc_rx = receivers.wallet_usdc_rx;
        let mut wallet_sol_rx = receivers.wallet_sol_rx;
        let mut tx_ctx_rx = receivers.tx_ctx_rx;
        let mut orders_rx = receivers.orders_rx;
        let mut top_positions_rx = receivers.top_positions_rx;
        let mut liquidation_rx = receivers.liquidation_rx;
        let mut spline_bootstrap_rx = receivers.spline_bootstrap_rx;

        // The liquidation feed task is independent of any wallet/market and
        // lives for the whole process. Toggling the modal doesn't stop it,
        // so the buffer is warm on first open and survives modal close/reopen.
        let mut liquidation_task = tasks::spawn_liquidation_feed_task(
            ws_url.clone(),
            rpc_http_url_from_env(),
            configs.clone(),
            channels.liquidation_tx.clone(),
        );

        let mut balance_interval = tokio::time::interval(Duration::from_millis(1100));
        balance_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut top_positions_interval = tokio::time::interval(Duration::from_secs(5));
        top_positions_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut top_positions_handle: Option<tokio::task::JoinHandle<()>> = None;
        let clock_start =
            tokio::time::Instant::now() + connection::duration_until_next_utc_second();
        let mut clock_interval = tokio::time::interval_at(clock_start, Duration::from_secs(1));
        clock_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut wallet_wss_handle: Option<tokio::task::JoinHandle<()>> = None;
        let mut balance_fetch_handle: Option<tokio::task::JoinHandle<()>> = None;
        let mut blockhash_refresh_handle: Option<tokio::task::JoinHandle<()>> = None;
        let mut trader_orders_handle: Option<tokio::task::JoinHandle<()>> = None;
        let mut tx_ctx_task: Option<tokio::task::JoinHandle<()>> = None;
        let mut pending_market_switch: Option<String> = None;
        let mut pending_full_reconnect = false;
        let mut awaiting_first_tx_ctx = false;

        'outer: loop {
            let pubsub = match PubsubClient::new(&ws_url).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(url = %ws_url, error = %e, "spline WSS connect failed; retry in 5s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

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
                let mut last_feed_paint = Instant::now()
                    .checked_sub(FEED_REDRAW_MIN_INTERVAL)
                    .unwrap_or_else(Instant::now);
                let mut stream_closed = false;
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
                                                    cfg.price_decimals,
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
                            update_handlers::handle_spline_account_update(
                                wss_slot,
                                data,
                                &cfg,
                                &mut state,
                                &gti_cache,
                                &mut terminal,
                                &rpc_host,
                                &mut last_seen_seq,
                                &mut last_feed_paint,
                            ).await;
                        }

                        status_update = rx_status.recv() => {
                            if let Some(msg) = status_update {
                                update_handlers::handle_tx_status_update(
                                    msg,
                                    &mut state,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                    &mut last_feed_paint,
                                );
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
                            if let Some(entries) = top_update {
                                update_handlers::handle_position_leaderboard_update(
                                    entries,
                                    &mut state,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                );
                            }
                        }

                        wallet_bal = wallet_usdc_rx.recv() => {
                            if let Some(bal) = wallet_bal {
                                update_handlers::handle_wallet_usdc_update(
                                    bal,
                                    &mut state,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                );
                            }
                        }

                        sol_bal = wallet_sol_rx.recv() => {
                            if let Some(bal) = sol_bal {
                                update_handlers::handle_wallet_sol_update(
                                    bal,
                                    &mut state,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                );
                            }
                        }

                        ctx = tx_ctx_rx.recv() => {
                            if let Some((wallet, sym, ctx)) = ctx {
                                update_handlers::handle_tx_context_update(
                                    wallet,
                                    sym,
                                    ctx,
                                    &mut state,
                                    &cfg,
                                    &mut blockhash_refresh_handle,
                                    &mut awaiting_first_tx_ctx,
                                    &mut terminal,
                                    &rpc_host,
                                );
                            }
                        }

                        bal_update = balance_rx.recv() => {
                            if let Some(update) = bal_update {
                                update_handlers::handle_balance_update(
                                    update,
                                    &mut state,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                );
                            }
                        }

                        orders_update = orders_rx.recv() => {
                            if let Some(orders) = orders_update {
                                update_handlers::handle_orders_update(
                                    orders,
                                    &mut state,
                                    &configs,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                );
                            }
                        }

                        liquidation = liquidation_rx.recv() => {
                            if let Some(entry) = liquidation {
                                update_handlers::handle_liquidation_update(
                                    entry,
                                    &mut state,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                );
                            }
                        }

                        boot = spline_bootstrap_rx.recv() => {
                            if let Some(msg) = boot {
                                // Drop stale bootstraps from prior switches and any
                                // bootstrap that lost the race to the first WSS push
                                // (clearing `switching_to`). `accountSubscribe` only
                                // dedupes within a session, so a late-arriving stale
                                // payload could otherwise overwrite live data.
                                if state.switching_to.as_deref() == Some(msg.symbol.as_str()) {
                                    update_handlers::handle_spline_account_update(
                                        msg.slot,
                                        msg.data,
                                        &cfg,
                                        &mut state,
                                        &gti_cache,
                                        &mut terminal,
                                        &rpc_host,
                                        &mut last_seen_seq,
                                        &mut last_feed_paint,
                                    ).await;
                                }
                            }
                        }

                        market_update = market_rx.recv() => {
                            if let Some(update) = market_update {
                                update_handlers::handle_market_list_update(
                                    update,
                                    &mut state,
                                    &mut configs,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                );
                            }
                        }

                        stat = stat_rx.recv() => {
                            if let Some(update) = stat {
                                update_handlers::handle_stat_update(
                                    update,
                                    &mut state,
                                    &cfg,
                                    &mut terminal,
                                    &rpc_host,
                                    &mut last_feed_paint,
                                );
                            }
                        }

                        l2_msg = l2_book_rx.recv() => {
                            let Some(msg) = l2_msg else { continue };
                            connection::handle_l2_book_msg(
                                msg,
                                &mut l2_book_rx,
                                &mut state,
                                &cfg,
                                &gti_cache,
                                &mut terminal,
                                &rpc_host,
                                &mut last_feed_paint,
                            ).await;
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
                if connection::handle_full_rpc_reconnect(
                    &mut pending_full_reconnect,
                    &mut ws_url,
                    &mut rpc_host,
                    &mut l2_book_task,
                    &l2_cfg_tx,
                    &l2_book_tx,
                    &gti_cache,
                    &gti_refresh,
                    &mut wallet_wss_handle,
                    &mut blockhash_refresh_handle,
                    &mut tx_ctx_task,
                    &mut liquidation_task,
                    &mut state,
                    &cfg,
                    &balance_http,
                    &channels,
                    &configs,
                ) {
                    break 'sub;
                }

                if stream_closed {
                    connection::sleep_before_reconnect().await;
                    break 'sub;
                }

                if let Some(new_symbol) = pending_market_switch.take() {
                    if connection::handle_pending_market_switch(
                        new_symbol,
                        &configs,
                        &mut state,
                        &mut cfg,
                        &l2_cfg_tx,
                        &balance_http,
                        &channels,
                        &mut tx_ctx_task,
                    ) {
                        continue 'sub;
                    }
                }

                connection::sleep_before_reconnect().await;
                break 'sub;
            } // end 'sub loop
        }

        connection::cleanup_tasks(
            &mut wallet_wss_handle,
            &mut balance_fetch_handle,
            &mut blockhash_refresh_handle,
            &mut trader_orders_handle,
            &mut tx_ctx_task,
            &mut top_positions_handle,
            &mut l2_book_task,
            &mut gti_loader_task,
            &mut liquidation_task,
            &mut terminal,
        );
    });

    Ok(handle)
}
