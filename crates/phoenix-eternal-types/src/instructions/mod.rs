//! Instruction builders for Phoenix Eternal spline operations.
//!
//! This module provides types and builder functions for constructing
//! UpdateSplinePrice and UpdateSplineParameters instructions.

use borsh::{BorshDeserialize, BorshSerialize};
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use std::sync::LazyLock;

use crate::discriminant::compute_discriminant;

// ============================================================================
// Instruction Discriminants
// ============================================================================

/// Instruction discriminants computed from SHA256 hashes.
pub mod discriminants {
    use super::*;

    pub static UPDATE_SPLINE_PRICE: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"global:update_spline_price"));

    pub static UPDATE_SPLINE_PARAMETERS: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"global:update_spline_parameters"));

    pub static UPDATE_SPLINE_POSITION_LIMITS_CONFIG: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"global:update_spline_position_limits_config"));
}

// ============================================================================
// TickRegionParams
// ============================================================================

/// Parameters for a tick region in a spline.
///
/// A tick region defines a price range with a specific liquidity density.
/// Regions are defined as offsets from the mid price.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct TickRegionParams {
    /// Starting offset from mid price in ticks.
    pub start_offset: u64,
    /// Ending offset from mid price in ticks (must be > start_offset).
    pub end_offset: u64,
    /// Liquidity density in base lots per tick.
    pub density: u64,
    /// Lifespan of orders in this region (in slots, 0 = GTC).
    pub lifespan: u64,
}

impl TickRegionParams {
    /// Create a new tick region.
    pub fn new(start_offset: u64, end_offset: u64, density: u64, lifespan: u64) -> Self {
        Self {
            start_offset,
            end_offset,
            density,
            lifespan,
        }
    }
}

// ============================================================================
// PositionSizeLimit
// ============================================================================

/// Per-side position-size limits for a spline, in base lots.
#[derive(BorshSerialize, BorshDeserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct PositionSizeLimits {
    pub long: u32,
    pub short: u32,
}

impl PositionSizeLimits {
    pub fn symmetric(size: u32) -> Self {
        Self {
            long: size,
            short: size,
        }
    }
}

/// Describes whether a position-size limit is active on a spline.
/// `Disabled` means no cap; `Limit(limits)` caps the position per-side
/// (`Limit(PositionSizeLimits { long: 0, short: 0 })` is reduce-only mode).
#[derive(BorshSerialize, BorshDeserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub enum PositionSizeLimit {
    Disabled,
    Limit(PositionSizeLimits),
}

// ============================================================================
// UpdateSplinePositionLimitsConfigParams
// ============================================================================

/// Parameters for the UpdateSplinePositionLimitsConfig instruction.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct UpdateSplinePositionLimitsConfigParams {
    /// `None` = don't update. `Some(limit)` = update to the given limit.
    /// When `Limit`, both long and short must be specified.
    pub max_position_size: Option<PositionSizeLimit>,
    /// `None` = don't update. `Some(v)` sets the leverage decrease factor in
    /// basis points (0..=10_000).
    pub leverage_decrease_in_bps: Option<u32>,
}

// ============================================================================
// UpdateSplinePriceParams
// ============================================================================

/// Parameters for the UpdateSplinePrice instruction.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct UpdateSplinePriceParams {
    /// New mid price in ticks.
    pub new_mid_price: u64,
    /// Optional user-provided slot for the update (for staleness tracking).
    pub user_update_slot: Option<u64>,
    /// Whether to rebuild region fill state when updating the price.
    pub refresh_regions: bool,
}

impl UpdateSplinePriceParams {
    /// Create new update spline price params.
    pub fn new(new_mid_price: u64) -> Self {
        Self {
            new_mid_price,
            user_update_slot: None,
            refresh_regions: false,
        }
    }

    /// Set the user update slot.
    pub fn with_user_update_slot(mut self, slot: u64) -> Self {
        self.user_update_slot = Some(slot);
        self
    }

    /// Enable region refresh.
    pub fn with_refresh_regions(mut self) -> Self {
        self.refresh_regions = true;
        self
    }
}

/// Parameters for the UpdateSplinePrice instruction with ordering protection.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct UpdateSplinePriceParamsWithOrdering {
    /// New mid price in ticks.
    pub new_mid_price: u64,
    /// Optional user-provided slot for the update.
    pub user_update_slot: Option<u64>,
    /// Whether to rebuild region fill state when updating the price.
    pub refresh_regions: bool,
    /// User-provided sequence number for anti-reordering protection.
    pub user_sequence_number: u64,
    /// Client order ID for tracking.
    pub client_order_id: [u8; 16],
    /// If true, bypass sequence number validation.
    pub override_sequence_number: bool,
}

