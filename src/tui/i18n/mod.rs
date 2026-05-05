//! Static UI string tables for English, Chinese (Simplified), Russian, and
//! Spanish.
//!
//! Call [`strings()`] to get the appropriate table based on the persisted
//! `UserConfig::language`. The result is a `&'static Strings` — zero
//! allocation and safe to call on every render frame.

use super::config::{current_user_config, Language};

/// All user-visible, translatable UI labels.
pub struct Strings {
    // ── Status bar ──────────────────────────────────────────────────────────
    pub status: &'static str,
    pub orders: &'static str,
    pub positions: &'static str,
    pub markets: &'static str,
    pub quit: &'static str,
    pub txid: &'static str,

    // ── Balance panel ────────────────────────────────────────────────────────
    pub balance: &'static str,
    pub wallet: &'static str,
    pub perps: &'static str,
    pub upnl: &'static str,
    pub deposit: &'static str,
    pub withdraw: &'static str,
    pub amt: &'static str,

    // ── Trading panel ────────────────────────────────────────────────────────
    pub trade: &'static str,
    pub not_loaded: &'static str,
    pub notional: &'static str,
    pub lev: &'static str,
    pub buy: &'static str,
    pub sell: &'static str,
    pub mkt: &'static str,
    pub lmt: &'static str,
    pub stp: &'static str,
    pub size: &'static str,
    pub px: &'static str,
    pub market_order: &'static str,
    pub limit_order: &'static str,
    pub stop_order: &'static str,
    pub close: &'static str,
    pub set: &'static str,
    pub cancel: &'static str,
    pub confirm: &'static str,
    pub close_position: &'static str,
    pub cancel_all_orders: &'static str,
    pub no_position: &'static str,
    pub liq: &'static str,

    // ── Position side labels (LONG / SHORT) ──────────────────────────────────
    pub long_label: &'static str,
    pub short_label: &'static str,

    // ── Orderbook ────────────────────────────────────────────────────────────
    pub vol: &'static str,
    pub oi: &'static str,
    pub mark: &'static str,
    pub index_price: &'static str,
    pub funding: &'static str,
    pub waiting_data: &'static str,
    pub asks: &'static str,
    pub bids: &'static str,
    pub slot: &'static str,
    pub trader: &'static str,
    pub depth: &'static str,
    pub bid_abbrev: &'static str,
    pub ask_abbrev: &'static str,
    pub no_spread: &'static str,

    // ── Chart panel ──────────────────────────────────────────────────────────
    /// Title-bar label on the price-chart frame (e.g. "Microprice EMA").
    /// Identifies the smoothed size-weighted touch the chart plots.
    pub microprice_ema: &'static str,

    // ── Common modal navigation hints ────────────────────────────────────────
    pub select: &'static str,
    pub back: &'static str,
    pub view_market: &'static str,
    pub close_pos: &'static str,
    pub close_all: &'static str,
    pub cxl_order: &'static str,
    pub cxl_all: &'static str,

    // ── Column headers shared across modals ──────────────────────────────────
    pub market: &'static str,
    pub side: &'static str,
    pub entry: &'static str,
    /// Shorter "Notional" label for table columns with limited width.
    pub notional_col: &'static str,
    pub pnl: &'static str,
    pub liq_col: &'static str,
    pub lev_col: &'static str,
    pub no_open_positions: &'static str,

    // ── Orders modal ─────────────────────────────────────────────────────────
    pub order_type: &'static str,
    pub price: &'static str,
    pub filled: &'static str,
    pub flags: &'static str,
    pub no_open_orders: &'static str,

    // ── Market selector ──────────────────────────────────────────────────────
    pub pct_change: &'static str,
    pub leverage: &'static str,
    pub vol_24h: &'static str,

    // ── Config modal ─────────────────────────────────────────────────────────
    pub config: &'static str,
    pub rpc_url: &'static str,
    pub language: &'static str,
    pub clob_orders: &'static str,
    pub clob_orders_note: &'static str,
    pub on: &'static str,
    pub off: &'static str,
    pub save_reconnect: &'static str,
    pub edit: &'static str,
    pub toggle: &'static str,
    pub rpc_default: &'static str,

    // ── Quit modal ───────────────────────────────────────────────────────────
    pub quit_confirm: &'static str,

    // ── Ledger modal ─────────────────────────────────────────────────────────
    pub ledger_title: &'static str,
    pub ledger_col_time: &'static str,
    pub ledger_col_action: &'static str,
    pub ledger_empty: &'static str,
    pub ledger_copied: &'static str,
    pub ledger_copy_failed: &'static str,

