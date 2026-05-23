//! TWAP bot runtime state.
//!
//! Each [`TwapBot`] represents a running time-weighted-average-price execution.
//! Bots are owned by [`TwapsView`] and advanced on a 1-second tick from the
//! event loop: when `elapsed_since(last_slice_at) >= slice_interval`, the next
//! slice is dispatched as a market order via the normal submit path.
//!
//! Bots are in-memory only — they don't survive process exit. The user
//! interacts with them through the bots modal (toggle with [b]):
//! pause/unpause, stop, restart, and remove.

use std::time::{Duration, Instant};

use super::super::trading::TradingSide;

/// Lifecycle of a running TWAP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwapStatus {
    Running,
    Paused,
    Stopped,
    Completed,
}

impl TwapStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, TwapStatus::Stopped | TwapStatus::Completed)
    }
}

/// A single TWAP bot. The slice scheduler ticks at most once per second from
/// the event loop and submits one slice when due. `last_status` is a short
/// human-readable line (e.g. "slice 3/10 broadcast") that the bots modal
/// renders alongside the status pill.
#[derive(Debug, Clone)]
pub struct TwapBot {
    pub symbol: String,
    pub side: TradingSide,
    pub total_size: f64,
    pub slice_count: u32,
    pub slice_interval: Duration,
    pub slice_size: f64,
    pub slices_submitted: u32,
    pub status: TwapStatus,
    pub started_at: Instant,
    pub last_slice_at: Option<Instant>,
    pub last_status: String,
}

impl TwapBot {
    /// Construct a fresh bot. The first slice is scheduled to fire immediately
    /// on the next event-loop tick (no initial delay) — TWAP execution is
    /// front-loaded so the user sees activity right away.
    pub fn new(
        symbol: String,
        side: TradingSide,
        total_size: f64,
        slice_count: u32,
        duration_secs: u64,
    ) -> Self {
        let slice_size = if slice_count == 0 {
            0.0
        } else {
            total_size / slice_count as f64
        };
        let slice_interval = if slice_count <= 1 {
            Duration::ZERO
        } else {
            Duration::from_secs(duration_secs / slice_count as u64)
        };
        Self {
            symbol,
            side,
            total_size,
            slice_count,
            slice_interval,
            slice_size,
            slices_submitted: 0,
            status: TwapStatus::Running,
            started_at: Instant::now(),
            last_slice_at: None,
            last_status: String::new(),
        }
    }

    /// True if the bot is in a state that consumes scheduler ticks.
    pub fn is_active(&self) -> bool {
        matches!(self.status, TwapStatus::Running)
    }

    /// True if it's time to fire the next slice. `now` is supplied so tests
    /// can pin time.
    pub fn slice_due(&self, now: Instant) -> bool {
        if !self.is_active() {
            return false;
        }
        if self.slices_submitted >= self.slice_count {
            return false;
        }
        match self.last_slice_at {
            None => true,
            Some(prev) => now.duration_since(prev) >= self.slice_interval,
        }
    }

    /// Record that a slice was just dispatched. Bumps the counter and flips to
    /// `Completed` once the last slice has been submitted.
    pub fn record_slice_submitted(&mut self, now: Instant, status_line: impl Into<String>) {
        self.slices_submitted += 1;
        self.last_slice_at = Some(now);
        self.last_status = status_line.into();
        if self.slices_submitted >= self.slice_count {
            self.status = TwapStatus::Completed;
        }
    }

    pub fn pause(&mut self) {
        if matches!(self.status, TwapStatus::Running) {
            self.status = TwapStatus::Paused;
        }
    }

    pub fn resume(&mut self) {
        if matches!(self.status, TwapStatus::Paused) {
            self.status = TwapStatus::Running;
        }
    }

    pub fn stop(&mut self) {
        if !self.status.is_terminal() {
            self.status = TwapStatus::Stopped;
        }
    }

