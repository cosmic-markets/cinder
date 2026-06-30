//! SplineCollection account types.
//!
//! This module provides read-only types for deserializing SplineCollection accounts
//! which contain automated market maker (AMM) spline data.

use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use solana_pubkey::Pubkey;

use crate::quantities::{
    BaseLots, BaseLotsPerTickU32, BaseLotsU32, BasisPointsU32, SequenceNumber, Slot, Symbol, Ticks,
};

/// Maximum number of regions per side in a spline.
pub const SPLINE_REGION_CAPACITY: usize = 10;

/// GTC (Good-'til-Cancelled) lifespan value.
pub const GTC_LIFESPAN: Slot = u64::MAX;

// ============================================================================
// TickRegion - On-chain tick region representation
// ============================================================================

/// A tick region in a spline (on-chain representation).
///
/// This represents the stored state of a tick region, including fill state.
/// Different from `TickRegionParams` which is used for instruction parameters.
///
/// Size: 48 bytes
#[repr(C)]
#[derive(Debug, Copy, Clone, Default, Pod, Zeroable, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TickRegion {
    /// Starting offset from mid price in ticks (inclusive).
    pub start_offset: Ticks,
    /// Ending offset from mid price in ticks (exclusive).
    pub end_offset: Ticks,
    /// Liquidity density in base lots per tick.
    density: BaseLotsPerTickU32,
    /// Hidden take size on the top level.
    top_level_hidden_take_size: BaseLotsPerTickU32,
    /// Total size in base lots (calculated from range * density).
    pub total_size: BaseLots,
    /// Amount filled in the current interval.
    filled_size: BaseLotsU32,
    /// Hidden amount filled in the current interval.
    hidden_filled_size: BaseLotsU32,
    /// Lifespan in slots (GTC_LIFESPAN = u64::MAX for good-'til-cancelled).
    pub lifespan: Slot,
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<TickRegion>(), 48);

impl TickRegion {
    /// Check if this region is active (has unfilled capacity and hasn't expired).
    pub fn is_active(&self, current_slot: Slot, last_updated_slot: Slot) -> bool {
        self.lifespan.saturating_add(last_updated_slot) >= current_slot
            && (self.filled_size.as_inner() as u64) < self.total_size.as_inner()
    }

    /// Get the unfilled size in this region.
    pub fn unfilled_size(&self) -> BaseLots {
        BaseLots::new(
            self.total_size
                .as_inner()
                .saturating_sub(self.filled_size.as_inner() as u64),
        )
    }

    /// Check if the region is empty (zero density).
    pub fn is_empty(&self) -> bool {
        self.density.as_inner() == 0
    }

    /// Get the density in base lots per tick.
    pub fn density(&self) -> BaseLotsPerTickU32 {
        self.density
    }

    /// Get the filled size.
    pub fn filled_size(&self) -> BaseLotsU32 {
        self.filled_size
    }

    /// Get the hidden filled size.
    pub fn hidden_filled_size(&self) -> BaseLotsU32 {
        self.hidden_filled_size
    }

    /// Get the top-level hidden take size.
    pub fn top_level_hidden_take_size(&self) -> BaseLotsPerTickU32 {
        self.top_level_hidden_take_size
    }
}

// ============================================================================
// SplineCollectionHeader
// ============================================================================

/// Header for a SplineCollection account.
///
/// Size: 112 bytes
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SplineCollectionHeader {
    pub discriminant: u64,
    /// The market account this spline collection belongs to.
    pub market: Pubkey,
    /// The asset symbol.
    pub asset_symbol: Symbol,
    /// Sequence number with slot tracking.
    pub sequence_number: SequenceNumber,
    /// Number of splines in the collection.
    pub num_splines: u32,
    /// Number of active splines in the collection.
    pub num_active: u32,
    /// Reserved bytes.
    _padding: [u8; 32],
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<SplineCollectionHeader>(), 112);

impl SplineCollectionHeader {
    /// Get the market account pubkey.
    pub fn market(&self) -> &Pubkey {
        &self.market
    }

    /// Get the asset symbol.
    pub fn asset_symbol(&self) -> &Symbol {
        &self.asset_symbol
    }

    /// Get the sequence number.
    pub fn sequence_number(&self) -> &SequenceNumber {
        &self.sequence_number
    }

    /// Get the number of splines in the collection.
    pub fn num_splines(&self) -> u32 {
        self.num_splines
    }

    /// Get the number of active splines.
    pub fn num_active(&self) -> u32 {
        self.num_active
    }

    /// Check if there are no active splines.
    pub fn has_no_active_splines(&self) -> bool {
        self.num_active == 0
    }
}

