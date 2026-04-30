//! SOL spline updates via Solana `accountSubscribe` (WSS).
//!
//! On each account change notification, renders the spline collection TUI
//! using ratatui with a price tick chart.

// Preserved from the pre-split `spline.rs` surface (transmute-heavy Solana interop, wide render
// API).
#![allow(clippy::missing_transmute_annotations)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod config;
mod constants;
mod format;
mod gti;
mod i18n;
pub mod math;
mod parse;
mod poller;
mod render;
mod state;
mod terminal;
mod top_positions;
mod trading;
mod tx;

pub use config::{build_spline_config, compute_price_decimals, SplineConfig};
pub use poller::spawn_spline_poller;
pub use state::{MarketInfo, MarketListUpdate, MarketStatUpdate};
pub use terminal::cleanup_terminal;
