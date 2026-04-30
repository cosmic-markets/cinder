//! Tick / lot price conversions.

use std::fmt;

use super::constants::QUOTE_LOT_DECIMALS;

/// Release-safety ceiling for user-entered base-asset size. This is far above
/// normal UI presets but prevents pathological input from ever reaching an
/// on-chain lot conversion.
pub const MAX_UI_ORDER_SIZE_UNITS: f64 = 1_000_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LotConversionError {
    NotFinite,
    NonPositive,
    AboveUiLimit,
    BelowMinimumLot,
    TooLarge,
}

impl fmt::Display for LotConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite => f.write_str("size must be a finite number"),
            Self::NonPositive => f.write_str("size must be greater than zero"),
            Self::AboveUiLimit => f.write_str("size is above the release safety limit"),
            Self::BelowMinimumLot => f.write_str("size is below one base lot for this market"),
            Self::TooLarge => f.write_str("size is too large to encode as base lots"),
        }
    }
}

/// Converts on-chain price ticks into an absolute USD float representation.
#[inline]
pub fn ticks_to_price(ticks: u64, tick_size: u64, base_lot_decimals: i8) -> f64 {
    ticks as f64 * tick_size as f64 * 10_f64.powi(base_lot_decimals as i32)
        / 10_f64.powi(QUOTE_LOT_DECIMALS)
}

/// Converts on-chain base lots into the absolute token unit size float.
#[inline]
pub fn base_lots_to_units(lots: u64, base_lot_decimals: i8) -> f64 {
    lots as f64 / 10_f64.powi(base_lot_decimals as i32)
}

/// 24h percentage change from mark vs. prior-day mark; `0.0` when `prev_day` is
/// zero.
#[inline]
pub fn pct_change_24h(mark: f64, prev_day: f64) -> f64 {
    if prev_day != 0.0 {
        ((mark - prev_day) / prev_day) * 100.0
    } else {
        0.0
    }
}

/// Converts a user-entered base-asset size into on-chain base lots with explicit
/// validation. Values are floored to whole lots, matching the previous
/// truncate-toward-zero behavior, but invalid and overflowing inputs now return
/// an error instead of silently saturating through `as u64`.
#[inline]
pub fn ui_size_to_num_base_lots(
    size: f64,
    base_lot_decimals: i8,
) -> Result<u64, LotConversionError> {
    if !size.is_finite() {
        return Err(LotConversionError::NotFinite);
    }
    if size <= 0.0 {
        return Err(LotConversionError::NonPositive);
    }
    if size > MAX_UI_ORDER_SIZE_UNITS {
        return Err(LotConversionError::AboveUiLimit);
    }

    let lots = size * 10_f64.powi(base_lot_decimals as i32);
    if !lots.is_finite() || lots > u64::MAX as f64 {
        return Err(LotConversionError::TooLarge);
    }
    if lots < 1.0 {
        return Err(LotConversionError::BelowMinimumLot);
    }

    Ok(lots.floor() as u64)
}

/// Convert Phoenix HTTP `Decimal` `(value, decimals)` into `num_base_lots` for
/// a market's `base_lot_decimals` (equivalent to `size * 10^base_lot_decimals`
/// with exact rationals). When scaling down (`base_lot_decimals <
/// value_decimals`), truncates toward zero like an exact integer quotient.
#[inline]
pub fn phoenix_decimal_to_num_base_lots(
    value: i64,
    value_decimals: i8,
    base_lot_decimals: i8,
) -> Option<u64> {
    let abs_val = value.unsigned_abs();
    let exp = i32::from(base_lot_decimals) - i32::from(value_decimals);

    match exp.cmp(&0) {
        std::cmp::Ordering::Greater => {
            let mult = 10_u64.checked_pow(exp as u32)?;
            abs_val.checked_mul(mult)
        }
        std::cmp::Ordering::Less => {
            let div = 10_u64.checked_pow((-exp) as u32)?;
            Some(abs_val / div)
        }
        std::cmp::Ordering::Equal => Some(abs_val),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_base_lots_from_decimal_sol_two_decimals() {
        // 3.47 SOL with API decimals=3 → 3.470; base lots with bld=2 → 347 lots
        assert_eq!(phoenix_decimal_to_num_base_lots(3470, 3, 2), Some(347));
    }

    #[test]
    fn num_base_lots_scale_down_truncates() {
        assert_eq!(phoenix_decimal_to_num_base_lots(1001, 3, 2), Some(100));
    }

    #[test]
    fn num_base_lots_scale_up_multiplies() {
        // value=5 with decimals=0 and bld=3 → 5_000 lots.
        assert_eq!(phoenix_decimal_to_num_base_lots(5, 0, 3), Some(5_000));
    }

    #[test]
    fn num_base_lots_negative_value_uses_absolute() {
        assert_eq!(phoenix_decimal_to_num_base_lots(-3470, 3, 2), Some(347));
    }

    #[test]
    fn num_base_lots_overflow_returns_none() {
        // 10^20 won't fit in u64.
        assert_eq!(phoenix_decimal_to_num_base_lots(1, 0, 20), None);
    }

    #[test]
    fn ui_size_to_num_base_lots_matches_existing_scale() {
        assert_eq!(ui_size_to_num_base_lots(3.47, 2), Ok(347));
        assert_eq!(ui_size_to_num_base_lots(50.0, -1), Ok(5));
    }

    #[test]
    fn ui_size_to_num_base_lots_rejects_bad_inputs() {
        assert_eq!(
            ui_size_to_num_base_lots(f64::NAN, 2),
            Err(LotConversionError::NotFinite)
        );
        assert_eq!(
            ui_size_to_num_base_lots(0.0, 2),
            Err(LotConversionError::NonPositive)
        );
        assert_eq!(
            ui_size_to_num_base_lots(0.001, 2),
            Err(LotConversionError::BelowMinimumLot)
        );
        assert_eq!(
            ui_size_to_num_base_lots(MAX_UI_ORDER_SIZE_UNITS + 1.0, 2),
            Err(LotConversionError::AboveUiLimit)
        );
    }

    #[test]
    fn pct_change_zero_prev() {
        assert_eq!(pct_change_24h(100.0, 0.0), 0.0);
    }

    #[test]
    fn pct_change_matches_formula() {
        assert!((pct_change_24h(110.0, 100.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn pct_change_handles_negative_direction() {
        assert!((pct_change_24h(90.0, 100.0) - -10.0).abs() < 1e-9);
    }

    #[test]
    fn base_lots_to_units_divides_by_decimal_power() {
        // 1_000 lots with bld=3 → 1.000 units.
        assert!((base_lots_to_units(1_000, 3) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn base_lots_to_units_handles_negative_decimals() {
        // bld=-1 → each lot is 10 units; 5 lots → 50.0.
        assert!((base_lots_to_units(5, -1) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn ticks_to_price_matches_known_market() {
        // QUOTE_LOT_DECIMALS = 6. tick_size=1, bld=0 → ticks → ticks * 1e-6 USD.
        assert!((ticks_to_price(150_000_000, 1, 0) - 150.0).abs() < 1e-9);
    }

    #[test]
    fn ticks_to_price_scales_with_base_lot_decimals() {
        // bld=2 multiplies by 1e2; 100 ticks * tick=10 * 100 / 1e6 = 0.1
        assert!((ticks_to_price(100, 10, 2) - 0.1).abs() < 1e-9);
    }
}
