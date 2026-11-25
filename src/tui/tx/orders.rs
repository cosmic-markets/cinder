//! Order submission (market, limit, and stop-market) plus Flight builder
//! routing and isolated-margin collateral estimation.

use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use phoenix_rise::api::PhoenixFlightClient;
use phoenix_rise::core::{BracketLeg, BracketLegOrders, PhoenixTxBuilder};
use phoenix_rise::ix::prelude::{
    LimitOrderParams, MarketOrderParams, OrderFlags, create_place_limit_order_ix,
    create_place_market_order_ix,
};
use phoenix_rise::ix::types::IsolatedCollateralFlow;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use tokio::sync::{mpsc, oneshot};

use super::super::i18n::strings;
use super::super::state::{SliceOutcome, TxStatusMsg};
use super::super::trading::TradingSide;
use super::confirmation::{CuBudget, TxReport, execute_tx};
use super::context::TxContext;
use super::error::parse_phoenix_tx_error;

/// Cosmic Markets registered Phoenix Flight builder authority.
const COSMIC_BUILDER_AUTHORITY: &str = "cosmiccdRa4xgCtPiawLwjJiDZNyukNXiXWa2gwRfyq";
const QUOTE_LOTS_PER_USDC: f64 = 1_000_000.0;
const COLLATERAL_BUFFER: f64 = 1.10;

static FLIGHT_CLIENT: LazyLock<PhoenixFlightClient> = LazyLock::new(|| {
    PhoenixFlightClient::new(
        Pubkey::from_str(COSMIC_BUILDER_AUTHORITY).expect("valid builder authority"),
        0,
        0,
    )
});

/// Wrap each instruction through Phoenix Flight; non-routable ixs pass through unchanged.
pub(super) fn wrap_order_ixs(
    ixs: Vec<Instruction>,
    trader_wallet: Pubkey,
) -> Result<Vec<Instruction>, String> {
    ixs.into_iter()
        .map(|ix| {
            FLIGHT_CLIENT
                .try_wrap_order_instruction(ix, trader_wallet)
                .map_err(|e| e.to_string())
        })
        .collect()
}

/// Estimate the cross-to-isolated collateral transfer (with 10% buffer) for an isolated order.
fn estimate_collateral_transfer(
    display_size: f64,
    reference_price_usd: f64,
    max_leverage: f64,
) -> Result<IsolatedCollateralFlow, String> {
    if !display_size.is_finite() || display_size <= 0.0 {
        return Err("invalid isolated order size".to_string());
    }
    if !reference_price_usd.is_finite() || reference_price_usd <= 0.0 {
        return Err("missing market price for isolated margin estimate".to_string());
    }
    if !max_leverage.is_finite() || max_leverage <= 0.0 {
        return Err("missing max leverage for isolated margin estimate".to_string());
    }

    let collateral_quote_lots = (display_size.abs() * reference_price_usd / max_leverage
        * COLLATERAL_BUFFER
        * QUOTE_LOTS_PER_USDC)
        .ceil();
    if collateral_quote_lots > u64::MAX as f64 {
        return Err("isolated margin estimate is too large".to_string());
    }

    Ok(IsolatedCollateralFlow::TransferFromCrossMargin {
        collateral: collateral_quote_lots.max(1.0) as u64,
    })
}

/// Build standard order status labels for [`execute_tx`].
fn order_report(
    order_summary: &str,
    log_kind: &str,
    trade_markers: Vec<(String, bool)>,
    silent_status: bool,
    outcome_tx: Option<oneshot::Sender<SliceOutcome>>,
) -> TxReport {
    let s = strings();
    TxReport {
        broadcasting_title: format!("{} {order_summary}…", s.tx_broadcasting),
        prepare_failed_title: format!("{} — {order_summary}", s.tx_failed_prepare),
        confirming_title: format!("{} — {order_summary}…", s.tx_awaiting_confirm),
        confirmed_title: format!("{} {order_summary}", s.tx_order_confirmed),
        rejected_title: format!("{} — {order_summary}", s.tx_tx_rejected),
        exec_failed_title: format!("{} — {order_summary}", s.tx_order_failed),
        not_confirmed_prefix: format!("{} — {order_summary}", s.tx_order_not_confirmed),
        log_context: format!("{log_kind} — {order_summary}"),
        trade_markers,
        silent_status,
        record_ledger_on_silent: silent_status,
        outcome_tx,
    }
}

/// Emit a pre-broadcast build error and resolve any pending TWAP slice outcome.
fn fail_build(
    tx_status: &mpsc::UnboundedSender<TxStatusMsg>,
    silent_status: bool,
    outcome_tx: Option<oneshot::Sender<SliceOutcome>>,
    prefix: &str,
    order_summary: &str,
    detail: String,
) {
    if !silent_status {
        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{prefix} — {order_summary}"),
            detail: detail.clone(),
        });
    }
    if let Some(tx) = outcome_tx {
        let _ = tx.send(SliceOutcome::Failed(detail));
    }
}

