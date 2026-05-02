//! Trader order stream task spawner and row conversion helpers.

use super::*;

/// Convert the phoenix-types `side` string ("Bid" / "Ask") to our internal
/// `TradingSide`. The SDK formats the enum via `{:?}` so variant names come
/// through verbatim.
pub(in crate::tui::runtime) fn trading_side_from_str(s: &str) -> TradingSide {
    match s {
        "Bid" | "Buy" | "Long" => TradingSide::Long,
        _ => TradingSide::Short,
    }
}

fn trigger_side_to_trading_side(side: StopLossTradeSide) -> TradingSide {
    match side {
        StopLossTradeSide::Bid => TradingSide::Long,
        StopLossTradeSide::Ask => TradingSide::Short,
    }
}

fn trigger_direction_to_phoenix(direction: StopLossDirection) -> Direction {
    match direction {
        StopLossDirection::GreaterThan => Direction::GreaterThan,
        StopLossDirection::LessThan => Direction::LessThan,
    }
}

fn conditional_order_type(kind: StopLossOrderKind) -> String {
    match kind {
        StopLossOrderKind::IOC => "Market".to_string(),
        StopLossOrderKind::Limit => "Limit".to_string(),
    }
}

fn push_conditional_trigger_row(
    rows: &mut Vec<OrderInfo>,
    symbol: &str,
    subaccount_index: u8,
    order_index: u8,
    order_sequence_number: u64,
    size_lots: u64,
    trigger: &ConditionalOrderTrigger,
) {
    if !trigger.is_active {
        return;
    }

    rows.push(OrderInfo {
        symbol: symbol.to_string(),
        subaccount_index,
        order_sequence_number,
        side: trigger_side_to_trading_side(trigger.trade_side),
        order_type: conditional_order_type(trigger.order_kind),
        price_usd: 0.0,
        price_ticks: trigger.trigger_price,
        size_remaining: size_lots as f64,
        initial_size: size_lots as f64,
        reduce_only: true,
        is_stop_loss: true,
        conditional_order_index: Some(order_index),
        conditional_trigger_direction: Some(trigger_direction_to_phoenix(
            trigger.execution_direction,
        )),
    });
}

fn conditional_order_rows(
    collection: &ConditionalOrderCollection,
    asset_symbols: &HashMap<u32, String>,
    subaccount_index: u8,
) -> Vec<OrderInfo> {
    let mut rows = Vec::new();
    for (order_index, order) in collection.active_orders() {
        let Some(symbol) = asset_symbols.get(&order.asset_id) else {
            continue;
        };
        let size_lots = order.fillable_size.max(order.max_size);
        let base_sequence = 1_000_000_000 + u64::from(order_index) * 2;
        push_conditional_trigger_row(
            &mut rows,
            symbol,
            subaccount_index,
            order_index,
            base_sequence,
            size_lots,
            &order.greater_trigger_order,
        );
        push_conditional_trigger_row(
            &mut rows,
            symbol,
            subaccount_index,
            order_index,
            base_sequence + 1,
            size_lots,
            &order.less_trigger_order,
        );
    }
    rows
}

async fn fetch_conditional_order_rows(
    rpc: &RpcClient,
    address: &Pubkey,
    asset_symbols: &HashMap<u32, String>,
    subaccount_index: u8,
) -> Vec<OrderInfo> {
    let response = match rpc
        .get_account_with_commitment(address, rpc.commitment())
        .await
    {
        Ok(response) => response,
        Err(err) => {
            warn!(error = %err, "failed to fetch conditional-orders account");
            return Vec::new();
        }
    };
    let Some(account) = response.value.filter(|account| !account.data.is_empty()) else {
        return Vec::new();
    };

    match ConditionalOrderCollection::try_from_account_bytes(&account.data) {
        Ok(collection) => conditional_order_rows(&collection, asset_symbols, subaccount_index),
        Err(err) => {
            warn!(error = %err, "failed to decode conditional-orders account");
            Vec::new()
        }
    }
}

