//! Trading domain types — sides, order kinds, positions, orders, and the
//! pending-action / input-mode state machine that drives the TUI.

mod balance;

use ratatui::style::Color;

use super::i18n::Strings;

pub use balance::fetch_phoenix_balance_and_position;

/// Trading direction (long or short).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingSide {
    Long,
    Short,
}

impl TradingSide {
    /// Display color for this side (green for long, red for short).
    pub fn color(self) -> Color {
        match self {
            Self::Long => Color::LightGreen,
            Self::Short => Color::LightRed,
        }
    }

    /// Flip between long and short.
    pub fn toggle(self) -> Self {
        match self {
            Self::Long => Self::Short,
            Self::Short => Self::Long,
        }
    }

    /// Localized uppercase label ("LONG" / "SHORT") from the active string table.
    pub fn label(self, s: &Strings) -> &'static str {
        match self {
            Self::Long => s.long_label,
            Self::Short => s.short_label,
        }
    }

    /// Convert to the Phoenix instruction `Side` (`Bid` for long, `Ask` for short).
    pub fn to_ix_side(self) -> phoenix_rise::ix::prelude::Side {
        match self {
            Self::Long => phoenix_rise::ix::prelude::Side::Bid,
            Self::Short => phoenix_rise::ix::prelude::Side::Ask,
        }
    }
}

/// Which order type the trader has selected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderKind {
    Market,
    Limit {
        price: f64,
    },
    /// Triggers a market execution when mark price crosses `trigger`.
    StopMarket {
        trigger: f64,
    },
    /// Time-Weighted Average Price: splits an order into timed market slices.
    Twap,
}

impl OrderKind {
    /// USD price attached to the kind, if any (`None` for `Market` and `Twap`).
    pub fn price(&self) -> Option<f64> {
        match self {
            OrderKind::Market | OrderKind::Twap => None,
            OrderKind::Limit { price } => Some(*price),
            OrderKind::StopMarket { trigger } => Some(*trigger),
        }
    }
}

/// Active-market position summary used by the trading panel and chart.
#[derive(Debug, Clone)]
pub struct PositionInfo {
    /// Phoenix trader subaccount index (`0` is cross-margin; `1+` are isolated).
    pub subaccount_index: u8,
    pub side: TradingSide,
    pub size: f64,
    /// Phoenix `position_size.value` / `position_size.decimals` for exact lot sizing on close.
    pub position_size_raw: Option<(i64, i8)>,
    pub entry_price: f64,
    pub unrealized_pnl: f64,
    pub liquidation_price: Option<f64>,
    pub notional: f64,
    pub leverage: Option<f64>,
}

/// Cross-market position row carrying its market `symbol` for the positions modal.
#[derive(Debug, Clone)]
pub struct FullPositionInfo {
    pub symbol: String,
    pub subaccount_index: u8,
    pub side: TradingSide,
    pub size: f64,
    pub position_size_raw: Option<(i64, i8)>,
    pub entry_price: f64,
    pub unrealized_pnl: f64,
    pub liquidation_price: Option<f64>,
    pub notional: f64,
    pub leverage: Option<f64>,
}