    // ── Top positions modal ──────────────────────────────────────────────────
    /// Modal title, e.g. "top positions".
    pub top_positions_title: &'static str,
    /// Placeholder while the first on-chain fetch is in flight.
    pub top_positions_loading: &'static str,
    /// Shown when the fetched tree is empty (no active positions).
    pub top_positions_empty: &'static str,
    /// Column header: "#" rank.
    pub top_positions_rank: &'static str,
    /// Column header: "Trader" (authority / node pointer).
    pub top_positions_trader: &'static str,
    /// Status hint shown when Enter is pressed on a row whose trader pubkey
    /// hasn't been resolved by the GTI cache yet (rare; resolves on next tick).
    pub top_positions_no_trader: &'static str,
    /// Modal footer hint for the Enter key — "copy trader" / "复制交易方".
    pub top_positions_copy_hint: &'static str,

    // ── Liquidation feed modal ─────────────────────────────────────────────
    /// Bottom-bar label for the [F] hotkey ("Liquidations" / "强平").
    pub liquidations_title: &'static str,
    /// Header status indicator next to the entry counter ("live" / "实时").
    pub liq_feed_live: &'static str,
    /// Header status indicator shown while the startup backfill is still
    /// streaming rows ("backfilling…" / "回填中…").
    pub liq_feed_backfilling: &'static str,
    /// Footer hint paired with ↑↓ ("scroll" / "滚动").
    pub liq_feed_scroll: &'static str,
    /// Footer hint paired with Enter ("open market" / "切换市场") — switches
    /// the active market to the one the selected liquidation occurred in.
    pub liq_feed_open_market: &'static str,
    /// Placeholder shown before the first liquidation arrives.
    pub liq_feed_waiting: &'static str,

    // ── Status bar messages — static (no format args) ────────────────────────
    pub st_loading_ctx: &'static str,
    pub st_wallet_disconnected: &'static str,
    pub st_wallet_not_loaded: &'static str,
    pub st_wallet_connected: &'static str,
    /// Title of the "Load Wallet" modal opened by [w].
    pub st_load_wallet_title: &'static str,
    /// Footer hint next to "Enter" inside the load-wallet modal.
    pub st_load_wallet_action: &'static str,
    /// Field label above the editable path in the load-wallet modal.
    pub st_wallet_path_label: &'static str,
    /// Inline error prefix in the load-wallet modal when a load fails.
    pub st_wallet_load_failed: &'static str,
    pub st_ctx_loading: &'static str,
    pub st_switching_market: &'static str,
    pub st_lim_cleared: &'static str,
    pub st_lim_must_positive: &'static str,
    pub st_invalid_price: &'static str,
    pub st_market_mode: &'static str,
    pub st_no_mark_price: &'static str,
    pub st_enter_price: &'static str,
    /// "Enter stop trigger price and press Enter (Esc to cancel)"
    pub st_enter_stop_price: &'static str,
    /// "Stop trigger price cleared — switching back to Market"
    pub st_stop_cleared: &'static str,
    /// "Stop trigger price must be a positive number"
    pub st_stop_must_positive: &'static str,
    /// "Stop trigger price set to $" — append value
    pub st_stop_set: &'static str,
    /// "Switched to Stop-Market @ $" — append value
    pub st_switched_stop: &'static str,
    pub st_enter_size: &'static str,
    pub st_invalid_size: &'static str,
    pub st_invalid_amount: &'static str,
    pub st_type_deposit: &'static str,
    pub st_type_withdraw: &'static str,
    pub st_close_all_yn: &'static str,
    pub st_no_positions: &'static str,
    pub st_no_positions_matched: &'static str,
    pub st_no_orders: &'static str,
    pub st_rpc_unchanged: &'static str,
    pub st_rpc_cleared: &'static str,
    pub st_no_position_to_close: &'static str,
    pub st_cancelled_close_pos: &'static str,
    pub st_cancelled_close_all: &'static str,
    pub st_cancelled_cancel_all: &'static str,

