//! One row of the "top positions on Phoenix" modal.

use super::TradingSide;

/// Derived from the on-chain `ActiveTraderBuffer` — one entry per active
/// (trader, market) position, ranked by notional-at-mark descending.
#[derive(Debug, Clone)]
pub struct TopPositionEntry {
    /// Market symbol (resolved from the position's asset id).
    pub symbol: String,
    /// Owning trader's wallet authority, when resolvable via the GTI cache.
    /// Falls back to `None` when the pointer hasn't been resolved yet.
    pub trader: Option<String>,
    /// Short prefix used for display ("AbCd\u{2026}") — stable even when
    /// `trader` is None (derived from the raw node-pointer in that case).
    pub trader_display: String,
    pub side: TradingSide,
    pub size: f64,
    pub entry_price: f64,
    /// `|size| * mark_price` at refresh time (or entry-price if mark is
    /// unknown).
    pub notional: f64,
    /// Unrealized PnL at the latest known mark price. Zero when mark is
    /// unknown.
    pub unrealized_pnl: f64,
}
