//! TradingState: in-flight order inputs, wallet connection, and status display.

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

use solana_keypair::Keypair;

use super::super::config::{current_user_config, UserConfig};
use super::super::trading::{InputMode, OrderKind, PositionInfo, TradingSide};
use super::super::tx::{TraderMirror, TxContext};
use super::make_status_timestamp;
use super::markers::LedgerEntry;

/// Maximum number of past actions retained in the Ledger modal. Entries are
/// stored newest-first and older entries are evicted beyond this cap.
const LEDGER_CAPACITY: usize = 50;

pub struct TradingState {
    pub side: TradingSide,
    pub size_index: usize,
    pub wallet_loaded: bool,
    pub wallet_label: String,
    pub position: Option<PositionInfo>,
    /// Timestamp string for the status body row, e.g. `[12:34:56 UTC]`.
    pub status_timestamp: String,
    /// Shown in the status frame body (e.g. "USDC Withdrawal Confirmed!").
    pub status_title: String,
    /// Single body row: tx signature or error text; empty when there is nothing
    /// to show.
    pub status_detail: String,
    pub keypair: Option<Arc<Keypair>>,
    pub tx_context: Option<Arc<TxContext>>,
    /// Shared mirror of the wallet's on-chain `Trader` state. Owned by
    /// `TradingState` so connection-change paths (RPC swap, market switch)
    /// can rebuild a fresh `TxContext` against the same live trader without
    /// re-subscribing the WS stream from scratch. Set on wallet connect,
    /// cleared on disconnect.
    pub shared_trader: Option<Arc<RwLock<TraderMirror>>>,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub deposit_buffer: String,
    pub withdraw_buffer: String,
    /// Editable path string for the "Load Wallet" modal. Seeded from
    /// `default_wallet_path()` when the modal opens.
    pub wallet_path_buffer: String,
    /// Last load error from the "Load Wallet" modal, rendered below the
    /// input until the user types again.
    pub wallet_path_error: Option<String>,
    /// Editable code for the "Custom referral code" modal. Empty string
    /// means the user wants to skip and self-register.
    pub referral_code_buffer: String,
    /// Last activation error from the referral modal, rendered below the
    /// input until the user types again.
    pub referral_code_error: Option<String>,
    /// Selected row in the first-run referral choice modal:
    /// 0 = use COSMIC, 1 = enter custom code, 2 = skip.
    pub referral_choice_index: usize,
    pub custom_size: Option<f64>,
    /// Currently selected order kind. `[t]` cycles Market → Limit → StopMarket;
    /// `[e]` edits the attached price for Limit/StopMarket. The next `[Enter]`
    /// submits using this kind.
    pub order_kind: OrderKind,
    pub usdc_balance: Option<f64>,
    pub phoenix_balance: Option<f64>,
    pub sol_balance: Option<f64>,
    /// Persisted user settings (RPC URL, language). Modified in-memory while
    /// the config modal is open and flushed to disk on save.
    pub config: UserConfig,
    /// Selected row in the config modal (0 = RPC URL, 1 = language,
    /// 2 = CLOB orders, 3 = public-RPC fan-out).
    pub config_selected_field: usize,
    /// Whether the first-run referral choice modal has already been shown
    /// (and dismissed) for the currently-loaded wallet. Reset on connect
    /// so a fresh wallet load re-evaluates from scratch.
    pub referral_choice_shown: bool,
    /// Recent user actions and their txids, newest-first.
    pub ledger: VecDeque<LedgerEntry>,
    /// Selected row in the ledger modal.
    pub ledger_selected: usize,
    /// In-progress draft for the "New TWAP" modal. Reset to defaults each
    /// time the modal opens via [`reset_twap_draft`](Self::reset_twap_draft).
    pub twap_draft: TwapDraft,
}

/// Editable form state backing the "New TWAP" modal. Field layout mirrors
/// the Binance TWAP modal: market, side, total size, and total time
/// (hours + minutes). Side is selectable from any field via [Tab]. Slice
/// cadence is fixed at one market slice per minute.
#[derive(Debug, Clone)]
pub struct TwapDraft {
    /// Cursor row: 0 = market, 1 = side, 2 = total size, 3 = total time
    /// hours, 4 = total time minutes.
    pub selected_field: usize,
    /// Symbol of the market the TWAP will run against. Seeded from the
    /// active market when the modal opens; cycled with ←/→ when the market
    /// field is focused.
    pub market: String,
    pub side: super::super::trading::TradingSide,
    pub size_buffer: String,
    pub duration_hour_buffer: String,
    pub duration_min_buffer: String,
    /// Set when the user presses [Enter] but a field fails validation —
    /// rendered below the form until the next keystroke.
    pub error: Option<String>,
    /// True after the user passed validation on the final row — the modal
    /// renders a "Start TWAP? [Y/N]" prompt instead of immediately spawning
    /// the bot. Y/Enter submits, N/Esc cancels back to editing.
    /// Bypassed when `skip_order_confirmation` is enabled.
    pub pending_confirm: bool,
}

