//! Trader account types.

use bytemuck::{Pod, Zeroable};
use solana_pubkey::Pubkey;

use crate::orderbook::NodePointer;
use crate::quantities::{
    AssetIndex, BaseLots, SequenceNumber, SequenceNumberU8, SignedBaseLots, SignedQuoteLots,
    SignedQuoteLotsI56, SignedQuoteLotsPerBaseLot, Slot, TraderCapabilityFlags,
};

// ============================================================================
// TraderState - Core trader state
// ============================================================================

/// Trader state containing collateral and fee information.
///
/// Size: 16 bytes
#[repr(C)]
#[derive(Copy, Clone, Default, Pod, Zeroable)]
pub struct TraderState {
    quote_lot_collateral: SignedQuoteLots,

    /// Capability flags (stored as u32, matches TraderCapabilityFlags).
    flags: u32,

    _padding: u8,

    /// Global position sequence number.
    global_position_sequence_number: u8,

    /// Maker fee adjustment: (adjustment + 10) / 10 multiplier.
    maker_fee_override_multiplier: i8,

    /// Taker fee adjustment: (adjustment + 10) / 10 multiplier.
    taker_fee_override_multiplier: i8,
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<TraderState>(), 16);

impl TraderState {
    /// Get the quote lot collateral.
    pub fn collateral(&self) -> SignedQuoteLots {
        self.quote_lot_collateral
    }

    /// Get the global position sequence number.
    pub fn global_position_sequence_number(&self) -> u8 {
        self.global_position_sequence_number
    }

    /// Get the raw flags value.
    pub fn flags_raw(&self) -> u32 {
        self.flags
    }

    /// Get decoded trader capability flags.
    pub fn flags(&self) -> TraderCapabilityFlags {
        TraderCapabilityFlags::new(self.flags)
    }

    /// Get the maker fee multiplier tenths ((adjustment + 10) gives multiplier in tenths).
    pub fn maker_fee_multiplier_tenths(&self) -> i32 {
        self.maker_fee_override_multiplier as i32 + 10
    }

    /// Get the taker fee multiplier tenths.
    pub fn taker_fee_multiplier_tenths(&self) -> i32 {
        self.taker_fee_override_multiplier as i32 + 10
    }

    /// Get the raw maker fee override multiplier.
    pub fn maker_fee_override_multiplier(&self) -> i8 {
        self.maker_fee_override_multiplier
    }

    /// Get the raw taker fee override multiplier.
    pub fn taker_fee_override_multiplier(&self) -> i8 {
        self.taker_fee_override_multiplier
    }
}

impl std::fmt::Debug for TraderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraderState")
            .field("collateral", &self.quote_lot_collateral)
            .field("flags", &self.flags())
            .field(
                "global_position_sequence_number",
                &self.global_position_sequence_number,
            )
            .field(
                "maker_fee_override_multiplier",
                &self.maker_fee_override_multiplier,
            )
            .field(
                "taker_fee_override_multiplier",
                &self.taker_fee_override_multiplier,
            )
            .finish()
    }
}

// ============================================================================
// TraderPosition - Position in a single market
// ============================================================================

/// A trader's position in a single market.
#[repr(C)]
#[derive(Copy, Clone, Default, Pod, Zeroable)]
pub struct TraderPosition {
    pub base_lot_position: SignedBaseLots,
    pub virtual_quote_lot_position: SignedQuoteLots,
    pub cumulative_funding_snapshot: SignedQuoteLotsPerBaseLot,
    pub position_sequence_number: SequenceNumberU8,
    pub accumulated_funding_for_active_position: SignedQuoteLotsI56,
}

impl TraderPosition {
    /// Get the base lot position (positive = long, negative = short).
    pub fn base_lot_position(&self) -> SignedBaseLots {
        self.base_lot_position
    }

    /// Get the virtual quote lot position.
    pub fn virtual_quote_lot_position(&self) -> SignedQuoteLots {
        self.virtual_quote_lot_position
    }

    /// Get the cumulative funding snapshot.
    pub fn cumulative_funding_snapshot(&self) -> SignedQuoteLotsPerBaseLot {
        self.cumulative_funding_snapshot
    }

    /// Get the position sequence number.
    pub fn position_sequence_number(&self) -> SequenceNumberU8 {
        self.position_sequence_number
    }

    /// Check if the position is neutral (no base lots).
    pub fn is_neutral(&self) -> bool {
        self.base_lot_position == SignedBaseLots::ZERO
    }

