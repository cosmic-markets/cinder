//! TUI runtime state: chart history, parsed book snapshot, trading panel.

use chrono::Utc;

pub mod book;
pub mod markers;
pub mod market;
pub mod orders_view;
pub mod positions_view;
pub mod top_positions_view;
pub mod trading;
pub mod tui;
pub mod updates;

pub use book::*;
pub use markers::*;
pub use market::*;
pub use orders_view::*;
pub use positions_view::*;
pub use top_positions_view::*;
pub use trading::*;
pub use tui::*;
pub use updates::*;

/// Returns a timestamp string formatted as `[HH:MM:SS UTC]`.
pub fn make_status_timestamp() -> String {
    format!(" {}", Utc::now().format("%H:%M:%S"))
}
