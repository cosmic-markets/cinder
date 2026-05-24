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
use super::super::state::{SliceOutcome, TuiState, TxStatusMsg};
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
        if let Some(bot) = state.twaps_view.bots.get_mut(i) {
            if let Some((slice_number, outcome)) = bot.try_take_outcome() {
                let s = strings();
                match outcome {
                    SliceOutcome::Confirmed => {
                        let line = format!(
                            "{} {}/{}",
                            s.twap_slice_confirmed, slice_number, bot.slice_count
                        );
                        bot.record_slice_confirmed(now, line);
                        dispatched_any = true;
                    }
                    SliceOutcome::Failed(detail) => {
                        // Cap detail length so a runaway error string can't
                        // blow the bots-modal column width.
                        let short = truncate_status(&detail, 120);
                        let line = format!(
                            "{} {}/{}: {}",
                            s.twap_slice_failed, slice_number, bot.slice_count, short
                        );
                        bot.record_slice_failed(now, line);
                        dispatched_any = true;
                    }
                    SliceOutcome::Unknown(detail) => {
                        let short = truncate_status(&detail, 120);
                        let line = format!(
                            "{} {}/{}: {}",
                            s.twap_slice_unconfirmed, slice_number, bot.slice_count, short
                        );
                        bot.record_slice_unconfirmed(now, line);
                        dispatched_any = true;
                    }
                }
            }
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

        let reference_price_usd = market_price_for_symbol(state, &symbol);

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

fn market_price_for_symbol(state: &TuiState, symbol: &str) -> f64 {
    state
        .market_selector
        .markets
        .iter()
        .find(|m| m.symbol == symbol)
        .map(|m| m.price)
        .filter(|price| price.is_finite() && *price > 0.0)
        .or_else(|| state.price_history.back().copied())
        .unwrap_or(0.0)
}