async fn fetch_conditional_order_rows_for_subaccounts(
    rpc: &RpcClient,
    authority: Pubkey,
    subaccount_indexes: impl IntoIterator<Item = u8>,
    asset_symbols: &HashMap<u32, String>,
) -> Vec<OrderInfo> {
    let mut rows = Vec::new();
    for subaccount_index in subaccount_indexes {
        let key = TraderKey::new_with_idx(authority, 0, subaccount_index);
        let address = get_conditional_orders_address(&key.pda());
        rows.extend(
            fetch_conditional_order_rows(rpc, &address, asset_symbols, subaccount_index).await,
        );
    }
    rows
}

fn build_order_rows(
    trader: &Trader,
    stop_triggers: &HashMap<(u8, String, String), TraderStateStopLossTrigger>,
    conditional_orders: &[OrderInfo],
) -> Vec<OrderInfo> {
    let mut orders: Vec<OrderInfo> = trader
        .subaccounts
        .values()
        .flat_map(|sub| {
            sub.orders.values().map(move |o| OrderInfo {
                symbol: o.symbol.clone(),
                subaccount_index: sub.subaccount_index,
                order_sequence_number: o.order_sequence_number,
                side: trading_side_from_str(&o.side),
                order_type: o.order_type.clone(),
                price_usd: o.price_usd.to_string().parse::<f64>().unwrap_or(0.0),
                // `price_ticks` is i64 in the SDK but always non-negative for live orders;
                // clamp at 0 just in case to keep the cast safe.
                price_ticks: o.price_ticks.max(0) as u64,
                // UI size is filled in by the main loop using `configs`; raw lots are
                // carried in the `f64` as a fallback so the modal still shows magnitude.
                size_remaining: o.size_remaining_lots as f64,
                initial_size: o.initial_size_lots as f64,
                reduce_only: o.reduce_only,
                is_stop_loss: o.is_stop_loss,
                conditional_order_index: None,
                conditional_trigger_direction: None,
            })
        })
        .collect();

    // Append synthetic rows for pending stop-loss triggers. These carry no size
    // (size is determined at fire-time from the opposing position) and no USD
    // price — the main loop converts `price_ticks` via the per-symbol config.
    for ((subaccount_index, symbol, stop_id), sl) in stop_triggers {
        let trigger_ticks: u64 = sl.trigger.trigger_price_ticks.parse().unwrap_or(0);
        let osn: u64 = stop_id
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        let side = match sl.trigger.side {
            phoenix_rise::types::Side::Bid => TradingSide::Long,
            phoenix_rise::types::Side::Ask => TradingSide::Short,
        };
        orders.push(OrderInfo {
            symbol: symbol.clone(),
            subaccount_index: *subaccount_index,
            order_sequence_number: osn,
            side,
            order_type: sl.trigger.kind.clone(),
            price_usd: 0.0,
            price_ticks: trigger_ticks,
            size_remaining: 0.0,
            initial_size: 0.0,
            reduce_only: true,
            is_stop_loss: true,
            conditional_order_index: None,
            conditional_trigger_direction: None,
        });
    }

    for conditional in conditional_orders {
        let duplicate = orders.iter().any(|order| {
            order.is_stop_loss
                && order.subaccount_index == conditional.subaccount_index
                && order.symbol == conditional.symbol
                && order.side == conditional.side
                && order.price_ticks == conditional.price_ticks
        });
        if !duplicate {
            orders.push(conditional.clone());
        }
    }

    orders
}

