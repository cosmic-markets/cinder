//! Phoenix HTTP snapshot of collateral balance and open positions for a
//! trader authority.

use phoenix_rise::PhoenixHttpClient;

use super::{FullPositionInfo, PositionInfo, TradingSide};

pub async fn fetch_phoenix_balance_and_position(
    http: &PhoenixHttpClient,
    authority_v2: &solana_pubkey::Pubkey,
    symbol: &str,
) -> (f64, Option<PositionInfo>, Vec<FullPositionInfo>) {
    match http.traders().get_trader(authority_v2).await {
        Ok(traders) if !traders.is_empty() => {
            let bal = traders
                .iter()
                .find(|t| t.trader_subaccount_index == 0)
                .or_else(|| traders.first())
                .map(|t| t.collateral_balance.ui.parse::<f64>().unwrap_or(0.0))
                .unwrap_or(0.0);

            let pos = traders.iter().find_map(|t| {
                let collateral = t.collateral_balance.ui.parse::<f64>().unwrap_or(0.0);
                t.positions
                    .iter()
                    .find(|p| p.symbol == symbol)
                    .and_then(|p| {
                        parse_position(p, collateral, t.trader_subaccount_index).map(|full| {
                            PositionInfo {
                                subaccount_index: full.subaccount_index,
                                side: full.side,
                                size: full.size,
                                position_size_raw: full.position_size_raw,
                                entry_price: full.entry_price,
                                unrealized_pnl: full.unrealized_pnl,
                                liquidation_price: full.liquidation_price,
                                notional: full.notional,
                                leverage: full.leverage,
                            }
                        })
                    })
            });

            let all_positions: Vec<FullPositionInfo> = traders
                .iter()
                .flat_map(|t| {
                    let collateral = t.collateral_balance.ui.parse::<f64>().unwrap_or(0.0);
                    t.positions.iter().filter_map(move |p| {
                        parse_position(p, collateral, t.trader_subaccount_index)
                    })
                })
                .collect();

            (bal, pos, all_positions)
        }
        _ => (0.0, None, Vec::new()),
    }
}

fn parse_position(
    p: &phoenix_rise::types::TraderPositionView,
    collateral: f64,
    subaccount_index: u8,
) -> Option<FullPositionInfo> {
    let size = p.position_size.ui.parse::<f64>().unwrap_or(0.0);
    // Discard negligible positions that are functionally zero.
    if size.abs() < 1e-9 {
        return None;
    }

    let entry = p.entry_price.ui.parse::<f64>().unwrap_or(0.0);
    let pnl = p.unrealized_pnl.ui.parse::<f64>().unwrap_or(0.0);
    let liq = p
        .liquidation_price
        .ui
        .parse::<f64>()
        .ok()
        .filter(|&v| v > 0.0);
    let notional = size.abs() * entry;

    let side = if size > 0.0 {
        TradingSide::Long
    } else {
        TradingSide::Short
    };

    let leverage = if collateral > 0.0 {
        Some(notional / collateral)
    } else {
        None
    };

    Some(FullPositionInfo {
        symbol: p.symbol.clone(),
        subaccount_index,
        side,
        size: size.abs(),
        position_size_raw: Some((p.position_size.value, p.position_size.decimals)),
        entry_price: entry,
        unrealized_pnl: pnl,
        liquidation_price: liq,
        notional,
        leverage,
    })
}
