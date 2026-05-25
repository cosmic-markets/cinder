//! Wallet connect / disconnect helpers.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use phoenix_rise::PhoenixHttpClient;
use solana_keypair::Keypair;

use super::super::config::SplineConfig;
use super::super::i18n::strings;
use super::super::state::TuiState;
use super::super::tx::TxContext;
use super::tasks::{
    spawn_initial_connect_flow, spawn_trader_orders_ws, spawn_tx_context_task, spawn_wallet_wss,
};
use super::Channels;

pub(super) struct WalletHandles {
    pub wallet_wss: tokio::task::JoinHandle<()>,
    pub initial_balance: tokio::task::JoinHandle<()>,
    pub trader_orders: tokio::task::JoinHandle<()>,
    pub tx_ctx: tokio::task::JoinHandle<()>,
}

/// Spawns the wallet WSS, initial balance/HTTP flow, trader-orders WSS
/// and TxContext loader for `kp`, returning their join handles. The
/// caller (the "Load Wallet" modal handler) owns the keypair source —
/// pass in a `Keypair` already loaded from an explicit path.
pub(super) fn connect_wallet_with_keypair(
    state: &mut TuiState,
    kp: Keypair,
    cfg: &SplineConfig,
    configs: &HashMap<String, SplineConfig>,
    channels: &Channels,
    ws_url: &str,
    http: Arc<PhoenixHttpClient>,
) -> Result<WalletHandles, String> {
    use solana_signer::Signer;
    // Resolve the pubkey BEFORE mutating any state, so an early-failure
    // path doesn't tear down a previously-loaded wallet. The previous
    // version returned no-op task handles and let the caller mistakenly
    // replace its real handles with them.
    let kp_arc = Arc::new(kp);
    // Signer::pubkey() already returns the `solana_pubkey::Pubkey` we want —
    // no base58 round-trip needed.
    let authority: solana_pubkey::Pubkey = kp_arc.pubkey();

    state.trading.wallet_label = authority.to_string();
    state.trading.wallet_loaded = true;
    state.trading.keypair = Some(Arc::clone(&kp_arc));
    // Drop the previous wallet's TxContext (if any) BEFORE the new
    // tx_ctx_task delivers. Otherwise the scheduler's next tick (and any
    // manual order placed during the warm-up window) would build with the
    // OLD ctx (old authority_v2 / trader_pda_v2 / market_addrs) but sign
    // with the NEW keypair — guaranteed on-chain rejection at best,
    // misrouted accounts at worst. The submission paths defer when
    // tx_context is None, so this gates them until handle_tx_context_update
    // installs the freshly-built ctx.
    state.trading.tx_context = None;
    // Reset the "modal already shown" flag on every (re)connect — without
    // this, a user who pressed Esc out of the choice modal during a
    // previous session of the same TUI would never see it again even
    // after a full disconnect/reconnect of the same or a different
    // wallet. The connect flow below re-evaluates and re-prompts when
    // the new authority has no Phoenix account.
    state.trading.referral_choice_shown = false;
    state.trading.set_status_title(strings().st_loading_ctx);

    // Shared `Trader` mirror used by both the trader-state WS task (writer) and
    // the isolated-margin tx builders (readers). Seeded empty here so the
    // `TxContext` future can move it into place even if it resolves before the
    // first WS update. Stored on `TradingState` so RPC swaps and market
    // switches can rebuild a `TxContext` against the same live trader.
    let shared_trader = Arc::new(RwLock::new(TxContext::empty_trader_mirror(authority)));
    state.trading.shared_trader = Some(Arc::clone(&shared_trader));

    let tx_ctx = spawn_tx_context_task(
        Arc::clone(&kp_arc),
        cfg.symbol.clone(),
        Arc::clone(&http),
        Arc::clone(&shared_trader),
        channels.tx_ctx_tx.clone(),
        channels.tx_status.clone(),
    );

    let pk_bytes = kp_arc.pubkey().to_bytes();

    let wallet_wss = spawn_wallet_wss(
        pk_bytes,
        ws_url.to_string(),
        channels.wallet_usdc_tx.clone(),
        channels.wallet_sol_tx.clone(),
    );

    // Keep the handle so disconnect_wallet can abort a slow first HTTP fetch and
    // stop it from delivering a stale balance to a wallet the user has since
    // replaced. The initial flow also activates the COSMIC referral when the
    // wallet has no Phoenix account, so later order submissions aren't rejected
    // by the backend.
    let initial_balance = spawn_initial_connect_flow(
        Arc::clone(&http),
        Arc::clone(&kp_arc),
        cfg.symbol.clone(),
        channels.balance_tx.clone(),
        channels.tx_status.clone(),
    );

    let conditional_asset_symbols = configs
        .values()
        .map(|cfg| (cfg.asset_id, cfg.symbol.clone()))
        .collect();
    let trader_orders = spawn_trader_orders_ws(
        Arc::clone(&kp_arc),
        channels.orders_tx.clone(),
        conditional_asset_symbols,
        Arc::clone(&shared_trader),
    );

    Ok(WalletHandles {
        wallet_wss,
        initial_balance,
        trader_orders,
        tx_ctx,
    })
}

