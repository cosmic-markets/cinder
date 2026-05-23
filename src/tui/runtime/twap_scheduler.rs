//! TWAP slice scheduler.
//!
//! Driven from the event loop on a 1-second tick (`tick_twap_scheduler`). For
//! every active bot whose `slice_due` predicate fires, this dispatches a
//! market order via the existing `submit_market_order` path and records the
//! slice on the bot. Pause/stop are reflected in `TwapBot::is_active`, so
//! the scheduler simply skips them.

use std::time::Instant;

use tokio::sync::mpsc::UnboundedSender;

use super::super::config::SplineConfig;
use super::super::i18n::strings;
use super::super::math::ui_size_to_num_base_lots;
use super::super::state::{TuiState, TxStatusMsg};
use super::super::trading::TradingSide;
use super::super::tx::submit_market_order;

pub(in crate::tui::runtime) fn tick_twap_scheduler(
    state: &mut TuiState,
    configs: &std::collections::HashMap<String, SplineConfig>,
    tx_status: &UnboundedSender<TxStatusMsg>,
) -> bool {
    let mut dispatched_any = false;
    let now = Instant::now();

    // Snapshot the keypair + tx context once. Both are required to submit
    // any order; if either is missing the bot will just defer this tick and
    // try again on the next.
    let Some(kp) = state.trading.keypair.clone() else {
        return false;
    };
    let Some(ctx) = state.trading.tx_context.clone() else {
        return false;
    };

    // Iterate by index because we mutate each bot in place and don't want to
    // hold an immutable borrow of the Vec across the submit call.
    let bot_count = state.twaps_view.bots.len();
    for i in 0..bot_count {
        // Decide upfront whether this bot fires this tick. Read-only borrow.
        let Some(bot) = state.twaps_view.bots.get(i) else {
            continue;
        };
        if !bot.slice_due(now) {
            continue;
        }

        let symbol = bot.symbol.clone();
        let side = bot.side;
        let slice_size = bot.slice_size;
        let total_slices = bot.slice_count;
        let next_slice_number = bot.slices_submitted + 1;

        // Pull the market config for the bot's symbol. If the user has
        // configs for the active symbol but not for a bot's symbol (rare —
        // would require the market to be delisted mid-bot), surface a
        // status line and stop the bot to prevent a busy retry loop.
        let market_cfg = match configs.get(&symbol) {
            Some(cfg) => cfg.clone(),
            None => {
                if let Some(bot) = state.twaps_view.bots.get_mut(i) {
                    bot.stop();
                    bot.last_status = format!(
                        "{} {}: {}",
                        strings().st_cannot_close,
                        symbol,
                        strings().st_no_market_cfg
                    );
                }
                continue;
            }
        };

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

        // Reference price for isolated-mode collateral sizing. Use the same
        // resolver as a normal market order would.
        let reference_price_usd = market_price_for_symbol(state, &symbol);

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
            total_slices,
        );
        if let Some(bot) = state.twaps_view.bots.get_mut(i) {
            bot.record_slice_submitted(now, status_line.clone());
        }
        state.trading.set_status_title(status_line);
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