    // ── Status bar messages — format fragments ───────────────────────────────
    /// "Switching to" — append "{}…"
    pub st_switching_to: &'static str,
    /// "Switched to" — append "{}"
    pub st_switched_to: &'static str,
    /// "Market switch failed: no config for" — append symbol, then
    /// st_market_switch_failed_suf
    pub st_market_switch_failed: &'static str,
    /// Suffix after symbol in market-switch-failed message ("" in EN, "的配置"
    /// in CN)
    pub st_market_switch_failed_suf: &'static str,
    /// "Wallet connected —" — append pubkey
    pub st_wallet_connected_as: &'static str,
    /// "Limit price set to $" — append value
    pub st_lim_set: &'static str,
    /// "Switched to Limit @ $" — append value
    pub st_switched_limit: &'static str,
    /// "— press [e] to change price"
    pub st_switched_limit_hint: &'static str,
    /// "Size manually set to" — append value
    pub st_size_set: &'static str,
    /// "Language set to" — append label
    pub st_language_set: &'static str,
    /// "CLOB orders" — append "On" or "Off"
    pub st_clob_set: &'static str,
    /// "Reconnecting to" — append URL
    pub st_reconnecting: &'static str,
    /// "Failed to save config:" — append error
    pub st_failed_save: &'static str,
    /// "Cannot close" — append "symbol: st_no_market_cfg"
    pub st_cannot_close: &'static str,
    /// "no market config found"
    pub st_no_market_cfg: &'static str,
    /// "Closing" verb — used in "Closing SIDE SIZE SYMBOL…" and "Closing N
    /// position(s)…"
    pub st_closing: &'static str,
    /// "position(s)" — pluralised noun for status messages
    pub st_position_s: &'static str,
    /// "order(s)" — pluralised noun for status messages
    pub st_order_s: &'static str,
    /// "Cancelling" verb
    pub st_cancelling: &'static str,
    /// "Submitting" — market order submit prefix
    pub st_submitting: &'static str,
    /// "Submitting LIMIT" — limit order submit prefix
    pub st_submitting_limit: &'static str,
    /// "Submitting STOP" — stop-market order submit prefix
    pub st_submitting_stop: &'static str,
    /// "Submitting Deposit" — deposit submit prefix
    pub st_submitting_deposit: &'static str,
    /// "Submitting Withdrawal" — withdrawal submit prefix
    pub st_submitting_withdraw: &'static str,
    /// "Confirm" — market order confirm prompt prefix
    pub st_confirm: &'static str,
    /// "Confirm LIMIT" — limit order confirm prompt prefix
    pub st_confirm_limit: &'static str,
    /// "Confirm STOP" — stop-market order confirm prompt prefix
    pub st_confirm_stop: &'static str,
    /// "Confirm Close" — close-position confirm prompt prefix
    pub st_confirm_close: &'static str,
    /// "Confirm Deposit" — deposit confirm prompt prefix
    pub st_confirm_deposit_st: &'static str,
    /// "Confirm Withdraw" — withdrawal confirm prompt prefix
    pub st_confirm_withdraw_st: &'static str,
    /// "Cancel" — cancel-order confirm prompt prefix
    pub st_cancel_order_yn: &'static str,
    /// "Cancel ALL" — cancel-all confirm prompt prefix
    pub st_cancel_all_yn: &'static str,
    /// "open order(s)?" — suffix in cancel-all confirm
    pub st_open_orders_yn: &'static str,
    /// "Close" — close-by-symbol confirm prompt prefix
    pub st_close_by_sym_yn: &'static str,
    /// "(Y/N)"
    pub st_yn: &'static str,
    /// "Cancelled:" — prefix for all cancel feedback messages
    pub st_cancelled: &'static str,
    /// "USDC deposit" — noun for cancelled deposit feedback
    pub st_usdc_deposit_noun: &'static str,
    /// "USDC withdrawal" — noun for cancelled withdrawal feedback
    pub st_usdc_withdraw_noun: &'static str,

