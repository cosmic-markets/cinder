use core::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use solana_pubkey::Pubkey;

use crate::accounts::TickRegion;
use crate::orderbook::{NodePointer, OrderFlags, Side};
use crate::quantities::{
    BaseLots, FundingRateUnitInSeconds, OptionalNonZeroU64, QuoteLots, SignedBaseLots,
    SignedFeeRateMicro, SignedQuoteLots, SignedQuoteLotsPerBaseLot,
    SignedQuoteLotsPerBaseLotUpcasted, SignedTicks, Symbol, Ticks, TraderCapabilityFlags,
};

#[repr(transparent)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Constant {
    inner: u64,
}

#[repr(transparent)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BasisPoints {
    inner: u64,
}

#[repr(u8)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValidationRule {
    #[default]
    Ignore = 0,
    Require = 1,
    Forbid = 2,
}

pub type RiskActionPriceValidityRules = [[[ValidationRule; 8]; 4]; 8];

#[repr(C)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LeverageTier {
    pub upper_bound_size: BaseLots,
    pub max_leverage: Constant,
    pub limit_order_risk_factor: BasisPoints,
}

#[repr(C)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LeverageTiers {
    tiers: [LeverageTier; 4],
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    GreaterThan,
    LessThan,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StopLossOrderKind {
    IOC,
    Limit,
}

#[repr(C)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AuthorityType {
    Root,
    Risk,
    Market,
    Oracle,
    Cancel,
    Backstop,
    ADL,
}

#[repr(u64)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MarketStatus {
    #[default]
    Uninitialized,
    Active,
    PostOnly,
    Paused,
    Closed,
    Tombstoned,
}

#[repr(C, u8)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EscrowAction {
    Noop {
        #[cfg_attr(feature = "serde", serde(with = "serde_big_array::BigArray"))]
        _padding0: [u8; 8],
        #[cfg_attr(feature = "serde", serde(with = "serde_big_array::BigArray"))]
        _padding1: [u8; 128],
    },
    Cash {
        amount: u64,
        #[cfg_attr(feature = "serde", serde(with = "serde_big_array::BigArray"))]
        _padding0: [u8; 128],
    },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SelfTradeBehavior {
    Abort,
    CancelProvide,
    DecrementTake,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, BorshSerialize, BorshDeserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrderPacket {
    kind: OrderPacketKind,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum OrderPacketKind {
    PostOnly {
        side: Side,
        price_in_ticks: Ticks,
        num_base_lots: BaseLots,
        client_order_id: [u8; 16],
        slide: bool,
        last_valid_slot: Option<u64>,
        order_flags: OrderFlags,
        cancel_existing: bool,
    },
    Limit {
        side: Side,
        price_in_ticks: Ticks,
        num_base_lots: BaseLots,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: [u8; 16],
        last_valid_slot: Option<u64>,
        order_flags: OrderFlags,
        cancel_existing: bool,
    },
    ImmediateOrCancel {
        side: Side,
        price_in_ticks: Option<Ticks>,
        num_base_lots: BaseLots,
        num_quote_lots: Option<QuoteLots>,
        min_base_lots_to_fill: BaseLots,
        min_quote_lots_to_fill: QuoteLots,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: [u8; 16],
        last_valid_slot: Option<u64>,
        order_flags: OrderFlags,
        cancel_existing: bool,
    },
}

impl OrderPacket {
    pub fn to_str(&self) -> &'static str {
        match self.kind {
            OrderPacketKind::PostOnly { .. } => "PostOnly",
            OrderPacketKind::Limit { .. } => "Limit",
            OrderPacketKind::ImmediateOrCancel { .. } => "ImmediateOrCancel",
        }
    }

    pub fn side(&self) -> Side {
        match self.kind {
            OrderPacketKind::PostOnly { side, .. } => side,
            OrderPacketKind::Limit { side, .. } => side,
            OrderPacketKind::ImmediateOrCancel { side, .. } => side,
        }
    }

    pub fn price_in_ticks(&self) -> Ticks {
        let ticks = match self.kind {
            OrderPacketKind::PostOnly { price_in_ticks, .. } => price_in_ticks,
            OrderPacketKind::Limit { price_in_ticks, .. } => price_in_ticks,
            OrderPacketKind::ImmediateOrCancel {
                price_in_ticks,
                side,
                ..
            } => match price_in_ticks {
                Some(t) => t,
                None => match side {
                    Side::Bid => Ticks::new(u64::MAX),
                    Side::Ask => Ticks::new(1),
                },
            },
        };
        if ticks < Ticks::new(1) {
            Ticks::new(1)
        } else {
            ticks
        }
    }

    pub fn num_base_lots(&self) -> BaseLots {
        match self.kind {
            OrderPacketKind::PostOnly { num_base_lots, .. } => num_base_lots,
            OrderPacketKind::Limit { num_base_lots, .. } => num_base_lots,
            OrderPacketKind::ImmediateOrCancel { num_base_lots, .. } => num_base_lots,
        }
    }
}

#[repr(C)]
#[derive(BorshSerialize, BorshDeserialize, Copy, Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LogHeader {
    pub log_batch_index: u32,
    pub total_events: u32,
}

#[derive(Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OffChainMarketEvent {
    pub batch_index: u32,
    pub events: Vec<MarketEvent>,
}

/// Payload of LogEventLengths instruction (after 8-byte discriminant).
/// Layout: batch_index (u32) + lengths (Vec<u16> in Borsh = length u32 +
/// elements).
#[derive(Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OffChainMarketEventLengths {
    pub batch_index: u32,
    pub lengths: Vec<u16>,
}

