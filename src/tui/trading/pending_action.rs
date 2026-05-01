//! Action awaiting confirmation from the trader before submission.

use super::{OrderKind, TradingSide};

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
        side: TradingSide,
        size: f64,
        position_size_raw: Option<(i64, i8)>,
    },
    CloseAllPositions,
    /// Cancel a single open order on the active market (or any market the
    /// order belongs to).
    CancelOrder {
        symbol: String,
        side: TradingSide,
        size: f64,
        price_usd: f64,
        price_ticks: u64,
        order_sequence_number: u64,
        /// Pending stops cancel via `cancel_stop_loss` (keyed on asset_id +
        /// direction), not the regular `cancel_orders_by_id` path.
        is_stop_loss: bool,
        conditional_order_index: Option<u8>,
        conditional_trigger_direction: Option<phoenix_rise::Direction>,
    },
    /// Cancel every open order across every market the trader has working.
    CancelAllOrders,
    DepositFunds {
        amount: f64,
    },
    WithdrawFunds {
        amount: f64,
    },
}
