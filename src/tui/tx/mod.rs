//! Solana transaction building and submission for the spline TUI.

pub mod cancel;
pub mod confirmation;
pub mod context;
pub mod funds;
pub mod limit_order;
pub mod market_order;
pub mod positions;
pub mod priority_fees;
pub mod stop_market_order;

mod compute_budget;
mod error;
mod isolated_margin;

pub use cancel::{submit_cancel_orders, CancelOrderEntry};
pub use context::TxContext;
pub use funds::submit_funds_transfer;
pub use limit_order::submit_limit_order;
pub use market_order::submit_market_order;
pub use positions::{submit_close_all_positions, ClosePositionEntry};
pub use priority_fees::{current_auto_priority_fee, spawn_auto_priority_fee_refresh};
pub use stop_market_order::submit_stop_market_order;
