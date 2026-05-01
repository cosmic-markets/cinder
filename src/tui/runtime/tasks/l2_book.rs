//! Phoenix L2 orderbook task spawner.

use super::*;

/// Resolve raw per-trader L2 levels into display-ready `(price, qty, trader)`
/// tuples using the cached GTI. Returns `had_miss = true` if any trader pointer
/// was unresolved (either because the cache hasn't loaded yet or a new trader
/// registered after the last refresh), so the caller can nudge the loader.
pub(in crate::tui::runtime) fn resolve_levels(
    cache: Option<&crate::tui::data::GtiCache>,
    bids_raw: &[L2Level],
    asks_raw: &[L2Level],
) -> (Vec<ClobLevel>, Vec<ClobLevel>, bool) {
    let mut had_miss = false;
    let mut resolve_side = |raw: &[L2Level]| -> Vec<ClobLevel> {
        raw.iter()
            .filter_map(|lvl| match cache.and_then(|c| c.resolve(lvl.trader_id)) {
                Some(pk) => Some((lvl.price, lvl.qty, pubkey_trader_prefix(&pk))),
                // Drop unresolved rows — they'll reappear on the next emit tick once
                // the loader refreshes. Rendering a placeholder would flash noise.
                None => {
                    had_miss = true;
                    None
                }
            })
            .collect()
    };
    let bids = resolve_side(bids_raw);
    let asks = resolve_side(asks_raw);
    (bids, asks, had_miss)
}

/// Maintain an L2 book for the current market via Solana `accountSubscribe` on
/// the Phoenix market (orderbook) account. Re-subscribes on market switch using
/// the same pubsub connection, and re-reads `ws_url_from_env()` on each
/// reconnect so RPC URL changes take effect.
pub(in crate::tui::runtime) fn spawn_phoenix_l2_book_rpc(
    mut cfg_rx: watch::Receiver<SplineConfig>,
    l2_tx: tokio::sync::mpsc::UnboundedSender<L2BookStreamMsg>,
    gti_cache: GtiHandle,
    gti_refresh: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let account_config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::processed()),
            ..Default::default()
        };
        let mut backoff = WSS_RETRY_INIT;

        'outer: loop {
            let ws_url = ws_url_from_env();
            let pubsub = match PubsubClient::new(&ws_url).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(url = %ws_url, error = %e, "L2 pubsub connect failed; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };
            backoff = WSS_RETRY_INIT;

            'sub: loop {
                let mut cfg = cfg_rx.borrow().clone();
                let market_pk = match Pubkey::from_str(&cfg.market_pubkey) {
                    Ok(pk) => pk,
                    Err(e) => {
                        warn!(
                            market = %cfg.market_pubkey,
                            error = %e,
                            "invalid market pubkey for L2 book"
                        );
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        break 'sub;
                    }
                };

                let (mut stream, unsub) = match pubsub
                    .account_subscribe(&market_pk, Some(account_config.clone()))
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(
                            symbol = %cfg.symbol,
                            error = %e,
                            "L2 market account_subscribe failed; reconnecting"
                        );
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        break 'sub;
                    }
                };

                // Coalesce bursts: keep the latest parsed snapshot and emit on the throttle
                // tick.
                let mut emit_ticker = tokio::time::interval(L2_EMIT_MIN_INTERVAL);
                emit_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut pending: Option<(Vec<L2Level>, Vec<L2Level>)> = None;

                'inner: loop {
                    tokio::select! {
                        r = cfg_rx.changed() => {
                            if r.is_err() {
                                unsub().await;
                                return;
                            }
                            let new_cfg = cfg_rx.borrow().clone();
                            if new_cfg.market_pubkey != cfg.market_pubkey {
                                unsub().await;
                                // Re-subscribe on the same pubsub connection for the new market.
                                continue 'sub;
                            }
                            // Same market but other fields (tick_size, base_lot_decimals,
                            // symbol) may have changed — refresh so we don't parse the next
                            // orderbook payload against stale conversion params.
                            cfg = new_cfg;
                        }
                        response = stream.next() => {
                            let Some(response) = response else {
                                unsub().await;
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                break 'sub;
                            };
                            let Some(data) = response.value.data.decode() else {
                                continue;
                            };
                            if let Some(levels) = parse_l2_book_from_market_account(
                                data,
                                cfg.tick_size,
                                cfg.base_lot_decimals,
                                L2_SNAPSHOT_DEPTH,
                            ) {
                                pending = Some(levels);
                            }
                        }
                        _ = emit_ticker.tick() => {
                            let Some((bids_raw, asks_raw)) = pending.take() else {
                                continue 'inner;
                            };
                            // Resolve trader ids under a shared read lock. Stuck on a miss?
                            // Nudge the loader; it debounces internally.
                            let (bids, asks, had_miss) = {
                                let cache = gti_cache.read().await;
                                resolve_levels(cache.as_ref(), &bids_raw, &asks_raw)
                            };
                            if had_miss {
                                gti_refresh.notify_one();
                            }
                            if l2_tx
                                .send(L2BookStreamMsg {
                                    symbol: cfg.symbol.clone(),
                                    bids,
                                    asks,
                                })
                                .is_err()
                            {
                                unsub().await;
                                return;
                            }
                        }
                    }
                }
            }
            // 'sub fell through: reconnect the pubsub with a fresh env read.
            continue 'outer;
        }
    })
}
