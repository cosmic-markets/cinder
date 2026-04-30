//! Perp Asset Map account types.

use bytemuck::{Pod, Zeroable};

use crate::quantities::{AssetIndex, SequenceNumber, Symbol};

/// Maximum number of perp assets in the map.
pub const MAX_NUMBER_OF_PERP_ASSETS: usize = 1024;

// ============================================================================
// PerpAssetMetadata
// ============================================================================

/// Perp asset metadata (simplified read-only skeleton).
///
/// This structure contains oracle pricing, risk parameters, funding accumulators,
/// and market configuration. Internal details are replaced with opaque padding to
/// maintain binary compatibility while hiding implementation details.
///
/// Size: 1568 bytes
///
/// Key offsets (verified against mainnet):
/// - static_market_params.market_account: offset 912 (32 bytes)
/// - static_market_params.tick_size: offset 944 (8 bytes)
/// - static_market_params.asset_id_lower_bytes: offset 952 (2 bytes)
/// - static_market_params.base_lot_decimals: offset 954 (1 byte)
/// - static_market_params.asset_id_upper_bytes: offset 956 (2 bytes)
/// - open_interest: offset 1424 (8 bytes)
/// - open_interest_cap: offset 1432 (8 bytes)
/// - short_map_metadata.index_num: offset 1448 (2 bytes)
/// - short_map_metadata.is_tombstoned: offset 1450 (1 byte)
#[repr(C)]
#[derive(Copy, Clone)]
pub struct PerpAssetMetadata {
    /// Raw data split into chunks for Pod/Zeroable compatibility.
    /// Total: 196 u64s = 1568 bytes
    _data0: [u64; 32],
    _data1: [u64; 32],
    _data2: [u64; 32],
    _data3: [u64; 32],
    _data4: [u64; 32],
    _data5: [u64; 32],
    _data6: [u64; 4],
}

// Manual Pod/Zeroable implementation since we have complex structure
unsafe impl Pod for PerpAssetMetadata {}
unsafe impl Zeroable for PerpAssetMetadata {}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<PerpAssetMetadata>(), 1568);

// Byte offsets for key fields (verified against mainnet):
// These offsets are relative to the start of PerpAssetMetadata.
const MARK_PRICE_OFFSET: usize = 24;
const MARKET_ACCOUNT_OFFSET: usize = 904;
const TICK_SIZE_OFFSET: usize = 936;
const ASSET_ID_LOWER_OFFSET: usize = 944;
const BASE_LOT_DECIMALS_OFFSET: usize = 946;
const ASSET_ID_UPPER_OFFSET: usize = 948;
const OPEN_INTEREST_OFFSET: usize = 1424;
const OPEN_INTEREST_CAP_OFFSET: usize = 1432;
const SHORT_MAP_METADATA_OFFSET: usize = 1440;

impl PerpAssetMetadata {
    /// Get raw bytes of the metadata for external parsing.
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

    pub fn mark_price(&self) -> u64 {
        let bytes = self.as_bytes();
        let price_bytes: [u8; 8] = bytes[MARK_PRICE_OFFSET..MARK_PRICE_OFFSET + 8]
            .try_into()
            .expect("slice must be 8 bytes");
        u64::from_le_bytes(price_bytes)
    }

    /// Check if this metadata entry is active (not tombstoned).
    ///
    /// Reads the is_tombstoned byte at offset 1450 (SHORT_MAP_METADATA_OFFSET + 2).
    pub fn is_active(&self) -> bool {
        let bytes = self.as_bytes();
        bytes[SHORT_MAP_METADATA_OFFSET + 2] == 0
    }

    /// Get the index number for this entry.
    ///
    /// Reads the index_num u16 at offset 1448 (SHORT_MAP_METADATA_OFFSET).
    pub fn index_num(&self) -> u16 {
        let bytes = self.as_bytes();
        u16::from_le_bytes([
            bytes[SHORT_MAP_METADATA_OFFSET],
            bytes[SHORT_MAP_METADATA_OFFSET + 1],
        ])
    }

    /// Get the market account pubkey for this asset.
    pub fn market_account(&self) -> solana_pubkey::Pubkey {
        let bytes = self.as_bytes();
        let key_bytes: [u8; 32] = bytes[MARKET_ACCOUNT_OFFSET..MARKET_ACCOUNT_OFFSET + 32]
            .try_into()
            .unwrap();
        solana_pubkey::Pubkey::new_from_array(key_bytes)
    }

    /// Get the tick size in quote lots per base lot per tick.
    pub fn tick_size(&self) -> crate::quantities::QuoteLotsPerBaseLotPerTick {
        let bytes = self.as_bytes();
        let val = u64::from_le_bytes(
            bytes[TICK_SIZE_OFFSET..TICK_SIZE_OFFSET + 8]
                .try_into()
                .unwrap(),
        );
        crate::quantities::QuoteLotsPerBaseLotPerTick::new(val)
    }

    /// Get the asset ID.
    pub fn asset_id(&self) -> AssetIndex {
        let bytes = self.as_bytes();
        let lower = u16::from_le_bytes([
            bytes[ASSET_ID_LOWER_OFFSET],
            bytes[ASSET_ID_LOWER_OFFSET + 1],
        ]);
        let upper = u16::from_le_bytes([
            bytes[ASSET_ID_UPPER_OFFSET],
            bytes[ASSET_ID_UPPER_OFFSET + 1],
        ]);
        AssetIndex::new((upper as u32) << 16 | (lower as u32))
    }

    /// Get the base lot decimals.
    pub fn base_lot_decimals(&self) -> i8 {
        let bytes = self.as_bytes();
        bytes[BASE_LOT_DECIMALS_OFFSET] as i8
    }