impl TwapDraft {
    /// Index of the Total Size row in the form. Cursor starts here on open
    /// — Market and Side both default to sensible values, so the user almost
    /// always wants to type a size first.
    pub const DEFAULT_FIELD: usize = 2;
    pub const FIELD_COUNT: usize = 5;

    pub fn new(market: String, side: super::super::trading::TradingSide) -> Self {
        Self {
            selected_field: Self::DEFAULT_FIELD,
            market,
            side,
            size_buffer: String::new(),
            duration_hour_buffer: String::new(),
            duration_min_buffer: String::new(),
            error: None,
            pending_confirm: false,
        }
    }

    pub fn move_field_up(&mut self) {
        if self.selected_field > 0 {
            self.selected_field -= 1;
        }
    }

    pub fn move_field_down(&mut self) {
        if self.selected_field + 1 < Self::FIELD_COUNT {
            self.selected_field += 1;
        }
    }

    /// Step to the previous or next market in `markets` (wrapping). No-op if
    /// the current market isn't in the list (shouldn't happen — the draft is
    /// always seeded from the active market) or the list is empty.
    pub fn cycle_market(&mut self, markets: &[String], step: i32) {
        if markets.is_empty() {
            return;
        }
        let idx = markets.iter().position(|s| *s == self.market).unwrap_or(0);
        let len = markets.len() as i32;
        let new_idx = ((idx as i32 + step).rem_euclid(len)) as usize;
        self.market = markets[new_idx].clone();
        self.error = None;
    }
}

impl TradingState {
    pub fn new() -> Self {
        use super::super::constants::DEFAULT_SIZE_INDEX;
        Self {
            side: TradingSide::Long,
            size_index: DEFAULT_SIZE_INDEX,
            wallet_loaded: false,
            wallet_label: String::new(),
            position: None,
            status_timestamp: make_status_timestamp(),
            status_title: "Ready — Wallet not connected".to_string(),
            status_detail: String::new(),
            keypair: None,
            tx_context: None,
            shared_trader: None,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            deposit_buffer: String::new(),
            withdraw_buffer: String::new(),
            wallet_path_buffer: String::new(),
            wallet_path_error: None,
            referral_code_buffer: String::new(),
            referral_code_error: None,
            // Default to "Use COSMIC" (index 0). The prompt handler resets
            // this each time the modal opens, but we keep the initial value
            // aligned so a fast reader of the state struct sees the same
            // default both places.
            referral_choice_index: 0,
            custom_size: None,
            order_kind: OrderKind::Market,
            usdc_balance: None,
            phoenix_balance: None,
            sol_balance: None,
            config: current_user_config(),
            config_selected_field: 0,
            referral_choice_shown: false,
            ledger: VecDeque::with_capacity(LEDGER_CAPACITY),
            ledger_selected: 0,
            twap_draft: TwapDraft::new(String::new(), TradingSide::Long),
        }
    }

    /// Reset the TWAP draft form to default values, seeding the market and
    /// side from the active context. Called when the "New TWAP" modal opens
    /// so each entry starts from a known-good baseline.
    pub fn reset_twap_draft(&mut self, market: String) {
        self.twap_draft = TwapDraft::new(market, self.side);
    }

    /// Record a user action in the ledger. If the same txid already appears as
    /// the most recent entry (e.g. an "awaiting confirm" status followed by a
    /// "confirmed" status for the same signature), update that entry in place
    /// instead of pushing a duplicate.
    pub fn record_ledger(&mut self, title: impl Into<String>, txid: impl Into<String>) {
        let title = title.into();
        let txid = txid.into();
        if let Some(front) = self.ledger.front_mut() {
            if front.txid == txid {
                front.timestamp = make_status_timestamp();
                front.title = title;
                return;
            }
        }
        self.ledger.push_front(LedgerEntry {
            timestamp: make_status_timestamp(),
            title,
            txid,
        });
        while self.ledger.len() > LEDGER_CAPACITY {
            self.ledger.pop_back();
        }
    }

    pub fn set_status_title(&mut self, title: impl Into<String>) {
        self.status_timestamp = make_status_timestamp();
        self.status_title = title.into();
        self.status_detail.clear();
    }

    pub fn order_size(&self) -> f64 {
        use super::super::constants::ORDER_SIZE_PRESETS;
        if let Some(custom) = self.custom_size {
            custom
        } else {
            ORDER_SIZE_PRESETS[self.size_index]
        }
    }