pub(super) fn disconnect_wallet(
    state: &mut TuiState,
    wallet_wss_handle: &mut Option<tokio::task::JoinHandle<()>>,
    blockhash_refresh_handle: &mut Option<tokio::task::JoinHandle<()>>,
    balance_fetch_handle: &mut Option<tokio::task::JoinHandle<()>>,
    trader_orders_handle: &mut Option<tokio::task::JoinHandle<()>>,
    tx_ctx_task: &mut Option<tokio::task::JoinHandle<()>>,
    awaiting_first_tx_ctx: &mut bool,
) {
    if let Some(h) = wallet_wss_handle.take() {
        h.abort();
    }
    if let Some(h) = blockhash_refresh_handle.take() {
        h.abort();
    }
    if let Some(h) = balance_fetch_handle.take() {
        h.abort();
    }
    if let Some(h) = trader_orders_handle.take() {
        h.abort();
    }
    if let Some(h) = tx_ctx_task.take() {
        h.abort();
    }
    *awaiting_first_tx_ctx = false;
    // Bots are authority-bound now — the scheduler refuses to fire when
    // `state.trading.keypair`'s pubkey doesn't match `bot.authority`. So
    // they can stay Running across a disconnect; if the user reconnects
    // the same wallet they resume firing, and if they connect a different
    // wallet the scheduler defers with `twap_waiting_authority`. Cancel
    // any in-flight broadcast though — the spawned task still holds the
    // OLD keypair Arc and would happily land an order against the wallet
    // the user just disconnected from.
    //
    // Before aborting, drain any outcome that the spawned task already
    // pushed into the oneshot. The scheduler polls in_flight at 1Hz, so
    // a slice that confirmed in the last second hasn't been recorded yet
    // — if we just drop the receiver, the Confirmed signal is lost,
    // `slices_submitted` stays stale, and on reconnect of the same wallet
    // the scheduler dispatches the SAME slice number again, executing
    // the slice twice on-chain.
    let now = std::time::Instant::now();
    let slice_count_label = strings().twap_slice_word;
    // Collect "TWAP done" lines to emit after the borrow ends. Disconnect
    // can simultaneously settle multiple bots' final slices; the last line
    // wins in `set_status_title`, which is acceptable — the bots-modal
    // detail rows hold each bot's individual completion line.
    let mut completion_lines: Vec<String> = Vec::new();
    for bot in state.twaps_view.bots.iter_mut() {
        if let Some((slice_number, outcome)) = bot.try_take_outcome() {
            use crate::tui::state::SliceOutcome;
            let slot_count = bot.slice_count;
            let completed = match outcome {
                SliceOutcome::Confirmed => bot.record_slice_confirmed(
                    now,
                    format!(
                        "{} {}/{} (drained on disconnect)",
                        slice_count_label, slice_number, slot_count
                    ),
                ),
                SliceOutcome::Failed(detail) => bot.record_slice_failed(
                    now,
                    format!(
                        "{} {}/{}: {}",
                        slice_count_label, slice_number, slot_count, detail
                    ),
                ),
                SliceOutcome::Unknown(detail) => bot.record_slice_unconfirmed(
                    now,
                    format!(
                        "{} {}/{}: {}",
                        slice_count_label, slice_number, slot_count, detail
                    ),
                ),
            };
            if completed {
                completion_lines.push(super::twap_scheduler::format_completion_status(bot));
            }
        }
        if let Some(in_flight) = bot.in_flight.take() {
            // If a slice is still genuinely in flight (no outcome queued
            // yet), record it as Unknown — the tx may still land on-chain
            // even after `abort()` because abort only cancels at the next
            // .await point. Marking Unknown advances next_slice_number so
            // a same-wallet reconnect doesn't double-broadcast.
            let slice_number = in_flight.slice_number;
            let slice_count = bot.slice_count;
            in_flight.task.abort();
            let completed = bot.record_slice_unconfirmed(
                now,
                format!(
                    "{} {}/{}: aborted on disconnect (may have landed)",
                    slice_count_label, slice_number, slice_count
                ),
            );
            if completed {
                completion_lines.push(super::twap_scheduler::format_completion_status(bot));
            }
        }
        bot.defer_reason = Some(strings().twap_waiting_wallet.to_string());
    }
    // Hold completion line(s) until after the disconnect status fires, so
    // the "TWAP done" message wins the status frame — the user already
    // knows they pressed disconnect, but the completion is fresh news.
    // If more than one bot completed during the drain (rare — would need
    // multiple bots with all-but-one slice already resolved at the moment
    // of disconnect), the LAST completion's line is shown; per-bot rows
    // in the bots modal keep the full per-bot completion text.
    let final_completion_line = completion_lines.into_iter().last();
    state.trading.wallet_loaded = false;
    state.trading.wallet_label.clear();
    state.trading.keypair = None;
    state.trading.tx_context = None;
    state.trading.shared_trader = None;
    state.trading.referral_choice_shown = false;
    state.trading.position = None;
    state.trading.usdc_balance = None;
    state.trading.phoenix_balance = None;
    state.trading.sol_balance = None;
    state.trading.order_kind = super::super::trading::OrderKind::Market;
    // Clear Positions modal rows too; otherwise they persist and the stat feed
    // keeps refreshing their notional/uPnL as if the wallet were still
    // connected.
    state.positions_view.positions.clear();
    state.positions_view.selected_index = 0;
    // Same for the Orders modal — the WS task is aborted above but stale rows would
    // linger.
    state.orders_view.orders.clear();
    state.orders_view.selected_index = 0;
    // And the chart-geometry markers tied to those orders.
    state.order_chart_markers.clear();
    state
        .trading
        .set_status_title(strings().st_wallet_disconnected);
    if let Some(line) = final_completion_line {
        state.trading.set_status_title(line);
    }
}