impl From<&FullPositionInfo> for PositionInfo {
    fn from(full: &FullPositionInfo) -> Self {
        Self {
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
    }
}

/// Open-order row as displayed in the orders modal.
#[derive(Debug, Clone)]
pub struct OrderInfo {
    pub symbol: String,
    /// Phoenix trader subaccount index (`0` is cross-margin; `1+` are isolated).
    pub subaccount_index: u8,
    /// Phoenix order sequence number, unique per market subaccount.
    pub order_sequence_number: u64,
    pub side: TradingSide,
    pub order_type: String,
    pub price_usd: f64,
    /// Raw price ticks required to build a `CancelId`.
    pub price_ticks: u64,
    pub size_remaining: f64,
    pub initial_size: f64,
    pub reduce_only: bool,
    pub is_stop_loss: bool,
    /// Conditional-order slot index and trigger direction for position stop-losses.
    pub conditional_order_index: Option<u8>,
    pub conditional_trigger_direction: Option<phoenix_rise::ix::types::Direction>,
}

/// One row of the "top positions on Phoenix" leaderboard modal.
#[derive(Debug, Clone)]
pub struct TopPositionEntry {
    pub symbol: String,
    /// Owning trader's wallet authority when resolved via the GTI cache.
    pub trader: Option<String>,
    /// Short display label ("AbCd…WXYZ" or "#<node_id>").
    pub trader_display: String,
    pub side: TradingSide,
    pub size: f64,
    pub entry_price: f64,
    pub notional: f64,
    pub unrealized_pnl: f64,
}

/// Action awaiting confirmation from the trader before submission.
#[derive(Debug, Clone, PartialEq)]
pub enum PendingAction {
    PlaceOrder {
        side: TradingSide,
        size: f64,
        kind: OrderKind,
    },
    ClosePosition,
    ClosePositionBySymbol {
        symbol: String,
        subaccount_index: u8,
        side: TradingSide,
        size: f64,
        position_size_raw: Option<(i64, i8)>,
    },
    CloseAllPositions,
    CancelOrder {
        symbol: String,
        subaccount_index: u8,
        side: TradingSide,
        size: f64,
        price_usd: f64,
        price_ticks: u64,
        order_sequence_number: u64,
        is_stop_loss: bool,
        conditional_order_index: Option<u8>,
        conditional_trigger_direction: Option<phoenix_rise::ix::types::Direction>,
    },
    CancelAllOrders,
    DepositFunds {
        amount: f64,
    },
    WithdrawFunds {
        amount: f64,
    },
}

/// TUI input mode — drives keyboard dispatch and modal overlays.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    EditingSize,
    EditingPrice,
    EditingDeposit,
    EditingWithdraw,
    Confirming(PendingAction),
    SelectingMarket,
    ViewingPositions,
    ViewingTopPositions,
    ViewingOrders,
    ViewingLiquidations,
    ViewingLedger,
    ViewingConfig,
    EditingRpcUrl,
    EditingComputeUnitPrice,
    EditingComputeUnitLimit,
    EditingWalletPath,
    ChoosingReferral,
    EditingReferralCode,
    EditingTwap,
    ViewingBots,
    ConfirmQuit,
}

/// Compute position notional and unrealized PnL at `mark` price.
pub fn position_notional_and_pnl(
    size: f64,
    entry_price: f64,
    mark: f64,
    side: TradingSide,
) -> (f64, f64) {
    let pnl = match side {
        TradingSide::Long => size * (mark - entry_price),
        TradingSide::Short => size * (entry_price - mark),
    };
    (size * mark, pnl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_is_involution() {
        assert_eq!(TradingSide::Long.toggle(), TradingSide::Short);
        assert_eq!(TradingSide::Short.toggle(), TradingSide::Long);
        assert_eq!(TradingSide::Long.toggle().toggle(), TradingSide::Long);
    }

    #[test]
    fn color_distinguishes_sides() {
        assert_eq!(TradingSide::Long.color(), Color::LightGreen);
        assert_eq!(TradingSide::Short.color(), Color::LightRed);
    }

    #[test]
    fn side_to_ix_side_maps_bid_and_ask() {
        assert_eq!(
            TradingSide::Long.to_ix_side(),
            phoenix_rise::ix::prelude::Side::Bid
        );
        assert_eq!(
            TradingSide::Short.to_ix_side(),
            phoenix_rise::ix::prelude::Side::Ask
        );
    }

    #[test]
    fn order_kind_price_extracts_attached_prices() {
        assert_eq!(OrderKind::Market.price(), None);
        assert_eq!(OrderKind::Limit { price: 123.45 }.price(), Some(123.45));
        assert_eq!(OrderKind::StopMarket { trigger: 99.5 }.price(), Some(99.5));
        assert_eq!(OrderKind::Twap.price(), None);
    }

    #[test]
    fn position_notional_and_pnl_handles_long_and_short() {
        let (notional, pnl) = position_notional_and_pnl(2.0, 100.0, 110.0, TradingSide::Long);
        assert!((notional - 220.0).abs() < 1e-9);
        assert!((pnl - 20.0).abs() < 1e-9);

        let (notional, pnl) = position_notional_and_pnl(2.0, 100.0, 90.0, TradingSide::Short);
        assert!((notional - 180.0).abs() < 1e-9);
        assert!((pnl - 20.0).abs() < 1e-9);
    }
}
