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
//!
//! Each bot also remembers the wallet authority it was created for. The
//! scheduler refuses to fire a slice when the connected wallet differs from
//! the bot's authority, so a disconnect-then-reconnect-as-another-wallet does
//! NOT redirect remaining slices to the new wallet's funds.

use std::time::{Duration, Instant};

use solana_pubkey::Pubkey;
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
    /// The tx was broadcast and signed but confirmation was never observed
    /// (e.g. RPC pubsub dropped mid-confirm, blockhash expired). Counted
    /// separately so a TWAP can complete without retrying — retrying would
    /// risk double-execution if the original eventually lands — while still
    /// surfacing to the user that some slices may be in limbo.
    Unknown(String),
}

/// A slice that has been dispatched and is waiting for its on-chain outcome.
/// Owns the spawned task handle so `stop()`/`restart()`/`remove()` can abort
/// the in-flight broadcast — without this, dropping just the receiver would
/// let the tx still land while the bot is marked stopped.
pub struct InFlightSlice {
    pub rx: oneshot::Receiver<SliceOutcome>,
    pub slice_number: u32,
    /// Spawned tx-submit task. `abort()` cancels the task at its next await
    /// point; tx that has already been broadcast cannot be unbroadcast, but
    /// canceling during sign/build/etc. prevents wasted network traffic.
    pub task: tokio::task::JoinHandle<()>,
    #[allow(dead_code)]
    pub started_at: Instant,
}

