//! Active Trader Buffer account types.

use bytemuck::{Pod, Zeroable};

use crate::accounts::trader::{TraderPositionId, TraderPositionState};
use crate::quantities::ExchangeSequenceNumber;
use crate::sokoban::{ConstDynamicRedBlackTree, Superblock};

/// Active trader buffer header.
///
/// Size: 48 bytes
///
/// Layout:
/// - offset 0: discriminant (8)
/// - offset 8: sequence_number (16) - ExchangeSequenceNumber
/// - offset 24: num_additional_nodes (4)
/// - offset 28: _padding0 (4)
/// - offset 32: _padding1 (16)
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ActiveTraderBufferHeader {
    pub discriminant: u64,
    pub sequence_number: ExchangeSequenceNumber,
    pub num_additional_nodes: u32,
    _padding0: [u8; 4],
    _padding1: [u8; 16],
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<ActiveTraderBufferHeader>(), 48);

impl ActiveTraderBufferHeader {
    pub fn discriminant(&self) -> u64 {
        self.discriminant
    }

    pub fn sequence_number(&self) -> &ExchangeSequenceNumber {
        &self.sequence_number
    }

    pub fn num_additional_nodes(&self) -> u32 {
        self.num_additional_nodes
    }
}

impl std::fmt::Debug for ActiveTraderBufferHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveTraderBufferHeader")
            .field("discriminant", &self.discriminant)
            .field("sequence_number", &self.sequence_number)
            .field("num_additional_nodes", &self.num_additional_nodes)
            .finish()
    }
}

/// Arena header for active trader buffer overflow accounts.
///
/// Size: 32 bytes
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ActiveTraderBufferArenaHeader {
    pub discriminant: u64,
    pub index: u16,
    _padding0: [u8; 6],
    _padding1: [u8; 16],
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<ActiveTraderBufferArenaHeader>(), 32);

impl ActiveTraderBufferArenaHeader {
    pub fn discriminant(&self) -> u64 {
        self.discriminant
    }

    pub fn index(&self) -> u16 {
        self.index
    }
}

impl std::fmt::Debug for ActiveTraderBufferArenaHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveTraderBufferArenaHeader")
            .field("discriminant", &self.discriminant)
            .field("index", &self.index)
            .finish()
    }
}

/// Read-only wrapper for the active trader buffer.
///
/// This provides access to the header and raw arena data.
/// For tree iteration, use the sokoban crate directly with the raw buffers.
pub struct ActiveTraderBuffer<'a> {
    pub header: &'a ActiveTraderBufferHeader,
    /// Raw data after the header (red-black tree arena)
    raw_data: &'a [u8],
}

impl<'a> ActiveTraderBuffer<'a> {
    /// Load from a buffer (read-only).
    pub fn load_from_buffer(data: &'a [u8]) -> Self {
        let header_size = std::mem::size_of::<ActiveTraderBufferHeader>();
        let header = bytemuck::from_bytes::<ActiveTraderBufferHeader>(&data[..header_size]);
        let raw_data = &data[header_size..];

        Self { header, raw_data }
    }

    /// Get the header.
    pub fn header(&self) -> &ActiveTraderBufferHeader {
        self.header
    }

    /// Get the raw tree data for external parsing.
    pub fn raw_tree_data(&self) -> &[u8] {
        self.raw_data
    }

    /// Get the number of additional arena nodes.
    pub fn num_additional_nodes(&self) -> u32 {
        self.header.num_additional_nodes
    }
}

impl std::fmt::Debug for ActiveTraderBuffer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveTraderBuffer")
            .field("header", &self.header)
            .field("raw_data_len", &self.raw_data.len())
            .finish()
    }
}

// ============================================================================
// ActiveTraderBufferTree - Red-black tree view
// ============================================================================

/// Read-only view into the active trader buffer red-black tree.
///
/// The tree maps TraderPositionId -> TraderPositionState, allowing lookup
/// of position state by (trader_id, asset_id) and iteration over all positions.
///
/// Buffer layout:
/// - Buffer 0: | ActiveTraderBufferHeader (48 bytes) | Superblock (32 bytes) | RedBlackTreeHeader (16 bytes) | Arena 0 |
/// - Buffer 1+: | ActiveTraderBufferArenaHeader (32 bytes) | Arena N |
pub struct ActiveTraderBufferTree<'a> {
    /// The account header.
    pub header: &'a ActiveTraderBufferHeader,
    /// The red-black tree.
    pub tree: ConstDynamicRedBlackTree<'a, TraderPositionId, TraderPositionState>,
}

impl<'a> ActiveTraderBufferTree<'a> {
    /// Load from multiple byte buffers.
    ///
    /// The first buffer must contain the ActiveTraderBufferHeader followed by the tree data.
    /// Additional buffers contain arena overflow accounts (with arena headers stripped).
    pub fn load_from_buffers(mut buffers: impl Iterator<Item = &'a [u8]>) -> Self {
        let first_buf = buffers.next().expect("At least one buffer is required");

        let header_size = std::mem::size_of::<ActiveTraderBufferHeader>();
        let header = bytemuck::from_bytes::<ActiveTraderBufferHeader>(&first_buf[..header_size]);

        // The tree data starts after the account header
        let tree_buf = &first_buf[header_size..];

        // Strip arena headers from additional buffers
        let arena_header_size = std::mem::size_of::<ActiveTraderBufferArenaHeader>();
        let arena_buffers = buffers.map(move |buf| &buf[arena_header_size..]);

        // Create tree from buffers
        let tree_buffers = std::iter::once(tree_buf).chain(arena_buffers);
        let tree = ConstDynamicRedBlackTree::load_from_buffers(tree_buffers);

        Self { header, tree }
    }

    /// Get the number of positions in the buffer.
    pub fn len(&self) -> usize {
        self.tree.len()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    /// Get the superblock info for debugging.
    pub fn superblock(&self) -> &Superblock {
        self.tree.allocator.superblock
    }

    /// Look up a position by id.
    pub fn get(&self, id: &TraderPositionId) -> Option<&TraderPositionState> {
        self.tree.get(id)
    }

    /// Check if a position id is in the buffer.
    pub fn contains(&self, id: &TraderPositionId) -> bool {
        self.tree.contains(id)
    }

    /// Iterate over all (position_id, position_state) pairs in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = (&TraderPositionId, &TraderPositionState)> {
        self.tree.iter()
    }
}

impl std::fmt::Debug for ActiveTraderBufferTree<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveTraderBufferTree")
            .field("header", &self.header)
            .field("tree_len", &self.tree.len())
            .field("superblock", &self.superblock())
            .finish()
    }
}
