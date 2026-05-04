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
    ConfirmQuit,
}
