//! Minimal read-only sokoban implementation for orderbook and tree access.
//!
//! This is a simplified, read-only version of the sokoban data structures.
//! It provides only the functionality needed to deserialize and iterate over:
//! - Orderbook entries (StaticOrderedListMap)
//! - Red-black tree entries (ConstDynamicRedBlackTree)

use core::cmp::Ordering;
use core::fmt::Debug;
use core::marker::PhantomData;

use bytemuck::{Pod, Zeroable};

/// Sentinel value indicating null/end of list.
pub const SENTINEL: u32 = 0;

/// Register index for the previous node pointer (ordered list).
const PREV_REG: usize = 0;
/// Register index for the next node pointer (ordered list).
const NEXT_REG: usize = 1;

/// Register indices for red-black tree nodes.
const RB_LEFT: usize = 0;
const RB_RIGHT: usize = 1;

// ============================================================================
// Node - Generic arena node with configurable registers
// ============================================================================

/// A node in the arena allocator with configurable number of registers.
///
/// Each node contains:
/// - Registers for pointers (left/right/parent/color or prev/next)
/// - The actual value data
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Node<T: Copy + Clone + Pod + Zeroable + Default, const NUM_REGISTERS: usize> {
    /// Registers for pointers.
    registers: [u32; NUM_REGISTERS],
    /// The stored value.
    value: T,
}

unsafe impl<T: Copy + Clone + Pod + Zeroable + Default, const NUM_REGISTERS: usize> Zeroable
    for Node<T, NUM_REGISTERS>
{
}
unsafe impl<T: Copy + Clone + Pod + Zeroable + Default, const NUM_REGISTERS: usize> Pod
    for Node<T, NUM_REGISTERS>
{
}

impl<T: Copy + Clone + Pod + Zeroable + Default, const NUM_REGISTERS: usize> Default
    for Node<T, NUM_REGISTERS>
{
    fn default() -> Self {
        Self {
            registers: [SENTINEL; NUM_REGISTERS],
            value: T::default(),
        }
    }
}

impl<T: Copy + Clone + Pod + Zeroable + Default, const NUM_REGISTERS: usize>
    Node<T, NUM_REGISTERS>
{
    /// Get a register value.
    #[inline(always)]
    pub fn get_register(&self, r: usize) -> u32 {
        self.registers[r]
    }

    /// Get the stored value.
    #[inline(always)]
    pub fn get_value(&self) -> &T {
        &self.value
    }
}

/// Type alias for ordered list nodes (2 registers: prev, next).
pub type OrderedListNode<T> = Node<T, 2>;

/// Type alias for red-black tree nodes (4 registers: left, right, parent, color).
pub type RedBlackTreeNode<T> = Node<T, 4>;

// ============================================================================
// KVNode - Key-value pair node
// ============================================================================

/// A key-value node stored in the ordered list.
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct KVNode<K, V>
where
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
    pub key: K,
    pub value: V,
}

unsafe impl<K, V> Zeroable for KVNode<K, V>
where
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
}

unsafe impl<K, V> Pod for KVNode<K, V>
where
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
}

// ============================================================================
// SimpleNodeAllocator - Arena allocator for static containers
// ============================================================================

/// A simple arena-based node allocator for ordered list maps.
///
/// The allocator manages a fixed-size array of nodes.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct SimpleNodeAllocator<T: Default + Copy + Clone + Pod + Zeroable, const MAX_SIZE: usize> {
    /// Number of allocated nodes.
    pub size: u64,
    /// Bump index for new allocations.
    bump_index: u32,
    /// Head of the free list.
    free_list_head: u32,
    /// The node array.
    pub nodes: [OrderedListNode<T>; MAX_SIZE],
}

unsafe impl<T: Default + Copy + Clone + Pod + Zeroable, const MAX_SIZE: usize> Zeroable
    for SimpleNodeAllocator<T, MAX_SIZE>
{
}

