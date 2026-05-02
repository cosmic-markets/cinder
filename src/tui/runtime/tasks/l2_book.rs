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

/// How often to poll the Phoenix market account for L2 book state.
///
/// We use HTTP `getAccount` polling instead of `accountSubscribe` because the
/// market account is large (sokoban order tree, tens-to-hundreds of KB) and
/// rewritten on nearly every slot of a busy market. `accountSubscribe` pushes
/// the *full* account body on every notification and the server cadence is not
/// bounded by client commitment level, so egress can hit tens of Mbps. Polling
/// gives a hard ceiling: `account_size × 1/INTERVAL`. 500 ms is responsive
/// enough for a TUI book and keeps the worst-case bandwidth bounded.
const L2_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Maintain an L2 book for the current market by polling the Phoenix market
/// (orderbook) account over HTTP at a fixed cadence. Switches markets via the
/// `cfg_rx` watch; RPC URL changes are handled by the event loop respawning
/// this task on `ReconnectRpc`.
pub(in crate::tui::runtime) fn spawn_phoenix_l2_book_rpc(
    mut cfg_rx: watch::Receiver<SplineConfig>,
    l2_tx: tokio::sync::mpsc::UnboundedSender<L2BookStreamMsg>,
    gti_cache: GtiHandle,
    gti_refresh: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let rpc = RpcClient::new_with_commitment(
            rpc_http_url_from_env(),
            CommitmentConfig::confirmed(),
        );

        let mut cfg = cfg_rx.borrow().clone();
        let mut market_pk = match Pubkey::from_str(&cfg.market_pubkey) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(
                    market = %cfg.market_pubkey,
                    error = %e,
                    "invalid market pubkey for L2 book"
                );
                return;
            }
        };
        // Skip emits when the account hasn't changed since the last poll —
        // server-reported context slot is monotonic per market, so an unchanged
        // slot means the orderbook view we already shipped is still current.
        let mut last_slot: u64 = 0;
        let mut poll_ticker = tokio::time::interval(L2_POLL_INTERVAL);
        poll_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                r = cfg_rx.changed() => {
                    if r.is_err() {
                        return;
                    }
                    let new_cfg = cfg_rx.borrow().clone();
                    if new_cfg.market_pubkey != cfg.market_pubkey {
                        match Pubkey::from_str(&new_cfg.market_pubkey) {
                            Ok(pk) => {
                                market_pk = pk;
                                last_slot = 0;
                            }
                            Err(e) => {
                                warn!(
                                    market = %new_cfg.market_pubkey,
                                    error = %e,
                                    "invalid market pubkey for L2 book"
                                );
                                continue;
                            }
                        }
                    }
                    cfg = new_cfg;
                }
                _ = poll_ticker.tick() => {
                    let resp = match rpc
                        .get_account_with_commitment(&market_pk, CommitmentConfig::processed())
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            warn!(
                                symbol = %cfg.symbol,
                                error = %e,
                                "L2 market getAccount failed"
                            );
                            continue;
                        }
                    };
                    if resp.context.slot <= last_slot {
                        continue;
                    }
                    last_slot = resp.context.slot;
                    let Some(account) = resp.value else {
                        continue;
                    };
                    let Some((bids_raw, asks_raw)) = parse_l2_book_from_market_account(
                        account.data,
                        cfg.tick_size,
                        cfg.base_lot_decimals,
                        L2_SNAPSHOT_DEPTH,
                    ) else {
                        continue;
                    };

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
                        return;
                    }
                }
            }
        }
    })
}
