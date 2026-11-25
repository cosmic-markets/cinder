//! Rendering helpers used by the runtime loop.

use super::super::config::SplineConfig;
use super::super::state::TuiState;
use super::super::terminal::TuiTerminal;
use super::super::ui;

/// Redraw the TUI if either spline or CLOB data has produced rows.
pub(super) fn redraw_tui(
    terminal: &mut TuiTerminal,
    state: &TuiState,
    cfg: &SplineConfig,
    rpc_host: &str,
) {
    let has_rows = !state.merged_book.bid_rows.is_empty() || !state.merged_book.ask_rows.is_empty();
    if state.last_parsed.is_none() && !has_rows {
        return;
    }
    redraw_tui_force(terminal, state, cfg, rpc_host);
}

/// Unconditionally paint one TUI frame.
pub(super) fn redraw_tui_force(
    terminal: &mut TuiTerminal,
    state: &TuiState,
    cfg: &SplineConfig,
    rpc_host: &str,
) {
    let _ = terminal.draw(|f| ui::render_frame(f, state, cfg, rpc_host));
}
