//! TWAP slice scheduler.
//!
//! Driven from the event loop on a 1-second tick (`tick_twap_scheduler`). For
//! every active bot the scheduler does two things:
//! 1. Polls any in-flight slice's outcome oneshot. `Confirmed` advances
//!    `slices_submitted`; `Failed` advances `slices_failed`; `Unknown`
//!    advances `slices_unconfirmed`. Every variant updates `last_slice_at`,
//!    so the next slice waits the full interval.
//! 2. If no slice is in flight and `slice_due` returns true, dispatches the
//!    next slice via `submit_market_order` with an outcome oneshot whose
//!    receiver is parked on the bot, alongside the spawned task's
//!    JoinHandle (so a later stop/restart/remove can abort the broadcast).
//!
//! Status updates flow through two channels:
//! - `bot.last_status` captures slice events (dispatched / confirmed /
//!   failed) and persists across transient defers.
//! - `bot.defer_reason` captures transient "waiting for wallet/cfg/sync"
//!   states. The bots-modal renderer prefers `last_status` and falls back to
//!   `defer_reason` when no slice event has happened recently.
//!
//! `submit_market_order` is invoked with `silent_status = true` so each
//! slice's broadcast/confirm milestones do NOT clobber the global
//! `state.trading.status_title` — that field is reserved for manual orders
//! and lifecycle events.

use std::time::Instant;
// FromStr import previously needed for `Pubkey::from_str(kp.pubkey().to_string())`
// — dropped after switching to direct `kp.pubkey()`. Kept commented to flag the
// pattern if a future contributor reaches for it.

use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use super::super::config::SplineConfig;
use super::super::i18n::strings;
use super::super::math::ui_size_to_num_base_lots;
use super::super::state::{SliceOutcome, TuiState, TwapBot, TxStatusMsg};
use super::super::trading::TradingSide;
use super::super::tx::submit_market_order;

