//! TWAP slice scheduler.
//!
//! Driven from the event loop on a 1-second tick (`tick_twap_scheduler`). For
//! every active bot the scheduler does two things:
//! 1. Polls any in-flight slice's outcome oneshot. A `Confirmed` advances
//!    `slices_submitted`; a `Failed` advances `slices_failed`. Both update
//!    `last_slice_at`, so the next slice waits the full interval.
//! 2. If no slice is in flight and `slice_due` returns true, dispatches the
//!    next slice via `submit_market_order` with an outcome oneshot whose
//!    receiver is parked on the bot.
//!
//! Status updates are written to `bot.last_status` — NOT to
//! `state.trading.status_title`, which would race with manual-tx confirmations
//! and clobber the visible signature/detail line.

use std::time::Instant;

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

    // Iterate by index because we mutate each bot in place and don't want to
    // hold an immutable borrow of the Vec across mutation.
    let bot_count = state.twaps_view.bots.len();
    for i in 0..bot_count {
        // Step 1 — poll any in-flight slice's outcome. Done before reading
        // keypair/ctx so a stuck wallet still allows bookkeeping to advance.
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
                        let line = format!(
                            "{} {}/{}: {}",
                            s.twap_slice_failed, slice_number, bot.slice_count, detail
                        );
                        bot.record_slice_failed(now, line);
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
        let next_slice_number = bot.slices_submitted + bot.slices_failed + 1;

        // Wallet + context must be live to dispatch. Both can be transiently
        // missing during a reconnect; surface the wait state on the bot and
        // try again next tick. Critically, do NOT stop the bot — connect
        // races shouldn't permanently kill it.
        let Some(kp) = state.trading.keypair.clone() else {
            if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                bot.last_status = strings().twap_waiting_wallet.to_string();
            }
            continue;
        };
        let Some(ctx) = state.trading.tx_context.clone() else {
            if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                bot.last_status = strings().twap_waiting_trader_sync.to_string();
            }
            continue;
        };

        // Pull the market config for the bot's symbol. If it's missing —
        // e.g. during an RPC swap while the configs map is being rebuilt —
        // surface the wait and skip the tick. Don't stop the bot.
        let market_cfg = match configs.get(&symbol) {
            Some(cfg) => cfg.clone(),
            None => {
                if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                    bot.last_status = format!("{} ({})", strings().twap_waiting_market_cfg, symbol);
                }
                continue;
            }
        };

        // If this bot is on a non-isolated market that isn't the active one,
        // we can't safely dispatch — the local non-isolated builder uses
        // `ctx.market_addrs.*` which is pinned to the active symbol. Defer
        // and surface a status so the user can switch markets to resume.
        if !market_cfg.isolated_only && symbol != active_cfg.symbol {
            if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                bot.last_status = format!(
                    "{} ({} \u{2192} {})",
                    strings().twap_waiting_active_market,
                    active_cfg.symbol,
                    symbol
                );
            }
            continue;
        }

        let num_base_lots = match ui_size_to_num_base_lots(slice_size, market_cfg.base_lot_decimals)
        {
            Ok(n) if n > 0 => n,
            _ => {
                // Slice rounds to zero base lots — this can only happen if
                // the market's lot decimals changed under us. Stop the bot;
                // no amount of retrying will fix it without user intervention.
                if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                    bot.stop();
                    bot.last_status = strings().twap_err_size_too_small.to_string();
                }
                continue;
            }
        };

        // Reference price for isolated-mode collateral sizing. Use the same
        // resolver as a normal market order would.
        let reference_price_usd = market_price_for_symbol(state, &symbol);

        // Create the outcome oneshot before spawning so we can park the
        // receiver on the bot atomically with the dispatch — the next tick
        // sees in_flight=Some and won't double-fire.
        let (otx, orx) = oneshot::channel();

        submit_market_order(
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
            bot.record_slice_dispatched(now, next_slice_number, orx);
            bot.last_status = status_line;
        }
        dispatched_any = true;
    }

    dispatched_any
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
