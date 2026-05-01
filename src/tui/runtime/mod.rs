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

/// Throttle for the Phoenix L2 producer task. Phoenix emits many book deltas
/// per second; anything faster than the TUI redraw cadence is coalesced away
/// downstream, so we cap emission here to avoid per-delta allocation and
/// channel traffic.
pub(super) const L2_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(100);

/// Max L2 levels per side pushed to the TUI. The orderbook only renders
/// `TOP_N` rows; we keep a small cushion so spline/CLOB merge logic has some
/// depth to choose from.
pub(super) const L2_SNAPSHOT_DEPTH: usize = 20;

/// Initial and maximum retry delays for WSS reconnect backoff.
pub(super) const WSS_RETRY_INIT: Duration = Duration::from_secs(2);
pub(super) const WSS_RETRY_CAP: Duration = Duration::from_secs(30);