impl InFlightSlice {
    /// Abort the underlying tokio task. Called when the user stops, restarts,
    /// or removes a bot mid-slice. The receiver is dropped on the way out.
    fn abort(&self) {
        self.task.abort();
    }
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
    /// Slices broadcast but never confirmed (RPC subscription dropped, etc.).
    /// Counted separately so the modal can surface "some slices may have
    /// landed without confirmation" rather than reporting them as confirmed
    /// fills.
    pub slices_unconfirmed: u32,
    pub status: TwapStatus,
    pub started_at: Instant,
    pub last_slice_at: Option<Instant>,
    pub last_status: String,
    /// Transient defer reason (e.g. "waiting for trader state to sync"). Kept
    /// separate from `last_status` so a real slice failure detail isn't
    /// clobbered by a 1-Hz defer update during a brief reconnect window.
    pub defer_reason: Option<String>,
    /// Set when paused so resume() can advance `last_slice_at` by the pause
    /// duration — without this, a paused-then-resumed bot fires the next
    /// slice immediately (defeating TWAP's time-weighting guarantee).
    pub paused_at: Option<Instant>,
    /// Slice currently in flight, waiting for its on-chain outcome. The
    /// scheduler refuses to dispatch a new slice while one is in flight,
    /// preventing the 1-Hz scheduler tick from spawning multiple overlapping
    /// orders for the same bot.
    pub in_flight: Option<InFlightSlice>,
    /// Wallet authority the bot was created for. The scheduler refuses to
    /// fire slices when the currently-connected wallet's authority differs;
    /// without this, a disconnect-then-reconnect-as-different-wallet would
    /// silently redirect remaining slices to the new wallet's funds.
    pub authority: Pubkey,
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
        authority: Pubkey,
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
            slices_unconfirmed: 0,
            status: TwapStatus::Running,
            started_at: Instant::now(),
            last_slice_at: None,
            last_status: String::new(),
            defer_reason: None,
            paused_at: None,
            in_flight: None,
            authority,
        }
    }

    /// True if the bot is in a state that consumes scheduler ticks.
    pub fn is_active(&self) -> bool {
        matches!(self.status, TwapStatus::Running)
    }

    /// Total resolved slices (confirmed + failed + unconfirmed). Used by both
    /// the slice-due predicate and the completion check so the two never
    /// disagree on when the bot is done.
    fn slices_resolved(&self) -> u32 {
        self.slices_submitted + self.slices_failed + self.slices_unconfirmed
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
        if self.slices_resolved() >= self.slice_count {
            return false;
        }
        match self.last_slice_at {
            None => true,
            Some(prev) => now.duration_since(prev) >= self.slice_interval,
        }
    }

    /// Update `last_slice_at` from a defer/wait branch in the scheduler.
    /// Without this, a long disconnect or hydration wait leaves
    /// `now - last_slice_at` huge, so the moment the wait clears the bot
    /// fires immediately — defeating the time-weighting guarantee. By
    /// touching `last_slice_at` on every defer tick the bot waits at least
    /// one full interval after recovery.
    pub fn touch_last_slice_at(&mut self, now: Instant) {
        // Only advance when we already have a baseline — never seed
        // `last_slice_at` from a defer, otherwise the first slice of a fresh
        // bot would wait a full interval before firing instead of going
        // immediately.
        if self.last_slice_at.is_some() {
            self.last_slice_at = Some(now);
        }
    }

    /// Helper: write/clear a transient defer reason. The bots modal renders
    /// this with lower priority than `last_status` so a real failure detail
    /// stays visible across a brief reconnect window.
    pub fn set_defer_reason(&mut self, reason: impl Into<String>) {
        self.defer_reason = Some(reason.into());
    }

    pub fn clear_defer_reason(&mut self) {
        self.defer_reason = None;
    }

    /// Recompute the completion status based on counters. Only flips to
    /// `Completed` if the bot is currently Running — a Paused bot whose
    /// in-flight slice resolves should stay Paused until the user resumes.
    fn maybe_complete(&mut self) {
        if matches!(self.status, TwapStatus::Running) && self.slices_resolved() >= self.slice_count
        {
            self.status = TwapStatus::Completed;
        }
    }

    /// Record a slice that confirmed on-chain.
    pub fn record_slice_confirmed(&mut self, now: Instant, status_line: impl Into<String>) {
        self.slices_submitted += 1;
        self.advance_last_slice_at(now);
        self.last_status = status_line.into();
        self.defer_reason = None;
        self.maybe_complete();
    }

    /// Record a slice that failed (build/sign/broadcast/confirm error).
    pub fn record_slice_failed(&mut self, now: Instant, status_line: impl Into<String>) {
        self.slices_failed += 1;
        self.advance_last_slice_at(now);
        self.last_status = status_line.into();
        self.defer_reason = None;
        self.maybe_complete();
    }

    /// Record a slice whose confirmation was never observed. Counted toward
    /// completion but tallied separately so the user can see it.
    pub fn record_slice_unconfirmed(&mut self, now: Instant, status_line: impl Into<String>) {
        self.slices_unconfirmed += 1;
        self.advance_last_slice_at(now);
        self.last_status = status_line.into();
        self.defer_reason = None;
        self.maybe_complete();
    }

    /// Advance `last_slice_at` to `now`, but only if we are currently
    /// Running. When the bot is Paused, anchor on `paused_at` instead so
    /// `resume()`'s pause-shift math doesn't end up pushing
    /// `last_slice_at` past `now`.
    fn advance_last_slice_at(&mut self, now: Instant) {
        match self.status {
            TwapStatus::Paused => {
                // The slice resolved while the user has the bot paused.
                // Record the slice but keep `last_slice_at` anchored at the
                // pause boundary — resume() will shift it forward by the
                // pause duration so the next slice's interval starts when
                // the user resumes, not when this slice happened to land.
                if let Some(paused_at) = self.paused_at {
                    self.last_slice_at = Some(paused_at);
                } else {
                    self.last_slice_at = Some(now);
                }
            }
            _ => {
                self.last_slice_at = Some(now);
            }
        }
    }

    /// Record that a slice was dispatched and is waiting for its outcome.
    /// Stores the oneshot receiver AND the spawned task handle so the bot
    /// can later abort the broadcast if the user stops/restarts/removes.
    pub fn record_slice_dispatched(
        &mut self,
        now: Instant,
        slice_number: u32,
        rx: oneshot::Receiver<SliceOutcome>,
        task: tokio::task::JoinHandle<()>,
    ) {
        self.in_flight = Some(InFlightSlice {
            rx,
            slice_number,
            task,
            started_at: now,
        });
        // Clear any stale defer reason now that a slice is in flight.
        self.defer_reason = None;
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
            // pre-pause `last_slice_at`. Cap the result at `now` so a slice
            // that landed mid-pause doesn't push `last_slice_at` into the
            // future (which would stall the bot via `duration_since`
            // saturating to zero).
            let now = Instant::now();
            if let (Some(paused_at), Some(last_at)) = (self.paused_at, self.last_slice_at) {
                let pause_duration = now.saturating_duration_since(paused_at);
                let shifted = last_at.checked_add(pause_duration).unwrap_or(now);
                // Clamp at `now` — never push the next-slice clock into the
                // future, otherwise `slice_due` would never become true.
                self.last_slice_at = Some(shifted.min(now));
            }
            self.paused_at = None;
            self.status = TwapStatus::Running;
            // A slice that resolved during pause may have crossed the
            // completion threshold but `maybe_complete` skipped the flip
            // because we were Paused. Re-check now.
            self.maybe_complete();
        }
    }

    pub fn stop(&mut self) {
        // Cancel any in-flight broadcast so the user's "stop" actually
        // stops the on-chain side instead of just the bookkeeping side.
        if let Some(in_flight) = self.in_flight.take() {
            in_flight.abort();
        }
        if !self.status.is_terminal() {
            self.status = TwapStatus::Stopped;
        }
        self.paused_at = None;
        self.defer_reason = None;
    }

    /// Reset progress and re-arm. Used by the bots-modal [r] hotkey to re-run
    /// a finished or stopped bot from scratch. Caller is responsible for
    /// confirming with the user — restart re-deploys live capital.
    pub fn restart(&mut self) {
        // Abort any in-flight slice from a previous run before re-arming —
        // otherwise the old tx lands AND slice 1 fires again on the next
        // tick.
        if let Some(in_flight) = self.in_flight.take() {
            in_flight.abort();
        }
        self.slices_submitted = 0;
        self.slices_failed = 0;
        self.slices_unconfirmed = 0;
        self.last_slice_at = None;
        self.status = TwapStatus::Running;
        self.started_at = Instant::now();
        self.last_status.clear();
        self.defer_reason = None;
        self.paused_at = None;
    }
}