    /// Check if the position is long.
    pub fn is_long(&self) -> bool {
        self.base_lot_position.as_inner() > 0
    }

    /// Check if the position is short.
    pub fn is_short(&self) -> bool {
        self.base_lot_position.as_inner() < 0
    }
}

impl std::fmt::Debug for TraderPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraderPosition")
            .field("base_lot_position", &self.base_lot_position)
            .field(
                "virtual_quote_lot_position",
                &self.virtual_quote_lot_position,
            )
            .field(
                "cumulative_funding_snapshot",
                &self.cumulative_funding_snapshot,
            )
            .field("position_sequence_number", &self.position_sequence_number)
            .finish()
    }
}

// ============================================================================
// LimitOrderLinkedListState - Order tracking
// ============================================================================

/// State for tracking a trader's limit orders on one side.
#[repr(C)]
#[derive(Copy, Clone, Default, Pod, Zeroable)]
pub struct LimitOrderLinkedListState {
    pub size: u32,
    pub head: NodePointer,
}

impl LimitOrderLinkedListState {
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn len(&self) -> u32 {
        self.size
    }
}

impl std::fmt::Debug for LimitOrderLinkedListState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LimitOrderLinkedListState")
            .field("size", &self.size)
            .field("head", &self.head)
            .finish()
    }
}

// ============================================================================
// TraderPositionState - Full position state including orders
// ============================================================================

/// Full position state including open order tracking.
#[repr(C)]
#[derive(Copy, Clone, Default, Pod, Zeroable)]
pub struct TraderPositionState {
    pub position: TraderPosition,
    pub ask_orders: LimitOrderLinkedListState,
    pub bid_orders: LimitOrderLinkedListState,
    pub total_non_reduce_only_ask_base_lots: BaseLots,
    pub total_non_reduce_only_bid_base_lots: BaseLots,
}

impl TraderPositionState {
    /// Check if the trader has any open orders.
    pub fn has_open_orders(&self) -> bool {
        self.ask_orders.size > 0 || self.bid_orders.size > 0
    }

    /// Check if the position state is empty (no position and no orders).
    pub fn is_empty(&self) -> bool {
        self.position.is_neutral() && !self.has_open_orders()
    }

    /// Get the number of ask orders.
    pub fn num_asks(&self) -> u32 {
        self.ask_orders.size
    }

    /// Get the number of bid orders.
    pub fn num_bids(&self) -> u32 {
        self.bid_orders.size
    }

    /// Get the total number of limit orders.
    pub fn num_limit_orders(&self) -> u64 {
        self.ask_orders.size as u64 + self.bid_orders.size as u64
    }
}

impl std::ops::Deref for TraderPositionState {
    type Target = TraderPosition;

    fn deref(&self) -> &Self::Target {
        &self.position
    }
}

impl std::fmt::Debug for TraderPositionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraderPositionState")
            .field("position", &self.position)
            .field("ask_orders", &self.ask_orders)
            .field("bid_orders", &self.bid_orders)
            .field(
                "total_non_reduce_only_ask_base_lots",
                &self.total_non_reduce_only_ask_base_lots,
            )
            .field(
                "total_non_reduce_only_bid_base_lots",
                &self.total_non_reduce_only_bid_base_lots,
            )
            .finish()
    }
}

// ============================================================================
// TraderPositionId - Key for active trader buffer
// ============================================================================

/// Key for trader positions in the active trader buffer.
///
/// # Binary Layout
/// This struct uses a 64-bit key format:
/// - Upper 32 bits: trader_id (NodePointer - index in global trader index)
/// - Lower 32 bits: asset_id (global asset identifier)
///
/// This layout enables efficient range queries for all positions of a trader
/// and maintains consistent ordering in the red-black tree.
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct TraderPositionId {
    trader_id: NodePointer,
    asset_id: AssetIndex,
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<TraderPositionId>(), 8);

impl TraderPositionId {
    /// Create a new TraderPositionId.
    #[inline(always)]
    pub fn new(trader_id: impl Into<NodePointer>, asset_id: AssetIndex) -> Self {
        Self {
            trader_id: trader_id.into(),
            asset_id,
        }
    }

    /// Create from a packed 64-bit value.
    /// Upper 32 bits = trader_id, lower 32 bits = asset_id
    #[inline(always)]
    pub fn from_u64(value: u64) -> Self {
        Self {
            trader_id: NodePointer::new((value >> 32) as u32),
            asset_id: AssetIndex::new((value & 0xFFFFFFFF) as u32),
        }
    }

