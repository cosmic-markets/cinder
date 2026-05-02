//! Helpers for automatic isolated-margin collateral transfers.

use phoenix_rise::IsolatedCollateralFlow;

const QUOTE_LOTS_PER_USDC: f64 = 1_000_000.0;
const COLLATERAL_BUFFER: f64 = 1.10;

pub(super) fn estimate_collateral_transfer(
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