/// Asynchronously constructs, signs, and dispatches a market order onto the network.
#[allow(clippy::too_many_arguments)]
pub fn submit_market_order(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    symbol: String,
    side: TradingSide,
    num_base_lots: u64,
    reduce_only: bool,
    display_size: f64,
    subaccount_index: u8,
    isolated_only: bool,
    max_leverage: f64,
    reference_price_usd: f64,
    tx_status: mpsc::UnboundedSender<TxStatusMsg>,
    outcome_tx: Option<oneshot::Sender<SliceOutcome>>,
    silent_status: bool,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let s = strings();
        let mut order_summary = format!("{} {} {}", side.label(s), display_size, symbol);
        if reduce_only {
            order_summary = format!("{order_summary} {}", s.tx_reduce_only);
        }

        let phx_side = side.to_ix_side();
        let isolated_only = isolated_only || ctx.market_isolated_only(&symbol);
        let max_leverage = ctx
            .max_leverage_for_symbol(&symbol)
            .filter(|lev| lev.is_finite() && *lev > 0.0)
            .unwrap_or(max_leverage);

        if isolated_only && reduce_only && subaccount_index == 0 {
            fail_build(
                &tx_status,
                silent_status,
                outcome_tx,
                s.tx_failed_build_ix,
                &order_summary,
                "isolated reduce-only orders require an isolated subaccount".to_string(),
            );
            return;
        }

        let (raw_ixs, cu_positions, log_kind) = if isolated_only && !reduce_only {
            let collateral =
                match estimate_collateral_transfer(display_size, reference_price_usd, max_leverage)
                {
                    Ok(c) => c,
                    Err(e) => {
                        fail_build(
                            &tx_status,
                            silent_status,
                            outcome_tx,
                            s.tx_failed_build_params,
                            &order_summary,
                            e,
                        );
                        return;
                    }
                };
            // Require hydrated WS trader state before building so we never create a
            // duplicate isolated subaccount.
            let Some(trader_snapshot) = ctx.snapshot_trader() else {
                fail_build(
                    &tx_status,
                    silent_status,
                    outcome_tx,
                    s.tx_failed_build_params,
                    &order_summary,
                    s.twap_waiting_trader_sync.to_string(),
                );
                return;
            };
            let builder = PhoenixTxBuilder::new(&ctx.metadata);
            match builder.build_isolated_market_order(
                &trader_snapshot,
                &symbol,
                phx_side,
                num_base_lots,
                Some(collateral),
                false,
                None,
            ) {
                Ok(ixs) => {
                    let cu = ixs.len().max(1) as u32;
                    (ixs, cu, "isolated market order")
                }
                Err(e) => {
                    fail_build(
                        &tx_status,
                        silent_status,
                        outcome_tx,
                        s.tx_failed_build_ix,
                        &order_summary,
                        parse_phoenix_tx_error(&format!("{e}")),
                    );
                    return;
                }
            }
        } else {
            // Non-isolated builder uses `ctx.market_addrs`, which are pinned to `active_symbol`.
            if symbol != ctx.active_symbol {
                fail_build(
                    &tx_status,
                    silent_status,
                    outcome_tx,
                    s.tx_failed_build_params,
                    &order_summary,
                    format!(
                        "{} ({} != active {})",
                        s.twap_waiting_active_market, symbol, ctx.active_symbol
                    ),
                );
                return;
            }
            let order_flags = if reduce_only {
                OrderFlags::ReduceOnly
            } else {
                OrderFlags::None
            };
            let params = match MarketOrderParams::builder()
                .trader(ctx.authority_v2)
                .trader_account(ctx.trader_pda_for_subaccount(subaccount_index))
                .perp_asset_map(ctx.market_addrs.perp_asset_map)
                .orderbook(ctx.market_addrs.orderbook)
                .spline_collection(ctx.market_addrs.spline_collection)
                .global_trader_index(ctx.market_addrs.global_trader_index.clone())
                .active_trader_buffer(ctx.market_addrs.active_trader_buffer.clone())
                .symbol(&symbol)
                .side(phx_side)
                .num_base_lots(num_base_lots)
                .order_flags(order_flags)
                .subaccount_index(subaccount_index)
                .build()
            {
                Ok(p) => p,
                Err(e) => {
                    fail_build(
                        &tx_status,
                        silent_status,
                        outcome_tx,
                        s.tx_failed_build_params,
                        &order_summary,
                        format!("{e}"),
                    );
                    return;
                }
            };
            match create_place_market_order_ix(params) {
                Ok(ix) => (vec![ix.into()], 1, "market order"),
                Err(e) => {
                    fail_build(
                        &tx_status,
                        silent_status,
                        outcome_tx,
                        s.tx_failed_build_ix,
                        &order_summary,
                        format!("{e}"),
                    );
                    return;
                }
            }
        };

        let ixs = match wrap_order_ixs(raw_ixs, ctx.authority_v2) {
            Ok(ixs) => ixs,
            Err(e) => {
                fail_build(
                    &tx_status,
                    silent_status,
                    outcome_tx,
                    s.tx_failed_build_ix,
                    &order_summary,
                    e,
                );
                return;
            }
        };

        let report = order_report(
            &order_summary,
            log_kind,
            vec![(symbol, matches!(side, TradingSide::Long))],
            silent_status,
            outcome_tx,
        );
        execute_tx(
            &ctx,
            &keypair,
            ixs,
            CuBudget::Scaled(cu_positions),
            &tx_status,
            report,
        )
        .await;
    })
}

