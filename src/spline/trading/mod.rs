//! Trading domain types — sides, order kinds, positions, orders, and the
//! pending-action / input-mode state machine that drives the TUI.

mod balance;
mod input_mode;
mod order_info;
mod order_kind;
mod pending_action;
mod position_info;
mod side;
mod top_position_entry;

pub use balance::fetch_phoenix_balance_and_position;
pub use input_mode::InputMode;
pub use order_info::OrderInfo;
pub use order_kind::OrderKind;
pub use pending_action::PendingAction;
pub use position_info::{FullPositionInfo, PositionInfo};
pub use side::TradingSide;
pub use top_position_entry::TopPositionEntry;
