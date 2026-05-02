//! Confirmed-action executor and cancel message builder.

use tokio::sync::mpsc::UnboundedSender;

use super::super::config::SplineConfig;
use super::super::i18n::strings;
use super::super::math::ui_size_to_num_base_lots;
use super::super::state::{TuiState, TxStatusMsg};
use super::super::trading::{OrderKind, PendingAction, TradingSide};
use super::super::tx::{
    submit_cancel_orders, submit_close_all_positions, submit_funds_transfer, submit_limit_order,
    submit_market_order, submit_stop_market_order, CancelOrderEntry, ClosePositionEntry,
};
use super::input::num_base_lots_for_close;

/// Map a synthetic stop-trigger row's side back to its `execution_direction`.
/// Placement uses Long→GreaterThan / Short→LessThan (see
/// `submit_stop_market_order`); cancellation must match.
fn stop_direction_for(side: TradingSide) -> phoenix_rise::Direction {
    match side {
        TradingSide::Long => phoenix_rise::Direction::LessThan,
        TradingSide::Short => phoenix_rise::Direction::GreaterThan,
    }
}

fn market_price_for_symbol(state: &TuiState, symbol: &str) -> f64 {
    state
        .market_selector
        .markets
        .iter()
        .find(|m| m.symbol == symbol)
        .map(|m| m.price)
        .filter(|price| price.is_finite() && *price > 0.0)
        .or_else(|| state.price_history.back().copied())
        .unwrap_or(0.0)
}