    pub fn increase_size(&mut self) {
        use super::super::constants::ORDER_SIZE_PRESETS;
        if self.size_index + 1 < ORDER_SIZE_PRESETS.len() {
            self.size_index += 1;
        }
    }

    pub fn decrease_size(&mut self) {
        if self.size_index > 0 {
            self.size_index -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::constants::{DEFAULT_SIZE_INDEX, ORDER_SIZE_PRESETS};
    use super::*;

    #[test]
    fn new_starts_at_default_size_preset() {
        let s = TradingState::new();
        assert_eq!(s.size_index, DEFAULT_SIZE_INDEX);
        assert_eq!(s.order_size(), ORDER_SIZE_PRESETS[DEFAULT_SIZE_INDEX]);
        assert!(matches!(s.order_kind, OrderKind::Market));
        assert_eq!(s.side, TradingSide::Long);
    }

    #[test]
    fn custom_size_overrides_preset() {
        let mut s = TradingState::new();
        s.custom_size = Some(1.5);
        assert_eq!(s.order_size(), 1.5);
    }

    #[test]
    fn increase_size_stops_at_last_preset() {
        let mut s = TradingState::new();
        s.size_index = ORDER_SIZE_PRESETS.len() - 1;
        s.increase_size();
        assert_eq!(s.size_index, ORDER_SIZE_PRESETS.len() - 1);
    }

    #[test]
    fn decrease_size_stops_at_zero() {
        let mut s = TradingState::new();
        s.size_index = 0;
        s.decrease_size();
        assert_eq!(s.size_index, 0);
    }

    #[test]
    fn set_status_title_clears_detail() {
        let mut s = TradingState::new();
        s.status_detail = "old".to_string();
        s.set_status_title("hello");
        assert_eq!(s.status_title, "hello");
        assert!(s.status_detail.is_empty());
    }

    #[test]
    fn record_ledger_pushes_new_entries_newest_first() {
        let mut s = TradingState::new();
        s.record_ledger("first", "sig1");
        s.record_ledger("second", "sig2");
        assert_eq!(s.ledger.len(), 2);
        assert_eq!(s.ledger.front().unwrap().txid, "sig2");
        assert_eq!(s.ledger.back().unwrap().txid, "sig1");
    }

    #[test]
    fn twap_draft_field_nav_clamps_at_bounds() {
        let mut d = TwapDraft::new("SOL".into(), TradingSide::Long);
        assert_eq!(d.selected_field, TwapDraft::DEFAULT_FIELD);
        for _ in 0..10 {
            d.move_field_up();
        }
        assert_eq!(d.selected_field, 0);
        for _ in 0..10 {
            d.move_field_down();
        }
        assert_eq!(d.selected_field, TwapDraft::FIELD_COUNT - 1);
    }

    #[test]
    fn reset_twap_draft_seeds_market_and_side() {
        let mut s = TradingState::new();
        s.side = TradingSide::Short;
        s.twap_draft.size_buffer = "leftover".to_string();
        s.reset_twap_draft("BTC".to_string());
        assert_eq!(s.twap_draft.market, "BTC");
        assert_eq!(s.twap_draft.side, TradingSide::Short);
        assert!(s.twap_draft.size_buffer.is_empty());
        assert_eq!(s.twap_draft.selected_field, TwapDraft::DEFAULT_FIELD);
    }

    #[test]
    fn cycle_market_wraps_at_ends() {
        let mut d = TwapDraft::new("SOL".into(), TradingSide::Long);
        let markets = vec!["SOL".to_string(), "BTC".to_string(), "ETH".to_string()];
        d.cycle_market(&markets, 1);
        assert_eq!(d.market, "BTC");
        d.cycle_market(&markets, 1);
        assert_eq!(d.market, "ETH");
        d.cycle_market(&markets, 1);
        assert_eq!(d.market, "SOL");
        d.cycle_market(&markets, -1);
        assert_eq!(d.market, "ETH");
    }

    #[test]
    fn record_ledger_updates_in_place_for_same_txid() {
        let mut s = TradingState::new();
        s.record_ledger("awaiting", "sig");
        s.record_ledger("confirmed", "sig");
        assert_eq!(s.ledger.len(), 1);
        assert_eq!(s.ledger.front().unwrap().title, "confirmed");
    }

    #[test]
    fn record_ledger_evicts_beyond_capacity() {
        let mut s = TradingState::new();
        for i in 0..(LEDGER_CAPACITY + 5) {
            s.record_ledger(format!("title-{i}"), format!("sig-{i}"));
        }
        assert_eq!(s.ledger.len(), LEDGER_CAPACITY);
        // Newest still at front.
        assert_eq!(
            s.ledger.front().unwrap().txid,
            format!("sig-{}", LEDGER_CAPACITY + 4)
        );
    }
}