/// CAUTION: new events must be added to THE VERY END of the enum, to maintain
/// the backward compatibility of the variant discriminator.
#[allow(clippy::large_enum_variant)]
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MarketEvent {
    // Header event
    SlotContext(SlotContextEvent),

    // Orderbook / Trade events
    Header(MarketEventHeader),
    OrderPlaced(OrderPlacedEvent),
    OrderFilled(OrderFilledEvent),
    OrderRejected(OrderRejectedEvent),
    SplineFilled(SplineFilledEvent),
    TradeSummary(TradeSummaryEvent),
    OrderModified(OrderModifiedEvent),
    MarketSummary(MarketSummaryEvent),

    // Trader events
    TraderRegistered(TraderRegisteredEvent),
    TraderCollateralTransferred(TraderCollateralTransferredEvent),
    TraderActivated(TraderActivatedEvent),
    TraderDeactivated(TraderDeactivatedEvent),
    TraderFundsDeposited(TraderFundsDepositedEvent),
    TraderFundsWithdrawn(TraderFundsWithdrawnEvent),
    TraderFundsWithdrawnEnqueued(TraderFundsWithdrawnEvent),
    TraderFundsWithdrawnDropped(TraderFundsWithdrawnEvent),
    TraderWithdrawCancelled(TraderWithdrawCancelledEvent),
    TraderFundingSettled(TraderFundingSettledEvent),

    // Spline events
    SplineRegistered(SplineRegisteredEvent),
    SplineActivated(SplineActivatedEvent),
    SplineDeactivated(SplineDeactivatedEvent),
    SplinePriceUpdated(SplinePriceUpdatedEvent),
    SplineParametersUpdated(SplineParametersUpdatedEvent),

    // Admin events
    MarketAdded(MarketAddedEvent),
    MarketStatusChanged(MarketStatusChangedEvent),
    MarketParametersUpdated(MarketParametersUpdatedEvent),
    FundingParametersUpdated(FundingParametersUpdatedEvent),
    FeesClaimed(FeesClaimedEvent),

    // Oracle events
    PricesUpdated(PricesUpdatedEvent),

    // Liquidation transfer events
    LiquidationTransferSummary(LiquidationTransferSummaryEvent),
    LiquidationTransfer(LiquidationTransferEvent),

    // Liquidation events
    Liquidation(LiquidationEvent),

    // Close Match Position events (ADL)
    CloseMatchedPositions(CloseMatchedPositionsEvent),

    // Authority events
    NameSuccessor(NameSuccessorEvent),
    ClaimAuthority(ClaimAuthorityEvent),

    // Withdrawal state events
    WithdrawStateTransition(WithdrawStateTransitionEvent),

    // PnL events
    PnL(PnLEvent),

    // Stop loss events
    StopLossPlaced(StopLossPlacedEvent),
    StopLossCancelled(StopLossCancelledEvent),
    StopLossExecuted(StopLossExecutedEvent),

    // Exchange / onboarding events
    ExchangeStatusChanged(ExchangeStatusChangedEvent),
    TraderCapabilitiesEnabled(TraderCapabilitiesEnabledEvent),

    // Withdrawal Fee Payment
    TraderFundsWithdrawnFeePayment(TraderFundsWithdrawnFeePaymentEvent),

    // Order packet events (raw order packet data)
    OrderPacket(OrderPacketEvent),

    // Permission events
    SetPermission(SetPermissionEvent),
    AuthorityChanged(AuthorityChangedEvent),

    // Trader delegation events
    TraderDelegated(TraderDelegatedEvent),
    AdminParameterUpdated(AdminParameterUpdatedEvent),

    // Trader fee update events
    TraderFeesUpdated(TraderFeesUpdatedEvent),

    // Market closure event (with finalized mark price)
    MarketClosed(MarketClosedEvent),

    // Market deletion event (tombstoned market permanently deleted)
    MarketDeleted(MarketDeletedEvent),

    // Escrow events
    EscrowAccountCreated(EscrowAccountCreatedEvent),
    EscrowRequestCreated(EscrowRequestCreatedEvent),
    EscrowRequestAccepted(EscrowRequestAcceptedEvent),
    EscrowRequestCancelled(EscrowRequestCancelledEvent),

    // Spline events with anti-reordering protection
    #[deprecated(note = "removed event emission to lower CU usage")]
    SplinePriceUpdatedWithOrdering(SplinePriceUpdatedWithOrderingEvent),
    SplineParametersUpdatedWithOrdering(SplineParametersUpdatedWithOrderingEvent),
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SlotContextEvent {
    /// The unix timestamp of the instruction. Sourced from the clock sysvar at the time of instruction execution.
    pub timestamp: u64,
    /// The current slot of the instruction. Sourced from the clock sysvar at the time of instruction execution.
    pub slot: u64,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarketEventHeader {
    /// The orderbook sequence number.
    pub sequence_number: u64,
    /// The slot when the orderbook was last updated. Used by an indexer to gapfill any missing events.
    pub prev_sequence_number_slot: u64,
    /// The symbol of the asset in the PerpAssetMap.
    pub asset_symbol: Symbol,
    /// The asset_id in the PerpAssetMap.
    pub asset_id: u32,
    /// The tick size of the market, used to convert price in ticks to native price.
    pub tick_size: u32,
    /// The lot size of the market, used to convert quantity in base lots to native quantity.
    pub base_lot_decimals: i8,
    /// The number of decimals for quote lots, This is always expected to equal `6`.
    pub quote_lot_decimals: u8,
    /// The signer of the instruction that triggered the event. This is typically the trader authority or position_authority for order-related events.
    pub signer: Pubkey,
    /// The trader pda address associated with the instruction that triggered the event. This is typically the trader pda for order-related events, but may be `Pubkey::default()` for non-trader events.
    pub trader_account: Pubkey,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted when an order is accepted onto the resting book.
///
/// This is a post-validation event for orders that end up resting after any
/// immediate matching. Fully-taker flows that never rest do not emit this.
///
/// Unit conventions:
/// - `price`: ticks
/// - `quantity`: signed base lots (`> 0` bid, `< 0` ask)
pub struct OrderPlacedEvent {
    /// Program order id assigned to this resting order.
    pub order_id: u32,
    /// Flags stored on the resting order (reduce-only, stop-loss, etc.).
    pub order_flags: OrderFlags,
    /// Raw encoded order sequence number.
    ///
    /// The side is encoded in the high bit (`1` => bid encoding), matching the
    /// FIFO order id scheme used on-chain.
    pub order_sequence_number: u64,
    /// Slot component associated with the previous order sequence number.
    pub prev_order_sequence_number_slot: u64,
    /// Opaque client-provided identifier copied from the order packet.
    pub client_order_id: [u8; 16],
    /// Resting limit price in ticks.
    pub price: Ticks,
    /// Resting quantity in signed base lots (`> 0` bid, `< 0` ask).
    pub quantity: SignedBaseLots,

    /// Absolute expiry slot for this order, if time-in-force is enabled.
    pub last_valid_slot: OptionalNonZeroU64,
    /// Slot when the order was initially accepted/rested.
    pub initial_slot: u64,
}

/// Raw order packet event emitted at order entry.
///
/// This captures the order packet as submitted by the caller before matching
/// and placement logic runs, so it is useful for reconstructing user intent.
///
/// Emission timing:
/// - Emitted at the start of `MatchingEngine::place_order`, before order
///   validation, matching, and resting-book insertion.
/// - In a successful instruction, this can be followed by:
///   - `OrderRejected` (if packet is rejected by matching/placement logic), or
///   - one or more `OrderFilled` / `SplineFilled` and then `TradeSummary`
///     (if matching occurs), and optionally `OrderPlaced` (if remainder rests).
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrderPacketEvent {
    /// Full order packet payload (side, limits, TIF, flags, etc.).
    pub order_packet: OrderPacket,
    /// Trader account that submitted the packet.
    pub trader: Pubkey,
    /// Current raw order-sequence cursor before processing this packet.
    pub next_order_sequence_number: u64,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OrderRejectionReason {
    TooManyLimitOrders,
    PostOnlyCross,
    InvalidOrderPacket,
    TiFInvalid,
}
impl OrderRejectionReason {
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        let s = match self {
            OrderRejectionReason::TooManyLimitOrders => "TooManyLimitOrders",
            OrderRejectionReason::PostOnlyCross => "PostOnlyCross",
            OrderRejectionReason::InvalidOrderPacket => "InvalidOrderPacket",
            OrderRejectionReason::TiFInvalid => "TiFInvalid",
        };
        bytes[..s.len()].copy_from_slice(s.as_bytes());
        bytes
    }
}
impl From<OrderRejectionReason> for [u8; 32] {
    fn from(val: OrderRejectionReason) -> Self {
        val.to_bytes()
    }
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted when an incoming order packet is rejected.
///
/// Unit conventions:
/// - `price`: ticks
/// - `num_base_lots`: unsigned base lots requested
pub struct OrderRejectedEvent {
    /// Instruction-local order index for rejection bookkeeping.
    pub order_index: u32,
    /// Opaque client-provided identifier copied from the order packet.
    pub client_order_id: [u8; 16],
    /// Requested limit/trigger price in ticks.
    pub price: Ticks,
    /// Side of the rejected order packet.
    pub side: Side,
    /// Requested order size in base lots.
    pub num_base_lots: BaseLots,
    // TODO: maybe put entire order packet here?
    /// UTF-8 rejection reason packed into a NUL-padded fixed 32-byte buffer.
    pub reason: [u8; 32],
}
impl OrderRejectedEvent {
    pub fn reason_str(&self) -> String {
        // Reasons are stored in a fixed-width buffer padded with NUL bytes; slice up to
        // the first padding byte
        let len = self
            .reason
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.reason.len());
        String::from_utf8_lossy(&self.reason[..len]).to_string()
    }

    pub fn reason(&self) -> Option<OrderRejectionReason> {
        let reason_str = self.reason_str();
        match reason_str.as_str() {
            "TooManyLimitOrders" => Some(OrderRejectionReason::TooManyLimitOrders),
            "PostOnlyCross" => Some(OrderRejectionReason::PostOnlyCross),
            "InvalidOrderPacket" => Some(OrderRejectionReason::InvalidOrderPacket),
            "TiFInvalid" => Some(OrderRejectionReason::TiFInvalid),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_str_trims_null_padding() {
        let mut reason = [0u8; 32];
        let text = b"TiFInvalid";
        reason[..text.len()].copy_from_slice(text);
        // Add explicit padding to ensure the implementation drops it
        reason[text.len()] = 0u8;

        let event = OrderRejectedEvent {
            order_index: 0,
            client_order_id: [0; 16],
            price: Ticks::from(0),
            side: Side::Bid,
            num_base_lots: BaseLots::from(0),
            reason,
        };

        assert_eq!(event.reason_str(), "TiFInvalid");
    }
}

/// Orderbook trade lifecycle notes (maker/taker correlation):
///
/// - A single taker execution can emit multiple `OrderFilled` and
///   `SplineFilled` events (one per maker fill leg).
/// - The engine then emits exactly one `TradeSummary` for that taker execution.
/// - Fill events do not include `trade_sequence_number`; to correlate maker
///   fills to a taker trade, buffer fills and assign them to the next
///   `TradeSummary` within the same instruction/header context.
///
/// Keep this data structure compact to minimize log payload size.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted for each matched fill leg against a resting orderbook maker.
///
/// Perspective and units:
/// - `side` is from the maker's perspective.
/// - `price` is in ticks.
/// - `base_lots_filled` and `quote_lots_filled` are absolute magnitudes.
/// - Post-fill maker state fields are snapshotted after applying this leg.
pub struct OrderFilledEvent {
    /// Raw encoded maker order sequence number (side encoded in MSB scheme).
    pub order_sequence_number: u64,

    /// Side from the maker's perspective.
    pub side: Side,
    /// Execution price in ticks.
    pub price: Ticks,
    /// Filled base quantity (absolute) in base lots.
    pub base_lots_filled: BaseLots,
    /// Filled quote quantity (absolute) in quote lots.
    pub quote_lots_filled: QuoteLots,
    /// Remaining resting maker size after this leg (base lots).
    pub quantity_remaining: BaseLots,

    /// Maker trader account that provided this resting liquidity.
    pub maker: Pubkey,
    /// Effective maker fee rate in fee-rate micros (1e-6).
    pub maker_fee_rate: SignedFeeRateMicro,
    /// Maker base position after applying this fill leg (signed base lots).
    pub maker_base_lot_position: SignedBaseLots,
    /// Maker virtual quote position after this leg (signed quote lots).
    pub maker_virtual_quote_lot_position: SignedQuoteLots,
    /// Maker collateral after this leg (signed quote lots).
    pub maker_quote_lot_collateral: SignedQuoteLots,
    /// Maker cumulative funding snapshot after this leg.
    pub maker_cumulative_funding_snapshot: SignedQuoteLotsPerBaseLot,
}
impl OrderFilledEvent {
    /// Signed base delta for this maker fill leg.
    ///
    /// `Bid` => positive (maker bought base), `Ask` => negative.
    pub fn quantity(&self) -> SignedBaseLots {
        match self.side {
            Side::Bid => SignedBaseLots::new(self.base_lots_filled.as_signed()),
            Side::Ask => SignedBaseLots::new(-self.base_lots_filled.as_signed()),
        }
    }
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted for each fill against spline liquidity.
///
/// This event represents one maker-side fill leg where a spline provided
/// liquidity to a taker order. Indexers can use this to update the spline
/// trader's position and collateral state incrementally.
///
/// Perspective and units:
/// - `side` is from the spline maker's perspective.
/// - `price` is in ticks.
/// - `base_lots_filled` and `quote_lots_filled` are absolute magnitudes.
/// - Post-fill maker state fields are snapshotted after applying this leg.
pub struct SplineFilledEvent {
    /// Sequence number on the spline trader after this fill is applied.
    pub spline_sequence_number: u64,

    /// Side from the spline maker's perspective.
    pub side: Side,
    /// Execution price in ticks for this fill leg.
    pub price: Ticks,
    /// Base lots filled in this fill leg.
    pub base_lots_filled: BaseLots,
    /// Quote lots exchanged in this fill leg.
    pub quote_lots_filled: QuoteLots,

    /// Trader account that owns the filled spline.
    pub maker: Pubkey,
    /// Effective maker fee rate (micros) applied to this fill.
    pub maker_fee_rate: SignedFeeRateMicro,
    /// Maker base-lot position after applying this fill.
    pub maker_base_lot_position: SignedBaseLots,
    /// Maker virtual quote-lot position after applying this fill.
    pub maker_virtual_quote_lot_position: SignedQuoteLots,
    /// Maker collateral balance (quote lots) after applying this fill.
    pub maker_quote_lot_collateral: SignedQuoteLots,
    /// Maker cumulative funding snapshot after applying this fill.
    pub maker_cumulative_funding_snapshot: SignedQuoteLotsPerBaseLot,
}
impl SplineFilledEvent {
    /// Signed base delta for this spline-maker fill leg.
    ///
    /// `Bid` => positive (maker bought base), `Ask` => negative.
    pub fn quantity(&self) -> SignedBaseLots {
        match self.side {
            Side::Bid => SignedBaseLots::new(self.base_lots_filled.as_signed()),
            Side::Ask => SignedBaseLots::new(-self.base_lots_filled.as_signed()),
        }
    }
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrderModifiedEvent {
    pub order_sequence_number: u64,
    pub price: Ticks,
    /// Use the sign of the quantity to determine the side of the order
    pub base_lots_released: SignedBaseLots,
    pub quote_lots_released: SignedQuoteLots,
    pub base_lots_remaining: BaseLots,
    pub reason: OrderModificationReason,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OrderModificationReason {
    /// User explicitly cancelled the order
    UserRequested,
    /// Order cancelled by cancel authority
    AuthorityForced,
    /// Order cancelled by self trade with CancelProvide behavior
    SelfTradeCancelProvide,
    /// Order cancelled by self trade with DecrementTake behavior
    SelfTradeDecrementTake,
    /// Order expired
    Expired,
    /// Reduce-only order invalidated
    ReduceOnlyInvalidated,
    /// Risk capacity exceeded
    RiskCapacityExceeded,
    /// Order evicted due to book capacity limit
    BookCapacityEvicted,
    /// Tombstone order
    Tombstone,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted once for the taker after a trade operation completes.
///
/// This is the taker-side aggregate summary across one matching operation
/// (potentially multiple maker/spline fills). Integrators should use this
/// as the canonical post-trade taker state update.
///
/// Correlation with maker fills:
/// - `OrderFilled`/`SplineFilled` events are emitted first, per fill leg.
/// - `TradeSummary` is emitted after matching for the taker is complete.
/// - Use this event's `trade_sequence_number` as the stable taker trade id and
///   attach it to the set of fill events seen since the previous summary in the
///   same instruction/header context.
pub struct TradeSummaryEvent {
    /// Taker trader account.
    pub trader: Pubkey,
    /// Updated trade sequence number on the taker account.
    ///
    /// This is the canonical taker trade identifier for downstream indexing.
    pub trade_sequence_number: u64,
    /// Slot component associated with the previous sequence number.
    pub prev_trade_sequence_number_slot: u64,
    /// Side from the taker's perspective.
    pub side: Side,
    /// Total base lots filled for this taker trade summary.
    ///
    /// Sum of base lots across all preceding fill legs for this taker match.
    pub base_lots_filled: BaseLots,
    /// Total quote lots exchanged for this taker trade summary.
    ///
    /// Sum of quote lots across all preceding fill legs for this taker match.
    pub quote_lots_filled: QuoteLots,
    /// Taker fee charged in quote lots.
    pub fee_in_quote_lots: QuoteLots,

    /// Taker base-lot position after the trade.
    pub base_lot_position: SignedBaseLots,
    /// Taker virtual quote-lot position after the trade.
    pub virtual_quote_lot_position: SignedQuoteLots,
    /// Taker collateral balance (quote lots) after the trade.
    pub quote_lot_collateral: SignedQuoteLots,
    /// Taker cumulative funding snapshot after the trade.
    pub cumulative_funding_snapshot: SignedQuoteLotsPerBaseLot,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted when market-level state is recomputed after open-interest changes.
///
/// Emission timing:
/// - Emitted by `PerpAssetMap::update_open_interest` when
///   `open_interest_change != 0`.
/// - Common call sites include post-trade matching-engine flows, liquidation
///   flows, and forced-close/settlement flows that mutate OI.
///
/// Unit conventions:
/// - `open_interest`: base lots
/// - `total_*_fees`: quote lots
/// - `mark_price` / `spot_price`: ticks
pub struct MarketSummaryEvent {
    /// Market symbol for this summary snapshot.
    pub asset_symbol: Symbol,
    /// Numeric market/asset identifier.
    pub asset_id: u32,
    /// Current market open interest after applying this update (base lots).
    pub open_interest: BaseLots,
    /// Total maker fees attributable to the triggering operation (quote lots).
    ///
    /// This may be `None` when the OI update path does not carry maker-fee
    /// accounting (for example some non-orderbook close/transfer flows).
    pub total_maker_quote_lot_fees: Option<SignedQuoteLots>,
    /// Total taker fees attributable to the triggering operation (quote lots).
    ///
    /// This may be `None` when the OI update path does not carry taker-fee
    /// accounting.
    pub total_taker_quote_lot_fees: Option<QuoteLots>,
    /// Current mark price snapshot in ticks.
    pub mark_price: Ticks,
    /// Current median spot/index price snapshot in ticks.
    pub spot_price: Ticks,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Trader events
////////////////////////////////////////////////////////////////////////////////////////////////

/// Trader was added to global index.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderRegisteredEvent {
    pub trader_sequence_number: u64,
    pub trader: Pubkey,
    pub authority: Pubkey,
    pub max_positions: u64,
    pub trader_pda_index: u8,
    pub trader_subaccount_index: u8,
}

/// Trader delegated their position authority to another wallet.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderDelegatedEvent {
    pub trader: Pubkey,
    pub authority: Pubkey,
    pub old_position_authority: Pubkey,
    pub new_position_authority: Pubkey,
}

/// Trader fee override multipliers were updated.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderFeesUpdatedEvent {
    /// The trader account whose fees were updated
    pub trader: Pubkey,
    /// The authority that signed the fee update
    pub authority: Pubkey,
    /// Previous maker fee override multiplier
    pub previous_maker_fee_override_multiplier: i8,
    /// New maker fee override multiplier
    pub new_maker_fee_override_multiplier: i8,
    /// Previous taker fee override multiplier
    pub previous_taker_fee_override_multiplier: i8,
    /// New taker fee override multiplier
    pub new_taker_fee_override_multiplier: i8,
    /// Whether the trader was found in GTI (hot) or trader account (cold)
    pub is_hot_trader: bool,
}

/// Trader capabilities were enabled by a delegated authority.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderCapabilitiesEnabledEvent {
    pub trader: Pubkey,
    pub authority: Pubkey,
    pub previous_flags: TraderCapabilityFlags,
    pub new_flags: TraderCapabilityFlags,
    /// Global trader index at enablement time (0 if still cold).
    pub global_trader_index: u32,
}

/// Trader was moved to hot state.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderActivatedEvent {
    pub global_trader_index: u32,
    pub authority: Pubkey,
}

/// Trader was moved to cold state.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderDeactivatedEvent {
    pub prev_global_trader_index: u32,
    pub authority: Pubkey,
}

/// Emitted when collateral is successfully deposited into a trader account.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderFundsDepositedEvent {
    /// Trader account credited by the deposit.
    pub trader: Pubkey,
    /// Authority signer that authorized the deposit.
    pub authority: Pubkey,
    /// Deposit amount in quote lots.
    pub amount: QuoteLots,
    /// Trader capability flags after the deposit is applied.
    pub trader_flags: TraderCapabilityFlags,
    /// Trader collateral balance (quote lots) after deposit.
    pub new_collateral_balance: SignedQuoteLots,
    /// Updated trader sequence number after deposit.
    pub trader_sequence_number: u64,
    /// Slot component associated with the previous sequence number. Used by the indexer to gapfill last slot where a deposit or withdraw occured.
    pub prev_sequence_number_slot: u64,
}

/// Emitted for withdrawal lifecycle actions that mutate trader balances/queue.
///
/// This payload is shared by:
/// - `TraderFundsWithdrawn`
/// - `TraderFundsWithdrawnEnqueued`
/// - `TraderFundsWithdrawnDropped`
///
/// Integrators can use the event variant plus this payload to track queue and
/// budget state transitions.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderFundsWithdrawnEvent {
    /// Trader account affected by the withdrawal action.
    pub trader: Pubkey,
    /// Authority signer that initiated/authorized the action.
    pub authority: Pubkey,
    /// Withdrawal amount in quote token native units.
    pub amount: u64,
    /// Updated trader sequence number after the action.
    pub trader_sequence_number: u64,
    /// Slot component associated with the previous trader sequence number.
    pub trader_prev_sequence_number_slot: u64,
    /// Remaining withdrawal budget after this action.
    pub post_withdrawal_budget: u64,
    /// Number of entries in the withdrawal queue after this action.
    pub post_queue_size: u64,
    /// Total queued withdrawal amount after this action.
    pub total_queued_amount: u64,
    /// Updated withdrawal queue sequence number after this action.
    pub withdraw_queue_sequence_number: u64,
    /// Slot component associated with the previous queue sequence number.
    pub withdraw_queue_prev_sequence_number_slot: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderFundsWithdrawnFeePaymentEvent {
    pub trader: Pubkey,
    pub authority: Pubkey,
    pub fee: u64,
    pub trader_sequence_number: u64,
    pub trader_prev_sequence_number_slot: u64,
    pub withdraw_queue_sequence_number: u64,
    pub withdraw_queue_prev_sequence_number_slot: u64,
}
/// Trader cancelled a pending withdrawal request.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderWithdrawCancelledEvent {
    pub trader: Pubkey,
    pub authority: Pubkey,
    pub amount: u64,
    pub trader_sequence_number: u64,
    pub trader_prev_sequence_number_slot: u64,
    pub withdraw_queue_sequence_number: u64,
    pub withdraw_queue_prev_sequence_number_slot: u64,
}

/// Collateral was transferred between trader accounts.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderCollateralTransferredEvent {
    pub authority: Pubkey,
    pub amount: QuoteLots,
    pub src_trader: Pubkey,
    pub src_trader_sequence_number: u64,
    pub src_trader_prev_sequence_number_slot: u64,
    pub src_trader_new_collateral_balance: SignedQuoteLots,
    pub dst_trader: Pubkey,
    pub dst_trader_sequence_number: u64,
    pub dst_trader_prev_sequence_number_slot: u64,
    pub dst_trader_new_collateral_balance: SignedQuoteLots,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted when funding payment is actually realized for one trader position.
///
/// Emission timing:
/// - Triggered during trade processing before applying the trade leg
///   (maker/taker paths settle funding first).
/// - Also triggered by explicit funding-settlement flows that iterate trader
///   positions and refresh funding snapshots.
/// - Emitted only when `funding_payment != 0` (no event for no-op settlements).
///
/// Ordering notes:
/// - In trade flows, this event can appear before the corresponding `PnL`
///   event for the same trader/asset.
pub struct TraderFundingSettledEvent {
    /// Trader account whose funding was settled.
    pub trader: Pubkey,
    /// Symbol of the market that was settled.
    pub asset_symbol: Symbol,
    /// Numeric asset identifier for the settled market.
    pub asset_id: u32,
    /// Signed funding payment applied to trader collateral.
    pub funding_payment: SignedQuoteLots,
    /// New cumulative funding snapshot stored for the trader position.
    pub cumulative_funding_snapshot: SignedQuoteLotsPerBaseLot,
    /// Trader collateral balance (quote lots) after settlement.
    pub new_collateral_balance: SignedQuoteLots,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Spline events
////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SplineRegisteredEvent {
    pub trader: Pubkey,
    pub sequence_number: u64,
    pub prev_sequence_number_slot: u64,
    pub market: Pubkey,
    pub symbol: Symbol,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SplineActivatedEvent {
    pub trader: Pubkey,
    pub sequence_number: u64,
    pub prev_sequence_number_slot: u64,
    pub authority: Pubkey,
    pub market: Pubkey,
    pub symbol: Symbol,
    pub mid_price: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SplineDeactivatedEvent {
    pub trader: Pubkey,
    pub sequence_number: u64,
    pub prev_sequence_number_slot: u64,
    pub authority: Pubkey,
    pub market: Pubkey,
    pub symbol: Symbol,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted when a spline price update is applied.
///
/// This event is produced by successful spline price update instructions and
/// captures the resulting spline sequencing and pricing state.
pub struct SplinePriceUpdatedEvent {
    /// Trader account that owns the spline.
    pub trader: Pubkey,
    /// Updated spline sequence number after this change.
    pub sequence_number: u64,
    /// Slot component associated with the previous sequence number.
    pub prev_sequence_number_slot: u64,
    /// Authority signer that applied the update.
    pub authority: Pubkey,
    /// Market account whose spline data was updated.
    pub market: Pubkey,
    /// Market symbol for the updated spline.
    pub symbol: Symbol,
    /// New spline reference price in ticks.
    pub price_in_ticks: u64,
    /// User-provided update slot recorded on the spline.
    pub user_update_slot: u64,
    /// Whether region refresh/reset logic was requested during update.
    pub refresh_regions: bool,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted when spline parameters (mid + regions) are updated.
///
/// This event is produced by successful spline parameter update instructions.
/// Integrators should treat bid/ask region arrays as the full post-update
/// spline configuration.
pub struct SplineParametersUpdatedEvent {
    /// Trader account that owns the spline.
    pub trader: Pubkey,
    /// Updated spline sequence number after this change.
    pub sequence_number: u64,
    /// Slot component associated with the previous sequence number.
    pub prev_sequence_number_slot: u64,
    /// Authority signer that applied the update.
    pub authority: Pubkey,
    /// Market account whose spline data was updated.
    pub market: Pubkey,
    /// Market symbol for the updated spline.
    pub symbol: Symbol,
    /// Updated spline mid price in ticks.
    pub mid_price: u64,
    /// Full post-update bid-side region configuration.
    pub bid_regions: [TickRegion; 10],
    /// Full post-update ask-side region configuration.
    pub ask_regions: [TickRegion; 10],
}

/// Spline price update event with anti-reordering protection.
/// Includes `user_sequence_number` and `client_order_id` fields.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SplinePriceUpdatedWithOrderingEvent {
    pub trader: Pubkey,
    pub sequence_number: u64,
    pub prev_sequence_number_slot: u64,
    pub authority: Pubkey,
    pub market: Pubkey,
    pub symbol: Symbol,
    pub price_in_ticks: u64,
    pub user_update_slot: u64,
    pub refresh_regions: bool,
    /// User-provided sequence number for anti-reordering protection.
    pub user_sequence_number: u64,
    /// Client order id for tracking across exchanges.
    pub client_order_id: [u8; 16],
}

/// Spline parameter update event with anti-reordering protection.
/// Includes `user_sequence_number` and `client_order_id` fields.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SplineParametersUpdatedWithOrderingEvent {
    pub trader: Pubkey,
    pub sequence_number: u64,
    pub prev_sequence_number_slot: u64,
    pub authority: Pubkey,
    pub market: Pubkey,
    pub symbol: Symbol,
    pub mid_price: u64,
    pub bid_regions: [TickRegion; 10],
    pub ask_regions: [TickRegion; 10],
    /// User-provided sequence number for anti-reordering protection.
    pub user_sequence_number: u64,
    /// Client order id for tracking across exchanges.
    pub client_order_id: [u8; 16],
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Stop loss events
////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StopLossPlacedEvent {
    pub trader: Pubkey,
    pub sequence_number: u64,
    pub prev_sequence_number_slot: u64,

    // Stop loss parameters
    pub asset_id: u64,
    pub trigger_price: Ticks,
    pub execution_price: Ticks,
    pub trade_size: BaseLots,
    pub trade_side: Side,
    pub execution_direction: Direction,
    pub position_sequence_number: u8,
    pub place_slot: u64,
    pub funding_key: Pubkey,
    pub order_kind: StopLossOrderKind,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StopLossCancelledEvent {
    pub trader: Pubkey,
    pub sequence_number: u64,
    pub prev_sequence_number_slot: u64,
    pub asset_id: u64,
    pub execution_direction: Direction,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StopLossExecutedEvent {
    pub trader: Pubkey,
    pub sequence_number: u64,
    pub prev_sequence_number_slot: u64,
    pub asset_id: u64,
    pub execution_direction: Direction,
    pub order_sequence_number: u64,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Admin events
////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarketAddedEvent {}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarketStatusChangedEvent {
    pub previous_market_status: MarketStatus,
    pub new_market_status: MarketStatus,
}

/// Event emitted when a market is closed with its finalized settlement price.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarketClosedEvent {
    pub previous_market_status: MarketStatus,
    pub finalized_mark_price: Ticks,
}

/// Event emitted when a tombstoned market is permanently deleted.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarketDeletedEvent {
    pub asset_id: u32,
    pub lamports_reclaimed: u64,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExchangeStatusChangedEvent {
    pub previous_bits: u8,
    pub new_bits: u8,
    pub authority: Pubkey,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarketParametersUpdatedEvent {}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FundingParametersUpdatedEvent {
    pub symbol: Symbol,
    pub new_funding_interval_seconds: Option<FundingRateUnitInSeconds>,
    pub new_funding_period_seconds: Option<FundingRateUnitInSeconds>,
    pub new_max_funding_rate: Option<SignedQuoteLotsPerBaseLot>,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FeesClaimedEvent {
    pub amount: u64,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SetPermissionEvent {
    pub authority: Pubkey,
    pub user: Pubkey,
    pub previous_permission: u64,
    pub new_permission: u64,
    pub previous_expires_at_timestamp: i64,
    pub new_expires_at_timestamp: i64,
    pub previous_num_signer_actions_remaining: i64,
    pub new_num_signer_actions_remaining: i64,
    pub created: bool,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Oracle events
////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted when oracle/exchange prices and derived mark/funding inputs change.
///
/// Not every optional component is guaranteed to update each emission.
/// Integrators should apply only fields that are `Some(...)` and always use
/// `new_mark_price` as the canonical mark after this event.
///
/// Emission timing:
/// - Emitted by `PerpAssetMap::update_mark_price`.
/// - This is called in both market-operation paths (place/cancel/liquidation
///   style flows that update book-derived prices) and oracle-admin price update
///   paths.
/// - Oracle-driven updates typically populate `oracle_signer` and may populate
///   funding fields; pure book-driven updates typically carry best bid/ask/last
///   trade changes with oracle fields unset.
pub struct PricesUpdatedEvent {
    /// The oracle signer who authorized the update.
    /// Only applicable if exchange spot/perp price were updated by an oracle.
    pub oracle_signer: Option<Pubkey>,
    /// Asset symbol whose prices were updated.
    pub asset_symbol: Symbol,
    /// Numeric asset identifier whose prices were updated.
    pub asset_id: u32,
    /// Updated best bid in ticks, if provided.
    pub new_best_bid: Option<Ticks>,
    /// Updated best ask in ticks, if provided.
    pub new_best_ask: Option<Ticks>,
    /// Updated last trade price in ticks, if provided.
    pub new_last_trade: Option<Ticks>,
    /// Updated exchange spot price in ticks, if provided.
    pub new_exchange_spot_price: Option<Ticks>,
    /// Updated exchange perp price in ticks, if provided.
    pub new_exchange_perp_price: Option<Ticks>,
    /// Updated EMA of mid-vs-spot difference in ticks, if provided.
    pub new_mid_spot_diff_ema_ticks: Option<SignedTicks>,
    /// Updated mark price in ticks (always populated).
    pub new_mark_price: Ticks,
    /// Updated cumulative funding rate, if funding moved this tick.
    pub cumulative_funding_rate: Option<SignedQuoteLotsPerBaseLot>,
    /// Incremental settled funding contribution applied this update, if present.
    pub settled_contribution: Option<SignedQuoteLotsPerBaseLot>,
    /// High-precision funding interval accumulator, if present.
    pub interval_accumulator: Option<SignedQuoteLotsPerBaseLotUpcasted>,
    /// Updated asset-level sequence number after this price update.
    pub asset_sequence_number: u64,
    /// Slot component associated with the previous asset sequence number.
    pub prev_asset_sequence_number_slot: u64,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Liquidation events
////////////////////////////////////////////////////////////////////////////////////////////////

/// Emitted when a liquidation market order executes against a trader position.
///
/// This captures the executed liquidation outcome for one market. Integrators
/// can use it to update both accounts and detect full position closure.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LiquidationEvent {
    /// Trader account performing the liquidation.
    pub liquidator: Pubkey,
    /// Trader account being liquidated.
    pub liquidated_trader: Pubkey,
    /// Market/asset identifier where liquidation occurred.
    pub asset_id: u32,
    /// Requested liquidation size in base lots.
    pub liquidation_size: BaseLots,
    /// Mark price in ticks used during liquidation checks/execution.
    pub mark_price: Ticks,
    /// Executed base lots actually filled by the liquidation order.
    pub base_lots_filled: BaseLots,
    /// Executed quote lots exchanged by the liquidation order.
    pub quote_lots_filled: QuoteLots,
    /// True if the liquidated position became fully closed on this market.
    pub position_closed: bool,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Liquidation transfer events
////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LiquidationTransferSummaryEvent {
    pub liquidatee: Pubkey,
    pub liquidator: Pubkey,
    pub total_transfers: u32,
    pub liquidatee_collateral_change: SignedQuoteLots,
    pub liquidator_collateral_change: SignedQuoteLots,
    pub haircut_collected: QuoteLots,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LiquidationTransferEvent {
    pub liquidatee: Pubkey,
    pub liquidator: Pubkey,
    pub asset_id: u64,
    pub base_lots_transferred: SignedBaseLots,
    pub virtual_quote_lots_transferred: SignedQuoteLots,
    pub haircut_rate: u16,
    pub liquidatee_collateral_change: SignedQuoteLots,
    pub liquidator_collateral_change: SignedQuoteLots,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Close Match Position events (ADL)
////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CloseMatchedPositionsEvent {
    pub caller: Pubkey,
    pub closed_short: Pubkey,
    pub closed_long: Pubkey,
    pub in_profit_account: Pubkey,
    pub asset_id: u64,
    pub base_lots_closed: SignedBaseLots,
    pub at_loss_close_value: SignedQuoteLots,
    pub in_profit_close_value: SignedQuoteLots,
    pub at_loss_collateral_change: SignedQuoteLots,
    pub in_profit_collateral_change: SignedQuoteLots,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Authority events
////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NameSuccessorEvent {
    pub authority: Pubkey,
    pub new_authority: Pubkey,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClaimAuthorityEvent {
    pub previous_authority: Pubkey,
    pub new_authority: Pubkey,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AuthorityChangedEvent {
    pub previous_authority: Pubkey,
    pub new_authority: Pubkey,
    pub authority_type: AuthorityType,
}

/// Event emitted when a withdrawal request transitions between states
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WithdrawStateTransitionEvent {
    /// The trader requesting the withdrawal
    pub trader: Pubkey,
    /// The amount being withdrawn
    pub amount: QuoteLots,
    /// The state before the transition (as u8)
    pub from_state: u8,
    /// The state after the transition (as u8)
    pub to_state: u8,
    /// The reason for the transition (as u8)
    pub reason: u8,
    /// Number of state transitions this request has gone through
    pub transition_count: u16,
    /// Queue node index if applicable
    pub node_index: NodePointer,
}

////////////////////////////////////////////////////////////////////////////////////////////////
// PnL events
////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Emitted when a trader PnL snapshot is recorded for a market.
///
/// This event contains both signed collateral deltas and before/after position
/// components so indexers can replay exact position state transitions.
///
/// Realized PnL is only generated when the existing position magnitude
/// decreases or the position flips side. In other words, pure position
/// increases do not realize PnL.
///
/// For `TradeState.quote_lot_collateral`, current engine behavior is:
/// - Realized PnL updates collateral.
/// - Trading fees update collateral.
/// - Deposits/withdrawals update collateral.
pub struct PnLEvent {
    /// Trader account whose PnL/funding was updated.
    pub trader: Pubkey,
    /// Market/asset identifier for this PnL update.
    pub asset_id: u32,
    /// Market symbol for this PnL update.
    pub asset_symbol: Symbol,
    /// Realized PnL delta applied to collateral (signed quote lots).
    ///
    /// Non-zero when position reduction/flip causes realization.
    pub realized_pnl: SignedQuoteLots,
    /// Funding component for this PnL snapshot (signed quote lots).
    ///
    /// Exposed for attribution/analytics alongside realized PnL.
    pub funding_payment: SignedQuoteLots,
    /// Base-lot position before applying this update.
    pub base_lots_before: SignedBaseLots,
    /// Base-lot position after applying this update.
    pub base_lots_after: SignedBaseLots,
    /// Virtual quote-lot position before applying this update.
    pub virtual_quote_lots_before: SignedQuoteLots,
    /// Virtual quote-lot position after applying this update.
    pub virtual_quote_lots_after: SignedQuoteLots,
}

impl PnLEvent {
    pub fn is_noop(&self) -> bool {
        self.base_lots_before == self.base_lots_after
            && self.virtual_quote_lots_before == self.virtual_quote_lots_after
            && self.realized_pnl == SignedQuoteLots::ZERO
            && self.funding_payment == SignedQuoteLots::ZERO
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////
// Admin parameter update events
////////////////////////////////////////////////////////////////////////////////////////////////

/// Lightweight struct capturing configurable mark price parameters.
/// Used for prev/new comparison in admin parameter update events.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarkPriceConfig {
    pub ema_period_slots: u64,
    pub ema_diff_radius: u64,
    pub book_price_radius: u64,
    pub spot_price_weight: u64,
    pub book_price_weight: u64,
    pub perp_price_weight: u64,
    pub spot_price_stale_threshold: u64,
    pub book_price_stale_threshold: u64,
    pub perp_price_stale_threshold: u64,
    pub risk_action_price_validity_rules: RiskActionPriceValidityRules,
    pub oracle_divergence_radius: u16,
    pub min_oracle_responses: u8,
}

/// Lightweight struct capturing withdraw queue parameters.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WithdrawConfig {
    pub deposit_cooldown_period_in_slots: u64,
    pub withdrawal_fee: u64,
    pub enqueueing_fee: u64,
}

/// Lightweight struct capturing withdraw rate limit parameters.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WithdrawRateLimitConfig {
    pub max_budget: u64,
    pub replenish_amount_per_slot: u64,
}

/// Lightweight struct capturing funding parameters.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FundingConfig {
    pub funding_interval_seconds: u64,
    pub funding_period_seconds: u64,
    pub max_funding_rate: i64,
}

/// The type of admin parameter update that occurred.
/// Used to emit events when risk/market/root authority parameters are changed.
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(clippy::large_enum_variant)]
pub enum AdminParameterUpdateKind {
    // Perp risk parameters (risk authority)
    CancelRiskFactor {
        previous: u16,
        new: u16,
    },
    IsolatedOnly {
        previous: bool,
        new: bool,
    },
    LeverageTiers {
        previous: LeverageTiers,
        new: LeverageTiers,
    },
    MarkPriceParameters {
        previous: MarkPriceConfig,
        new: MarkPriceConfig,
    },
    OpenInterestCap {
        previous: u64,
        new: u64,
    },
    UpnlRiskFactor {
        previous: u16,
        new: u16,
    },
    UpnlRiskFactorForWithdrawals {
        previous: u16,
        new: u16,
    },

    // Global risk parameters (risk authority)
    WithdrawParameters {
        previous: WithdrawConfig,
        new: WithdrawConfig,
    },
    WithdrawRateLimits {
        previous: WithdrawRateLimitConfig,
        new: WithdrawRateLimitConfig,
    },

    // Trader parameters (risk authority)
    TraderCapability {
        trader: Pubkey,
        previous_flags: TraderCapabilityFlags,
        new_flags: TraderCapabilityFlags,
    },

    // Market authority parameters
    FundingParameters {
        previous: FundingConfig,
        new: FundingConfig,
    },

    // Root authority parameters
    OpenInterestAdjustment {
        previous_open_interest: u64,
        new_open_interest: u64,
    },
}

impl fmt::Display for AdminParameterUpdateKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CancelRiskFactor { previous, new } => {
                write!(f, "CancelRiskFactor({previous} -> {new})")
            }
            Self::IsolatedOnly { previous, new } => {
                write!(f, "IsolatedOnly({previous} -> {new})")
            }
            Self::LeverageTiers { previous, new } => {
                write!(f, "LeverageTiers({previous:?} -> {new:?})")
            }
            Self::MarkPriceParameters { previous, new } => {
                write!(f, "MarkPriceParameters({previous:?} -> {new:?})")
            }
            Self::OpenInterestCap { previous, new } => {
                write!(f, "OpenInterestCap({previous} -> {new})")
            }
            Self::UpnlRiskFactor { previous, new } => {
                write!(f, "UpnlRiskFactor({previous} -> {new})")
            }
            Self::UpnlRiskFactorForWithdrawals { previous, new } => {
                write!(f, "UpnlRiskFactorForWithdrawals({previous} -> {new})")
            }
            Self::WithdrawParameters { previous, new } => {
                write!(f, "WithdrawParameters({previous:?} -> {new:?})")
            }
            Self::WithdrawRateLimits { previous, new } => {
                write!(f, "WithdrawRateLimits({previous:?} -> {new:?})")
            }
            Self::TraderCapability {
                trader,
                previous_flags,
                new_flags,
            } => {
                write!(
                    f,
                    "TraderCapability(trader={trader:?}, {previous_flags:?} -> {new_flags:?})"
                )
            }
            Self::FundingParameters { previous, new } => {
                write!(f, "FundingParameters({previous:?} -> {new:?})")
            }
            Self::OpenInterestAdjustment {
                previous_open_interest,
                new_open_interest,
            } => {
                write!(
                    f,
                    "OpenInterestAdjustment({previous_open_interest} -> {new_open_interest})"
                )
            }
        }
    }
}

/// Event emitted when admin parameters are updated
#[derive(Copy, Clone, BorshDeserialize, BorshSerialize, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdminParameterUpdatedEvent {
    /// The authority that made the change
    pub authority: Pubkey,
    /// Symbol of the perp asset (if applicable)
    pub asset_symbol: Option<Symbol>,
    /// ID of the perp asset (if applicable)
    pub asset_id: Option<u32>,
    /// The type of parameter update
    pub update_kind: AdminParameterUpdateKind,
}

// Escrow events
////////////////////////////////////////////////////////////////////////////////////////////////

/// Escrow account was created for a trader.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EscrowAccountCreatedEvent {
    pub authority: Pubkey,
    pub capacity: u64,
}

/// Escrow request was created.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EscrowRequestCreatedEvent {
    pub receiver_authority: Pubkey,
    pub sender_authority: Pubkey,
    pub sequence_number: u64,
    pub sender_pda_index: u8,
    pub sender_subaccount_index: u8,
    pub receiver_pda_index: u8,
    pub receiver_subaccount_index: u8,
    pub expiration_offset: u32, // expiration_offset from initial_slot (0 = no expiration)
    pub initial_slot: u64,
    pub actions: [EscrowAction; 4],
}

/// Escrow request was accepted and actions were executed.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EscrowRequestAcceptedEvent {
    pub receiver_authority: Pubkey,
    pub sequence_number: u64,
}

/// Reason why an escrow request was cancelled.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EscrowRequestCancelReason {
    /// Request expired (e.g. removed when receiver tried to accept after
    /// last_valid_slot).
    Expiration,
    /// Sender cancelled the request.
    CancelledBySender,
    /// Receiver cancelled the request.
    CancelledByReceiver,
}

/// Escrow request was cancelled.
#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EscrowRequestCancelledEvent {
    pub receiver_authority: Pubkey,
    pub sequence_number: u64,
    pub reason: EscrowRequestCancelReason,
}
