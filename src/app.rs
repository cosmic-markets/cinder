//! Wire HTTP, WebSocket, and the TUI.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use phoenix_rise::{
    PhoenixClient, PhoenixClientEvent, PhoenixClientSubscriptionHandle, PhoenixHttpClient,
    PhoenixSubscription, SubscriptionKey,
};
use tracing::warn;

use crate::tui::math::pct_change_24h;
pub use crate::tui::MarketInfo;
use crate::tui::{
    build_spline_config, compute_price_decimals, restore_terminal, setup_terminal, spawn_splash,
    spawn_spline_poller, MarketListUpdate, MarketStatUpdate, SplineConfig,
};

const MARKETS_POLL_INTERVAL: Duration = Duration::from_secs(60);

struct MarketSnapshot {
    price: f64,
    volume_24h: f64,
    open_interest_usd: f64,
    change_24h: f64,
}

fn compute_change(update: &phoenix_rise::MarketStatsUpdate) -> f64 {
    pct_change_24h(update.mark_price, update.prev_day_mark_price)
}

fn spawn_stat_forwarder(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<PhoenixClientEvent>,
    tx: tokio::sync::mpsc::Sender<MarketStatUpdate>,
) {
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let PhoenixClientEvent::MarketUpdate { update, .. } = event {
                match tx.try_send(update) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        // Consumer behind: drop updates instead of blocking
                        // Phoenix recv loops indefinitely.
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => break,
                }
            }
        }
    });
}

/// Subscribe to market stats for a set of symbols. Waits up to `timeout` for an
/// initial snapshot per symbol, then spawns persistent forwarder tasks that
/// pipe all subsequent updates into `stat_tx`.
///
/// Returns collected snapshots and the subscription handles (must be kept
/// alive).
async fn subscribe_market_stats(
    ws: &PhoenixClient,
    symbols: &[String],
    stat_tx: &tokio::sync::mpsc::Sender<MarketStatUpdate>,
    timeout: Duration,
) -> (
    HashMap<String, MarketSnapshot>,
    Vec<PhoenixClientSubscriptionHandle>,
) {
    let mut handles = Vec::new();
    let mut initial_rxs = Vec::new();

    for sym in symbols {
        match ws
            .subscribe(PhoenixSubscription::Key(SubscriptionKey::market(
                sym.clone(),
            )))
            .await
        {
            Ok((rx, handle)) => {
                initial_rxs.push((sym.clone(), rx));
                handles.push(handle);
            }
            Err(e) => {
                warn!(symbol = %sym, error = %e, "stats subscribe failed");
            }
        }
    }

    let mut snapshots = HashMap::new();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut remaining_rxs = Vec::new();

    for (sym, mut rx) in initial_rxs {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if !left.is_zero() {
            if let Ok(Some(PhoenixClientEvent::MarketUpdate { update, .. })) =
                tokio::time::timeout(left, rx.recv()).await
            {
                snapshots.insert(
                    sym.clone(),
                    MarketSnapshot {
                        price: update.mark_price,
                        volume_24h: update.day_volume_usd,
                        open_interest_usd: update.open_interest * update.mark_price,
                        change_24h: compute_change(&update),
                    },
                );
                // Also forward the captured update into the runtime channel so
                // the TUI's per-symbol stats cache is hot before the first
                // frame renders. Without this, the active market's header
                // briefly shows "Waiting for market data…" until Phoenix's
                // next periodic push (which can be several seconds out).
                let _ = stat_tx.try_send(update);
            }
        }
        remaining_rxs.push((sym, rx));
    }

    for (_sym, rx) in remaining_rxs {
        spawn_stat_forwarder(rx, stat_tx.clone());
    }

    (snapshots, handles)
}

fn tradable(m: &phoenix_rise::ExchangeMarketConfig) -> bool {
    matches!(
        m.market_status,
        phoenix_rise::types::MarketStatus::Active | phoenix_rise::types::MarketStatus::PostOnly
    )
}

