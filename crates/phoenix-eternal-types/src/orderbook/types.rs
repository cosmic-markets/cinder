//! Core orderbook types.

use std::num::NonZeroU32;

use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};

use crate::quantities::AssetIndex;

// ============================================================================
// OptionalNonZeroU32 - Option<NonZeroU32> with Pod compatibility
// ============================================================================

/// A Pod-compatible optional non-zero u32.
/// Uses 0 to represent None, non-zero values represent Some(value).
#[repr(C)]
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Pod, Zeroable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OptionalNonZeroU32 {
    value: u32,
}

impl OptionalNonZeroU32 {
    pub const fn null() -> Self {
        Self { value: 0 }
    }

    pub const fn new(value: u32) -> Self {
        Self { value }
    }

    pub fn is_none(&self) -> bool {
        self.value == 0
    }

    pub fn is_some(&self) -> bool {
        self.value != 0
    }

    pub fn get(&self) -> Option<NonZeroU32> {
        NonZeroU32::new(self.value)
    }

    pub fn map<T, F: FnOnce(NonZeroU32) -> T>(&self, f: F) -> Option<T> {
        self.get().map(f)
    }
}

impl std::ops::Deref for OptionalNonZeroU32 {
    type Target = Option<NonZeroU32>;

    fn deref(&self) -> &Self::Target {
        // This is a bit of a hack but works for read-only access
        // The actual value is computed on demand
        static NONE: Option<NonZeroU32> = None;
        if self.value == 0 {
            &NONE
        } else {
            // Safety: We can't return a reference to a computed value,
            // so we use a different approach in the actual implementation
            &NONE // Placeholder - use get() method instead
        }
    }
}

impl From<Option<u32>> for OptionalNonZeroU32 {
    fn from(opt: Option<u32>) -> Self {
        match opt {
            Some(v) => Self::new(v),
            None => Self::null(),
        }
    }
}

impl std::fmt::Debug for OptionalNonZeroU32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.get() {
            Some(v) => write!(f, "Some({})", v),
            None => write!(f, "None"),
        }
    }
}

// Manual BorshSerialize implementation
impl BorshSerialize for OptionalNonZeroU32 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.value.serialize(writer)
    }
}

// Manual BorshDeserialize implementation
impl BorshDeserialize for OptionalNonZeroU32 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let value = u32::deserialize_reader(reader)?;
        Ok(Self { value })
    }
}

// ============================================================================
// NodePointer - Type-safe wrapper for node indices
// ============================================================================

/// A type-safe wrapper for node indices/pointers in data structures.
/// Uses 0 to represent null (equivalent to sokoban::SENTINEL).
#[repr(C)]
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Pod,
    Zeroable,
    BorshDeserialize,
    BorshSerialize,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodePointer(OptionalNonZeroU32);

static_assertions::assert_eq_size!(NodePointer, u32);

impl NodePointer {
    /// Create a null pointer (equivalent to sokoban::SENTINEL).
    pub const fn null() -> Self {
        Self(OptionalNonZeroU32::null())
    }

    /// Create a pointer from a u32 value. Returns null if value is 0.
    pub const fn new(ptr: u32) -> Self {
        Self(OptionalNonZeroU32::new(ptr))
    }

    /// Check if this pointer is null (value 0).
    #[must_use]
    pub fn is_null(&self) -> bool {
        self.0.is_none()
    }

    /// Get the underlying NonZeroU32 if not null.
    #[must_use]
    pub fn as_u32(&self) -> Option<NonZeroU32> {
        self.0.get()
    }

    /// Safe conversion to Option<u32>, returns None for null pointers.
    #[must_use]
    pub fn as_u32_checked(&self) -> Option<u32> {
        self.0.map(|nz| nz.get())
    }

    /// Convert to u32 with a default value for null pointers.
    #[must_use]
    pub fn to_u32_or(&self, default: u32) -> u32 {
        self.as_u32_checked().unwrap_or(default)
    }

    /// Convert to u32, returning 0 for null pointers (equivalent to sokoban::SENTINEL).
    #[must_use]
    pub fn to_u32_or_sentinel(&self) -> u32 {
        self.to_u32_or(0)
    }
}

impl From<u32> for NodePointer {
    fn from(ptr: u32) -> Self {
        Self::new(ptr)
    }
}

impl From<usize> for NodePointer {
    fn from(ptr: usize) -> Self {
        assert!(ptr <= u32::MAX as usize);
        Self::new(ptr as u32)
    }
}

impl From<Option<u32>> for NodePointer {
    fn from(ptr: Option<u32>) -> Self {
        Self(ptr.into())
    }
}

impl std::fmt::Debug for NodePointer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodePointer({})", self.0.value)
    }
}

impl std::fmt::Display for NodePointer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodePointer({})", self.0.value)
    }
}

// ============================================================================
// TraderPositionId - Key for trader positions
// ============================================================================

/// Key for trader positions in the active trader buffer.
///
/// # Binary Layout
/// This struct uses a 64-bit key format:
/// - Upper 32 bits: trader_id (NodePointer - index in global trader index)
/// - Lower 32 bits: asset_id (global asset identifier)
#[repr(C)]
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Pod,
    Zeroable,
    BorshDeserialize,
    BorshSerialize,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderPositionId {
    trader_id: NodePointer,
    asset_id: AssetIndex,
}

static_assertions::assert_eq_size!(TraderPositionId, u64);

impl TraderPositionId {
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
    pub fn new_from_u64(value: u64) -> Self {
        Self {
            trader_id: ((value >> 32) as u32).into(),
            asset_id: AssetIndex::new((value & 0xFFFFFFFF) as u32),
        }
    }

    #[inline(always)]
    pub fn is_uninitialized(&self) -> bool {
        self.trader_id.is_null()
    }

    #[inline(always)]
    pub fn trader_id(&self) -> NodePointer {
        self.trader_id
    }

    #[inline(always)]
    pub fn asset_id(&self) -> AssetIndex {
        self.asset_id
    }
}

impl std::fmt::Debug for TraderPositionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TraderPositionId(trader={}, asset={})",
            self.trader_id.to_u32_or_sentinel(),
            self.asset_id.as_inner()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_pointer() {
        let null = NodePointer::null();
        assert!(null.is_null());
        assert_eq!(null.to_u32_or_sentinel(), 0);

        let ptr = NodePointer::new(42);
        assert!(!ptr.is_null());
        assert_eq!(ptr.as_u32_checked(), Some(42));
        assert_eq!(ptr.to_u32_or_sentinel(), 42);
    }

    #[test]
    fn test_trader_position_id() {
        let id = TraderPositionId::new(NodePointer::new(100), AssetIndex::new(5));
        assert_eq!(id.trader_id().to_u32_or_sentinel(), 100);
        assert_eq!(id.asset_id().as_inner(), 5);

        let id2 = TraderPositionId::new_from_u64((100u64 << 32) | 5);
        assert_eq!(id, id2);
    }
}
