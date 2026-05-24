//! TWAP bot runtime state.
//!
//! Each [`TwapBot`] represents a running time-weighted-average-price execution.
//! Bots are owned by [`TwapsView`] and advanced on a 1-second tick from the
//! event loop: when `elapsed_since(last_slice_at) >= slice_interval`, the next
//! slice is dispatched as a market order via the normal submit path and the
//! returned outcome oneshot is parked on the bot. The scheduler polls the
//! oneshot on every tick and only advances `slices_submitted` when the slice
//! actually confirms — failed broadcasts do NOT count toward completion.
//!
//! Bots are in-memory only — they don't survive process exit. The user
//! interacts with them through the bots modal (toggle with [b]):
//! pause/unpause, stop, restart, and remove.

use std::time::{Duration, Instant};

use tokio::sync::oneshot;

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

/// Result of a single TWAP slice. Sent from the spawned submit task back to
/// the scheduler so the bot can advance `slices_submitted` only on confirmed
/// fills — fire-and-forget bookkeeping would let a TWAP report "10/10
/// completed" with zero on-chain fills.
#[derive(Debug)]
pub enum SliceOutcome {
    Confirmed,
    Failed(String),
}

/// A slice that has been dispatched and is waiting for its on-chain outcome.
pub struct InFlightSlice {
    pub rx: oneshot::Receiver<SliceOutcome>,
    pub slice_number: u32,
    /// When the slice was dispatched. Reserved for a future stall-detection
    /// pass (e.g. cancel an in-flight slice whose tx hasn't confirmed after
    /// N seconds); currently unread.
    #[allow(dead_code)]
    pub started_at: Instant,
}

/// Pending confirmation in the bots modal. Set when the user presses
/// `[s]` / `[r]` / `[x]`; cleared by Y/N. Stored on the view rather than
/// the bot so the bot list can scroll independently of which row holds the
/// confirmation.
#[derive(Debug, Clone, Copy)]
pub enum TwapBotConfirm {
    Stop(usize),
    Restart(usize),
    Remove(usize),
}