fn build_market_infos(
    tradable_markets: &[&phoenix_rise::ExchangeMarketConfig],
    snapshots: &HashMap<String, MarketSnapshot>,
) -> Vec<MarketInfo> {
    let mut infos: Vec<MarketInfo> = tradable_markets
        .iter()
        .map(|m| {
            let max_leverage = m
                .leverage_tiers
                .first()
                .map(|t| t.max_leverage)
                .unwrap_or(1.0);
            let price_decimals = compute_price_decimals(m.tick_size, m.base_lots_decimals);
            snapshots.get(&m.symbol).map_or(
                MarketInfo {
                    symbol: m.symbol.clone(),
                    price: 0.0,
                    volume_24h: 0.0,
                    open_interest_usd: 0.0,
                    max_leverage,
                    change_24h: 0.0,
                    price_decimals,
                },
                |snap| MarketInfo {
                    symbol: m.symbol.clone(),
                    price: snap.price,
                    volume_24h: snap.volume_24h,
                    open_interest_usd: snap.open_interest_usd,
                    max_leverage,
                    change_24h: snap.change_24h,
                    price_decimals,
                },
            )
        })
        .collect();

    infos.sort_by(|a, b| {
        b.volume_24h
            .partial_cmp(&a.volume_24h)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    infos
}

fn build_spline_configs(
    tradable_markets: &[&phoenix_rise::ExchangeMarketConfig],
) -> HashMap<String, SplineConfig> {
    let mut out = HashMap::new();
    for m in tradable_markets {
        match build_spline_config(m) {
            Ok(cfg) => {
                out.insert(m.symbol.clone(), cfg);
            }
            Err(e) => warn!(symbol = %m.symbol, error = %e, "failed to build spline config"),
        }
    }
    out
}

struct LoadedSetup {
    http: Arc<PhoenixHttpClient>,
    ws: Arc<PhoenixClient>,
    market_infos: Vec<MarketInfo>,
    spline_configs: HashMap<String, SplineConfig>,
    symbols: Vec<String>,
    stat_tx: tokio::sync::mpsc::Sender<MarketStatUpdate>,
    stat_rx: tokio::sync::mpsc::Receiver<MarketStatUpdate>,
    // Dropping these unsubscribes the per-market stats streams. Must outlive
    // `run()` — otherwise the order-book header is stuck on "Waiting for
    // market data…" because no stat updates ever flow.
    stat_handles: Vec<PhoenixClientSubscriptionHandle>,
}

async fn load_setup() -> Result<LoadedSetup, Box<dyn std::error::Error>> {
    let http = Arc::new(PhoenixHttpClient::new_from_env()?);
    let ws = Arc::new(PhoenixClient::new_from_env().await?);

    let markets = http.get_markets().await?;
    let tradable_markets: Vec<_> = markets.iter().filter(|m| tradable(m)).collect();
    let symbols: Vec<String> = tradable_markets.iter().map(|m| m.symbol.clone()).collect();

    // Bounded buffer; 512 × stat payload was unused headroom on typical machines.
    let (stat_tx, stat_rx) = tokio::sync::mpsc::channel::<MarketStatUpdate>(128);
    let (snapshots, stat_handles) =
        subscribe_market_stats(&ws, &symbols, &stat_tx, Duration::from_secs(3)).await;

    let market_infos = build_market_infos(&tradable_markets, &snapshots);
    let spline_configs = build_spline_configs(&tradable_markets);

    Ok(LoadedSetup {
        http,
        ws,
        market_infos,
        spline_configs,
        symbols,
        stat_tx,
        stat_rx,
        stat_handles,
    })
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Bring up the alt-screen terminal up-front so the splash can paint over
    // the otherwise blank startup window.
    let terminal = setup_terminal()?;
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let splash = spawn_splash(terminal, stop_rx);

    let setup_result = load_setup().await;

    // Stop the splash and reclaim the terminal regardless of outcome.
    let _ = stop_tx.send(());
    let mut terminal = match splash.await {
        Ok(t) => t,
        Err(e) => {
            crate::tui::cleanup_terminal();
            return Err(e.into());
        }
    };

    let LoadedSetup {
        http,
        ws,
        market_infos,
        spline_configs,
        symbols,
        stat_tx,
        stat_rx,
        stat_handles: _stat_handles,
    } = match setup_result {
        Ok(s) => s,
        Err(e) => {
            restore_terminal(&mut terminal);
            return Err(e);
        }
    };

    let (market_tx, market_rx) = tokio::sync::mpsc::channel::<MarketListUpdate>(16);

    let tui_task = spawn_spline_poller(
        terminal,
        &ws,
        market_infos,
        spline_configs,
        market_rx,
        stat_rx,
        Arc::clone(&http),
    )
    .await?;

    let ws_poll = Arc::clone(&ws);
    let stat_tx_poll = stat_tx.clone();
    tokio::spawn(async move {
        let mut known: HashSet<String> = symbols.into_iter().collect();
        let mut _poll_handles: Vec<PhoenixClientSubscriptionHandle> = Vec::new();
        let mut interval = tokio::time::interval(MARKETS_POLL_INTERVAL);
        interval.tick().await;
        loop {
            interval.tick().await;
            let list = match http.get_markets().await {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "poll get_markets failed");
                    continue;
                }
            };
            let tradable: Vec<_> = list.iter().filter(|m| tradable(m)).collect();
            let mut new_markets = Vec::new();
            let mut new_configs = HashMap::new();
            let mut new_symbols = Vec::new();

            for m in &tradable {
                if known.contains(&m.symbol) {
                    continue;
                }
                known.insert(m.symbol.clone());
                new_symbols.push(m.symbol.clone());

                let max_leverage = m
                    .leverage_tiers
                    .first()
                    .map(|t| t.max_leverage)
                    .unwrap_or(1.0);
                new_markets.push(MarketInfo {
                    symbol: m.symbol.clone(),
                    price: 0.0,
                    volume_24h: 0.0,
                    open_interest_usd: 0.0,
                    max_leverage,
                    change_24h: 0.0,
                    price_decimals: compute_price_decimals(m.tick_size, m.base_lots_decimals),
                });

                if let Ok(cfg) = build_spline_config(m) {
                    new_configs.insert(m.symbol.clone(), cfg);
                }
            }

            if !new_symbols.is_empty() {
                let (_, new_handles) = subscribe_market_stats(
                    &ws_poll,
                    &new_symbols,
                    &stat_tx_poll,
                    Duration::from_secs(5),
                )
                .await;
                _poll_handles.extend(new_handles);

                if market_tx
                    .send(MarketListUpdate {
                        markets: new_markets,
                        configs: new_configs,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    });

    tokio::select! {
        sig = tokio::signal::ctrl_c() => {
            sig?;
        }
        res = tui_task => {
            if let Err(e) = res {
                warn!(error = %e, "tui task ended with join error");
            }
        }
    }
    crate::tui::cleanup_terminal();
    Ok(())
}
