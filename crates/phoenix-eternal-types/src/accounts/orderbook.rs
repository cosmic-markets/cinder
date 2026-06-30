//! Orderbook account types.

use bytemuck::{Pod, Zeroable};
use solana_pubkey::Pubkey;

use crate::orderbook::{FIFOOrderId, FIFORestingOrder};
use crate::quantities::{AssetIndex, SequenceNumber, SignedBaseLots, Symbol, Ticks};
use crate::sokoban::{StaticOrderedListMap, StaticOrderedListMapPod};

/// Capacity of the orderbook (orders per side).
pub const ORDERBOOK_CAPACITY: usize = 8192;

/// Capacity of the trade list (recent trades).
pub const TRADE_LIST_CAPACITY: usize = 64;

/// A trade event in the orderbook.
#[repr(C)]
#[derive(Default, Copy, Clone, Pod, Zeroable)]
pub struct TradeEvent {
    pub slot: u64,
    pub timestamp: i64,
    pub trade_sequence_number: u64,
    pub prev_trade_sequence_number_slot: u64,
    pub maker_fill: OrderFill,
    pub maker: Pubkey,
}

impl std::fmt::Debug for TradeEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TradeEvent")
            .field("slot", &self.slot)
            .field("timestamp", &self.timestamp)
            .field("trade_sequence_number", &self.trade_sequence_number)
            .field("maker_fill", &self.maker_fill)
            .field("maker", &self.maker)
            .finish()
    }
}

/// An order fill.
#[repr(C)]
#[derive(Default, Copy, Clone, Pod, Zeroable)]
pub struct OrderFill {
    pub price: Ticks,
    pub quantity: SignedBaseLots,
}

impl std::fmt::Debug for OrderFill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderFill")
            .field("price", &self.price.as_inner())
            .field("quantity", &self.quantity.as_inner())
            .finish()
    }
}

/// Circular buffer for storing recent trades.
///
/// This matches the on-chain CircBuf layout with ptr (u32) and len (u32).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CircBuf {
    /// Points to the start of the buffer (oldest element).
    ptr: u32,
    /// Number of elements in the buffer.
    len: u32,
    /// The data buffer.
    buf: [TradeEvent; TRADE_LIST_CAPACITY],
}

impl Default for CircBuf {
    fn default() -> Self {
        Self {
            ptr: 0,
            len: 0,
            buf: [TradeEvent::default(); TRADE_LIST_CAPACITY],
        }
    }
}

impl CircBuf {
    /// Get the number of elements in the buffer.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get the capacity of the buffer.
    pub fn capacity(&self) -> usize {
        TRADE_LIST_CAPACITY
    }

    /// Get an element by index (0 = oldest).
    pub fn get(&self, idx: usize) -> Option<&TradeEvent> {
        if idx >= self.len() {
            None
        } else {
            let real_idx = (self.ptr as usize + idx) % TRADE_LIST_CAPACITY;
            Some(&self.buf[real_idx])
        }
    }

    /// Iterate over elements from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &TradeEvent> {
        (0..self.len as usize).map(move |i| {
            let idx = ((self.ptr as usize) + i) % TRADE_LIST_CAPACITY;
            &self.buf[idx]
        })
    }
}

impl std::fmt::Debug for CircBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircBuf")
            .field("ptr", &self.ptr)
            .field("len", &self.len)
            .finish()
    }
}

/// Orderbook header.
///
/// Size: 5504 bytes
///
/// Layout (verified against mainnet):
/// - offset 0: discriminant (8)
/// - offset 8: market_status (1) + base_lots_decimals (1) + padding (6)
/// - offset 16: sequence_number (8)
/// - offset 24: order_sequence_number (8)
/// - offset 32: asset_id (4) + padding (4)
/// - offset 40: asset_symbol (16)
/// - offset 56: tick_size_in_quote_lots_per_base_lot (8)
/// - offset 64: trade_sequence_number (8)
/// - offset 72: _reserved (8)
/// - offset 80: _reserved (8)
/// - offset 88: _reserved (8)
/// - offset 96: _padding1 (256)
/// - offset 352: trade_list (5152)
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct OrderbookHeader {
    pub discriminant: u64,

    pub market_status: u8,

    /// The number of decimals for the base lot.
    pub base_lots_decimals: i8,

    _padding0: [u8; 6],

    pub sequence_number: SequenceNumber,

    /// The sequence number of the next order.
    pub order_sequence_number: SequenceNumber,

    /// The ID of the asset being traded in the market.
    pub asset_id: AssetIndex,
    _asset_id_padding: [u8; 4],

    /// The symbol of the asset being traded in the market.
    pub asset_symbol: Symbol,

    /// Tick size in quote lots per base lot.
    pub tick_size_in_quote_lots_per_base_lot: u64,

    /// The sequence number of the next trade.
    pub trade_sequence_number: SequenceNumber,

    /// Reserved bytes.
    _padding1a: [u64; 32],
    _padding1b: [u64; 3],

    /// List of recent trades.
    pub trade_list: CircBuf,
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<OrderbookHeader>(), 5504);

