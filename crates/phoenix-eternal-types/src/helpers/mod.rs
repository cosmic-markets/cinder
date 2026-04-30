//! Helper functions for price and lot conversions.

use crate::accounts::OrderbookHeader;
use crate::quantities::{BaseLots, QuoteLots, SignedBaseLots, SignedQuoteLots, Ticks};

impl OrderbookHeader {
    /// Convert base lots and price in ticks to quote lots.
    ///
    /// quote_lots = base_lots * price_in_ticks * tick_size
    pub fn quote_lots_from_base_lots_and_ticks(
        &self,
        base_lots: BaseLots,
        price_in_ticks: Ticks,
    ) -> QuoteLots {
        QuoteLots::new(
            base_lots
                .as_inner()
                .saturating_mul(price_in_ticks.as_inner())
                .saturating_mul(self.tick_size_in_quote_lots_per_base_lot),
        )
    }

    /// Convert quote lots and price in ticks to base lots.
    ///
    /// base_lots = quote_lots / (price_in_ticks * tick_size)
    pub fn base_lots_from_quote_lots_and_ticks(
        &self,
        quote_lots: QuoteLots,
        price_in_ticks: Ticks,
    ) -> BaseLots {
        let divisor = price_in_ticks
            .as_inner()
            .saturating_mul(self.tick_size_in_quote_lots_per_base_lot);
        if divisor == 0 {
            return BaseLots::ZERO;
        }
        BaseLots::new(quote_lots.as_inner() / divisor)
    }

    /// Calculate the effective entry price from a position.
    ///
    /// For a long position: entry_price = -virtual_quote_lots / base_lots
    /// For a short position: entry_price = virtual_quote_lots / -base_lots
    pub fn effective_entry_price_ticks(
        &self,
        base_lots: SignedBaseLots,
        virtual_quote_lots: SignedQuoteLots,
    ) -> Option<Ticks> {
        if base_lots == SignedBaseLots::ZERO {
            return None;
        }

        let tick_size = self.tick_size_in_quote_lots_per_base_lot;
        if tick_size == 0 {
            return None;
        }

        // Calculate price in quote lots per base lot
        let abs_base = base_lots.abs_as_unsigned();
        let abs_quote = virtual_quote_lots.abs_as_unsigned();

        // price_in_quote_lots_per_base_lot = abs_quote / abs_base
        // price_in_ticks = price_in_quote_lots_per_base_lot / tick_size
        let price_in_ticks = abs_quote / (abs_base * tick_size);

        Some(Ticks::new(price_in_ticks))
    }

    /// Convert a price in ticks to a human-readable price.
    ///
    /// human_price = price_in_ticks * tick_size * quote_lot_size / base_lot_size
    ///
    /// For simplicity, this returns the raw quote lots per base lot value.
    pub fn ticks_to_quote_lots_per_base_lot(&self, price_in_ticks: Ticks) -> u64 {
        price_in_ticks
            .as_inner()
            .saturating_mul(self.tick_size_in_quote_lots_per_base_lot)
    }
}

/// Calculate unrealized PnL for a position.
///
/// For a long position: uPnL = (mark_price - entry_price) * position_size
/// For a short position: uPnL = (entry_price - mark_price) * |position_size|
pub fn calculate_unrealized_pnl(
    base_lots: SignedBaseLots,
    virtual_quote_lots: SignedQuoteLots,
    mark_price_quote_lots_per_base_lot: u64,
) -> SignedQuoteLots {
    if base_lots == SignedBaseLots::ZERO {
        return SignedQuoteLots::ZERO;
    }

    // uPnL = base_lots * mark_price + virtual_quote_lots
    // For long: virtual_quote_lots is negative (cost), base_lots * mark_price is positive (value)
    // For short: virtual_quote_lots is positive (proceeds), base_lots * mark_price is negative
    let position_value =
        (base_lots.as_inner() as i128).saturating_mul(mark_price_quote_lots_per_base_lot as i128);
    let total = position_value.saturating_add(virtual_quote_lots.as_inner() as i128);

    SignedQuoteLots::new(total.clamp(i64::MIN as i128, i64::MAX as i128) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unrealized_pnl_long() {
        // Long 100 base lots at entry price 1000, mark price 1100
        // Entry: -100,000 quote lots (cost)
        // Value: 100 * 1100 = 110,000
        // uPnL = 110,000 - 100,000 = 10,000
        let upnl = calculate_unrealized_pnl(
            SignedBaseLots::new(100),
            SignedQuoteLots::new(-100_000),
            1100,
        );
        assert_eq!(upnl.as_inner(), 10_000);
    }

    #[test]
    fn test_unrealized_pnl_short() {
        // Short 100 base lots at entry price 1000, mark price 900
        // Entry: 100,000 quote lots (proceeds)
        // Value: -100 * 900 = -90,000
        // uPnL = -90,000 + 100,000 = 10,000
        let upnl = calculate_unrealized_pnl(
            SignedBaseLots::new(-100),
            SignedQuoteLots::new(100_000),
            900,
        );
        assert_eq!(upnl.as_inner(), 10_000);
    }
}