impl UpdateSplinePriceParamsWithOrdering {
    /// Create new update spline price params with ordering.
    pub fn new(new_mid_price: u64, user_sequence_number: u64) -> Self {
        Self {
            new_mid_price,
            user_update_slot: None,
            refresh_regions: false,
            user_sequence_number,
            client_order_id: [0u8; 16],
            override_sequence_number: false,
        }
    }

    /// Set the user update slot.
    pub fn with_user_update_slot(mut self, slot: u64) -> Self {
        self.user_update_slot = Some(slot);
        self
    }

    /// Enable region refresh.
    pub fn with_refresh_regions(mut self) -> Self {
        self.refresh_regions = true;
        self
    }

    /// Set the client order ID.
    pub fn with_client_order_id(mut self, client_order_id: [u8; 16]) -> Self {
        self.client_order_id = client_order_id;
        self
    }

    /// Enable sequence number override.
    pub fn with_override_sequence_number(mut self) -> Self {
        self.override_sequence_number = true;
        self
    }
}

// ============================================================================
// UpdateSplineParametersParams
// ============================================================================

/// Parameters for the UpdateSplineParameters instruction.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct UpdateSplineParametersParams {
    /// Bid-side tick regions.
    pub bid_regions: Vec<TickRegionParams>,
    /// Ask-side tick regions.
    pub ask_regions: Vec<TickRegionParams>,
    /// Whether to rebuild region fill state when updating parameters.
    pub refresh_regions: bool,
}

impl UpdateSplineParametersParams {
    /// Create new update spline parameters params.
    pub fn new(bid_regions: Vec<TickRegionParams>, ask_regions: Vec<TickRegionParams>) -> Self {
        Self {
            bid_regions,
            ask_regions,
            refresh_regions: false,
        }
    }

    /// Enable region refresh.
    pub fn with_refresh_regions(mut self) -> Self {
        self.refresh_regions = true;
        self
    }
}

/// Parameters for the UpdateSplineParameters instruction with ordering protection.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct UpdateSplineParametersParamsWithOrdering {
    /// Bid-side tick regions.
    pub bid_regions: Vec<TickRegionParams>,
    /// Ask-side tick regions.
    pub ask_regions: Vec<TickRegionParams>,
    /// Whether to rebuild region fill state when updating parameters.
    pub refresh_regions: bool,
    /// User-provided sequence number for anti-reordering protection.
    pub user_sequence_number: u64,
    /// Client order ID for tracking.
    pub client_order_id: [u8; 16],
    /// If true, bypass sequence number validation.
    pub override_sequence_number: bool,
}

impl UpdateSplineParametersParamsWithOrdering {
    /// Create new update spline parameters params with ordering.
    pub fn new(
        bid_regions: Vec<TickRegionParams>,
        ask_regions: Vec<TickRegionParams>,
        user_sequence_number: u64,
    ) -> Self {
        Self {
            bid_regions,
            ask_regions,
            refresh_regions: false,
            user_sequence_number,
            client_order_id: [0u8; 16],
            override_sequence_number: false,
        }
    }

    /// Enable region refresh.
    pub fn with_refresh_regions(mut self) -> Self {
        self.refresh_regions = true;
        self
    }

    /// Set the client order ID.
    pub fn with_client_order_id(mut self, client_order_id: [u8; 16]) -> Self {
        self.client_order_id = client_order_id;
        self
    }

    /// Enable sequence number override.
    pub fn with_override_sequence_number(mut self) -> Self {
        self.override_sequence_number = true;
        self
    }
}

// ============================================================================
// PDA Derivation
// ============================================================================

/// Derive the log authority PDA for the given program.
pub fn get_log_authority(program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"log"], program_id).0
}

// ============================================================================
// Instruction Builders
// ============================================================================