pub(super) fn execute_confirmed_action(
    action: &PendingAction,
    state: &mut TuiState,
    cfg: &SplineConfig,
    configs: &std::collections::HashMap<String, SplineConfig>,
    tx_status: &UnboundedSender<TxStatusMsg>,
) {
    let Some(kp) = state.trading.keypair.clone() else {
        state
            .trading
            .set_status_title(strings().st_wallet_not_loaded);
        return;
    };

    let Some(ctx) = state.trading.tx_context.clone() else {
        state.trading.set_status_title(strings().st_ctx_loading);
        return;
    };

    if let PendingAction::ClosePositionBySymbol {
        symbol,
        subaccount_index,
        side,
        size,
        position_size_raw,
    } = action
    {
        let Some(market_cfg) = configs.get(symbol) else {
            let s = strings();
            state.trading.set_status_title(format!(
                "{} {}: {}",
                s.st_cannot_close, symbol, s.st_no_market_cfg
            ));
            return;
        };
        let num_base_lots = match num_base_lots_for_close(market_cfg, *size, *position_size_raw) {
            Ok(n) => n,
            Err(e) => {
                state
                    .trading
                    .set_status_title(format!("Invalid close size for {}: {}", symbol, e));
                return;
            }
        };
        {
            let s = strings();
            let side_lbl = match side {
                TradingSide::Long => s.long_label,
                TradingSide::Short => s.short_label,
            };
            state.trading.set_status_title(format!(
                "{} {} {} {}\u{2026}",
                s.st_closing, side_lbl, size, symbol
            ));
        }
        submit_close_all_positions(
            kp,
            ctx,
            vec![ClosePositionEntry {
                symbol: symbol.clone(),
                subaccount_index: *subaccount_index,
                close_side: side.toggle(),
                num_base_lots,
                display_size: *size,
            }],
            cfg.symbol.clone(),
            tx_status.clone(),
        );
        return;
    }

    if matches!(action, PendingAction::CloseAllPositions) {
        let positions = state.positions_view.positions.clone();
        if positions.is_empty() {
            state.trading.set_status_title(strings().st_no_positions);
            return;
        }
        let count = positions.len();
        state.trading.set_status_title(format!(
            "{} {} {}\u{2026}",
            strings().st_closing,
            count,
            strings().st_position_s
        ));
        let entries: Vec<ClosePositionEntry> = positions
            .iter()
            .filter_map(|pos| {
                let market_cfg = configs.get(&pos.symbol)?;
                let num_base_lots =
                    num_base_lots_for_close(market_cfg, pos.size, pos.position_size_raw).ok()?;
                Some(ClosePositionEntry {
                    symbol: pos.symbol.clone(),
                    subaccount_index: pos.subaccount_index,
                    close_side: pos.side.toggle(),
                    num_base_lots,
                    display_size: pos.size,
                })
            })
            .collect();
        if entries.is_empty() {
            state
                .trading
                .set_status_title(strings().st_no_positions_matched);
            return;
        }
        submit_close_all_positions(kp, ctx, entries, cfg.symbol.clone(), tx_status.clone());
        return;
    }

    if let PendingAction::CancelOrder {
        symbol,
        subaccount_index,
        side,
        size,
        price_usd,
        price_ticks,
        order_sequence_number,
        is_stop_loss,
        conditional_order_index,
        conditional_trigger_direction,
    } = action
    {
        {
            let s = strings();
            let side_lbl = match side {
                TradingSide::Long => s.long_label,
                TradingSide::Short => s.short_label,
            };
            state.trading.set_status_title(format!(
                "{} {} {} {} @ ${:.2}\u{2026}",
                s.st_cancelling, side_lbl, size, symbol, price_usd
            ));
        }
        submit_cancel_orders(
            kp,
            ctx,
            vec![CancelOrderEntry {
                symbol: symbol.clone(),
                subaccount_index: *subaccount_index,
                price_ticks: *price_ticks,
                order_sequence_number: *order_sequence_number,
                is_stop_loss: *is_stop_loss,
                stop_direction: if *is_stop_loss {
                    Some(stop_direction_for(*side))
                } else {
                    None
                },
                conditional_order_index: *conditional_order_index,
                conditional_trigger_direction: *conditional_trigger_direction,
            }],
            format!("1 order on {}", symbol),
            tx_status.clone(),
        );
        return;
    }

    if matches!(action, PendingAction::CancelAllOrders) {
        let orders = state.orders_view.orders.clone();
        if orders.is_empty() {
            state.trading.set_status_title(strings().st_no_orders);
            return;
        }
        let count = orders.len();
        state.trading.set_status_title(format!(
            "{} {} {}\u{2026}",
            strings().st_cancelling,
            count,
            strings().st_order_s
        ));
        let entries: Vec<CancelOrderEntry> = orders
            .iter()
            .map(|o| CancelOrderEntry {
                symbol: o.symbol.clone(),
                subaccount_index: o.subaccount_index,
                price_ticks: o.price_ticks,
                order_sequence_number: o.order_sequence_number,
                is_stop_loss: o.is_stop_loss,
                stop_direction: if o.is_stop_loss {
                    Some(stop_direction_for(o.side))
                } else {
                    None
                },
                conditional_order_index: o.conditional_order_index,
                conditional_trigger_direction: o.conditional_trigger_direction,
            })
            .collect();
        submit_cancel_orders(
            kp,
            ctx,
            entries,
            format!("{} order(s)", count),
            tx_status.clone(),
        );
        return;
    }

    match action {
        PendingAction::PlaceOrder { side, size, kind } => {
            let num_base_lots = match ui_size_to_num_base_lots(*size, cfg.base_lot_decimals) {
                Ok(n) => n,
                Err(e) => {
                    state.trading.set_status_title(format!(
                        "{} ({})",
                        strings().st_invalid_size,
                        e
                    ));
                    return;
                }
            };
            let s = strings();
            let side_lbl = match side {
                TradingSide::Long => s.long_label,
                TradingSide::Short => s.short_label,
            };
            match kind {
                OrderKind::Limit { price } => {
                    state.trading.set_status_title(format!(
                        "{} {} {} {} @ ${:.2}\u{2026}",
                        s.st_submitting_limit, side_lbl, size, cfg.symbol, price
                    ));
                    submit_limit_order(
                        kp,
                        ctx,
                        cfg.symbol.clone(),
                        *side,
                        num_base_lots,
                        *price,
                        *size,
                        cfg.isolated_only,
                        cfg.max_leverage,
                        tx_status.clone(),
                    );
                }
                OrderKind::StopMarket { trigger } => {
                    state.trading.set_status_title(format!(
                        "{} {} {} {} @ ${:.2}\u{2026}",
                        s.st_submitting_stop, side_lbl, size, cfg.symbol, trigger
                    ));
                    let subaccount_index = if cfg.isolated_only {
                        state
                            .trading
                            .position
                            .as_ref()
                            .map(|p| p.subaccount_index)
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    submit_stop_market_order(
                        kp,
                        ctx,
                        cfg.symbol.clone(),
                        *side,
                        num_base_lots,
                        *trigger,
                        *size,
                        subaccount_index,
                        cfg.isolated_only,
                        tx_status.clone(),
                    );
                }
                OrderKind::Market => {
                    state.trading.set_status_title(format!(
                        "{} {} {} {}\u{2026}",
                        s.st_submitting, side_lbl, size, cfg.symbol
                    ));
                    let reference_price_usd = market_price_for_symbol(state, &cfg.symbol);
                    submit_market_order(
                        kp,
                        ctx,
                        cfg.symbol.clone(),
                        *side,
                        num_base_lots,
                        false,
                        *size,
                        0,
                        cfg.isolated_only,
                        cfg.max_leverage,
                        reference_price_usd,
                        tx_status.clone(),
                    );
                }
            }
        }
        PendingAction::ClosePosition => {
            let Some(pos) = state.trading.position.clone() else {
                return;
            };
            let close_side = pos.side.toggle();
            let num_base_lots = match num_base_lots_for_close(cfg, pos.size, pos.position_size_raw)
            {
                Ok(n) => n,
                Err(e) => {
                    state
                        .trading
                        .set_status_title(format!("Invalid close size for {}: {}", cfg.symbol, e));
                    return;
                }
            };
            {
                let s = strings();
                let side_lbl = match pos.side {
                    TradingSide::Long => s.long_label,
                    TradingSide::Short => s.short_label,
                };
                state.trading.set_status_title(format!(
                    "{} {} {} {}\u{2026}",
                    s.st_closing, side_lbl, pos.size, cfg.symbol
                ));
            }
            submit_market_order(
                kp,
                ctx,
                cfg.symbol.clone(),
                close_side,
                num_base_lots,
                true,
                pos.size,
                pos.subaccount_index,
                cfg.isolated_only,
                cfg.max_leverage,
                pos.entry_price,
                tx_status.clone(),
            );
        }
        PendingAction::DepositFunds { amount } => {
            state.trading.set_status_title(format!(
                "{} {} USDC\u{2026}",
                strings().st_submitting_deposit,
                amount
            ));
            submit_funds_transfer(kp, ctx, *amount, true, tx_status.clone());
        }
        PendingAction::WithdrawFunds { amount } => {
            state.trading.set_status_title(format!(
                "{} {} USDC\u{2026}",
                strings().st_submitting_withdraw,
                amount
            ));
            submit_funds_transfer(kp, ctx, *amount, false, tx_status.clone());
        }
        PendingAction::CloseAllPositions
        | PendingAction::ClosePositionBySymbol { .. }
        | PendingAction::CancelOrder { .. }
        | PendingAction::CancelAllOrders => {
            state
                .trading
                .set_status_title("Action was already handled; no transaction was sent");
        }
    }
}

pub(super) fn cancel_message(
    action: &PendingAction,
    state: &TuiState,
    cfg: &SplineConfig,
) -> String {
    let s = strings();
    let side_lbl = |side: &TradingSide| match side {
        TradingSide::Long => s.long_label,
        TradingSide::Short => s.short_label,
    };
    match action {
        PendingAction::PlaceOrder { side, size, kind } => match kind {
            OrderKind::Limit { price } => format!(
                "{} {} {} {} {} @ ${:.2}",
                s.st_cancelled,
                s.lmt,
                side_lbl(side),
                size,
                cfg.symbol,
                price
            ),
            OrderKind::StopMarket { trigger } => format!(
                "{} {} {} {} {} @ ${:.2}",
                s.st_cancelled,
                s.stp,
                side_lbl(side),
                size,
                cfg.symbol,
                trigger
            ),
            OrderKind::Market => format!(
                "{} {} {} {}",
                s.st_cancelled,
                side_lbl(side),
                size,
                cfg.symbol
            ),
        },
        PendingAction::DepositFunds { amount } => {
            format!(
                "{} {:.2} {}",
                s.st_cancelled, amount, s.st_usdc_deposit_noun
            )
        }
        PendingAction::WithdrawFunds { amount } => {
            format!(
                "{} {:.2} {}",
                s.st_cancelled, amount, s.st_usdc_withdraw_noun
            )
        }
        PendingAction::ClosePosition => {
            if let Some(pos) = &state.trading.position {
                format!(
                    "{} {} {} {} {}",
                    s.st_cancelled,
                    s.close,
                    pos.size,
                    cfg.symbol,
                    side_lbl(&pos.side)
                )
            } else {
                s.st_cancelled_close_pos.to_string()
            }
        }
        PendingAction::ClosePositionBySymbol {
            symbol, side, size, ..
        } => {
            format!(
                "{} {} {} {} {}",
                s.st_cancelled,
                s.close,
                size,
                symbol,
                side_lbl(side)
            )
        }
        PendingAction::CloseAllPositions => s.st_cancelled_close_all.to_string(),
        PendingAction::CancelOrder {
            symbol,
            side,
            size,
            price_usd,
            ..
        } => {
            format!(
                "{} {} {} {} {} @ ${:.2}",
                s.st_cancelled,
                s.cancel,
                side_lbl(side),
                size,
                symbol,
                price_usd
            )
        }
        PendingAction::CancelAllOrders => s.st_cancelled_cancel_all.to_string(),
    }
}
