//! Phoenix TUI: terminal trading client for Phoenix perpetuals.

pub mod app;
pub mod tui;

pub use app::run;

/// Backwards-compatible module name for older callers. New code should use
/// [`tui`], which matches the on-disk layout.
pub mod spline {
    pub use crate::tui::*;
}