    /// Get the current open interest in base lots.
    pub fn open_interest(&self) -> crate::quantities::BaseLots {
        let bytes = self.as_bytes();
        let val = u64::from_le_bytes(
            bytes[OPEN_INTEREST_OFFSET..OPEN_INTEREST_OFFSET + 8]
                .try_into()
                .unwrap(),
        );
        crate::quantities::BaseLots::new(val)
    }

    /// Get the open interest cap in base lots.
    pub fn open_interest_cap(&self) -> crate::quantities::BaseLots {
        let bytes = self.as_bytes();
        let val = u64::from_le_bytes(
            bytes[OPEN_INTEREST_CAP_OFFSET..OPEN_INTEREST_CAP_OFFSET + 8]
                .try_into()
                .unwrap(),
        );
        crate::quantities::BaseLots::new(val)
    }
}

impl std::fmt::Debug for PerpAssetMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerpAssetMetadata")
            .field("size", &std::mem::size_of::<Self>())
            .finish()
    }
}

// ============================================================================
// PerpAssetEntry
// ============================================================================

/// A single entry in the perp asset map (symbol + metadata).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct PerpAssetEntry {
    pub symbol: Symbol,
    pub metadata: PerpAssetMetadata,
}

unsafe impl Pod for PerpAssetEntry {}
unsafe impl Zeroable for PerpAssetEntry {}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<PerpAssetEntry>(), 1584);

// ============================================================================
// PerpAssetMap
// ============================================================================

/// Perp asset map header.
///
/// Size: 24 bytes
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PerpAssetMapHeader {
    pub discriminant: u64,
    pub sequence_number: SequenceNumber,
    pub num_assets: u16,
    pub _padding0: [u8; 6],
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<PerpAssetMapHeader>(), 32);

impl PerpAssetMapHeader {
    /// Get the discriminant.
    pub fn discriminant(&self) -> u64 {
        self.discriminant
    }

    /// Get the sequence number.
    pub fn sequence_number(&self) -> SequenceNumber {
        self.sequence_number
    }

    /// Get the number of assets.
    pub fn num_assets(&self) -> u16 {
        self.num_assets
    }
}

impl std::fmt::Debug for PerpAssetMapHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerpAssetMapHeader")
            .field("discriminant", &self.discriminant)
            .field("sequence_number", &self.sequence_number)
            .field("num_assets", &self.num_assets)
            .finish()
    }
}

/// ShortMapV2 internal header.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ShortMapV2Header {
    /// Total slots that have ever been used.
    slots_used: u32,
    /// Number of tombstoned entries.
    tombstones: u32,
    /// Capacity.
    capacity: u64,
}

/// Read-only view into the perp asset map.
pub struct PerpAssetMapRef<'a> {
    pub header: &'a PerpAssetMapHeader,
    short_map_header: &'a ShortMapV2Header,
    entries: &'a [PerpAssetEntry],
}

impl<'a> PerpAssetMapRef<'a> {
    /// Load from a buffer (read-only).
    pub fn load_from_buffer(data: &'a [u8]) -> Self {
        let header_size = std::mem::size_of::<PerpAssetMapHeader>();
        let short_map_header_size = std::mem::size_of::<ShortMapV2Header>();

        let header = bytemuck::from_bytes::<PerpAssetMapHeader>(&data[..header_size]);

        let short_map_start = header_size;
        let short_map_header = bytemuck::from_bytes::<ShortMapV2Header>(
            &data[short_map_start..short_map_start + short_map_header_size],
        );

        let entries_start = short_map_start + short_map_header_size;
        let entry_size = std::mem::size_of::<PerpAssetEntry>();
        let num_entries = (data.len() - entries_start) / entry_size;
        let entries = bytemuck::cast_slice::<u8, PerpAssetEntry>(
            &data[entries_start..entries_start + num_entries * entry_size],
        );

        Self {
            header,
            short_map_header,
            entries,
        }
    }

    /// Get the number of active assets.
    pub fn len(&self) -> usize {
        (self.short_map_header.slots_used - self.short_map_header.tombstones) as usize
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the number of slots used.
    pub fn slots_used(&self) -> usize {
        self.short_map_header.slots_used as usize
    }

    /// Get asset metadata by symbol.
    pub fn get_by_symbol(&self, symbol: &Symbol) -> Option<&PerpAssetMetadata> {
        for i in 0..self.short_map_header.slots_used as usize {
            if i >= self.entries.len() {
                break;
            }
            let entry = &self.entries[i];
            if entry.metadata.is_active() && &entry.symbol == symbol {
                return Some(&entry.metadata);
            }
        }
        None
    }

    /// Get asset metadata by index.
    pub fn get_by_index(&self, index: AssetIndex) -> Option<&PerpAssetMetadata> {
        let index_u32 = index.as_inner();
        let slot = (index_u32 & 0xFFFF) as usize;
        let expected_index_num = (index_u32 >> 16) as u16;

        if slot >= self.entries.len() {
            return None;
        }

        let entry = &self.entries[slot];
        if entry.metadata.is_active() && entry.metadata.index_num() == expected_index_num {
            Some(&entry.metadata)
        } else {
            None
        }
    }

    /// Iterate over all active assets.
    pub fn iter(&self) -> impl Iterator<Item = (&Symbol, &PerpAssetMetadata)> {
        self.entries
            .iter()
            .take(self.short_map_header.slots_used as usize)
            .filter_map(|entry| {
                if entry.metadata.is_active() {
                    Some((&entry.symbol, &entry.metadata))
                } else {
                    None
                }
            })
    }
}

impl std::fmt::Debug for PerpAssetMapRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerpAssetMapRef")
            .field("header", &self.header)
            .field("len", &self.len())
            .field("slots_used", &self.slots_used())
            .finish()
    }
}