impl Drop for TwapBot {
    fn drop(&mut self) {
        // If the user removed the bot mid-flight, abort the in-flight
        // broadcast so the tx doesn't land for a bot the user has dismissed.
        if let Some(in_flight) = self.in_flight.take() {
            in_flight.abort();
        }
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

    fn auth() -> Pubkey {
        Pubkey::new_unique()
    }

    fn make_bot() -> TwapBot {
        TwapBot::new("SOL".to_string(), TradingSide::Long, 1.0, 4, 40, auth())
    }

    #[test]
    fn slice_size_divides_total_evenly() {
        let bot = make_bot();
        assert_eq!(bot.slice_size, 0.25);
        assert_eq!(bot.slice_interval, Duration::from_secs(10));
    }

    #[test]
    fn slice_interval_clamps_to_at_least_one_second() {
        let bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 10, 5, auth());
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
        assert!(!bot.slice_due(t0));
        assert!(bot.slice_due(t0 + Duration::from_secs(10)));
    }

    #[test]
    fn completes_after_last_slice_confirmed() {
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 2, 2, auth());
        let t0 = Instant::now();
        bot.record_slice_confirmed(t0, "1/2");
        assert_eq!(bot.status, TwapStatus::Running);
        bot.record_slice_confirmed(t0 + Duration::from_secs(1), "2/2");
        assert_eq!(bot.status, TwapStatus::Completed);
        assert_eq!(bot.slices_submitted, bot.slice_count);
    }

    #[test]
    fn failures_count_toward_completion() {
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 3, 3, auth());
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
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 4, 40, auth());
        let t0 = Instant::now();
        bot.record_slice_confirmed(t0, "slice 1/4");
        bot.pause();
        std::thread::sleep(Duration::from_millis(20));
        bot.resume();
        assert!(!bot.slice_due(Instant::now()));
    }

    #[test]
    fn slice_resolving_during_pause_does_not_stall_resume() {
        // Before the fix: a slice that confirmed while Paused wrote
        // last_slice_at=now (past paused_at). resume() then added pause
        // duration on top, pushing last_slice_at into the future and
        // saturating `duration_since` to zero — bot would never fire again.
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 4, 4, auth());
        let t0 = Instant::now();
        // Slice 1 dispatched + paused while still in flight.
        bot.pause();
        // Slice resolves while Paused — last_slice_at should anchor at
        // paused_at, not at the late confirm time.
        let confirm_at = t0 + Duration::from_millis(500);
        bot.record_slice_confirmed(confirm_at, "1/4 confirmed");
        bot.resume();
        // The bot must not be stalled — slice_due must eventually return
        // true after enough wall-clock elapses.
        let later = Instant::now() + Duration::from_secs(5);
        assert!(bot.slice_due(later));
    }

    #[test]
    fn record_slice_during_pause_does_not_flip_status_to_completed() {
        // If a bot's last slice confirms while paused, the bot must remain
        // Paused — only the user resuming should advance to Completed.
        let mut bot = TwapBot::new("SOL".into(), TradingSide::Long, 1.0, 1, 1, auth());
        bot.pause();
        bot.record_slice_confirmed(Instant::now(), "done");
        assert_eq!(bot.status, TwapStatus::Paused);
        bot.resume();
        assert_eq!(bot.status, TwapStatus::Completed);
    }

    #[test]
    fn stopped_bot_is_terminal() {
        let mut bot = make_bot();
        bot.stop();
        assert!(bot.status.is_terminal());
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
        assert_eq!(bot.slices_unconfirmed, 0);
        assert_eq!(bot.status, TwapStatus::Running);
        assert!(bot.slice_due(Instant::now()));
    }

    #[test]
    fn touch_last_slice_at_does_not_seed_when_none() {
        // A fresh bot's first slice must still fire immediately even after
        // defer ticks — touch_last_slice_at must NOT promote None to Some.
        let mut bot = make_bot();
        bot.touch_last_slice_at(Instant::now());
        assert!(bot.slice_due(Instant::now()));
    }

    #[test]
    fn touch_last_slice_at_advances_when_some() {
        let mut bot = make_bot();
        let t0 = Instant::now();
        bot.record_slice_confirmed(t0, "1/4");
        // Should NOT be due yet.
        assert!(!bot.slice_due(t0));
        // Defer paths tick last_slice_at forward; bot stays not-due.
        bot.touch_last_slice_at(t0 + Duration::from_secs(20));
        assert!(!bot.slice_due(t0 + Duration::from_secs(20)));
        // After interval from the touched time, due again.
        assert!(bot.slice_due(t0 + Duration::from_secs(30)));
    }

    #[test]
    fn slice_due_false_while_slice_in_flight() {
        let mut bot = make_bot();
        let (_tx, rx) = oneshot::channel();
        let task = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async { tokio::spawn(async {}) });
        bot.record_slice_dispatched(Instant::now(), 1, rx, task);
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