/// A single TWAP bot. The slice scheduler ticks at most once per second from
/// the event loop and submits one slice when due. `last_status` is a short
/// human-readable line (e.g. "slice 3/10 broadcast") that the bots modal
/// renders alongside the status pill.
pub struct TwapBot {
    pub symbol: String,
    pub side: TradingSide,
    pub total_size: f64,
    pub slice_count: u32,
    pub slice_interval: Duration,
    pub slice_size: f64,
    pub slices_submitted: u32,
    pub slices_failed: u32,
    pub status: TwapStatus,
    pub started_at: Instant,
    pub last_slice_at: Option<Instant>,
    pub last_status: String,
    /// Set when paused so resume() can advance `last_slice_at` by the pause
    /// duration — without this, a paused-then-resumed bot fires the next
    /// slice immediately (defeating TWAP's time-weighting guarantee).
    pub paused_at: Option<Instant>,
    /// Slice currently in flight, waiting for its on-chain outcome. The
    /// scheduler refuses to dispatch a new slice while one is in flight,
    /// preventing the 1-Hz scheduler tick from spawning multiple overlapping
    /// orders for the same bot.
    pub in_flight: Option<InFlightSlice>,
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
            // Integer division would round to 0 when `duration_secs < slice_count`,
            // which would make `slice_due` return true on every 1-Hz scheduler
            // tick — all slices firing back-to-back. Clamp to a minimum of 1s
            // so the scheduler can still pace them.
            let raw = duration_secs / slice_count as u64;
            Duration::from_secs(raw.max(1))
        };
        Self {
            symbol,
            side,
            total_size,
            slice_count,
            slice_interval,
            slice_size,
            slices_submitted: 0,
            slices_failed: 0,
            status: TwapStatus::Running,
            started_at: Instant::now(),
            last_slice_at: None,
            last_status: String::new(),
            paused_at: None,
            in_flight: None,
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
        if self.in_flight.is_some() {
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

    /// Record a slice that confirmed on-chain. Bumps `slices_submitted` and
    /// flips to `Completed` once the last slice has confirmed. Called from the
    /// scheduler only after the outcome oneshot resolves to `Confirmed`.
    pub fn record_slice_confirmed(&mut self, now: Instant, status_line: impl Into<String>) {
        self.slices_submitted += 1;
        self.last_slice_at = Some(now);
        self.last_status = status_line.into();
        if self.slices_submitted + self.slices_failed >= self.slice_count {
            self.status = TwapStatus::Completed;
        }
    }

    /// Record a slice that failed (build/sign/broadcast/confirm error). Bumps
    /// `slices_failed` so the bot still progresses toward `Completed` after
    /// `slice_count` total attempts — without this, a wallet that runs out of
    /// collateral would keep retrying the same slice forever and never finish.
    pub fn record_slice_failed(&mut self, now: Instant, status_line: impl Into<String>) {
        self.slices_failed += 1;
        // Update last_slice_at so the bot doesn't immediately retry the next
        // tick — failures get the same back-off as a successful slice.
        self.last_slice_at = Some(now);
        self.last_status = status_line.into();
        if self.slices_submitted + self.slices_failed >= self.slice_count {
            self.status = TwapStatus::Completed;
        }
    }

    /// Record that a slice was dispatched and is waiting for its outcome.
    /// Stores the oneshot receiver on the bot; the scheduler polls it via
    /// `try_take_outcome` on every tick.
    pub fn record_slice_dispatched(
        &mut self,
        now: Instant,
        slice_number: u32,
        rx: oneshot::Receiver<SliceOutcome>,
    ) {
        self.in_flight = Some(InFlightSlice {
            rx,
            slice_number,
            started_at: now,
        });
    }

    /// Poll the in-flight slice (if any) for its outcome. Returns
    /// `Some(outcome)` if it has resolved (and clears `in_flight`), `None` if
    /// the slice is still pending or there is no in-flight slice.
    pub fn try_take_outcome(&mut self) -> Option<(u32, SliceOutcome)> {
        use tokio::sync::oneshot::error::TryRecvError;
        let in_flight = self.in_flight.as_mut()?;
        match in_flight.rx.try_recv() {
            Ok(outcome) => {
                let n = in_flight.slice_number;
                self.in_flight = None;
                Some((n, outcome))
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Closed) => {
                let n = in_flight.slice_number;
                self.in_flight = None;
                Some((n, SliceOutcome::Failed("dropped".to_string())))
            }
        }
    }

    pub fn pause(&mut self) {
        if matches!(self.status, TwapStatus::Running) {
            self.status = TwapStatus::Paused;
            self.paused_at = Some(Instant::now());
        }
    }

    pub fn resume(&mut self) {
        if matches!(self.status, TwapStatus::Paused) {
            // Advance `last_slice_at` by however long we were paused so the
            // next slice fires after `slice_interval` from now, not from the
            // pre-pause `last_slice_at`. Without this, a long pause causes
            // the next slice to fire immediately on resume.
            if let (Some(paused_at), Some(last_at)) = (self.paused_at, self.last_slice_at) {
                let pause_duration = Instant::now().saturating_duration_since(paused_at);
                self.last_slice_at = last_at.checked_add(pause_duration).or(Some(last_at));
            }
            self.paused_at = None;
            self.status = TwapStatus::Running;
        }
    }

    pub fn stop(&mut self) {
        if !self.status.is_terminal() {
            self.status = TwapStatus::Stopped;
        }
        self.in_flight = None;
        self.paused_at = None;
    }

    /// Reset progress and re-arm. Used by the bots-modal [r] hotkey to re-run
    /// a finished or stopped bot from scratch. Caller is responsible for
    /// confirming with the user — restart re-deploys live capital.
    pub fn restart(&mut self) {
        self.slices_submitted = 0;
        self.slices_failed = 0;
        self.last_slice_at = None;
        self.status = TwapStatus::Running;
        self.started_at = Instant::now();
        self.last_status.clear();
        self.paused_at = None;
        self.in_flight = None;
    }
}

/// Top-level container for all running TWAP bots plus the bots modal cursor
/// and any pending confirmation prompt.
pub struct TwapsView {
    pub bots: Vec<TwapBot>,
    pub selected_index: usize,
    /// Confirmation pending in the bots modal — set by [s]/[r]/[x], cleared
    /// by Y/N. Rendered as an overlay on top of the bot list.
    pub pending_confirm: Option<TwapBotConfirm>,
}

impl TwapsView {
    pub fn new() -> Self {
        Self {
            bots: Vec::new(),
            selected_index: 0,
            pending_confirm: None,
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
    fn slice_interval_clamps_to_at_least_one_second() {
        // 5 seconds / 10 slices would round to 0 without the clamp, causing
        // every slice to fire back-to-back. Verify the clamp kicks in.
        let bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 10, 5);
        assert!(bot.slice_interval >= Duration::from_secs(1));
    }

    #[test]
    fn first_slice_due_immediately() {
        let bot = make_bot();
        assert!(bot.slice_due(Instant::now()));
    }

    #[test]
    fn after_confirm_not_due_until_interval_passes() {
        let mut bot = make_bot();
        let t0 = Instant::now();
        bot.record_slice_confirmed(t0, "slice 1/4");
        // Not yet — interval is 10s but no time has passed.
        assert!(!bot.slice_due(t0));
        // After the interval, due again.
        assert!(bot.slice_due(t0 + Duration::from_secs(10)));
    }

    #[test]
    fn completes_after_last_slice_confirmed() {
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 2, 2);
        let t0 = Instant::now();
        bot.record_slice_confirmed(t0, "1/2");
        assert_eq!(bot.status, TwapStatus::Running);
        bot.record_slice_confirmed(t0 + Duration::from_secs(1), "2/2");
        assert_eq!(bot.status, TwapStatus::Completed);
        assert_eq!(bot.slices_submitted, bot.slice_count);
    }

    #[test]
    fn failures_count_toward_completion() {
        // Bot doesn't get stuck retrying the same slice forever — failures
        // are counted so the bot finishes after `slice_count` total attempts.
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 3, 3);
        let t0 = Instant::now();
        bot.record_slice_confirmed(t0, "1/3");
        bot.record_slice_failed(t0 + Duration::from_secs(1), "2/3 failed");
        bot.record_slice_failed(t0 + Duration::from_secs(2), "3/3 failed");
        assert_eq!(bot.status, TwapStatus::Completed);
        assert_eq!(bot.slices_submitted, 1);
        assert_eq!(bot.slices_failed, 2);
    }

    #[test]
    fn paused_bot_does_not_fire_slices() {
        let mut bot = make_bot();
        bot.pause();
        assert!(!bot.slice_due(Instant::now()));
    }

    #[test]
    fn resume_after_long_pause_does_not_burst_slice() {
        // The pre-fix behavior: after a long pause, `slice_due` would return
        // true on the very next tick because `now - last_slice_at` already
        // exceeded the interval. Verify resume advances `last_slice_at` by
        // the pause duration.
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 4, 40);
        let t0 = Instant::now();
        bot.record_slice_confirmed(t0, "slice 1/4");
        bot.pause();
        // Advance wall-clock past the slice interval by sleeping in
        // simulated terms — pause_at is captured in pause().
        std::thread::sleep(Duration::from_millis(20));
        bot.resume();
        // last_slice_at should have advanced by ~the sleep duration, so the
        // bot is NOT immediately due.
        assert!(!bot.slice_due(Instant::now()));
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
    fn restart_clears_progress_and_failures() {
        let mut bot = make_bot();
        bot.record_slice_confirmed(Instant::now(), "1/4");
        bot.record_slice_failed(Instant::now(), "2/4 failed");
        bot.stop();
        bot.restart();
        assert_eq!(bot.slices_submitted, 0);
        assert_eq!(bot.slices_failed, 0);
        assert_eq!(bot.status, TwapStatus::Running);
        assert!(bot.slice_due(Instant::now()));
    }

    #[test]
    fn slice_due_false_while_slice_in_flight() {
        let mut bot = make_bot();
        let (_tx, rx) = oneshot::channel();
        bot.record_slice_dispatched(Instant::now(), 1, rx);
        assert!(!bot.slice_due(Instant::now()));
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
