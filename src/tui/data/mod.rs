//! On-chain data loaders and decoders that feed the TUI.

pub mod position_leaderboard;
pub mod spline_book;
pub mod trader_index;

pub use position_leaderboard::fetch_top_positions;
pub use spline_book::{
    L2Level, ParsedSplineData, parse_l2_book_from_market_account, parse_spline_data,
    parse_spline_sequence,
};
pub use trader_index::{GtiCache, GtiHandle, spawn_gti_loader};
