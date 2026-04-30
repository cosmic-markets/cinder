//! Static UI string tables for English and Chinese (Simplified).
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
    pub price_range: &'static str,
    pub depth: &'static str,
    pub bid_abbrev: &'static str,
    pub ask_abbrev: &'static str,
    pub no_spread: &'static str,

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
    /// Post-only order would cross the spread.
    pub tx_err_post_only_no_cross: &'static str,
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
}

pub static EN: Strings = Strings {
    status: "Status",
    orders: "orders",
    positions: "positions",
    markets: "markets",
    quit: "quit",
    txid: "ledger",

    balance: "Balance",
    wallet: "Wallet",
    perps: "Perps",
    upnl: "uPnL",
    deposit: "Deposit",
    withdraw: "Withdraw",
    amt: "Amt",

    trade: "Trade",
    not_loaded: "Not loaded",
    notional: "Notional",
    lev: "Lev",
    buy: "BUY",
    sell: "SELL",
    mkt: "MKT",
    lmt: "LMT",
    stp: "STP",
    size: "Size",
    px: "Px",
    market_order: "market order",
    limit_order: "limit order",
    stop_order: "stop-market order",
    close: "close",
    set: "set",
    cancel: "cancel",
    confirm: "CONFIRM",
    close_position: "CLOSE POSITION",
    cancel_all_orders: "CANCEL ALL ORDERS",
    no_position: "No Position",
    liq: "Liq",

    long_label: "LONG",
    short_label: "SHORT",

    vol: "Vol",
    oi: "OI",
    mark: "Mark",
    index_price: "Index",
    funding: "Funding",
    waiting_data: "Waiting for market data\u{2026}",
    asks: "Asks",
    bids: "Bids",
    slot: "Slot",
    trader: "Trader",
    price_range: "Price Range",
    depth: "Depth",
    bid_abbrev: "B",
    ask_abbrev: "A",
    no_spread: "no spread",

    select: "select",
    back: "back",
    view_market: "view market",
    close_pos: "close pos",
    close_all: "close all",
    cxl_order: "cxl order",
    cxl_all: "cxl all",

    market: "Market",
    side: "Side",
    entry: "Entry",
    notional_col: "Notional",
    pnl: "PnL",
    liq_col: "Liq",
    lev_col: "Lev",
    no_open_positions: "No open positions",

    order_type: "Type",
    price: "Price",
    filled: "Filled",
    flags: "Flags",
    no_open_orders: "No open orders",

    pct_change: "% Change",
    leverage: "Leverage",
    vol_24h: "24h Volume",

    config: "Config",
    rpc_url: "RPC URL",
    language: "Language",
    clob_orders: "Show CLOB orders",
    clob_orders_note: "uses extra bandwidth",
    on: "On",
    off: "Off",
    save_reconnect: "save & reconnect",
    edit: "edit",
    toggle: "toggle",
    rpc_default: "<using env / default>",

    quit_confirm: "Quit?",

    ledger_title: "ledger",
    ledger_col_time: "Time",
    ledger_col_action: "Action",
    ledger_empty: "No actions yet — your submitted trades will appear here",
    ledger_copied: "Copied to clipboard:",
    ledger_copy_failed: "Could not copy to clipboard",

    top_positions_title: "top positions",
    top_positions_loading: "Loading top positions from on-chain\u{2026}",
    top_positions_empty: "No active positions on Phoenix",
    top_positions_rank: "#",
    top_positions_trader: "Trader",
    top_positions_no_trader: "Trader pubkey not yet resolved \u{2014} try again in a moment",
    top_positions_copy_hint: "copy trader",

    st_loading_ctx: "Loading Phoenix trading context — fetching exchange metadata\u{2026}",
    st_wallet_disconnected: "Wallet disconnected — press [w] to reconnect",
    st_wallet_not_loaded: "Wallet not loaded — press [w] to connect",
    st_wallet_connected: "Wallet connected",
    st_load_wallet_title: "Load Wallet",
    st_load_wallet_action: "load",
    st_wallet_path_label: "Path, base58, or [byte,array]:",
    st_wallet_load_failed: "Load failed:",
    st_ctx_loading: "Trading context still loading — wait a moment and try again",
    st_switching_market: "Switching market — reconnecting to feed\u{2026}",
    st_lim_cleared: "Limit price cleared — orders will execute at market",
    st_lim_must_positive: "Limit price must be a positive number",
    st_invalid_price: "Invalid price — enter a number like 185.50",
    st_market_mode: "Switched to Market order mode",
    st_no_mark_price: "No mark price available — press [e] to set limit price",
    st_enter_price: "Enter limit price and press Enter (Esc to cancel)",
    st_enter_stop_price: "Enter stop trigger price and press Enter (Esc to cancel)",
    st_stop_cleared: "Stop trigger cleared — switching back to Market",
    st_stop_must_positive: "Stop trigger price must be a positive number",
    st_stop_set: "Stop trigger price set to $",
    st_switched_stop: "Switched to Stop-Market @ $",
    st_enter_size: "Enter order size and press Enter (Esc to cancel)",
    st_invalid_size: "Invalid size — enter a number like 0.1",
    st_invalid_amount: "Invalid amount — enter a number like 100.00",
    st_type_deposit: "Type USDC amount to Deposit and press Enter\u{2026}",
    st_type_withdraw: "Type USDC amount to Withdraw and press Enter\u{2026}",
    st_close_all_yn: "Close ALL positions? (Y/N)",
    st_no_positions: "No positions to close",
    st_no_positions_matched: "No positions matched a known market config — nothing to close",
    st_no_orders: "No orders to cancel",
    st_rpc_unchanged: "RPC URL unchanged",
    st_rpc_cleared: "RPC URL cleared — falling back to env/default, reconnecting\u{2026}",
    st_no_position_to_close: "No position to close",
    st_cancelled_close_pos: "Cancelled: close position",
    st_cancelled_close_all: "Cancelled: close all positions",
    st_cancelled_cancel_all: "Cancelled: cancel all orders",

    st_switching_to: "Switching to",
    st_switched_to: "Switched to",
    st_market_switch_failed: "Market switch failed: no config for",
    st_market_switch_failed_suf: "",
    st_wallet_connected_as: "Wallet connected \u{2014}",
    st_lim_set: "Limit price set to $",
    st_switched_limit: "Switched to Limit @ $",
    st_switched_limit_hint: "\u{2014} press [e] to change price",
    st_size_set: "Size manually set to",
    st_language_set: "Language set to",
    st_clob_set: "CLOB orders",
    st_reconnecting: "Reconnecting to",
    st_failed_save: "Failed to save config:",
    st_cannot_close: "Cannot close",
    st_no_market_cfg: "no market config found",
    st_closing: "Closing",
    st_position_s: "position(s)",
    st_order_s: "order(s)",
    st_cancelling: "Cancelling",
    st_submitting: "Submitting",
    st_submitting_limit: "Submitting LIMIT",
    st_submitting_stop: "Submitting STOP",
    st_submitting_deposit: "Submitting Deposit",
    st_submitting_withdraw: "Submitting Withdrawal",
    st_confirm: "Confirm",
    st_confirm_limit: "Confirm LIMIT",
    st_confirm_stop: "Confirm STOP",
    st_confirm_close: "Confirm Close",
    st_confirm_deposit_st: "Confirm Deposit",
    st_confirm_withdraw_st: "Confirm Withdraw",
    st_cancel_order_yn: "Cancel",
    st_cancel_all_yn: "Cancel ALL",
    st_open_orders_yn: "open order(s)?",
    st_close_by_sym_yn: "Close",
    st_yn: "(Y/N)",
    st_cancelled: "Cancelled:",
    st_usdc_deposit_noun: "USDC deposit",
    st_usdc_withdraw_noun: "USDC withdrawal",

    tx_reduce_only: "(reduce-only)",
    tx_registering_trader: "Registering trader",
    tx_failed_build_params: "Failed to build order params",
    tx_failed_build_ix: "Failed to build order instruction",
    tx_failed_build_reg: "Failed to build trader registration",
    tx_broadcasting: "Preparing transaction",
    tx_failed_prepare: "❌ Failed to prepare",
    tx_awaiting_confirm: "Broadcast sent; awaiting confirmation",
    tx_order_confirmed: "✅ Order confirmed:",
    tx_tx_rejected: "❌ Transaction rejected",
    tx_order_not_confirmed: "Confirmation still pending",
    tx_err_stop_opposite_direction: "Stop loss order must be in opposite direction of trader \
                                     position",
    tx_err_not_enough_sol: "Not enough SOL",
    tx_err_balance_too_low: "Balance too low",
    tx_err_order_size_nonzero: "Order size must be greater than zero. Set a non-zero size before \
                               submitting.",
    tx_err_post_only_no_cross: "Market is PostOnly: can't cross the spread.",
    tx_err_capability_denied: "Trader account lacks permissions. Register via app.phoenix.trade \
                               with an invite code first.",
    tx_err_trader_frozen: "Trader account is frozen. Register at app.phoenix.trade with an invite \
                            code to activate.",
    tx_err_withdraw_insufficient_margin: "Withdrawal request rejected: InsufficientMargin",
    tx_err_insufficient_balance: "Insufficient balance.",
    tx_err_insufficient_funds: "Insufficient funds.",
    tx_err_failed_prefix: "Tx Failed: ",
    tx_flow_deposit: "deposit",
    tx_flow_withdraw: "withdraw",
    tx_failed_build_deposit: "❌ Failed to build deposit",
    tx_failed_build_withdrawal: "❌ Failed to build withdrawal",
    tx_deposit_confirmed: "✅ Deposit of",
    tx_withdrawal_confirmed: "✅ Withdrawal of",
    tx_usdc_confirmed: "USDC confirmed!",
    tx_transfer_not_confirmed: "Transfer confirmation still pending",
    tx_building_close_all: "Building close-all for",
    tx_not_found_skip: "not found, skipping",
    tx_close_all_aborted: "Close-all aborted: no valid close instructions could be built",
    tx_close_label: "close",
    tx_batch: "Batch",
    tx_broadcasting_batch: "Preparing batch",
    tx_confirming_batch: "Confirming batch",
    tx_failed_prepare_batch: "❌ Failed to prepare batch",
    tx_batch_confirmed_suf: "confirmed:",
    tx_batch_rejected_suf: "rejected by the network",
    tx_batch_not_confirmed_suf: "confirmation still pending",
    tx_close_all_complete: "✅ Close-all complete",
    tx_building_cancel: "Building cancel for",
    tx_cancel_aborted: "Cancel aborted: no valid cancel instructions could be built",
    tx_cancel_label: "cancel",
    tx_cancel_batch: "Cancel batch",
    tx_broadcasting_cancel_batch: "Preparing cancel batch",
    tx_confirming_cancel_batch: "Confirming cancel batch",
    tx_failed_prepare_cancel_batch: "❌ Failed to prepare cancel batch",
    tx_cancel_complete: "✅ Cancel complete",
};

