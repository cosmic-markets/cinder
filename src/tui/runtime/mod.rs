//! TUI runtime: event loop, input routing, and background streams.

use std::time::Duration;

mod channels;
mod connection;
mod event_loop;
mod input;
mod keyboard;
mod redraw;
mod submit;
mod tasks;
mod update_handlers;
mod wallet;

pub use event_loop::spawn_spline_poller;

pub(in crate::tui::runtime) use channels::{new_channels, Channels, KeyAction, TxCtxMsg};

/// Full `terminal.draw` at most this often for stream + stats; state still
/// updates every message. Increase (e.g. 150-250ms) if CPU is still high;
/// decrease for snappier visuals.
const FEED_REDRAW_MIN_INTERVAL: Duration = Duration::from_millis(150);

/// Max L2 levels per side pushed to the TUI. The orderbook only renders
/// `TOP_N` rows; we keep a small cushion so spline/CLOB merge logic has some
/// depth to choose from.
pub(super) const L2_SNAPSHOT_DEPTH: usize = 20;

/// Initial and maximum retry delays for WSS reconnect backoff.
pub(super) const WSS_RETRY_INIT: Duration = Duration::from_secs(2);
pub(super) const WSS_RETRY_CAP: Duration = Duration::from_secs(30);