/// Spawn a persistent `PhoenixWSClient` subscription to the wallet's trader
/// state. Each update is applied to a local `Trader`, then `all_orders()` is
/// flattened into `Vec<OrderInfo>` and pushed to the main loop, which owns the
/// `configs` map needed for lot→UI conversion.
///
/// Uses raw base-lots in the payload; the main loop converts to UI units via
/// the live configs.
pub(in crate::tui::runtime) fn spawn_trader_orders_ws(
    kp: Arc<Keypair>,
    orders_tx: UnboundedSender<Vec<OrderInfo>>,
    conditional_asset_symbols: HashMap<u32, String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let authority = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(error = %e, "failed to convert wallet pubkey for trader WS");
                return;
            }
        };

        let mut backoff = WSS_RETRY_INIT;
        loop {
            let client = match PhoenixWSClient::new_from_env() {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, "PhoenixWSClient::new_from_env failed; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };
            let (mut rx, handle) = match client.subscribe_to_trader_state(&authority) {
                Ok(pair) => pair,
                Err(e) => {
                    warn!(error = %e, "subscribe_to_trader_state failed; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };
            backoff = WSS_RETRY_INIT;

            let key = TraderKey::new(authority);
            let conditional_rpc = RpcClient::new_with_commitment(
                rpc_http_url_from_env(),
                CommitmentConfig::processed(),
            );
            let mut trader = Trader::new(key);
            // Stop-loss triggers live on `TraderStatePositionRow`, not in
            // `subaccount.orders`, so the SDK's `Trader::all_orders()` never
            // surfaces them. Track them ourselves from the raw ws payload so
            // we can emit synthetic OrderInfo rows for the orders modal.
            //
            // Keyed by `(subaccount, symbol, stop_loss_id)`. On `Snapshot` we rebuild
            // from scratch; on `Delta`, updated positions replace their
            // subaccount/symbol triggers wholesale (matching
            // `TraderStatePositionRow` semantics, where the triggers field is
            // always the full current set), and closed positions drop their
            // symbol for that subaccount entirely.
            let mut stop_triggers: HashMap<(u8, String, String), TraderStateStopLossTrigger> =
                HashMap::new();
            let mut conditional_orders = fetch_conditional_order_rows_for_subaccounts(
                &conditional_rpc,
                authority,
                [0],
                &conditional_asset_symbols,
            )
            .await;
            let mut conditional_interval = tokio::time::interval(Duration::from_millis(1500));
            conditional_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        let Some(msg) = msg else {
                            break;
                        };
                        trader.apply_update(&msg);

                        match &msg.content {
                            TraderStatePayload::Snapshot(s) => {
                                stop_triggers.clear();
                                for sub in &s.subaccounts {
                                    for pos in &sub.positions {
                                        for sl in &pos.position.stop_loss_triggers {
                                            stop_triggers.insert(
                                                (
                                                    sub.subaccount_index,
                                                    pos.symbol.clone(),
                                                    sl.stop_loss_id.clone(),
                                                ),
                                                sl.clone(),
                                            );
                                        }
                                    }
                                }
                            }
                            TraderStatePayload::Delta(d) => {
                                for sub in &d.deltas {
                                    for pos_delta in &sub.positions {
                                        match pos_delta.change {
                                            TraderStateRowChangeKind::Closed => {
                                                stop_triggers
                                                    .retain(|(sub_idx, sym, _), _| {
                                                        *sub_idx != sub.subaccount_index
                                                            || sym != &pos_delta.symbol
                                                    });
                                            }
                                            TraderStateRowChangeKind::Updated => {
                                                stop_triggers
                                                    .retain(|(sub_idx, sym, _), _| {
                                                        *sub_idx != sub.subaccount_index
                                                            || sym != &pos_delta.symbol
                                                    });
                                                if let Some(row) = &pos_delta.position {
                                                    for sl in &row.stop_loss_triggers {
                                                        stop_triggers.insert(
                                                            (
                                                                sub.subaccount_index,
                                                                pos_delta.symbol.clone(),
                                                                sl.stop_loss_id.clone(),
                                                            ),
                                                            sl.clone(),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if orders_tx.send(build_order_rows(&trader, &stop_triggers, &conditional_orders)).is_err() {
                            // Receiver dropped (poller shutting down).
                            drop(handle);
                            return;
                        }
                    }
                    _ = conditional_interval.tick() => {
                        let mut subaccount_indexes: Vec<u8> =
                            trader.subaccounts.keys().copied().collect();
                        if subaccount_indexes.is_empty() {
                            subaccount_indexes.push(0);
                        }
                        subaccount_indexes.sort_unstable();
                        conditional_orders = fetch_conditional_order_rows_for_subaccounts(
                            &conditional_rpc,
                            authority,
                            subaccount_indexes,
                            &conditional_asset_symbols,
                        )
                        .await;
                        if orders_tx.send(build_order_rows(&trader, &stop_triggers, &conditional_orders)).is_err() {
                            drop(handle);
                            return;
                        }
                    }
                }
            }

            // Stream closed — reconnect. Drop handle first so the old subscription is
            // released.
            drop(handle);
            tokio::time::sleep(backoff).await;
        }
    })
}
