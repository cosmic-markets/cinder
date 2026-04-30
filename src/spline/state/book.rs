//! Order-book types shared between the spline WSS feed and the Phoenix CLOB L2
//! stream.

/// Origin of a [`BookRow`]: Phoenix on-chain splines or CLOB L2 from the
/// Phoenix WS feed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowSource {
    Spline,
    Clob,
}

/// Unified row for the coalesced book display. `price_start == price_end` for
/// CLOB levels.
#[derive(Clone, Debug)]
pub struct BookRow {
    pub source: RowSource,
    pub trader: String,
    pub price_start: f64,
    pub price_end: f64,
    pub size: f64,
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