    /// Check if uninitialized.
    #[inline(always)]
    pub fn is_uninitialized(&self) -> bool {
        self.trader_id.is_null()
    }

    /// Get the trader ID.
    #[inline(always)]
    pub fn trader_id(&self) -> NodePointer {
        self.trader_id
    }

    /// Get the asset ID.
    #[inline(always)]
    pub fn asset_id(&self) -> AssetIndex {
        self.asset_id
    }
}

// ============================================================================
// DynamicTraderHeader - Trader account header
// ============================================================================

/// Header for a dynamic trader account.
///
/// Size: 224 bytes
///
/// Layout (verified against mainnet):
/// - offset 0: discriminant (8)
/// - offset 8: sequence_number (8)
/// - offset 16: _reserved (8) - legacy field, unused
/// - offset 24: key (32)
/// - offset 56: authority (32)
/// - offset 88: trader_state (16)
/// - offset 104: _padding0 (4)
/// - offset 108: withdraw_queue_node (4)
/// - offset 112: max_positions (8)
/// - offset 120: position_authority (32)
/// - offset 152: num_markets_with_splines (2)
/// - offset 154: trader_pda_index (1)
/// - offset 155: trader_subaccount_index (1)
/// - offset 156: funding_key (32)
/// - offset 188: _padding1 (4)
/// - offset 192: last_deposit_slot (8)
/// - offset 200: _padding2 (24)
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct DynamicTraderHeader {
    pub discriminant: u64,
    pub sequence_number: SequenceNumber,
    pub key: Pubkey,
    pub authority: Pubkey,
    pub trader_state: TraderState,
    _padding0: [u8; 4],
    pub withdraw_queue_node: NodePointer,
    pub max_positions: u64,
    pub position_authority: Pubkey,
    pub num_markets_with_splines: u16,
    pub trader_pda_index: u8,
    pub trader_subaccount_index: u8,
    pub funding_key: Pubkey,
    _padding1: [u8; 4],
    pub last_deposit_slot: Slot,
    _padding2: [u8; 24],
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<DynamicTraderHeader>(), 224);

impl DynamicTraderHeader {
    /// Get the discriminant.
    pub fn discriminant(&self) -> u64 {
        self.discriminant
    }

    /// Get the sequence number.
    pub fn sequence_number(&self) -> SequenceNumber {
        self.sequence_number
    }

    /// Get the trader account key.
    pub fn key(&self) -> &Pubkey {
        &self.key
    }

    /// Get the authority pubkey.
    pub fn authority(&self) -> &Pubkey {
        &self.authority
    }

    /// Get the trader state.
    pub fn trader_state(&self) -> &TraderState {
        &self.trader_state
    }

    /// Get the position authority pubkey.
    pub fn position_authority(&self) -> &Pubkey {
        &self.position_authority
    }

    /// Get the maximum number of positions.
    pub fn max_positions(&self) -> u64 {
        self.max_positions
    }

    /// Get the trader PDA index.
    pub fn trader_pda_index(&self) -> u8 {
        self.trader_pda_index
    }

    /// Get the trader subaccount index.
    pub fn trader_subaccount_index(&self) -> u8 {
        self.trader_subaccount_index
    }

    /// Get the last deposit slot.
    pub fn last_deposit_slot(&self) -> Slot {
        self.last_deposit_slot
    }

    /// Check if the trader has a pending withdrawal request.
    pub fn has_pending_withdrawal(&self) -> bool {
        !self.withdraw_queue_node.is_null()
    }

    /// Get the collateral balance.
    pub fn collateral(&self) -> SignedQuoteLots {
        self.trader_state.collateral()
    }
}

impl std::fmt::Debug for DynamicTraderHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicTraderHeader")
            .field("discriminant", &self.discriminant)
            .field("sequence_number", &self.sequence_number)
            .field("key", &self.key)
            .field("authority", &self.authority)
            .field("trader_state", &self.trader_state)
            .field("max_positions", &self.max_positions)
            .field("position_authority", &self.position_authority)
            .field("trader_pda_index", &self.trader_pda_index)
            .field("trader_subaccount_index", &self.trader_subaccount_index)
            .field("last_deposit_slot", &self.last_deposit_slot)
            .field("has_pending_withdrawal", &self.has_pending_withdrawal())
            .finish()
    }
}
