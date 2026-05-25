//! TUI input mode — drives which keys the poller dispatches and which modal
//! the renderer overlays.

use super::PendingAction;

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    EditingSize,
    /// Same row as size edit: type USD limit price, Enter to save (empty
    /// clears → market orders).
    EditingPrice,
    EditingDeposit,
    EditingWithdraw,
    Confirming(PendingAction),
    SelectingMarket,
    ViewingPositions,
    /// "Top positions on Phoenix" modal — top-N largest active positions
    /// across every trader on the protocol, fetched from the on-chain
    /// ActiveTraderBuffer.
    ViewingTopPositions,
    ViewingOrders,
    /// Live liquidation feed modal (toggled with `F`) — shows recent
    /// `LiquidationEvent`s parsed from on-chain Phoenix Eternal transactions.
    ViewingLiquidations,
    ViewingLedger,
    ViewingConfig,
    EditingRpcUrl,
    /// Text editor for the `SetComputeUnitPrice` override (microlamports per
    /// CU). Empty + Enter clears the override and falls back to env / default.
    EditingComputeUnitPrice,
    /// Text editor for the `SetComputeUnitLimit` per-position override
    /// (compute units per trader position). Empty + Enter clears the override
    /// and falls back to env / default.
    EditingComputeUnitLimit,
    /// "Load Wallet" modal opened by [w] when no wallet is loaded. Edits a
    /// path string seeded from `default_wallet_path()`; Enter attempts the
    /// load and closes on success.
    EditingWalletPath,
    /// First-run referral choice modal. Opens automatically after a wallet
    /// with no Phoenix account connects. Three options: use the COSMIC
    /// referral (10% fee discount, Cinder earns a share), enter a custom
    /// code, or skip and self-register at phoenix.trade.
    ChoosingReferral,
    /// "Custom referral code" text-input modal. Reached from the
    /// `ChoosingReferral` modal's "Use custom code" option. Empty + Enter or
    /// Esc skips and points the user at phoenix.trade.
    EditingReferralCode,
    /// "New TWAP" modal opened from Normal mode by pressing [Enter] while the
    /// active `OrderKind` is `Twap`. Collects side, total size, duration, and
    /// slice count via a four-row field editor; [Enter] starts the bot and
    /// returns to Normal mode, [Esc] discards.
    EditingTwap,
    /// "Bots" modal opened by [b] — lists running TWAP bots with
    /// pause/unpause/stop/restart/remove hotkeys.
    ViewingBots,
    ConfirmQuit,
}
