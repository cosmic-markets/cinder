//! TUI runtime state: chart history, parsed book snapshot, trading panel, and modal views.

use chrono::Utc;

pub mod book;
pub mod liquidation_feed_view;
pub mod market;
pub mod messages;
pub mod trade_panel;
pub mod tui;
pub mod twap_bots_view;
pub mod views;

pub use book::*;
pub use liquidation_feed_view::*;
pub use market::*;
pub use messages::*;
pub use trade_panel::*;
pub use tui::*;
pub use twap_bots_view::*;
pub use views::*;

/// Returns a short UTC clock stamp (`" HH:MM:SS"`) for the status tray.
pub fn make_status_timestamp() -> String {
    format!(" {}", Utc::now().format("%H:%M:%S"))
}

/// Clamp `index` into `0..len` (or `0` when `len == 0`).
pub fn clamp_list_index(index: &mut usize, len: usize) {
    *index = if len == 0 { 0 } else { (*index).min(len - 1) };
}

/// Advance a list cursor by one row if not already at the last item.
pub fn move_list_down(index: &mut usize, len: usize) {
    if *index + 1 < len {
        *index += 1;
    }
}

/// Move a list cursor up by one row if not already at row 0.
pub fn move_list_up(index: &mut usize) {
    *index = index.saturating_sub(1);
}

/// Compute the top-visible row offset so `selected` stays within `visible_slots`.
/// Safely returns `0` when `visible_slots == 0`.
pub fn list_scroll_offset(selected: usize, visible_slots: usize) -> usize {
    if visible_slots == 0 || selected < visible_slots {
        0
    } else {
        selected - visible_slots + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_cursor_helpers_clamp_and_step_safely() {
        let mut idx = 0usize;
        move_list_up(&mut idx);
        move_list_down(&mut idx, 0);
        assert_eq!(idx, 0);

        move_list_down(&mut idx, 2);
        move_list_down(&mut idx, 2);
        assert_eq!(idx, 1);

        clamp_list_index(&mut idx, 1);
        assert_eq!(idx, 0);

        idx = 5;
        clamp_list_index(&mut idx, 0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn list_scroll_offset_handles_zero_slots_and_scrolling() {
        assert_eq!(list_scroll_offset(0, 0), 0);
        assert_eq!(list_scroll_offset(5, 0), 0);
        assert_eq!(list_scroll_offset(3, 5), 0);
        assert_eq!(list_scroll_offset(5, 5), 1);
        assert_eq!(list_scroll_offset(9, 4), 6);
    }
}