/// Asynchronously constructs, signs, and dispatches a limit order onto the network.
#[allow(clippy::too_many_arguments)]
pub fn submit_limit_order(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    symbol: String,
    side: TradingSide,
    num_base_lots: u64,
    limit_price_usd: f64,
    display_size: f64,
    isolated_only: bool,
    max_leverage: f64,
    tx_status: mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        let s = strings();
        let order_summary = format!(
            "{} {} {} {} @ ${:.2}",
            s.lmt,
            side.label(s),
            display_size,
            symbol,
            limit_price_usd
        );

        let phx_side = side.to_ix_side();
        let isolated_only = isolated_only || ctx.market_isolated_only(&symbol);
        let max_leverage = ctx
            .max_leverage_for_symbol(&symbol)
            .filter(|lev| lev.is_finite() && *lev > 0.0)
            .unwrap_or(max_leverage);

        let (raw_ixs, cu_positions, log_kind) = if isolated_only {
            let collateral =
                match estimate_collateral_transfer(display_size, limit_price_usd, max_leverage) {
                    Ok(c) => c,
                    Err(e) => {
                        fail_build(
                            &tx_status,
                            false,
                            None,
                            s.tx_failed_build_params,
                            &order_summary,
                            e,
                        );
                        return;
                    }
                };
            let Some(trader_snapshot) = ctx.snapshot_trader() else {
                fail_build(
                    &tx_status,
                    false,
                    None,
                    s.tx_failed_build_params,
                    &order_summary,
                    s.twap_waiting_trader_sync.to_string(),
                );
                return;
            };
            let builder = PhoenixTxBuilder::new(&ctx.metadata);
            match builder.build_isolated_limit_order(
                &trader_snapshot,
                &symbol,
                phx_side,
                limit_price_usd,
                num_base_lots,
                Some(collateral),
                false,
            ) {
                Ok(ixs) => {
                    let cu = ixs.len().max(1) as u32;
                    (ixs, cu, "isolated limit order")
                }
                Err(e) => {
                    fail_build(
                        &tx_status,
                        false,
                        None,
                        s.tx_failed_build_ix,
                        &order_summary,
                        parse_phoenix_tx_error(&format!("{e}")),
                    );
                    return;
                }
            }
        } else {
            if symbol != ctx.active_symbol {
                fail_build(
                    &tx_status,
                    false,
                    None,
                    s.tx_failed_build_params,
                    &order_summary,
                    format!(
                        "{} ({} != active {})",
                        s.twap_waiting_active_market, symbol, ctx.active_symbol
                    ),
                );
                return;
            }
            let Some(calc) = ctx.metadata.get_market_calculator(&symbol) else {
                fail_build(
                    &tx_status,
                    false,
                    None,
                    s.tx_failed_build_params,
                    &order_summary,
                    format!("no calculator for {symbol}"),
                );
                return;
            };
            let price_ticks = match calc.price_to_ticks(limit_price_usd) {
                Ok(t) => t.as_inner(),
                Err(e) => {
                    fail_build(
                        &tx_status,
                        false,
                        None,
                        s.tx_failed_build_params,
                        &order_summary,
                        format!("{e}"),
                    );
                    return;
                }
            };
            let params = match LimitOrderParams::builder()
                .trader(ctx.authority_v2)
                .trader_account(ctx.trader_pda_v2)
                .perp_asset_map(ctx.market_addrs.perp_asset_map)
                .orderbook(ctx.market_addrs.orderbook)
                .spline_collection(ctx.market_addrs.spline_collection)
                .global_trader_index(ctx.market_addrs.global_trader_index.clone())
                .active_trader_buffer(ctx.market_addrs.active_trader_buffer.clone())
                .symbol(&symbol)
                .side(phx_side)
                .price_in_ticks(price_ticks)
                .num_base_lots(num_base_lots)
                .order_flags(OrderFlags::None)
                .subaccount_index(0)
                .build()
            {
                Ok(p) => p,
                Err(e) => {
                    fail_build(
                        &tx_status,
                        false,
                        None,
                        s.tx_failed_build_params,
                        &order_summary,
                        format!("{e}"),
                    );
                    return;
                }
            };
            match create_place_limit_order_ix(params) {
                Ok(ix) => (vec![ix.into()], 1, "limit order"),
                Err(e) => {
                    fail_build(
                        &tx_status,
                        false,
                        None,
                        s.tx_failed_build_ix,
                        &order_summary,
                        format!("{e}"),
                    );
                    return;
                }
            }
        };

        let ixs = match wrap_order_ixs(raw_ixs, ctx.authority_v2) {
            Ok(ixs) => ixs,
            Err(e) => {
                fail_build(
                    &tx_status,
                    false,
                    None,
                    s.tx_failed_build_ix,
                    &order_summary,
                    e,
                );
                return;
            }
        };

        let report = order_report(&order_summary, log_kind, Vec::new(), false, None);
        execute_tx(
            &ctx,
            &keypair,
            ixs,
            CuBudget::Scaled(cu_positions),
            &tx_status,
            report,
        )
        .await;
    });
}

