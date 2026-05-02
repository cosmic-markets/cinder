//! Order-book types shared between the spline WSS feed and the Phoenix CLOB L2
//! stream.

/// Origin of a [`BookRow`] entry: Phoenix on-chain splines or CLOB L2 from the
/// Phoenix WS feed. Tracked per-trader inside a row so a price level shared
/// between sources can still mark the user's CLOB-resting orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowSource {
    Spline,
    Clob,
}

/// Unified row for the coalesced book display. Splines are abstracted into
/// point quotes at their most aggressive price (`price_start` of the spline
/// region) so the table reads as a normal CLOB. Multiple traders quoting the
/// same price level are merged into a single row, with their pubkey prefixes
/// retained in `traders` for display and user-order detection.
#[derive(Clone, Debug)]
pub struct BookRow {
    pub price: f64,
    pub size: f64,
    pub traders: Vec<(String, RowSource)>,
}

/// Sorted, coalesced (splines + CLOB) view of the active market's book.
#[derive(Clone, Debug, Default)]
pub struct MergedBook {
    pub bid_rows: Vec<BookRow>,
    pub ask_rows: Vec<BookRow>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
}

/// One CLOB level: `(price, size, trader)` where `trader` is a short pubkey
/// prefix for the resting order's owner, or a placeholder if the
/// `GlobalTraderIndex` hasn't resolved that pointer yet. Multiple entries can
/// share a price when different traders have orders at the same tick.
pub type ClobLevel = (f64, f64, String);

/// Full L2 snapshot emitted by the Phoenix L2 task; `symbol` must match the
/// poller's active market before applying.
#[derive(Clone, Debug)]
pub struct L2BookStreamMsg {
    pub symbol: String,
    pub bids: Vec<ClobLevel>,
    pub asks: Vec<ClobLevel>,
}
