//! Program IDs and PDA derivation for Phoenix Eternal.

use solana_pubkey::Pubkey;

/// Phoenix Eternal production program ID.
pub const PHOENIX_ETERNAL_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("EtrnLzgbS7nMMy5fbD42kXiUzGg8XQzJ972Xtk1cjWih");

/// Phoenix Eternal beta/devnet program ID.
pub const PHOENIX_ETERNAL_BETA_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("phDEVv4w6BcfkLrLNeXr8HhhgQxnxziVGXpGPcaadMf");

// ============================================================================
// PDA Seeds
// ============================================================================

/// Seed prefix for global configuration account.
pub const GLOBAL_CONFIG_SEED: &[u8] = b"global";

/// Seed prefix for trader accounts.
pub const TRADER_SEED: &[u8] = b"trader";

/// Seed prefix for global trader index accounts.
pub const GLOBAL_TRADER_INDEX_SEED: &[u8] = b"global_trader_index";

/// Seed prefix for active trader buffer accounts.
pub const ACTIVE_TRADER_BUFFER_SEED: &[u8] = b"active_trader_buffer";

/// Seed prefix for spline collection accounts.
pub const SPLINE_SEED: &[u8] = b"spline";

// ============================================================================
// PDA Derivation Functions
// ============================================================================

/// Derive the global configuration PDA address.
///
/// Seeds: `["global"]`
pub fn get_global_config_address(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[GLOBAL_CONFIG_SEED], program_id)
}

/// Derive the global configuration PDA address using the production program ID.
pub fn get_global_config_address_default() -> (Pubkey, u8) {
    get_global_config_address(&PHOENIX_ETERNAL_PROGRAM_ID)
}

/// Derive a trader account PDA address.
///
/// Seeds: `["trader", trader_wallet, [pda_index, subaccount_index]]`
///
/// # Arguments
/// * `program_id` - The program ID
/// * `trader_wallet` - The trader's wallet pubkey (authority)
/// * `pda_index` - The trader PDA index (0-255)
/// * `subaccount_index` - The subaccount index (0 for main account, 1+ for subaccounts)
pub fn get_trader_address(
    program_id: &Pubkey,
    trader_wallet: &Pubkey,
    pda_index: u8,
    subaccount_index: u8,
) -> (Pubkey, u8) {
    let pda_schema = [pda_index, subaccount_index];
    Pubkey::find_program_address(
        &[TRADER_SEED, trader_wallet.as_ref(), pda_schema.as_ref()],
        program_id,
    )
}

/// Derive a trader account PDA address using the production program ID.
pub fn get_trader_address_default(
    trader_wallet: &Pubkey,
    pda_index: u8,
    subaccount_index: u8,
) -> (Pubkey, u8) {
    get_trader_address(
        &PHOENIX_ETERNAL_PROGRAM_ID,
        trader_wallet,
        pda_index,
        subaccount_index,
    )
}

/// Derive a global trader index PDA address.
///
/// Seeds: `["global_trader_index", index_le_bytes]`
///
/// # Arguments
/// * `program_id` - The program ID
/// * `index` - The arena index (0 for header account, 1+ for arena accounts)
pub fn get_global_trader_index_address(program_id: &Pubkey, index: u16) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[GLOBAL_TRADER_INDEX_SEED, &index.to_le_bytes()],
        program_id,
    )
}

/// Derive a global trader index PDA address using the production program ID.
pub fn get_global_trader_index_address_default(index: u16) -> (Pubkey, u8) {
    get_global_trader_index_address(&PHOENIX_ETERNAL_PROGRAM_ID, index)
}

/// Derive an active trader buffer PDA address.
///
/// Seeds: `["active_trader_buffer", index_le_bytes]`
///
/// # Arguments
/// * `program_id` - The program ID
/// * `index` - The arena index (0 for header account, 1+ for arena accounts)
pub fn get_active_trader_buffer_address(program_id: &Pubkey, index: u16) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[ACTIVE_TRADER_BUFFER_SEED, &index.to_le_bytes()],
        program_id,
    )
}

/// Derive an active trader buffer PDA address using the production program ID.
pub fn get_active_trader_buffer_address_default(index: u16) -> (Pubkey, u8) {
    get_active_trader_buffer_address(&PHOENIX_ETERNAL_PROGRAM_ID, index)
}

/// Derive a spline collection PDA address.
///
/// Seeds: `["spline", market_account]`
///
/// # Arguments
/// * `program_id` - The program ID
/// * `market_account` - The market (orderbook) account pubkey
pub fn get_spline_collection_address(program_id: &Pubkey, market_account: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SPLINE_SEED, market_account.as_ref()], program_id)
}

/// Derive a spline collection PDA address using the production program ID.
pub fn get_spline_collection_address_default(market_account: &Pubkey) -> (Pubkey, u8) {
    get_spline_collection_address(&PHOENIX_ETERNAL_PROGRAM_ID, market_account)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_config_address() {
        let (address, bump) = get_global_config_address_default();
        // Verify the address is a valid PDA (not on the ed25519 curve)
        // Note: is_on_curve() returns false for valid PDAs
        assert!(!address.is_on_curve());
        let _ = bump; // PDA derivation returned a valid bump byte.

        // Verify derivation is deterministic
        let (address2, bump2) = get_global_config_address_default();
        assert_eq!(address, address2);
        assert_eq!(bump, bump2);
    }

    #[test]
    fn test_trader_address() {
        let wallet = Pubkey::new_unique();
        let (address, _bump) = get_trader_address_default(&wallet, 0, 0);

        // Verify derivation is deterministic
        let (address2, _) = get_trader_address_default(&wallet, 0, 0);
        assert_eq!(address, address2);

        // Different pda_index should give different address
        let (address3, _) = get_trader_address_default(&wallet, 1, 0);
        assert_ne!(address, address3);

        // Different subaccount_index should give different address
        let (address4, _) = get_trader_address_default(&wallet, 0, 1);
        assert_ne!(address, address4);
    }

    #[test]
    fn test_global_trader_index_address() {
        // Header account (index 0)
        let (header, _) = get_global_trader_index_address_default(0);

        // Arena account (index 1)
        let (arena1, _) = get_global_trader_index_address_default(1);
        assert_ne!(header, arena1);

        // Arena account (index 2)
        let (arena2, _) = get_global_trader_index_address_default(2);
        assert_ne!(header, arena2);
        assert_ne!(arena1, arena2);
    }

    #[test]
    fn test_active_trader_buffer_address() {
        // Header account (index 0)
        let (header, _) = get_active_trader_buffer_address_default(0);

        // Arena account (index 1)
        let (arena1, _) = get_active_trader_buffer_address_default(1);
        assert_ne!(header, arena1);
    }
}