impl std::fmt::Debug for SplineCollectionHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SplineCollectionHeader")
            .field("discriminant", &self.discriminant)
            .field("market", &self.market)
            .field("asset_symbol", &self.asset_symbol)
            .field("sequence_number", &self.sequence_number)
            .field("num_splines", &self.num_splines)
            .field("num_active", &self.num_active)
            .finish()
    }
}

// ============================================================================
// Spline
// ============================================================================

/// A single spline in a SplineCollection.
///
/// Size: 1336 bytes
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Spline {
    /// The trader account that owns this spline.
    trader: Pubkey,
    /// Whether this spline is active (1) or inactive (0).
    is_active: u8,
    _padding: [u8; 7],
    /// The mid price in ticks.
    mid_price: Ticks,
    /// Sequence number with slot tracking.
    sequence_number: SequenceNumber,
    /// The slot when the user last updated this spline.
    user_update_slot: Slot,
    /// Current offset into bid regions (regions before this are exhausted).
    bid_offset: u64,
    /// Current offset into ask regions (regions before this are exhausted).
    ask_offset: u64,
    /// Total filled amount on bid side.
    bid_filled_amount: BaseLots,
    /// Total filled amount on ask side.
    ask_filled_amount: BaseLots,
    /// Number of bid regions configured.
    bid_num_regions: u64,
    /// Number of ask regions configured.
    ask_num_regions: u64,
    /// Bid-side tick regions.
    bid_regions: [TickRegion; SPLINE_REGION_CAPACITY],
    /// Ask-side tick regions.
    ask_regions: [TickRegion; SPLINE_REGION_CAPACITY],
    /// User-provided sequence number for anti-reordering protection (price updates).
    user_price_sequence_number: u64,
    /// User-provided sequence number for anti-reordering protection (parameter updates).
    user_parameter_sequence_number: u64,
    /// Flags bitfield.
    flags: u32,
    /// Leverage decrease in basis points.
    leverage_decrease_in_bps: BasisPointsU32,
    /// Maximum position size on the long side.
    max_position_size_long: BaseLotsU32,
    /// Maximum position size on the short side.
    max_position_size_short: BaseLotsU32,
    _padding2: [u64; 28],
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<Spline>(), 1336);

impl Spline {
    /// Get the trader pubkey that owns this spline.
    pub fn trader(&self) -> &Pubkey {
        &self.trader
    }

    /// Check if this spline is active.
    pub fn is_active(&self) -> bool {
        self.is_active == 1
    }

    /// Check if this spline is enabled (active and has a mid price).
    pub fn is_enabled(&self) -> bool {
        self.is_active() && self.mid_price.as_inner() != 0
    }

    /// Get the mid price in ticks.
    pub fn mid_price(&self) -> Ticks {
        self.mid_price
    }

    /// Get the sequence number.
    pub fn sequence_number(&self) -> &SequenceNumber {
        &self.sequence_number
    }

    /// Get the user update slot.
    pub fn user_update_slot(&self) -> Slot {
        self.user_update_slot
    }

    /// Get the bid offset (regions before this are exhausted).
    pub fn bid_offset(&self) -> u64 {
        self.bid_offset
    }

    /// Get the ask offset (regions before this are exhausted).
    pub fn ask_offset(&self) -> u64 {
        self.ask_offset
    }

    /// Get the total filled amount on the bid side.
    pub fn bid_filled_amount(&self) -> BaseLots {
        self.bid_filled_amount
    }

    /// Get the total filled amount on the ask side.
    pub fn ask_filled_amount(&self) -> BaseLots {
        self.ask_filled_amount
    }

    /// Get the number of bid regions configured.
    pub fn bid_num_regions(&self) -> u64 {
        self.bid_num_regions
    }

    /// Get the number of ask regions configured.
    pub fn ask_num_regions(&self) -> u64 {
        self.ask_num_regions
    }

    /// Get the bid regions.
    pub fn bid_regions(&self) -> &[TickRegion; SPLINE_REGION_CAPACITY] {
        &self.bid_regions
    }

    /// Get the ask regions.
    pub fn ask_regions(&self) -> &[TickRegion; SPLINE_REGION_CAPACITY] {
        &self.ask_regions
    }

    /// Get the active bid regions (from current offset to num_regions).
    pub fn active_bid_regions(&self) -> &[TickRegion] {
        let start = self.bid_offset as usize;
        let end = self.bid_num_regions as usize;
        if start < end && end <= SPLINE_REGION_CAPACITY {
            &self.bid_regions[start..end]
        } else {
            &[]
        }
    }

    /// Get the active ask regions (from current offset to num_regions).
    pub fn active_ask_regions(&self) -> &[TickRegion] {
        let start = self.ask_offset as usize;
        let end = self.ask_num_regions as usize;
        if start < end && end <= SPLINE_REGION_CAPACITY {
            &self.ask_regions[start..end]
        } else {
            &[]
        }
    }

