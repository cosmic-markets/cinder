//! Market order submission — builds, signs, and dispatches a Phoenix market
//! order on a background tokio task.

use std::sync::Arc;

use solana_keypair::Keypair;

use super::super::i18n::strings;
use super::super::state::TxStatusMsg;
use super::super::trading::TradingSide;
use super::compute_budget::build_compute_budget_ixs;
use super::confirmation::{compile_and_sign, subscribe_send_confirm, ConfirmError};
use super::context::TxContext;
use super::error::{format_not_confirmed_error, log_tx_error, parse_phoenix_tx_error};
use super::isolated_margin::estimate_collateral_transfer;

/// Asynchronously constructs, signs, and dispatches a market order payload
/// onto the network.
pub fn submit_market_order(
    keypair: Arc<Keypair>,
    ctx: Arc<TxContext>,
    symbol: String,
    side: TradingSide,
    num_base_lots: u64,
    reduce_only: bool,
    // Human size for status messages (same units as the TUI order line).
    display_size: f64,
    subaccount_index: u8,
    isolated_only: bool,
    max_leverage: f64,
    reference_price_usd: f64,
    tx_status: tokio::sync::mpsc::UnboundedSender<TxStatusMsg>,
) {
    tokio::spawn(async move {
        use phoenix_rise::ix::{create_place_market_order_ix, MarketOrderParams, OrderFlags, Side};
        use phoenix_rise::PhoenixTxBuilder;

        let s = strings();
        let side_lbl = match side {
            TradingSide::Long => s.long_label,
            TradingSide::Short => s.short_label,
        };
        let order_summary = format!("{} {} {}", side_lbl, display_size, symbol);
        let order_summary = if reduce_only {
            format!("{} {}", order_summary, s.tx_reduce_only)
        } else {
            order_summary
        };

        let phx_side = match side {
            TradingSide::Long => Side::Bid,
            TradingSide::Short => Side::Ask,
        };
        let isolated_only = isolated_only || ctx.market_isolated_only(&symbol);
        let max_leverage = ctx
            .max_leverage_for_symbol(&symbol)
            .filter(|lev| lev.is_finite() && *lev > 0.0)
            .unwrap_or(max_leverage);

        if isolated_only && reduce_only && subaccount_index == 0 {
            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                detail: "isolated reduce-only orders require an isolated subaccount".to_string(),
            });
            return;
        }

        if isolated_only && !reduce_only {
            let collateral =
                match estimate_collateral_transfer(display_size, reference_price_usd, max_leverage)
                {
                    Ok(collateral) => collateral,
                    Err(e) => {
                        let _ = tx_status.send(TxStatusMsg::SetStatus {
                            title: format!("{} — {}", s.tx_failed_build_params, order_summary),
                            detail: e,
                        });
                        return;
                    }
                };
            let (mut ixs, _) = match ctx
                .http_client
                .build_isolated_market_order_tx_enhanced(
                    &ctx.authority_v2,
                    &symbol,
                    phx_side,
                    num_base_lots,
                    Some(collateral),
                    false,
                    None,
                )
                .await
            {
                Ok(ixs) => ixs,
                Err(e) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                        detail: format!("{}", e),
                    });
                    return;
                }
            };
            let cu_positions = ixs.len().max(1) as u32;
            ixs.extend(build_compute_budget_ixs(cu_positions));

            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: format!("{} {}…", s.tx_broadcasting, order_summary),
                detail: String::new(),
            });

            let (tx, sig) = match compile_and_sign(&ctx, &keypair, &ixs).await {
                Ok(pair) => pair,
                Err(e) => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_failed_prepare, order_summary),
                        detail: e,
                    });
                    return;
                }
            };
            let sig_str = sig.to_string();
            let _ = tx_status.send(TxStatusMsg::SetStatus {
                title: format!("{} — {}…", s.tx_awaiting_confirm, order_summary),
                detail: sig_str.clone(),
            });

            match subscribe_send_confirm(&ctx, &tx, &sig).await {
                Ok(()) => {
                    let _ = tx_status.send(TxStatusMsg::TradeMarker {
                        is_buy: matches!(side, TradingSide::Long),
                    });
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} {}", s.tx_order_confirmed, order_summary),
                        detail: sig_str,
                    });
                }
                Err(ConfirmError::Rejected(e)) => {
                    log_tx_error(
                        None,
                        &format!("isolated market order rejected — {}", order_summary),
                        &e,
                    );
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} — {}", s.tx_tx_rejected, order_summary),
                        detail: parse_phoenix_tx_error(&e),
                    });
                }
                Err(ConfirmError::NotConfirmed(e)) => {
                    log_tx_error(
                        Some(&sig_str),
                        &format!("isolated market order not confirmed — {}", order_summary),
                        &e,
                    );
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!(
                            "{} — {} ({})",
                            s.tx_order_not_confirmed,
                            order_summary,
                            format_not_confirmed_error(&e)
                        ),
                        detail: sig_str,
                    });
                }
            }
            return;
        }

        let builder = PhoenixTxBuilder::new(&ctx.metadata);
        let trader_account = ctx.trader_pda_for_subaccount(subaccount_index);

        let order_flags = if reduce_only {
            OrderFlags::ReduceOnly
        } else {
            OrderFlags::None
        };

        let params = match MarketOrderParams::builder()
            .trader(ctx.authority_v2)
            .trader_account(trader_account)
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
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_build_params, order_summary),
                    detail: format!("{}", e),
                });
                return;
            }
        };

        let mut ixs = match create_place_market_order_ix(params) {
            Ok(ix) => vec![ix.into()],
            Err(e) => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_build_ix, order_summary),
                    detail: format!("{}", e),
                });
                return;
            }
        };

        let mut includes_register = false;
        if subaccount_index == 0
            && !ctx
                .trader_registered
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            match ctx.rpc_client.get_account(&trader_account).await {
                Ok(acc) if !acc.data.is_empty() => {
                    ctx.trader_registered
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                _ => {
                    let _ = tx_status.send(TxStatusMsg::SetStatus {
                        title: format!("{} ({})…", s.tx_registering_trader, order_summary),
                        detail: String::new(),
                    });
                    match builder.build_register_trader(ctx.authority_v2, 0, 0) {
                        Ok(mut reg_ixs) => {
                            reg_ixs.extend(ixs);
                            ixs = reg_ixs;
                            includes_register = true;
                        }
                        Err(e) => {
                            let _ = tx_status.send(TxStatusMsg::SetStatus {
                                title: format!("{} — {}", s.tx_failed_build_reg, order_summary),
                                detail: format!("{}", e),
                            });
                            return;
                        }
                    }
                }
            }
        }

        let mapped_ixs = ixs;

        let cu_positions = if includes_register { 2 } else { 1 };
        let mut final_ixs = mapped_ixs;
        final_ixs.extend(build_compute_budget_ixs(cu_positions));
        let mapped_ixs = final_ixs;

        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} {}…", s.tx_broadcasting, order_summary),
            detail: String::new(),
        });

        let (tx, sig) = match compile_and_sign(&ctx, &keypair, &mapped_ixs).await {
            Ok(pair) => pair,
            Err(e) => {
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_failed_prepare, order_summary),
                    detail: e,
                });
                return;
            }
        };
        let sig_str = sig.to_string();
        let _ = tx_status.send(TxStatusMsg::SetStatus {
            title: format!("{} — {}…", s.tx_awaiting_confirm, order_summary),
            detail: sig_str.clone(),
        });

        match subscribe_send_confirm(&ctx, &tx, &sig).await {
            Ok(()) => {
                ctx.trader_registered
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = tx_status.send(TxStatusMsg::TradeMarker {
                    is_buy: matches!(side, TradingSide::Long),
                });
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} {}", s.tx_order_confirmed, order_summary),
                    detail: sig_str,
                });
            }
            Err(ConfirmError::Rejected(e)) => {
                log_tx_error(
                    None,
                    &format!("market order rejected — {}", order_summary),
                    &e,
                );
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!("{} — {}", s.tx_tx_rejected, order_summary),
                    detail: parse_phoenix_tx_error(&e),
                });
            }
            Err(ConfirmError::NotConfirmed(e)) => {
                log_tx_error(
                    Some(&sig_str),
                    &format!("market order not confirmed — {}", order_summary),
                    &e,
                );
                let _ = tx_status.send(TxStatusMsg::SetStatus {
                    title: format!(
                        "{} — {} ({})",
                        s.tx_order_not_confirmed,
                        order_summary,
                        format_not_confirmed_error(&e)
                    ),
                    detail: sig_str,
                });
            }
        }
    });
}
