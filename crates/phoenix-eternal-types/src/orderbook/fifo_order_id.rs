//! FIFO Order ID type.

use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};

use crate::quantities::Ticks;

/// Order ID for FIFO orderbook orders.
///
/// The ordering is determined by price (in ticks) and sequence number.
/// Bids and asks use inverted sequence numbers to achieve proper ordering.
#[repr(C)]
#[derive(Default, Copy, Clone, PartialEq, Eq, Pod, Zeroable, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FIFOOrderId {
    /// The price of the order, in ticks.
    pub price_in_ticks: Ticks,

    /// Unique identifier derived from the market's sequence number.
    /// For bids, this is inverted (~sequence_number).
    /// For asks, this is the raw sequence number.
    pub order_sequence_number: u64,
}

impl FIFOOrderId {
    pub fn new(price_in_ticks: impl Into<Ticks>, order_sequence_number: u64) -> Self {
        Self {
            price_in_ticks: price_in_ticks.into(),
            order_sequence_number,
        }
    }

    /// Returns the side of this order based on the sequence number.
    /// Leading bit 0 = Ask, leading bit 1 = Bid.
    pub fn side(&self) -> Side {
        Side::from_order_sequence_number(self.order_sequence_number)
    }

    /// Returns the raw sequence number (without side encoding).
    pub fn raw_sequence_number(&self) -> u64 {
        match self.side() {
            Side::Bid => !self.order_sequence_number,
            Side::Ask => self.order_sequence_number,
        }
    }
}

impl std::fmt::Debug for FIFOOrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FIFOOrderId(price={}, seq={}, side={:?})",
            self.price_in_ticks.as_inner(),
            self.raw_sequence_number(),
            self.side()
        )
    }
}

impl PartialOrd for FIFOOrderId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FIFOOrderId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_side = self.side();
        let other_side = other.side();

        // Cross-side comparison should never happen in a well-formed orderbook
        if self_side != other_side {
            // If it does happen, sort asks before bids
            return match self_side {
                Side::Ask => std::cmp::Ordering::Less,
                Side::Bid => std::cmp::Ordering::Greater,
            };
        }

        // Same side comparison
        let (tick_cmp, seq_cmp) = match self_side {
            Side::Bid => (
                other.price_in_ticks.cmp(&self.price_in_ticks),
                other.order_sequence_number.cmp(&self.order_sequence_number),
            ),
            Side::Ask => (
                self.price_in_ticks.cmp(&other.price_in_ticks),
                self.order_sequence_number.cmp(&other.order_sequence_number),
            ),
        };

        if tick_cmp == std::cmp::Ordering::Equal {
            seq_cmp
        } else {
            tick_cmp
        }
    }
}

/// Order side (bid or ask).
#[derive(Debug, Copy, Clone, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    /// Determine side from order sequence number.
    /// Leading bit 1 = Bid, leading bit 0 = Ask.
    pub fn from_order_sequence_number(seq: u64) -> Self {
        if seq & (1 << 63) != 0 {
            Side::Bid
        } else {
            Side::Ask
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifo_order_id_side() {
        // Ask has leading bit 0
        let ask = FIFOOrderId::new(100u64, 1);
        assert_eq!(ask.side(), Side::Ask);

        // Bid has leading bit 1 (inverted sequence number)
        let bid = FIFOOrderId::new(100u64, !1u64);
        assert_eq!(bid.side(), Side::Bid);
    }

    #[test]
    fn test_fifo_order_id_ordering() {
        // Asks: lower price first, then lower sequence number
        let ask1 = FIFOOrderId::new(100u64, 1);
        let ask2 = FIFOOrderId::new(100u64, 2);
        let ask3 = FIFOOrderId::new(101u64, 1);

        assert!(ask1 < ask2); // Same price, lower seq first
        assert!(ask1 < ask3); // Lower price first

        // Bids: higher price first, then earlier order (higher inverted seq)
        let bid1 = FIFOOrderId::new(100u64, !1u64);
        let bid2 = FIFOOrderId::new(100u64, !2u64);
        let bid3 = FIFOOrderId::new(101u64, !1u64);

        assert!(bid1 < bid2); // Same price, earlier order first (higher inverted seq = lower raw seq)
        assert!(bid3 < bid1); // Higher price first
    }
}
