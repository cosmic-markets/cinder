//! FIFO Resting Order type.

use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};

use crate::quantities::BaseLots;

use super::types::{NodePointer, OptionalNonZeroU32, TraderPositionId};

/// Flags for a resting order.
#[repr(C)]
#[derive(Default, Copy, Clone, PartialEq, Eq, Pod, Zeroable, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrderFlags {
    flags: u8,
}

impl OrderFlags {
    const REDUCE_ONLY_BIT: u8 = 1 << 7;
    const IS_STOP_LOSS_BIT: u8 = 1 << 6;

    pub fn from_bits(flags: u8) -> Self {
        Self { flags }
    }

    pub fn new() -> Self {
        Self { flags: 0 }
    }

    pub fn as_u8(&self) -> u8 {
        self.flags
    }

    pub fn is_reduce_only(&self) -> bool {
        self.flags & Self::REDUCE_ONLY_BIT != 0
    }

    pub fn is_stop_loss(&self) -> bool {
        self.flags & Self::IS_STOP_LOSS_BIT != 0
    }
}

impl std::fmt::Debug for OrderFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OrderFlags(reduce_only={}, stop_loss={})",
            self.is_reduce_only(),
            self.is_stop_loss()
        )
    }
}

/// A resting order in the FIFO orderbook.
#[repr(C)]
#[derive(Default, Copy, Clone, Pod, Zeroable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FIFORestingOrder {
    trader_position_id: TraderPositionId,
    initial_trade_size: BaseLots,
    num_base_lots_remaining: BaseLots,
    order_flags: OrderFlags,
    _padding: [u8; 3],
    expiration_offset: OptionalNonZeroU32,
    initial_slot: u64,
    prev: NodePointer,
    next: NodePointer,
}

impl FIFORestingOrder {
    /// Get the trader position ID for this order.
    pub fn trader_position_id(&self) -> TraderPositionId {
        self.trader_position_id
    }

    /// Get the initial trade size.
    pub fn initial_trade_size(&self) -> BaseLots {
        self.initial_trade_size
    }

    /// Get the number of base lots remaining to be filled.
    pub fn num_base_lots_remaining(&self) -> BaseLots {
        self.num_base_lots_remaining
    }

    /// Get the order flags.
    pub fn order_flags(&self) -> OrderFlags {
        self.order_flags
    }

    /// Check if this order is reduce-only.
    pub fn is_reduce_only(&self) -> bool {
        self.order_flags.is_reduce_only()
    }

    /// Check if this order is a stop-loss order.
    pub fn is_stop_loss(&self) -> bool {
        self.order_flags.is_stop_loss()
    }

    /// Get the initial slot when the order was placed.
    pub fn initial_slot(&self) -> u64 {
        self.initial_slot
    }

    /// Get the last valid slot for this order (None if no expiration).
    pub fn last_valid_slot(&self) -> Option<u64> {
        self.expiration_offset
            .map(|offset| self.initial_slot.saturating_add(offset.get() as u64))
    }

    /// Check if the order is expired at the given slot.
    pub fn is_expired(&self, current_slot: u64) -> bool {
        self.last_valid_slot()
            .map(|last_valid| current_slot > last_valid)
            .unwrap_or(false)
    }

    /// Get the previous order pointer in the linked list.
    pub fn prev(&self) -> NodePointer {
        self.prev
    }

    /// Get the next order pointer in the linked list.
    pub fn next(&self) -> NodePointer {
        self.next
    }
}

impl std::fmt::Debug for FIFORestingOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FIFORestingOrder")
            .field("trader_position_id", &self.trader_position_id)
            .field("initial_trade_size", &self.initial_trade_size.as_inner())
            .field(
                "num_base_lots_remaining",
                &self.num_base_lots_remaining.as_inner(),
            )
            .field("order_flags", &self.order_flags)
            .field("initial_slot", &self.initial_slot)
            .field("last_valid_slot", &self.last_valid_slot())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifo_resting_order_size() {
        // Verify the struct size matches expectations
        // trader_position_id: 8 + initial_trade_size: 8 + num_base_lots_remaining: 8 +
        // order_flags: 1 + _padding: 3 + expiration_offset: 4 + initial_slot: 8 +
        // prev: 4 + next: 4 = 48 bytes
        assert_eq!(std::mem::size_of::<FIFORestingOrder>(), 48);
    }

    #[test]
    fn test_order_flags() {
        let flags = OrderFlags::from_bits(0x80); // reduce_only set
        assert!(flags.is_reduce_only());
        assert!(!flags.is_stop_loss());

        let flags = OrderFlags::from_bits(0xC0); // both set
        assert!(flags.is_reduce_only());
        assert!(flags.is_stop_loss());
    }
}