unsafe impl<T: Default + Copy + Clone + Pod + Zeroable, const MAX_SIZE: usize> Pod
    for SimpleNodeAllocator<T, MAX_SIZE>
{
}

impl<T: Default + Copy + Clone + Pod + Zeroable, const MAX_SIZE: usize>
    SimpleNodeAllocator<T, MAX_SIZE>
{
    /// Get the number of allocated nodes.
    #[inline(always)]
    pub fn size(&self) -> usize {
        self.size as usize
    }

    /// Get a node by index (1-indexed).
    #[inline(always)]
    pub fn get(&self, i: u32) -> &OrderedListNode<T> {
        &self.nodes[(i - 1) as usize]
    }
}

// ============================================================================
// Superblock - Header for multi-arena allocator
// ============================================================================

/// Superblock header for multi-arena node allocator.
///
/// This is the first 32 bytes of the first buffer in a multi-arena structure.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Superblock {
    /// Size of the allocator (number of allocated nodes).
    pub size: u32,
    /// Number of registered arenas.
    pub num_arenas: u16,
    /// Number of active (non-empty) arenas.
    pub num_active_arenas: u16,
    /// Number of nodes per arena.
    pub num_nodes_per_arena: u32,
    /// Bump index for allocations.
    pub bump_index: u32,
    /// Head of the free list.
    pub free_list_head: u32,
    /// Padding.
    _padding: [u32; 3],
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<Superblock>(), 32);

// ============================================================================
// ConstMultiArenaNodeAllocator - Read-only multi-arena allocator
// ============================================================================

/// Read-only multi-arena node allocator.
///
/// This spans multiple buffers (Solana accounts) to support large data structures.
pub struct ConstMultiArenaNodeAllocator<
    'a,
    T: Default + Copy + Clone + Pod + Zeroable,
    const NUM_REGISTERS: usize,
> {
    /// The superblock containing allocator metadata.
    pub superblock: &'a Superblock,
    /// The arena slices containing the nodes.
    pub arenas: Vec<&'a [Node<T, NUM_REGISTERS>]>,
}

impl<'a, T: Default + Copy + Clone + Pod + Zeroable, const NUM_REGISTERS: usize>
    ConstMultiArenaNodeAllocator<'a, T, NUM_REGISTERS>
{
    /// Create from pre-parsed superblock and arena slices.
    pub fn new(superblock: &'a Superblock, arenas: Vec<&'a [Node<T, NUM_REGISTERS>]>) -> Self {
        Self { superblock, arenas }
    }

    /// Load from raw byte buffers.
    ///
    /// Buffer layout:
    /// - Buffer 0: | Superblock (32 bytes) | Header | Arena 0 |
    /// - Buffer 1+: | Arena N |
    ///
    /// The `header_size` parameter is the size of the header that follows the superblock.
    pub fn from_buffers(
        superblock_buffer: &'a [u8],
        arena_buffers: impl Iterator<Item = &'a [u8]>,
    ) -> Self {
        let mut arenas = Vec::new();
        for buf in arena_buffers {
            if !buf.is_empty() {
                arenas.push(bytemuck::cast_slice::<u8, Node<T, NUM_REGISTERS>>(buf));
            }
        }

        let superblock = bytemuck::from_bytes::<Superblock>(
            &superblock_buffer[..std::mem::size_of::<Superblock>()],
        );

        Self::new(superblock, arenas)
    }

    /// Convert a 1-indexed node index to (arena_index, node_index_within_arena).
    #[inline(always)]
    fn index_conv(&self, i: u32) -> (usize, usize) {
        let page_no = i - 1;
        let block_no = page_no / self.superblock.num_nodes_per_arena;
        let node_no = page_no % self.superblock.num_nodes_per_arena;
        (block_no as usize, node_no as usize)
    }

    /// Get the number of allocated nodes.
    #[inline(always)]
    pub fn size(&self) -> usize {
        self.superblock.size as usize
    }

    /// Get the total capacity across all arenas.
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.arenas.iter().map(|a| a.len()).sum::<usize>()
    }

    /// Get a node by index (1-indexed).
    #[inline(always)]
    pub fn get(&self, i: u32) -> &Node<T, NUM_REGISTERS> {
        let (arena_index, node_index) = self.index_conv(i);
        &self.arenas[arena_index][node_index]
    }

    /// Get a register value for a node.
    #[inline(always)]
    pub fn get_register(&self, i: u32, r: u32) -> u32 {
        if i != SENTINEL {
            self.get(i).get_register(r as usize)
        } else {
            SENTINEL
        }
    }
}