    /// Reset progress and re-arm. Used by the bots-modal [r] hotkey to re-run
    /// a finished or stopped bot from scratch.
    pub fn restart(&mut self) {
        self.slices_submitted = 0;
        self.last_slice_at = None;
        self.status = TwapStatus::Running;
        self.started_at = Instant::now();
        self.last_status.clear();
    }
}

/// Top-level container for all running TWAP bots plus the bots modal cursor.
pub struct TwapsView {
    pub bots: Vec<TwapBot>,
    pub selected_index: usize,
}

impl TwapsView {
    pub fn new() -> Self {
        Self {
            bots: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn push(&mut self, bot: TwapBot) {
        self.bots.push(bot);
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.bots.len() {
            self.selected_index += 1;
        }
    }

    pub fn clamp_index(&mut self) {
        if !self.bots.is_empty() {
            self.selected_index = self.selected_index.min(self.bots.len() - 1);
        } else {
            self.selected_index = 0;
        }
    }

    pub fn selected_mut(&mut self) -> Option<&mut TwapBot> {
        self.bots.get_mut(self.selected_index)
    }

    pub fn remove_selected(&mut self) -> Option<TwapBot> {
        if self.selected_index >= self.bots.len() {
            return None;
        }
        let bot = self.bots.remove(self.selected_index);
        self.clamp_index();
        Some(bot)
    }
}

impl Default for TwapsView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bot() -> TwapBot {
        TwapBot::new("SOL".to_string(), TradingSide::Long, 1.0, 4, 40)
    }

    #[test]
    fn slice_size_divides_total_evenly() {
        let bot = make_bot();
        assert_eq!(bot.slice_size, 0.25);
        assert_eq!(bot.slice_interval, Duration::from_secs(10));
    }

    #[test]
    fn first_slice_due_immediately() {
        let bot = make_bot();
        assert!(bot.slice_due(Instant::now()));
    }

    #[test]
    fn after_submit_not_due_until_interval_passes() {
        let mut bot = make_bot();
        let t0 = Instant::now();
        bot.record_slice_submitted(t0, "slice 1/4");
        // Not yet — interval is 10s but no time has passed.
        assert!(!bot.slice_due(t0));
        // After the interval, due again.
        assert!(bot.slice_due(t0 + Duration::from_secs(10)));
    }

    #[test]
    fn completes_after_last_slice() {
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 2, 2);
        let t0 = Instant::now();
        bot.record_slice_submitted(t0, "1/2");
        assert_eq!(bot.status, TwapStatus::Running);
        bot.record_slice_submitted(t0 + Duration::from_secs(1), "2/2");
        assert_eq!(bot.status, TwapStatus::Completed);
        assert_eq!(bot.slices_submitted, bot.slice_count);
    }

    #[test]
    fn paused_bot_does_not_fire_slices() {
        let mut bot = make_bot();
        bot.pause();
        assert!(!bot.slice_due(Instant::now()));
        bot.resume();
        assert!(bot.slice_due(Instant::now()));
    }

    #[test]
    fn stopped_bot_is_terminal() {
        let mut bot = make_bot();
        bot.stop();
        assert!(bot.status.is_terminal());
        // Resume from terminal is a no-op.
        bot.resume();
        assert_eq!(bot.status, TwapStatus::Stopped);
    }

    #[test]
    fn restart_clears_progress() {
        let mut bot = make_bot();
        bot.record_slice_submitted(Instant::now(), "1/4");
        bot.stop();
        bot.restart();
        assert_eq!(bot.slices_submitted, 0);
        assert_eq!(bot.status, TwapStatus::Running);
        assert!(bot.slice_due(Instant::now()));
    }

    #[test]
    fn view_remove_selected_keeps_cursor_valid() {
        let mut v = TwapsView::new();
        v.push(make_bot());
        v.push(make_bot());
        v.selected_index = 1;
        v.remove_selected();
        assert_eq!(v.selected_index, 0);
        v.remove_selected();
        assert_eq!(v.selected_index, 0);
        assert!(v.bots.is_empty());
    }
}