    /// Get the user price sequence number (for anti-reordering protection).
    pub fn user_price_sequence_number(&self) -> u64 {
        self.user_price_sequence_number
    }

    /// Get the user parameter sequence number (for anti-reordering protection).
    pub fn user_parameter_sequence_number(&self) -> u64 {
        self.user_parameter_sequence_number
    }

    /// Get the flags bitfield.
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Get the leverage decrease in basis points.
    pub fn leverage_decrease_in_bps(&self) -> BasisPointsU32 {
        self.leverage_decrease_in_bps
    }

    /// Get the maximum position size on the long side.
    pub fn max_position_size_long(&self) -> BaseLotsU32 {
        self.max_position_size_long
    }

    /// Get the maximum position size on the short side.
    pub fn max_position_size_short(&self) -> BaseLotsU32 {
        self.max_position_size_short
    }

    /// Check if this spline has configured regions on either side.
    pub fn has_configured_regions(&self) -> bool {
        self.bid_offset < self.bid_num_regions || self.ask_offset < self.ask_num_regions
    }
}

impl Default for Spline {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl std::fmt::Debug for Spline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spline")
            .field("trader", &self.trader)
            .field("is_active", &self.is_active())
            .field("mid_price", &self.mid_price)
            .field("sequence_number", &self.sequence_number)
            .field("user_update_slot", &self.user_update_slot)
            .field("bid_offset", &self.bid_offset)
            .field("ask_offset", &self.ask_offset)
            .field("bid_filled_amount", &self.bid_filled_amount)
            .field("ask_filled_amount", &self.ask_filled_amount)
            .field("bid_num_regions", &self.bid_num_regions)
            .field("ask_num_regions", &self.ask_num_regions)
            .field(
                "user_price_sequence_number",
                &self.user_price_sequence_number,
            )
            .field(
                "user_parameter_sequence_number",
                &self.user_parameter_sequence_number,
            )
            .field("flags", &self.flags)
            .field("leverage_decrease_in_bps", &self.leverage_decrease_in_bps)
            .field("max_position_size_long", &self.max_position_size_long)
            .field("max_position_size_short", &self.max_position_size_short)
            .finish()
    }
}

// ============================================================================
// SplineCollectionRef - Read-only view
// ============================================================================

/// Read-only view into a SplineCollection account.
pub struct SplineCollectionRef<'a> {
    pub header: &'a SplineCollectionHeader,
    pub splines: &'a [Spline],
}

impl<'a> SplineCollectionRef<'a> {
    /// Load from a buffer (read-only).
    pub fn load_from_buffer(data: &'a [u8]) -> Self {
        let header_size = std::mem::size_of::<SplineCollectionHeader>();
        let spline_size = std::mem::size_of::<Spline>();

        let header = bytemuck::from_bytes::<SplineCollectionHeader>(&data[..header_size]);

        let splines_start = header_size;
        let num_splines = header.num_splines as usize;
        let splines_end = splines_start + num_splines * spline_size;

        let splines = if splines_end <= data.len() {
            bytemuck::cast_slice::<u8, Spline>(&data[splines_start..splines_end])
        } else {
            &[]
        };

        Self { header, splines }
    }

    /// Get the number of splines.
    pub fn len(&self) -> usize {
        self.splines.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.splines.is_empty()
    }

    /// Get a spline by index.
    pub fn get(&self, index: usize) -> Option<&Spline> {
        self.splines.get(index)
    }

    /// Find a spline by trader pubkey.
    pub fn find_by_trader(&self, trader: &Pubkey) -> Option<&Spline> {
        self.splines.iter().find(|s| s.trader() == trader)
    }

    /// Iterate over all splines.
    pub fn iter(&self) -> impl Iterator<Item = &Spline> {
        self.splines.iter()
    }

    /// Iterate over active splines only.
    pub fn active_splines(&self) -> impl Iterator<Item = &Spline> {
        self.splines.iter().filter(|s| s.is_active())
    }

    /// Iterate over enabled splines only (active with mid price set).
    pub fn enabled_splines(&self) -> impl Iterator<Item = &Spline> {
        self.splines.iter().filter(|s| s.is_enabled())
    }
}

impl std::fmt::Debug for SplineCollectionRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SplineCollectionRef")
            .field("header", &self.header)
            .field("num_splines", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_region_size() {
        assert_eq!(std::mem::size_of::<TickRegion>(), 48);
    }

    #[test]
    fn test_spline_collection_header_size() {
        assert_eq!(std::mem::size_of::<SplineCollectionHeader>(), 112);
    }

    #[test]
    fn test_spline_size() {
        assert_eq!(std::mem::size_of::<Spline>(), 1336);
    }
}