// ============================================================================
// RedBlackTreeHeader - Tree header
// ============================================================================

/// Header for a red-black tree.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct RedBlackTreeHeader<K, V>
where
    K: Debug + PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
    /// Root node index.
    pub root: u32,
    _padding: [u32; 3],
    _phantom: PhantomData<(K, V)>,
}

unsafe impl<K, V> Zeroable for RedBlackTreeHeader<K, V>
where
    K: Debug + PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
}

unsafe impl<K, V> Pod for RedBlackTreeHeader<K, V>
where
    K: Debug + PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<RedBlackTreeHeader<u64, u64>>(), 16);

// ============================================================================
// ConstDynamicRedBlackTree - Read-only red-black tree
// ============================================================================

/// Read-only view into a dynamic red-black tree.
///
/// This provides iteration and lookup for red-black trees backed by
/// a multi-arena allocator. Used for GlobalTraderIndex and ActiveTraderBuffer.
pub struct ConstDynamicRedBlackTree<
    'a,
    K: Debug + PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
> {
    /// The tree header containing the root pointer.
    pub header: &'a RedBlackTreeHeader<K, V>,
    /// The allocator containing the nodes.
    pub allocator: ConstMultiArenaNodeAllocator<'a, KVNode<K, V>, 4>,
}

impl<
        'a,
        K: Debug + PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
        V: Default + Copy + Clone + Pod + Zeroable,
    > ConstDynamicRedBlackTree<'a, K, V>
{
    /// Load from byte buffers.
    ///
    /// Buffer layout:
    /// - Buffer 0: | Superblock (32 bytes) | RedBlackTreeHeader (16 bytes) | Arena 0 |
    /// - Buffer 1+: | Arena N |
    pub fn load_from_buffers(mut buffers: impl Iterator<Item = &'a [u8]>) -> Self {
        let first_buf = buffers.next().expect("At least one buffer is required");

        let superblock_size = std::mem::size_of::<Superblock>();
        let header_size = std::mem::size_of::<RedBlackTreeHeader<K, V>>();

        let (superblock_buf, rest) = first_buf.split_at(superblock_size);
        let (header_buf, first_arena_buf) = rest.split_at(header_size);

        let superblock = bytemuck::from_bytes::<Superblock>(superblock_buf);
        let header = bytemuck::from_bytes::<RedBlackTreeHeader<K, V>>(header_buf);

        // Collect remaining arenas
        let mut arenas: Vec<&'a [Node<KVNode<K, V>, 4>]> = Vec::new();
        if !first_arena_buf.is_empty() {
            arenas.push(bytemuck::cast_slice(first_arena_buf));
        }
        for buf in buffers {
            if !buf.is_empty() {
                arenas.push(bytemuck::cast_slice(buf));
            }
        }

        let allocator = ConstMultiArenaNodeAllocator::new(superblock, arenas);

        Self { header, allocator }
    }

    /// Get the number of elements in the tree.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.allocator.size()
    }

    /// Check if the tree is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the root node index.
    #[inline(always)]
    pub fn root(&self) -> u32 {
        self.header.root
    }

    /// Get the left child index of a node.
    #[inline(always)]
    fn get_left(&self, node: u32) -> u32 {
        self.allocator.get_register(node, RB_LEFT as u32)
    }

    /// Get the right child index of a node.
    #[inline(always)]
    fn get_right(&self, node: u32) -> u32 {
        self.allocator.get_register(node, RB_RIGHT as u32)
    }

    /// Get the node value.
    #[inline(always)]
    fn get_node(&self, node: u32) -> &KVNode<K, V> {
        self.allocator.get(node).get_value()
    }

    /// Get a value by key.
    pub fn get(&self, key: &K) -> Option<&V> {
        let addr = self.get_addr(key);
        if addr == SENTINEL {
            None
        } else {
            Some(&self.get_node(addr).value)
        }
    }

    /// Get the node address for a key.
    pub fn get_addr(&self, key: &K) -> u32 {
        let mut node_index = self.header.root;
        if node_index == SENTINEL {
            return SENTINEL;
        }
        loop {
            let curr_key = self.get_node(node_index).key;
            let target = match key.cmp(&curr_key) {
                Ordering::Less => self.get_left(node_index),
                Ordering::Greater => self.get_right(node_index),
                Ordering::Equal => return node_index,
            };
            if target == SENTINEL {
                return SENTINEL;
            }
            node_index = target;
        }
    }

    /// Get a key-value pair by node pointer.
    pub fn get_node_from_pointer(&self, addr: u32) -> Option<(&K, &V)> {
        if addr == SENTINEL {
            None
        } else {
            let node = self.get_node(addr);
            Some((&node.key, &node.value))
        }
    }

    /// Check if the tree contains a key.
    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Iterate over all key-value pairs in sorted order.
    pub fn iter(&self) -> RedBlackTreeIterator<'_, K, V> {
        RedBlackTreeIterator {
            tree: self,
            stack: Vec::new(),
            current: self.header.root,
            remaining: self.len(),
        }
    }
}