    // ── Transaction status messages ──────────────────────────────────────────
    /// "(reduce-only)" suffix appended to order summary when reduce_only is set
    pub tx_reduce_only: &'static str,
    /// "Registering trader" — progress when trader account creation is needed
    pub tx_registering_trader: &'static str,
    /// "Failed to build order params" — order params error prefix
    pub tx_failed_build_params: &'static str,
    /// "Failed to build order instruction" — order IX build error prefix
    pub tx_failed_build_ix: &'static str,
    /// "Failed to build trader registration" — registration error prefix
    pub tx_failed_build_reg: &'static str,
    /// "Preparing transaction" — before compile/sign (append " {summary}…")
    pub tx_broadcasting: &'static str,
    /// "❌ Failed to prepare" — compile/sign failure prefix
    pub tx_failed_prepare: &'static str,
    /// "Awaiting confirmation" — after send, waiting on-chain
    pub tx_awaiting_confirm: &'static str,
    /// "✅ Order confirmed:" — success (append " {summary}")
    pub tx_order_confirmed: &'static str,
    /// "❌ Transaction rejected" — RPC rejection prefix
    pub tx_tx_rejected: &'static str,
    /// "❌ Order not confirmed" — timeout/unconfirmed prefix
    pub tx_order_not_confirmed: &'static str,
    /// Phoenix program error 7002: user-facing explanation.
    pub tx_err_stop_opposite_direction: &'static str,
    /// Custom program error 0x1 / lamports — not enough SOL for fees.
    pub tx_err_not_enough_sol: &'static str,
    /// `InsufficientFunds` in confirmation failure status.
    pub tx_err_balance_too_low: &'static str,
    /// Order size zero from RPC / simulation.
    pub tx_err_order_size_nonzero: &'static str,
    /// Market-wide post-only mode rejects crossing the spread (simulation / program logs).
    pub tx_err_post_only_no_cross: &'static str,
    /// Isolated market rejects a cross-margin trader account (simulation / program logs).
    pub tx_err_isolated_only_cross_margin: &'static str,
    /// On-chain `CapabilityDenied`.
    pub tx_err_capability_denied: &'static str,
    /// On-chain `TraderFrozen`.
    pub tx_err_trader_frozen: &'static str,
    /// Withdrawal rejected for insufficient margin.
    pub tx_err_withdraw_insufficient_margin: &'static str,
    /// Balance-related wording from RPC logs (e.g. Phoenix on-chain errors parsed in `tx::error`).
    pub tx_err_insufficient_balance: &'static str,
    /// Generic insufficient funds (margin simulation, etc.).
    pub tx_err_insufficient_funds: &'static str,
    /// Prefix before raw RPC error when unmapped (`"Tx Failed: "`).
    pub tx_err_failed_prefix: &'static str,
    /// "deposit" — flow noun in "{amount} USDC deposit" fund scope
    pub tx_flow_deposit: &'static str,
    /// "withdraw" — flow noun in "{amount} USDC withdraw" fund scope
    pub tx_flow_withdraw: &'static str,
    /// "❌ Failed to build deposit" — deposit IX build failure prefix
    pub tx_failed_build_deposit: &'static str,
    /// "❌ Failed to build withdrawal" — withdrawal IX build failure prefix
    pub tx_failed_build_withdrawal: &'static str,
    /// "✅ Deposit of" — deposit confirmed prefix (append " {amount}
    /// {tx_usdc_confirmed}")
    pub tx_deposit_confirmed: &'static str,
    /// "✅ Withdrawal of" — withdrawal confirmed prefix
    pub tx_withdrawal_confirmed: &'static str,
    /// "USDC confirmed!" — suffix after amount in transfer success
    pub tx_usdc_confirmed: &'static str,
    /// "❌ Transfer not confirmed" — transfer timeout/unconfirmed prefix
    pub tx_transfer_not_confirmed: &'static str,
    /// "Building close-all for" — close-all progress prefix
    pub tx_building_close_all: &'static str,
    /// "not found, skipping" — market lookup failure suffix in close-all
    pub tx_not_found_skip: &'static str,
    /// Full message when close-all has no valid instructions
    pub tx_close_all_aborted: &'static str,
    /// "close" — verb fragment in per-entry batch label ("close 1.0 SOL")
    pub tx_close_label: &'static str,
    /// "Batch" — noun in close-all batch status messages
    pub tx_batch: &'static str,
    /// "Preparing batch" — close batch progress prefix
    pub tx_broadcasting_batch: &'static str,
    /// "Confirming batch" — close batch confirm prefix
    pub tx_confirming_batch: &'static str,
    /// "❌ Failed to prepare batch" — close batch preparation failure prefix
    pub tx_failed_prepare_batch: &'static str,
    /// "confirmed:" — batch success verb suffix (shared by close and cancel)
    pub tx_batch_confirmed_suf: &'static str,
    /// "rejected by the network" — batch rejected suffix
    pub tx_batch_rejected_suf: &'static str,
    /// "not confirmed" — batch unconfirmed suffix
    pub tx_batch_not_confirmed_suf: &'static str,
    /// "✅ Close-all complete" — final close-all success
    pub tx_close_all_complete: &'static str,
    /// "Building cancel for" — cancel progress prefix
    pub tx_building_cancel: &'static str,
    /// Full message when cancel has no valid instructions
    pub tx_cancel_aborted: &'static str,
    /// "cancel" — verb fragment in per-chunk batch label ("cancel SOL×3")
    pub tx_cancel_label: &'static str,
    /// "Cancel batch" — noun in cancel batch status messages
    pub tx_cancel_batch: &'static str,
    /// "Preparing cancel batch" — cancel batch progress prefix
    pub tx_broadcasting_cancel_batch: &'static str,
    /// "Confirming cancel batch" — cancel batch confirm prefix
    pub tx_confirming_cancel_batch: &'static str,
    /// "❌ Failed to prepare cancel batch" — cancel batch preparation failure
    /// prefix
    pub tx_failed_prepare_cancel_batch: &'static str,
    /// "✅ Cancel complete" — final cancel success
    pub tx_cancel_complete: &'static str,