/// Build an UpdateSplinePrice instruction.
///
/// # Accounts
/// 1. `phoenix_program` - The Phoenix Eternal program ID (read-only)
/// 2. `log_authority` - The log authority PDA (read-only)
/// 3. `signer` - The authority signing the transaction (signer, read-only)
/// 4. `trader` - The trader account (read-only)
/// 5. `spline_collection` - The spline collection account (writable)
pub fn update_spline_price(
    program_id: &Pubkey,
    signer: &Pubkey,
    trader: &Pubkey,
    spline_collection: &Pubkey,
    params: UpdateSplinePriceParams,
) -> Instruction {
    let log_authority = get_log_authority(program_id);

    let accounts = vec![
        AccountMeta::new_readonly(*program_id, false),
        AccountMeta::new_readonly(log_authority, false),
        AccountMeta::new_readonly(*signer, true),
        AccountMeta::new_readonly(*trader, false),
        AccountMeta::new(*spline_collection, false),
    ];

    let mut data = Vec::with_capacity(8 + 32);
    data.extend_from_slice(&discriminants::UPDATE_SPLINE_PRICE.to_le_bytes());
    borsh::to_writer(&mut data, &params).expect("serialization should not fail");

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// Build an UpdateSplinePrice instruction with ordering protection.
///
/// # Accounts
/// 1. `phoenix_program` - The Phoenix Eternal program ID (read-only)
/// 2. `log_authority` - The log authority PDA (read-only)
/// 3. `signer` - The authority signing the transaction (signer, read-only)
/// 4. `trader` - The trader account (read-only)
/// 5. `spline_collection` - The spline collection account (writable)
pub fn update_spline_price_with_ordering(
    program_id: &Pubkey,
    signer: &Pubkey,
    trader: &Pubkey,
    spline_collection: &Pubkey,
    params: UpdateSplinePriceParamsWithOrdering,
) -> Instruction {
    let log_authority = get_log_authority(program_id);

    let accounts = vec![
        AccountMeta::new_readonly(*program_id, false),
        AccountMeta::new_readonly(log_authority, false),
        AccountMeta::new_readonly(*signer, true),
        AccountMeta::new_readonly(*trader, false),
        AccountMeta::new(*spline_collection, false),
    ];

    let mut data = Vec::with_capacity(8 + 64);
    data.extend_from_slice(&discriminants::UPDATE_SPLINE_PRICE.to_le_bytes());
    borsh::to_writer(&mut data, &params).expect("serialization should not fail");

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// Build an UpdateSplineParameters instruction.
///
/// # Accounts
/// 1. `phoenix_program` - The Phoenix Eternal program ID (read-only)
/// 2. `log_authority` - The log authority PDA (read-only)
/// 3. `signer` - The authority signing the transaction (signer, read-only)
/// 4. `trader` - The trader account (read-only)
/// 5. `spline_collection` - The spline collection account (writable)
pub fn update_spline_parameters(
    program_id: &Pubkey,
    signer: &Pubkey,
    trader: &Pubkey,
    spline_collection: &Pubkey,
    params: UpdateSplineParametersParams,
) -> Instruction {
    let log_authority = get_log_authority(program_id);

    let accounts = vec![
        AccountMeta::new_readonly(*program_id, false),
        AccountMeta::new_readonly(log_authority, false),
        AccountMeta::new_readonly(*signer, true),
        AccountMeta::new_readonly(*trader, false),
        AccountMeta::new(*spline_collection, false),
    ];

    let mut data = Vec::with_capacity(8 + 256);
    data.extend_from_slice(&discriminants::UPDATE_SPLINE_PARAMETERS.to_le_bytes());
    borsh::to_writer(&mut data, &params).expect("serialization should not fail");

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// Build an UpdateSplineParameters instruction with ordering protection.
///
/// # Accounts
/// 1. `phoenix_program` - The Phoenix Eternal program ID (read-only)
/// 2. `log_authority` - The log authority PDA (read-only)
/// 3. `signer` - The authority signing the transaction (signer, read-only)
/// 4. `trader` - The trader account (read-only)
/// 5. `spline_collection` - The spline collection account (writable)
pub fn update_spline_parameters_with_ordering(
    program_id: &Pubkey,
    signer: &Pubkey,
    trader: &Pubkey,
    spline_collection: &Pubkey,
    params: UpdateSplineParametersParamsWithOrdering,
) -> Instruction {
    let log_authority = get_log_authority(program_id);

    let accounts = vec![
        AccountMeta::new_readonly(*program_id, false),
        AccountMeta::new_readonly(log_authority, false),
        AccountMeta::new_readonly(*signer, true),
        AccountMeta::new_readonly(*trader, false),
        AccountMeta::new(*spline_collection, false),
    ];

    let mut data = Vec::with_capacity(8 + 256);
    data.extend_from_slice(&discriminants::UPDATE_SPLINE_PARAMETERS.to_le_bytes());
    borsh::to_writer(&mut data, &params).expect("serialization should not fail");

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

/// Build an UpdateSplinePositionLimitsConfig instruction.
///
/// # Accounts
/// 1. `phoenix_program` - The Phoenix Eternal program ID (read-only)
/// 2. `log_authority` - The log authority PDA (read-only)
/// 3. `signer` - The authority signing the transaction (signer, read-only)
/// 4. `trader` - The trader account (read-only)
/// 5. `spline_collection` - The spline collection account (writable)
pub fn update_spline_position_limits_config(
    program_id: &Pubkey,
    signer: &Pubkey,
    trader: &Pubkey,
    spline_collection: &Pubkey,
    params: UpdateSplinePositionLimitsConfigParams,
) -> Instruction {
    let log_authority = get_log_authority(program_id);

    let accounts = vec![
        AccountMeta::new_readonly(*program_id, false),
        AccountMeta::new_readonly(log_authority, false),
        AccountMeta::new_readonly(*signer, true),
        AccountMeta::new_readonly(*trader, false),
        AccountMeta::new(*spline_collection, false),
    ];

    let mut data = Vec::with_capacity(8 + 16);
    data.extend_from_slice(&discriminants::UPDATE_SPLINE_POSITION_LIMITS_CONFIG.to_le_bytes());
    borsh::to_writer(&mut data, &params).expect("serialization should not fail");

    Instruction {
        program_id: *program_id,
        accounts,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshDeserialize;

    #[test]
    fn test_tick_region_params_serialization() {
        let params = TickRegionParams::new(10, 100, 1000, 0);
        let serialized = borsh::to_vec(&params).unwrap();
        let deserialized = TickRegionParams::try_from_slice(&serialized).unwrap();
        assert_eq!(params, deserialized);
    }

    #[test]
    fn test_update_spline_price_params_serialization() {
        let params = UpdateSplinePriceParams::new(50000)
            .with_user_update_slot(12345)
            .with_refresh_regions();

        let serialized = borsh::to_vec(&params).unwrap();
        let deserialized = UpdateSplinePriceParams::try_from_slice(&serialized).unwrap();

        assert_eq!(params.new_mid_price, deserialized.new_mid_price);
        assert_eq!(params.user_update_slot, deserialized.user_update_slot);
        assert_eq!(params.refresh_regions, deserialized.refresh_regions);
    }

    #[test]
    fn test_update_spline_parameters_params_serialization() {
        let bid_regions = vec![
            TickRegionParams::new(0, 10, 1000, 0),
            TickRegionParams::new(10, 50, 500, 0),
        ];
        let ask_regions = vec![TickRegionParams::new(1, 20, 800, 0)];

        let params = UpdateSplineParametersParams::new(bid_regions.clone(), ask_regions.clone())
            .with_refresh_regions();

        let serialized = borsh::to_vec(&params).unwrap();
        let deserialized = UpdateSplineParametersParams::try_from_slice(&serialized).unwrap();

        assert_eq!(params.bid_regions.len(), deserialized.bid_regions.len());
        assert_eq!(params.ask_regions.len(), deserialized.ask_regions.len());
        assert_eq!(params.refresh_regions, deserialized.refresh_regions);
    }

    #[test]
    fn test_update_spline_position_limits_config_params_serialization() {
        let params = UpdateSplinePositionLimitsConfigParams {
            max_position_size: Some(PositionSizeLimit::Limit(PositionSizeLimits {
                long: 1000,
                short: 500,
            })),
            leverage_decrease_in_bps: Some(250),
        };

        let serialized = borsh::to_vec(&params).unwrap();
        let deserialized =
            UpdateSplinePositionLimitsConfigParams::try_from_slice(&serialized).unwrap();

        assert_eq!(params.max_position_size, deserialized.max_position_size);
        assert_eq!(
            params.leverage_decrease_in_bps,
            deserialized.leverage_decrease_in_bps
        );

        // Also test with Disabled + None
        let params2 = UpdateSplinePositionLimitsConfigParams {
            max_position_size: Some(PositionSizeLimit::Disabled),
            leverage_decrease_in_bps: None,
        };
        let serialized2 = borsh::to_vec(&params2).unwrap();
        let deserialized2 =
            UpdateSplinePositionLimitsConfigParams::try_from_slice(&serialized2).unwrap();
        assert_eq!(params2.max_position_size, deserialized2.max_position_size);
        assert_eq!(
            params2.leverage_decrease_in_bps,
            deserialized2.leverage_decrease_in_bps
        );
    }

    #[test]
    fn test_discriminants() {
        // Ensure discriminants are non-zero and unique
        let price_disc = *discriminants::UPDATE_SPLINE_PRICE;
        let params_disc = *discriminants::UPDATE_SPLINE_PARAMETERS;
        let limits_disc = *discriminants::UPDATE_SPLINE_POSITION_LIMITS_CONFIG;

        assert_ne!(price_disc, 0);
        assert_ne!(params_disc, 0);
        assert_ne!(limits_disc, 0);
        assert_ne!(price_disc, params_disc);
        assert_ne!(price_disc, limits_disc);
        assert_ne!(params_disc, limits_disc);

        println!("UPDATE_SPLINE_PRICE: {:#018x}", price_disc);
        println!("UPDATE_SPLINE_PARAMETERS: {:#018x}", params_disc);
        println!(
            "UPDATE_SPLINE_POSITION_LIMITS_CONFIG: {:#018x}",
            limits_disc
        );
    }

    #[test]
    fn test_instruction_builder() {
        let program_id = Pubkey::new_unique();
        let signer = Pubkey::new_unique();
        let trader = Pubkey::new_unique();
        let spline_collection = Pubkey::new_unique();

        let params = UpdateSplinePriceParams::new(50000);
        let ix = update_spline_price(&program_id, &signer, &trader, &spline_collection, params);

        assert_eq!(ix.program_id, program_id);
        assert_eq!(ix.accounts.len(), 5);

        // Account 0: phoenix_program (read-only, not signer)
        assert_eq!(ix.accounts[0].pubkey, program_id);
        assert!(!ix.accounts[0].is_signer);
        assert!(!ix.accounts[0].is_writable);

        // Account 1: log_authority (read-only, not signer)
        let expected_log_authority = get_log_authority(&program_id);
        assert_eq!(ix.accounts[1].pubkey, expected_log_authority);
        assert!(!ix.accounts[1].is_signer);
        assert!(!ix.accounts[1].is_writable);

        // Account 2: signer (signer, read-only)
        assert_eq!(ix.accounts[2].pubkey, signer);
        assert!(ix.accounts[2].is_signer);
        assert!(!ix.accounts[2].is_writable);

        // Account 3: trader (read-only)
        assert_eq!(ix.accounts[3].pubkey, trader);
        assert!(!ix.accounts[3].is_signer);
        assert!(!ix.accounts[3].is_writable);

        // Account 4: spline_collection (writable)
        assert_eq!(ix.accounts[4].pubkey, spline_collection);
        assert!(!ix.accounts[4].is_signer);
        assert!(ix.accounts[4].is_writable);

        // Verify discriminant is at the start
        let disc_bytes: [u8; 8] = ix.data[..8].try_into().unwrap();
        let disc = u64::from_le_bytes(disc_bytes);
        assert_eq!(disc, *discriminants::UPDATE_SPLINE_PRICE);
    }

    #[test]
    fn test_update_spline_position_limits_config_builder() {
        let program_id = Pubkey::new_unique();
        let signer = Pubkey::new_unique();
        let trader = Pubkey::new_unique();
        let spline_collection = Pubkey::new_unique();

        let params = UpdateSplinePositionLimitsConfigParams {
            max_position_size: Some(PositionSizeLimit::Limit(PositionSizeLimits::symmetric(100))),
            leverage_decrease_in_bps: Some(500),
        };
        let ix = update_spline_position_limits_config(
            &program_id,
            &signer,
            &trader,
            &spline_collection,
            params,
        );

        assert_eq!(ix.program_id, program_id);
        assert_eq!(ix.accounts.len(), 5);

        assert_eq!(ix.accounts[0].pubkey, program_id);
        assert!(!ix.accounts[0].is_writable);

        let expected_log_authority = get_log_authority(&program_id);
        assert_eq!(ix.accounts[1].pubkey, expected_log_authority);

        assert_eq!(ix.accounts[2].pubkey, signer);
        assert!(ix.accounts[2].is_signer);

        assert_eq!(ix.accounts[3].pubkey, trader);

        assert_eq!(ix.accounts[4].pubkey, spline_collection);
        assert!(ix.accounts[4].is_writable);

        let disc_bytes: [u8; 8] = ix.data[..8].try_into().unwrap();
        let disc = u64::from_le_bytes(disc_bytes);
        assert_eq!(disc, *discriminants::UPDATE_SPLINE_POSITION_LIMITS_CONFIG);
    }
}