// ============================================================================
// RedBlackTreeIterator - In-order tree iterator
// ============================================================================

/// In-order iterator over a red-black tree.
pub struct RedBlackTreeIterator<
    'a,
    K: Debug + PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
> {
    tree: &'a ConstDynamicRedBlackTree<'a, K, V>,
    stack: Vec<u32>,
    current: u32,
    remaining: usize,
}

impl<
        'a,
        K: Debug + PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
        V: Default + Copy + Clone + Pod + Zeroable,
    > Iterator for RedBlackTreeIterator<'a, K, V>
{
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        // Morris-style in-order traversal without parent pointers
        while self.current != SENTINEL {
            self.stack.push(self.current);
            self.current = self.tree.get_left(self.current);
        }

        if let Some(node_idx) = self.stack.pop() {
            let node = self.tree.get_node(node_idx);
            self.current = self.tree.get_right(node_idx);
            self.remaining -= 1;
            Some((&node.key, &node.value))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<
        K: Debug + PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
        V: Default + Copy + Clone + Pod + Zeroable,
    > ExactSizeIterator for RedBlackTreeIterator<'_, K, V>
{
}

// ============================================================================
// OrderedListMapHeader - List header
// ============================================================================

/// Header for an ordered list map.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct OrderedListMapHeader<K, V>
where
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
    /// Head of the linked list.
    pub head: u32,
    /// Tail of the linked list.
    pub tail: u32,
    _padding: [u32; 2],
    _phantom: PhantomData<(K, V)>,
}

unsafe impl<K, V> Zeroable for OrderedListMapHeader<K, V>
where
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
}

unsafe impl<K, V> Pod for OrderedListMapHeader<K, V>
where
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
{
}

// ============================================================================
// StaticOrderedListMapPod - Combined header + allocator
// ============================================================================

/// The Pod (plain old data) representation of a static ordered list map.
///
/// This is the actual on-disk/on-chain layout combining the header
/// and allocator.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct StaticOrderedListMapPod<
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
    const MAX_SIZE: usize,
> {
    pub header: OrderedListMapHeader<K, V>,
    pub allocator: SimpleNodeAllocator<KVNode<K, V>, MAX_SIZE>,
}

unsafe impl<
        K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
        V: Default + Copy + Clone + Pod + Zeroable,
        const MAX_SIZE: usize,
    > Zeroable for StaticOrderedListMapPod<K, V, MAX_SIZE>
{
}

