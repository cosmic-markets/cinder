//! TradingState: in-flight order inputs, wallet connection, and status display.

use std::collections::VecDeque;
use std::sync::Arc;

use solana_keypair::Keypair;

use super::super::config::{current_user_config, UserConfig};
use super::super::trading::{InputMode, OrderKind, PositionInfo, TradingSide};
use super::super::tx::TxContext;
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
    /// Selected row in the config modal (0 = RPC URL, 1 = language).
    pub config_selected_field: usize,
    /// Recent user actions and their txids, newest-first.
    pub ledger: VecDeque<LedgerEntry>,
    /// Selected row in the ledger modal.
    pub ledger_selected: usize,
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
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            deposit_buffer: String::new(),
            withdraw_buffer: String::new(),
            wallet_path_buffer: String::new(),
            wallet_path_error: None,
            custom_size: None,
            order_kind: OrderKind::Market,
            usdc_balance: None,
            phoenix_balance: None,
            sol_balance: None,
            config: current_user_config(),
            config_selected_field: 0,
            ledger: VecDeque::with_capacity(LEDGER_CAPACITY),
            ledger_selected: 0,
        }
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