pub static CN: Strings = Strings {
    status: "状态",
    orders: "订单",
    positions: "持仓",
    markets: "市场",
    quit: "退出",
    txid: "账本",

    balance: "余额",
    wallet: "钱包",
    perps: "合约",
    upnl: "浮动盈亏",
    deposit: "存入",
    withdraw: "取出",
    amt: "金额",

    trade: "交易",
    not_loaded: "未加载",
    notional: "名义价值",
    lev: "杠杆",
    buy: "买入",
    sell: "卖出",
    mkt: "市价",
    lmt: "限价",
    stp: "止损",
    size: "数量",
    px: "价格",
    market_order: "市价单",
    limit_order: "限价单",
    stop_order: "止损市价单",
    close: "平仓",
    set: "确定",
    cancel: "取消",
    confirm: "确认",
    close_position: "平仓",
    cancel_all_orders: "取消所有订单",
    no_position: "无持仓",
    liq: "清算价",

    long_label: "做多",
    short_label: "做空",

    vol: "量",
    oi: "持仓量",
    mark: "标价",
    index_price: "指数",
    funding: "资金费",
    waiting_data: "等待数据\u{2026}",
    asks: "卖单",
    bids: "买单",
    slot: "槽位",
    trader: "交易方",
    price_range: "价格区间",
    depth: "深度",
    bid_abbrev: "买",
    ask_abbrev: "卖",
    no_spread: "无价差",

    select: "选择",
    back: "返回",
    view_market: "查看市场",
    close_pos: "平仓",
    close_all: "全平",
    cxl_order: "取消订单",
    cxl_all: "全取消",

    market: "市场",
    side: "方向",
    entry: "开仓价",
    notional_col: "名义",
    pnl: "盈亏",
    liq_col: "清算价",
    lev_col: "杠杆",
    no_open_positions: "无持仓",

    order_type: "类型",
    price: "价格",
    filled: "成交",
    flags: "标志",
    no_open_orders: "无订单",

    pct_change: "涨跌幅",
    leverage: "杠杆",
    vol_24h: "24h量",

    config: "设置",
    rpc_url: "RPC 地址",
    language: "语言",
    clob_orders: "显示 CLOB 挂单",
    clob_orders_note: "占用额外带宽",
    on: "开",
    off: "关",
    save_reconnect: "保存重连",
    edit: "编辑",
    toggle: "切换",
    rpc_default: "<使用环境/默认>",

    quit_confirm: "退出?",

    ledger_title: "账本",
    ledger_col_time: "时间",
    ledger_col_action: "操作",
    ledger_empty: "暂无操作 — 提交交易后将显示在此",
    ledger_copied: "已复制到剪贴板:",
    ledger_copy_failed: "无法复制到剪贴板",

    top_positions_title: "顶级持仓",
    top_positions_loading: "正在从链上加载顶级持仓\u{2026}",
    top_positions_empty: "Phoenix 当前无活跃持仓",
    top_positions_rank: "#",
    top_positions_trader: "交易方",
    top_positions_no_trader: "交易方公钥尚未解析 \u{2014} 请稍后再试",
    top_positions_copy_hint: "复制交易方",

    st_loading_ctx: "正在加载 Phoenix 交易环境 — 获取交易所数据\u{2026}",
    st_wallet_disconnected: "钱包已断开 — 按 [w] 重连",
    st_wallet_not_loaded: "钱包未加载 — 按 [w] 连接",
    st_wallet_connected: "钱包已连接",
    st_load_wallet_title: "加载钱包",
    st_load_wallet_action: "加载",
    st_wallet_path_label: "路径、base58 或 [字节,数组]：",
    st_wallet_load_failed: "加载失败：",
    st_ctx_loading: "交易环境仍在加载 — 请稍候再试",
    st_switching_market: "正在切换市场 — 重连中\u{2026}",
    st_lim_cleared: "限价已清除 — 将以市价执行",
    st_lim_must_positive: "限价必须为正数",
    st_invalid_price: "价格无效 — 请输入如 185.50 的数字",
    st_market_mode: "已切换至市价模式",
    st_no_mark_price: "无标记价格 — 按 [e] 设置限价",
    st_enter_price: "请输入限价后按 Enter（Esc 取消）",
    st_enter_stop_price: "请输入止损触发价后按 Enter（Esc 取消）",
    st_stop_cleared: "止损触发价已清除 — 切回市价",
    st_stop_must_positive: "止损触发价必须为正数",
    st_stop_set: "止损触发价已设置为 $",
    st_switched_stop: "已切换至止损市价 @ $",
    st_enter_size: "请输入下单数量后按 Enter（Esc 取消）",
    st_invalid_size: "数量无效 — 请输入如 0.1 的数字",
    st_invalid_amount: "金额无效 — 请输入如 100.00 的数字",
    st_type_deposit: "请输入 USDC 存入金额后按 Enter\u{2026}",
    st_type_withdraw: "请输入 USDC 取出金额后按 Enter\u{2026}",
    st_close_all_yn: "全平所有持仓? (Y/N)",
    st_no_positions: "无持仓可平",
    st_no_positions_matched: "无持仓匹配已知市场配置 — 无法平仓",
    st_no_orders: "无订单可取消",
    st_rpc_unchanged: "RPC 地址未变更",
    st_rpc_cleared: "RPC 地址已清除 — 回退至环境/默认值，重连中\u{2026}",
    st_no_position_to_close: "无持仓可平",
    st_cancelled_close_pos: "已取消：平仓",
    st_cancelled_close_all: "已取消：全平",
    st_cancelled_cancel_all: "已取消：取消所有订单",

    st_switching_to: "正在切换至",
    st_switched_to: "已切换至",
    st_market_switch_failed: "市场切换失败：未找到",
    st_market_switch_failed_suf: "的配置",
    st_wallet_connected_as: "钱包已连接 \u{2014}",
    st_lim_set: "限价已设置为 $",
    st_switched_limit: "已切换至限价 @ $",
    st_switched_limit_hint: "\u{2014} 按 [e] 修改价格",
    st_size_set: "下单数量已设置为",
    st_language_set: "语言已设置为",
    st_clob_set: "CLOB 挂单",
    st_reconnecting: "正在重连至",
    st_failed_save: "配置保存失败：",
    st_cannot_close: "无法平仓",
    st_no_market_cfg: "未找到市场配置",
    st_closing: "正在平仓",
    st_position_s: "个持仓",
    st_order_s: "个订单",
    st_cancelling: "正在取消",
    st_submitting: "正在提交",
    st_submitting_limit: "正在提交限价单",
    st_submitting_stop: "正在提交止损单",
    st_submitting_deposit: "正在提交存入",
    st_submitting_withdraw: "正在提交取出",
    st_confirm: "确认",
    st_confirm_limit: "确认限价",
    st_confirm_stop: "确认止损",
    st_confirm_close: "确认平仓",
    st_confirm_deposit_st: "确认存入",
    st_confirm_withdraw_st: "确认取出",
    st_cancel_order_yn: "取消订单",
    st_cancel_all_yn: "取消全部",
    st_open_orders_yn: "个未成交订单?",
    st_close_by_sym_yn: "平仓",
    st_yn: "(Y/N)",
    st_cancelled: "已取消：",
    st_usdc_deposit_noun: "USDC 存入",
    st_usdc_withdraw_noun: "USDC 取出",

    tx_reduce_only: "（仅减仓）",
    tx_registering_trader: "正在注册交易方",
    tx_failed_build_params: "订单参数构建失败",
    tx_failed_build_ix: "订单指令构建失败",
    tx_failed_build_reg: "交易方注册构建失败",
    tx_broadcasting: "正在准备交易",
    tx_failed_prepare: "❌ 准备失败",
    tx_awaiting_confirm: "已广播，等待确认中",
    tx_order_confirmed: "✅ 订单已确认：",
    tx_tx_rejected: "❌ 交易已被拒绝",
    tx_order_not_confirmed: "确认仍在等待中",
    tx_err_stop_opposite_direction: "止损单方向必须与持仓方向相反",
    tx_err_not_enough_sol: "SOL 不足",
    tx_err_balance_too_low: "余额过低",
    tx_err_order_size_nonzero: "订单数量必须大于零。提交前请先设置非零数量。",
    tx_err_post_only_no_cross: "市场为 PostOnly：无法穿过买卖价差。",
    tx_err_capability_denied: "交易账户缺少权限。请先在 app.phoenix.trade 使用邀请码注册。",
    tx_err_trader_frozen: "交易账户已冻结。请在 app.phoenix.trade 使用邀请码激活。",
    tx_err_withdraw_insufficient_margin: "取款被拒：保证金不足",
    tx_err_insufficient_balance: "余额不足。",
    tx_err_insufficient_funds: "资金不足。",
    tx_err_failed_prefix: "交易失败：",
    tx_flow_deposit: "存入",
    tx_flow_withdraw: "取出",
    tx_failed_build_deposit: "❌ 存入指令构建失败",
    tx_failed_build_withdrawal: "❌ 取出指令构建失败",
    tx_deposit_confirmed: "✅ 已存入",
    tx_withdrawal_confirmed: "✅ 已取出",
    tx_usdc_confirmed: "USDC 已确认！",
    tx_transfer_not_confirmed: "转账确认仍在等待中",
    tx_building_close_all: "正在构建全平，共",
    tx_not_found_skip: "未找到，跳过",
    tx_close_all_aborted: "全平已中止：无法构建有效平仓指令",
    tx_close_label: "平仓",
    tx_batch: "批次",
    tx_broadcasting_batch: "正在准备批次",
    tx_confirming_batch: "确认批次",
    tx_failed_prepare_batch: "❌ 批次准备失败",
    tx_batch_confirmed_suf: "已确认：",
    tx_batch_rejected_suf: "已被网络拒绝",
    tx_batch_not_confirmed_suf: "确认仍在等待中",
    tx_close_all_complete: "✅ 全平完成",
    tx_building_cancel: "正在构建取消，共",
    tx_cancel_aborted: "取消已中止：无法构建有效取消指令",
    tx_cancel_label: "取消",
    tx_cancel_batch: "取消批次",
    tx_broadcasting_cancel_batch: "正在准备取消批次",
    tx_confirming_cancel_batch: "确认取消批次",
    tx_failed_prepare_cancel_batch: "❌ 取消批次准备失败",
    tx_cancel_complete: "✅ 取消完成",
};

/// Returns the string table for the current user language.
/// Cheap: one `RwLock::read` + static pointer dereference.
pub fn strings() -> &'static Strings {
    match current_user_config().language {
        Language::Chinese => &CN,
        Language::English => &EN,
    }
}