pub(in crate::tui::runtime) fn tick_twap_scheduler(
    state: &mut TuiState,
    configs: &std::collections::HashMap<String, SplineConfig>,
    active_cfg: &SplineConfig,
    tx_status: &UnboundedSender<TxStatusMsg>,
) -> bool {
    let mut dispatched_any = false;
    let now = Instant::now();

    // Snapshot the live wallet authority once per tick so each bot can
    // verify it owns the connected wallet before dispatching a slice.
    let live_authority: Option<solana_pubkey::Pubkey> = state.trading.keypair.as_ref().map(|kp| {
        use solana_signer::Signer;
        kp.pubkey()
    });

    let bot_count = state.twaps_view.bots.len();
    for i in 0..bot_count {
        // Step 1 — poll any in-flight slice's outcome. Run even when the bot
        // is Paused so the receiver doesn't bloat indefinitely; pause-aware
        // bookkeeping in `record_slice_*` will anchor `last_slice_at` to
        // `paused_at` and `maybe_complete` will refuse to flip status while
        // Paused.
        let mut just_completed: Option<String> = None;
        if let Some(bot) = state.twaps_view.bots.get_mut(i)
            && let Some((slice_number, outcome)) = bot.try_take_outcome()
        {
            let completed = record_resolved_slice_outcome(bot, now, slice_number, outcome);
            dispatched_any = true;
            if completed {
                // Capture the formatted line now while we still have the
                // bot borrow — then drop the borrow before touching
                // `state.trading.set_status_title` below.
                just_completed = Some(format_completion_status(bot));
            }
        }
        if let Some(line) = just_completed {
            state.trading.set_status_title(line);
        }

        // Step 2 — decide whether to dispatch a new slice this tick.
        let Some(bot) = state.twaps_view.bots.get(i) else {
            continue;
        };
        if !bot.slice_due(now) {
            continue;
        }

        let symbol = bot.symbol.clone();
        let side = bot.side;
        let slice_size = bot.slice_size;
        let bot_authority = bot.authority;
        let next_slice_number =
            bot.slices_submitted + bot.slices_failed + bot.slices_unconfirmed + 1;

        // Authority check: the connected wallet must match the wallet that
        // created the bot. Without this, a disconnect-then-reconnect-with-a-
        // different-wallet would silently redirect remaining slices.
        let Some(live_authority) = live_authority else {
            defer_with_reason(state, i, now, strings().twap_waiting_wallet);
            continue;
        };
        if live_authority != bot_authority {
            defer_with_reason(state, i, now, strings().twap_waiting_authority);
            continue;
        }

        // Wallet + context must be live to dispatch. Both can be transiently
        // missing during a reconnect. Defer paths now touch `last_slice_at`
        // so the bot waits at least one full interval after recovery
        // instead of burst-firing the moment hydration completes.
        let Some(kp) = state.trading.keypair.clone() else {
            defer_with_reason(state, i, now, strings().twap_waiting_wallet);
            continue;
        };
        let Some(ctx) = state.trading.tx_context.clone() else {
            defer_with_reason(state, i, now, strings().twap_waiting_trader_sync);
            continue;
        };

        // Pull the market config for the bot's symbol. Missing → defer.
        let market_cfg = match configs.get(&symbol) {
            Some(cfg) => cfg.clone(),
            None => {
                let reason = format!("{} ({})", strings().twap_waiting_market_cfg, symbol);
                defer_with_reason_owned(state, i, now, reason);
                continue;
            }
        };

        // If this bot is on a non-isolated market that isn't the active one,
        // we can't safely dispatch — the local non-isolated builder uses
        // `ctx.market_addrs.*` which is pinned to the active symbol.
        if !market_cfg.isolated_only && symbol != active_cfg.symbol {
            let reason = format!(
                "{} ({} \u{2192} {})",
                strings().twap_waiting_active_market,
                active_cfg.symbol,
                symbol
            );
            defer_with_reason_owned(state, i, now, reason);
            continue;
        }

        // For isolated markets, also require trader-state hydration before
        // dispatching — `submit_market_order` would otherwise fail
        // synchronously, consuming a slot in the failure budget.
        if market_cfg.isolated_only && ctx.snapshot_trader().is_none() {
            defer_with_reason(state, i, now, strings().twap_waiting_trader_sync);
            continue;
        }

        let num_base_lots = match ui_size_to_num_base_lots(slice_size, market_cfg.base_lot_decimals)
        {
            Ok(n) if n > 0 => n,
            _ => {
                if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                    bot.stop();
                    bot.last_status = strings().twap_err_size_too_small.to_string();
                }
                continue;
            }
        };

        let reference_price_usd = match market_price_for_symbol(state, &symbol, &active_cfg.symbol)
        {
            Some(price) => price,
            None if market_cfg.isolated_only => {
                let reason = format!("{} ({})", strings().waiting_data, symbol);
                defer_with_reason_owned(state, i, now, reason);
                continue;
            }
            None => 0.0,
        };

        // Create the outcome oneshot before spawning so we can park the
        // receiver on the bot atomically with the dispatch — the next tick
        // sees in_flight=Some and won't double-fire.
        let (otx, orx) = oneshot::channel();

        let task = submit_market_order(
            kp.clone(),
            ctx.clone(),
            symbol.clone(),
            side,
            num_base_lots,
            false,
            slice_size,
            0,
            market_cfg.isolated_only,
            market_cfg.max_leverage,
            reference_price_usd,
            tx_status.clone(),
            Some(otx),
            true, // silent_status — slice updates go to bot.last_status only
        );

        let s = strings();
        let side_lbl = match side {
            TradingSide::Long => s.long_label,
            TradingSide::Short => s.short_label,
        };
        let status_line = format!(
            "{}: {} {} {} ({} {}/{})",
            s.twap_slice_sent,
            side_lbl,
            slice_size,
            symbol,
            s.twap_slice_word,
            next_slice_number,
            bot.slice_count,
        );
        if let Some(bot) = state.twaps_view.bots.get_mut(i) {
            bot.record_slice_dispatched(now, next_slice_number, orx, task);
            bot.last_status = status_line;
            bot.clear_defer_reason();
        }
        dispatched_any = true;
    }

    dispatched_any
}

