//! Wallet connect / disconnect helpers.

use std::collections::HashMap;
use std::str::FromStr;
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
    let authority = solana_pubkey::Pubkey::from_str(&kp_arc.pubkey().to_string())
        .map_err(|e| format!("{}: {}", strings().st_wallet_load_failed, e))?;

    state.trading.wallet_label = kp_arc.pubkey().to_string();
    state.trading.wallet_loaded = true;
    state.trading.keypair = Some(Arc::clone(&kp_arc));
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
    for bot in state.twaps_view.bots.iter_mut() {
        if let Some(in_flight) = bot.in_flight.take() {
            in_flight.task.abort();
        }
        bot.defer_reason = Some(strings().twap_waiting_wallet.to_string());
    }
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
}