unsafe impl<
        K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
        V: Default + Copy + Clone + Pod + Zeroable,
        const MAX_SIZE: usize,
    > Pod for StaticOrderedListMapPod<K, V, MAX_SIZE>
{
}

// ============================================================================
// StaticOrderedListMap - Read-only wrapper
// ============================================================================

/// A read-only view into a static ordered list map.
///
/// This provides iteration and access to the ordered key-value pairs
/// without any mutation capabilities.
pub struct StaticOrderedListMap<
    'a,
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
    const MAX_SIZE: usize,
> {
    inner: &'a StaticOrderedListMapPod<K, V, MAX_SIZE>,
}

impl<
        'a,
        K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
        V: Default + Copy + Clone + Pod + Zeroable,
        const MAX_SIZE: usize,
    > StaticOrderedListMap<'a, K, V, MAX_SIZE>
{
    /// Load from a byte buffer.
    pub fn load_from_buffer(data: &'a mut [u8]) -> Self {
        let inner = bytemuck::from_bytes::<StaticOrderedListMapPod<K, V, MAX_SIZE>>(
            &data[..std::mem::size_of::<StaticOrderedListMapPod<K, V, MAX_SIZE>>()],
        );
        Self { inner }
    }

    /// Get the number of elements.
    pub fn len(&self) -> usize {
        self.inner.allocator.size()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the head index.
    pub fn get_head_index(&self) -> u32 {
        self.inner.header.head
    }

    /// Get the tail index.
    pub fn get_tail_index(&self) -> u32 {
        self.inner.header.tail
    }

    /// Get the next node index.
    pub fn get_next(&self, index: u32) -> u32 {
        if index == SENTINEL || index == 0 {
            SENTINEL
        } else {
            self.inner.allocator.get(index).get_register(NEXT_REG)
        }
    }

    /// Get the previous node index.
    pub fn get_prev(&self, index: u32) -> u32 {
        if index == SENTINEL || index == 0 {
            SENTINEL
        } else {
            self.inner.allocator.get(index).get_register(PREV_REG)
        }
    }

    /// Get key-value pair by node index.
    pub fn get_by_index(&self, index: u32) -> Option<(&K, &V)> {
        if index == SENTINEL || index == 0 {
            None
        } else {
            let node_data = self.inner.allocator.get(index).get_value();
            Some((&node_data.key, &node_data.value))
        }
    }

    /// Iterate over all key-value pairs in order.
    pub fn iter(&self) -> StaticOrderedListMapIterator<'_, K, V, MAX_SIZE> {
        StaticOrderedListMapIterator {
            map: self,
            current: self.inner.header.head,
            remaining: self.len(),
        }
    }
}

// ============================================================================
// Iterator
// ============================================================================

/// Iterator over a static ordered list map.
pub struct StaticOrderedListMapIterator<
    'a,
    K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
    V: Default + Copy + Clone + Pod + Zeroable,
    const MAX_SIZE: usize,
> {
    map: &'a StaticOrderedListMap<'a, K, V, MAX_SIZE>,
    current: u32,
    remaining: usize,
}

impl<
        'a,
        K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
        V: Default + Copy + Clone + Pod + Zeroable,
        const MAX_SIZE: usize,
    > Iterator for StaticOrderedListMapIterator<'a, K, V, MAX_SIZE>
{
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 || self.current == SENTINEL {
            return None;
        }

        let node_data = self.map.inner.allocator.get(self.current).get_value();
        let result = (&node_data.key, &node_data.value);

        self.current = self.map.get_next(self.current);
        self.remaining -= 1;

        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<
        K: PartialOrd + Ord + Copy + Clone + Default + Pod + Zeroable,
        V: Default + Copy + Clone + Pod + Zeroable,
        const MAX_SIZE: usize,
    > ExactSizeIterator for StaticOrderedListMapIterator<'_, K, V, MAX_SIZE>
{
}
