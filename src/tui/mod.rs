//! Terminal UI runtime for Phoenix perpetual markets.
//!
//! Owns the Solana account streams, ratatui rendering, trading state, and
//! transaction submission wiring.

// Preserved from the pre-split module surface (transmute-heavy Solana interop,
// wide render API).
#![allow(clippy::missing_transmute_annotations)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod config;
mod constants;
mod data;
mod format;
mod i18n;
pub mod math;
mod runtime;
mod state;
mod terminal;
mod trading;
mod tx;
mod ui;

pub use config::{build_spline_config, compute_price_decimals, SplineConfig};
pub use runtime::spawn_spline_poller;
pub use state::{MarketInfo, MarketListUpdate, MarketStatUpdate};
pub use terminal::cleanup_terminal;
