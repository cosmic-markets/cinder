//! Chart markers and transaction status messages.

#[derive(Clone)]
pub struct TradeMarker {
    pub x: f64,
    pub y: f64,
    pub is_buy: bool,
}

/// Per-open-order chart marker. `x` is captured when we first see the order in
/// a WS snapshot and then advances leftward on each `push_price`, so the square
/// tracks the rolling chart. The map is keyed by `(symbol,
/// order_sequence_number)` so fills/cancels prune cleanly.
#[derive(Clone)]
pub struct OrderChartMarker {
    pub x: f64,
    pub price: f64,
}

#[derive(Clone, Debug)]
pub enum TxStatusMsg {
    TradeMarker {
        is_buy: bool,
    },
    SetStatus {
        title: String,
        detail: String,
    },
    /// Record a confirmed tx in the user's Ledger ring without touching
    /// the visible status line. Used by TWAP slices so each slice's
    /// signature still appears in the [L]edger modal for audit, but the
    /// per-slice broadcast/awaiting chatter doesn't clobber the global
    /// status frame that manual orders own.
    LedgerOnly {
        title: String,
        signature: String,
    },
    /// Open the first-run referral choice modal. Sent by the connect flow
    /// when a wallet with no Phoenix account connects. The modal lets the
    /// user pick between COSMIC, a custom code, or continuing without one;
    /// the custom-code text input is reached by direct state transition
    /// from the choice handler.
    PromptReferralChoice,
}

impl TxStatusMsg {
    /// True if this message represents per-slice status chatter that the
    /// TWAP scheduler suppresses with `silent_status=true` (each slice's
    /// "broadcasting..." / "awaiting confirm..." / "order confirmed: <sig>"
    /// would otherwise clobber the user's manual-order status line). Exhaustive
    /// match: every new variant MUST classify itself here so the filter
    /// doesn't silently leak new chatter through.
    pub fn is_per_slice_chatter(&self) -> bool {
        match self {
            TxStatusMsg::SetStatus { .. } => true,
            // Chart pin: every fill belongs on the chart regardless of source.
            TxStatusMsg::TradeMarker { .. } => false,
            // Ledger entry: TWAP slices want these recorded.
            TxStatusMsg::LedgerOnly { .. } => false,
            // Modal prompts must always reach the user.
            TxStatusMsg::PromptReferralChoice => false,
        }
    }
}

/// One row in the ledger modal: a user-initiated action that produced a
/// confirmed Solana signature (orders, cancels, close position, deposits,
/// withdrawals, stop-market orders).
#[derive(Clone, Debug)]
pub struct LedgerEntry {
    pub timestamp: String,
    pub title: String,
    pub txid: String,
}