    // ── Splash screen ────────────────────────────────────────────────────────
    /// One-line risk disclaimer rendered under the splash credit. Must fit
    /// within the 45-column banner width.
    pub splash_risk_disclaimer: &'static str,

    // ── Connect flow / referral disclosure ───────────────────────────────────
    /// "🐦‍🔥 No Phoenix account — registering with COSMIC referral
    /// (10% fee discount)…" — toast title shown while the referral activation
    /// HTTP call is in flight.
    pub tx_registering_referral: &'static str,
    /// "🐦‍🔥 Registered with COSMIC referral — 10% fee discount applied" —
    /// toast title on successful referral activation.
    pub tx_registered_referral: &'static str,
    /// "❌ Phoenix registration failed" — toast title on referral activation
    /// failure.
    pub tx_registration_failed: &'static str,
    /// "🐦‍🔥 Referral skipped — register at phoenix.trade before trading" —
    /// toast title when the user picks "Skip" in the choice modal (or
    /// presses Esc on either referral modal). Trading will fail until they
    /// self-register, so the message points them to the website.
    pub tx_referral_skipped: &'static str,

    // ── Custom referral code modal ───────────────────────────────────────────
    /// Modal title — "Custom Referral Code".
    pub referral_modal_title: &'static str,
    /// Field label above the input — "Phoenix invite / referral code:".
    pub referral_modal_label: &'static str,
    /// Footer hint paired with Enter — "register".
    pub referral_modal_action: &'static str,
    /// Footer hint paired with Esc — "skip".
    pub referral_modal_skip: &'static str,
    /// Helper line below the input describing the empty-input behavior.
    pub referral_modal_help: &'static str,
    /// Status toast title shown while a custom-referral activation is in
    /// flight — prepended to the user-typed code.
    pub tx_registering_custom_prefix: &'static str,
    /// Status toast title shown after a custom-referral activation succeeds —
    /// prepended to the user-typed code.
    pub tx_registered_custom_prefix: &'static str,

    // ── Referral choice modal (first-run opt-in) ─────────────────────────────
    /// Modal title — text rendered before the orange "Phoenix" word.
    /// Empty for languages that put "Phoenix" first (e.g. EN, ZH); set for
    /// languages where the noun follows (e.g. ES "Registro ", RU
    /// "Регистрация в ").
    pub referral_choice_title_prefix: &'static str,
    /// Modal title — text rendered after the orange "Phoenix" word
    /// (e.g. EN " Registration", ZH " 注册"). Empty when "Phoenix" comes
    /// last in the language's word order.
    pub referral_choice_title_suffix: &'static str,
    /// Header line above the three options summarizing what the choice
    /// affects.
    pub referral_choice_intro: &'static str,
    /// Option 1: "Use COSMIC referral (10% fee discount)".
    pub referral_choice_cosmic: &'static str,
    /// Helper text under option 1 disclosing that Cinder earns a share of
    /// fees from referred wallets.
    pub referral_choice_cosmic_note: &'static str,
    /// Option 2: "Enter a custom referral / invite code".
    pub referral_choice_custom: &'static str,
    /// Option 3: "Skip — register manually at phoenix.trade".
    pub referral_choice_skip: &'static str,
    /// Footer hint paired with ↑↓ — "select".
    pub referral_choice_nav: &'static str,
    /// Footer hint paired with Enter — "choose".
    pub referral_choice_action: &'static str,
    /// Disclosure note rendered at the bottom: attribution is permanent on
    /// Phoenix's side and cannot be changed later.
    pub referral_choice_sticky_note: &'static str,
}

mod en;
mod es;
mod ru;
mod zh;

pub use en::EN;
pub use es::ES;
pub use ru::RU;
pub use zh::CN;

/// Returns the string table for the current user language.
/// Cheap: one `RwLock::read` + static pointer dereference.
pub fn strings() -> &'static Strings {
    match current_user_config().language {
        Language::Chinese => &CN,
        Language::English => &EN,
        Language::Russian => &RU,
        Language::Spanish => &ES,
    }
}