/// Settle a bot's in-flight slice on an interruption (RPC reconnect,
/// wallet swap). Drains a queued outcome if one is already on the
/// receiver; otherwise marks the in-flight slice as `Unknown` so the
/// counter advances and a same-wallet reconnect doesn't double-broadcast.
/// Returns `Some(status_line)` if settling this bot crossed the
/// completion threshold — the caller emits it on the status frame.
pub(in crate::tui::runtime) fn settle_in_flight_for_interrupt(
    bot: &mut TwapBot,
    now: Instant,
    unknown_detail: impl Into<String>,
) -> Option<String> {
    if let Some((slice_number, outcome)) = bot.try_take_outcome() {
        let completed = record_resolved_slice_outcome(bot, now, slice_number, outcome);
        if completed {
            return Some(format_completion_status(bot));
        }
        return None;
    }

    if let Some(in_flight) = bot.in_flight.take() {
        let slice_number = in_flight.slice_number;
        in_flight.task.abort();
        let completed = record_resolved_slice_outcome(
            bot,
            now,
            slice_number,
            SliceOutcome::Unknown(unknown_detail.into()),
        );
        if completed {
            return Some(format_completion_status(bot));
        }
    }

    None
}

/// Record a resolved slice outcome on the bot. Returns `true` if this
/// resolution completed the bot — the caller emits a one-shot status-line
/// "TWAP done" message so the user sees the finish even when the bots modal
/// is closed.
#[must_use]
fn record_resolved_slice_outcome(
    bot: &mut TwapBot,
    now: Instant,
    slice_number: u32,
    outcome: SliceOutcome,
) -> bool {
    let s = strings();
    match outcome {
        SliceOutcome::Confirmed => {
            let line = format!(
                "{} {}/{}",
                s.twap_slice_confirmed, slice_number, bot.slice_count
            );
            bot.record_slice_confirmed(now, line)
        }
        SliceOutcome::Failed(detail) => {
            // Cap detail length so a runaway error string can't blow the
            // bots-modal column width.
            let short = truncate_status(&detail, 120);
            let line = format!(
                "{} {}/{}: {}",
                s.twap_slice_failed, slice_number, bot.slice_count, short
            );
            bot.record_slice_failed(now, line)
        }
        SliceOutcome::Unknown(detail) => {
            let short = truncate_status(&detail, 120);
            let line = format!(
                "{} {}/{}: {}",
                s.twap_slice_unconfirmed, slice_number, bot.slice_count, short
            );
            bot.record_slice_unconfirmed(now, line)
        }
    }
}

/// Format the "TWAP done" status-line emitted on completion. Includes the
/// breakdown when any slice failed or went unconfirmed so the user can
/// immediately tell whether the bot finished cleanly.
pub(crate) fn format_completion_status(bot: &TwapBot) -> String {
    let s = strings();
    let side_lbl = match bot.side {
        TradingSide::Long => s.long_label,
        TradingSide::Short => s.short_label,
    };
    let imperfect = bot.slices_failed > 0 || bot.slices_unconfirmed > 0;
    if imperfect {
        format!(
            "{}: {} {} {} \u{2014} {}\u{2713}/{}\u{2717}/{}? of {}",
            s.twap_completed,
            side_lbl,
            bot.total_size,
            bot.symbol,
            bot.slices_submitted,
            bot.slices_failed,
            bot.slices_unconfirmed,
            bot.slice_count,
        )
    } else {
        format!(
            "{}: {} {} {} \u{2014} {}/{} {}",
            s.twap_completed,
            side_lbl,
            bot.total_size,
            bot.symbol,
            bot.slices_submitted,
            bot.slice_count,
            s.twap_slice_confirmed,
        )
    }
}

fn defer_with_reason(state: &mut TuiState, i: usize, now: Instant, reason: &str) {
    if let Some(bot) = state.twaps_view.bots.get_mut(i) {
        bot.set_defer_reason(reason);
        bot.touch_last_slice_at(now);
    }
}

fn defer_with_reason_owned(state: &mut TuiState, i: usize, now: Instant, reason: String) {
    if let Some(bot) = state.twaps_view.bots.get_mut(i) {
        bot.set_defer_reason(reason);
        bot.touch_last_slice_at(now);
    }
}

fn truncate_status(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

fn market_price_for_symbol(state: &TuiState, symbol: &str, active_symbol: &str) -> Option<f64> {
    let market_mark = state
        .market_selector
        .markets
        .iter()
        .find(|m| m.symbol == symbol)
        .map(|m| m.price)
        .filter(|price| price.is_finite() && *price > 0.0);
    if market_mark.is_some() {
        return market_mark;
    }
    if symbol == active_symbol {
        state
            .price_history
            .back()
            .copied()
            .filter(|price| price.is_finite() && *price > 0.0)
    } else {
        None
    }
}
