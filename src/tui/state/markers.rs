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
    /// Open the first-run referral choice modal. Sent by the connect flow
    /// when a wallet with no Phoenix account connects. The modal lets the
    /// user pick between COSMIC, a custom code, or continuing without one;
    /// the custom-code text input is reached by direct state transition
    /// from the choice handler.
    PromptReferralChoice,
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
