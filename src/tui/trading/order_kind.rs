//! User-selected order type for the trading panel.

/// Which order type the trader has selected. Stored on `TradingState` and
/// threaded through to `PendingAction::PlaceOrder` so submission can dispatch
/// to the matching builder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderKind {
    Market,
    Limit {
        price: f64,
    },
    /// Stop-market: triggers a market execution when the mark price crosses
    /// `trigger`. Direction is derived from `TradingSide` at submit time
    /// (BUY → trigger on price rising, SELL → trigger on price falling).
    StopMarket {
        trigger: f64,
    },
}

impl OrderKind {
    /// USD price attached to the kind, if any. `None` for `Market`.
    pub fn price(&self) -> Option<f64> {
        match self {
            OrderKind::Market => None,
            OrderKind::Limit { price } => Some(*price),
            OrderKind::StopMarket { trigger } => Some(*trigger),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_has_no_price() {
        assert_eq!(OrderKind::Market.price(), None);
    }

    #[test]
    fn limit_returns_attached_price() {
        assert_eq!(OrderKind::Limit { price: 123.45 }.price(), Some(123.45));
    }

    #[test]
    fn stop_market_returns_trigger() {
        assert_eq!(OrderKind::StopMarket { trigger: 99.5 }.price(), Some(99.5));
    }
}
