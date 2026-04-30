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
            let t = &traders[0];
            let bal = t.collateral_balance.ui.parse::<f64>().unwrap_or(0.0);

            let pos = t
                .positions
                .iter()
                .find(|p| p.symbol == symbol)
                .and_then(|p| {
                    parse_position(p, bal).map(|full| PositionInfo {
                        side: full.side,
                        size: full.size,
                        position_size_raw: full.position_size_raw,
                        entry_price: full.entry_price,
                        unrealized_pnl: full.unrealized_pnl,
                        liquidation_price: full.liquidation_price,
                        notional: full.notional,
                        leverage: full.leverage,
                    })
                });

            let all_positions: Vec<FullPositionInfo> = t
                .positions
                .iter()
                .filter_map(|p| parse_position(p, bal))
                .collect();

            (bal, pos, all_positions)
        }
        _ => (0.0, None, Vec::new()),
    }
}

fn parse_position(
    p: &phoenix_rise::types::TraderPositionView,
    collateral: f64,
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