/// Asynchronously constructs, signs, and dispatches a legacy stop-market
/// (`PlaceStopLoss`) order onto the network.
#[allow(clippy::too_many_arguments)]
pub fn submit_stop_market_order(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    symbol: String,
    side: TradingSide,
    _num_base_lots: u64,
    trigger_price_usd: f64,
    display_size: f64,
    subaccount_index: u8,
    isolated_only: bool,
    tx_status: mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        let s = strings();
        let order_summary = format!(
            "{} {} {} {} @ ${:.2}",
            s.stp,
            side.label(s),
            display_size,
            symbol,
            trigger_price_usd
        );

        let isolated_only = isolated_only || ctx.market_isolated_only(&symbol);
        if isolated_only && subaccount_index == 0 {
            fail_build(
                &tx_status,
                false,
                None,
                s.tx_failed_build_ix,
                &order_summary,
                "isolated stop orders require an existing isolated position".to_string(),
            );
            return;
        }

        let bracket = BracketLegOrders {
            stop_loss: Some(BracketLeg::new(trigger_price_usd)),
            take_profit: None,
        };
        let builder = PhoenixTxBuilder::new(&ctx.metadata);
        let raw_ixs = match builder.build_stop_loss_orders(
            ctx.authority_v2,
            ctx.trader_pda_for_subaccount(subaccount_index),
            &symbol,
            side.to_ix_side(),
            &bracket,
        ) {
            Ok(v) => v,
            Err(e) => {
                fail_build(
                    &tx_status,
                    false,
                    None,
                    s.tx_failed_build_ix,
                    &order_summary,
                    format!("{e}"),
                );
                return;
            }
        };

        let ixs = match wrap_order_ixs(raw_ixs, ctx.authority_v2) {
            Ok(ixs) => ixs,
            Err(e) => {
                fail_build(
                    &tx_status,
                    false,
                    None,
                    s.tx_failed_build_ix,
                    &order_summary,
                    e,
                );
                return;
            }
        };

        let report = order_report(&order_summary, "stop-market order", Vec::new(), false, None);
        execute_tx(&ctx, &keypair, ixs, CuBudget::Scaled(1), &tx_status, report).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosmic_builder_authority_parses() {
        assert!(Pubkey::from_str(COSMIC_BUILDER_AUTHORITY).is_ok());
    }

    #[test]
    fn flight_client_uses_cosmic_builder_authority() {
        assert_eq!(
            FLIGHT_CLIENT.builder_authority,
            Pubkey::from_str(COSMIC_BUILDER_AUTHORITY).unwrap()
        );
    }

    #[test]
    fn estimate_collateral_transfer_validates_and_buffers() {
        assert!(estimate_collateral_transfer(0.0, 100.0, 10.0).is_err());
        assert!(estimate_collateral_transfer(1.0, 0.0, 10.0).is_err());
        assert!(estimate_collateral_transfer(1.0, 100.0, 0.0).is_err());

        // 1 unit * $100 / 10x * 1.10 buffer = $11 = 11_000_000 quote lots.
        let flow = estimate_collateral_transfer(1.0, 100.0, 10.0).unwrap();
        assert!(matches!(
            flow,
            IsolatedCollateralFlow::TransferFromCrossMargin {
                collateral: 11_000_000
            }
        ));
    }
}