impl OrderbookHeader {
    /// Get the market status.
    pub fn market_status(&self) -> u8 {
        self.market_status
    }

    /// Get the base lots decimals.
    pub fn base_lots_decimals(&self) -> i8 {
        self.base_lots_decimals
    }

    /// Get the asset ID.
    pub fn asset_id(&self) -> AssetIndex {
        self.asset_id
    }

    /// Get the asset symbol.
    pub fn asset_symbol(&self) -> &Symbol {
        &self.asset_symbol
    }

    /// Get the tick size in quote lots per base lot.
    pub fn tick_size_in_quote_lots_per_base_lot(&self) -> u64 {
        self.tick_size_in_quote_lots_per_base_lot
    }

    /// Get the sequence number.
    pub fn sequence_number(&self) -> SequenceNumber {
        self.sequence_number
    }

    /// Get the trade sequence number.
    pub fn trade_sequence_number(&self) -> SequenceNumber {
        self.trade_sequence_number
    }
}

impl std::fmt::Debug for OrderbookHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderbookHeader")
            .field("discriminant", &self.discriminant)
            .field("market_status", &self.market_status)
            .field("base_lots_decimals", &self.base_lots_decimals)
            .field("sequence_number", &self.sequence_number)
            .field("asset_id", &self.asset_id)
            .field("asset_symbol", &self.asset_symbol)
            .field(
                "tick_size_in_quote_lots_per_base_lot",
                &self.tick_size_in_quote_lots_per_base_lot,
            )
            .field("order_sequence_number", &self.order_sequence_number)
            .field("trade_sequence_number", &self.trade_sequence_number)
            .finish()
    }
}

/// Full orderbook with header and order trees.
pub struct Orderbook<'a> {
    pub header: &'a OrderbookHeader,
    pub bids_tree: StaticOrderedListMap<'a, FIFOOrderId, FIFORestingOrder, ORDERBOOK_CAPACITY>,
    pub asks_tree: StaticOrderedListMap<'a, FIFOOrderId, FIFORestingOrder, ORDERBOOK_CAPACITY>,
}

impl<'a> Orderbook<'a> {
    /// Load orderbook from a buffer (read-only).
    ///
    /// # Safety
    /// The buffer must contain valid orderbook data with the correct layout.
    pub fn load_from_buffer(data: &'a mut [u8]) -> Self {
        let (header_bytes, book_bytes) = data.split_at_mut(std::mem::size_of::<OrderbookHeader>());
        let (bids_tree_bytes, asks_tree_bytes) = book_bytes.split_at_mut(std::mem::size_of::<
            StaticOrderedListMapPod<FIFOOrderId, FIFORestingOrder, ORDERBOOK_CAPACITY>,
        >());

        let header = bytemuck::from_bytes::<OrderbookHeader>(header_bytes);
        let bids_tree = StaticOrderedListMap::load_from_buffer(bids_tree_bytes);
        let asks_tree = StaticOrderedListMap::load_from_buffer(asks_tree_bytes);

        Self {
            header,
            bids_tree,
            asks_tree,
        }
    }

    /// Check if the orderbook has no resting orders.
    pub fn has_no_orders(&self) -> bool {
        self.bids_tree.is_empty() && self.asks_tree.is_empty()
    }

    /// Get the number of bids.
    pub fn num_bids(&self) -> usize {
        self.bids_tree.len()
    }

    /// Get the number of asks.
    pub fn num_asks(&self) -> usize {
        self.asks_tree.len()
    }
}
