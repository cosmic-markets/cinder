//! Account discriminant constants for Phoenix Eternal.
//!
//! Each account type has a unique 64-bit discriminant computed from
//! the first 8 bytes of SHA256("account:<account_name>").

use sha2::{Digest, Sha256};

/// Compute the discriminant for an account type.
/// Takes the first 8 bytes of SHA256(input) as a little-endian u64.
pub const fn sha2_const(input: &[u8]) -> u64 {
    // Note: This is a runtime implementation. For compile-time, you'd need
    // a const-compatible SHA256 implementation.
    // We provide precomputed values below.
    let _ = input;
    0 // Placeholder, see precomputed values below
}

/// Compute discriminant at runtime.
pub fn compute_discriminant(input: &[u8]) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let result = hasher.finalize();
    u64::from_le_bytes(result[..8].try_into().unwrap())
}

/// Account discriminant constants.
pub mod accounts {
    use super::compute_discriminant;
    use std::sync::LazyLock;

    // Precomputed discriminants (computed once at runtime for safety)
    pub static GLOBAL_CONFIGURATION: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:global_configuration"));

    pub static ORDERBOOK_HEADER: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:orderbook"));

    pub static TRADER: LazyLock<u64> = LazyLock::new(|| compute_discriminant(b"account:trader"));

    pub static PERP_ASSET_MAP: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:perp_asset_map"));

    pub static GLOBAL_TRADER_INDEX_HEADER: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:global_trader_index"));

    pub static GLOBAL_TRADER_INDEX_ARENA_HEADER: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:global_trader_index_arena"));

    pub static ACTIVE_TRADER_BUFFER_HEADER: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:active_trader_buffer"));

    pub static ACTIVE_TRADER_BUFFER_ARENA_HEADER: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:active_trader_buffer_arena"));

    pub static SPLINE_COLLECTION_HEADER: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:spline_collection"));

    pub static WITHDRAW_QUEUE_HEADER: LazyLock<u64> =
        LazyLock::new(|| compute_discriminant(b"account:withdraw_queue"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discriminant_computation() {
        // Verify discriminants are non-zero and unique
        let global_config = *accounts::GLOBAL_CONFIGURATION;
        let orderbook = *accounts::ORDERBOOK_HEADER;
        let trader = *accounts::TRADER;

        assert_ne!(global_config, 0);
        assert_ne!(orderbook, 0);
        assert_ne!(trader, 0);
        assert_ne!(global_config, orderbook);
        assert_ne!(global_config, trader);
        assert_ne!(orderbook, trader);

        // Print discriminants for reference
        println!("GLOBAL_CONFIGURATION: {:#018x}", global_config);
        println!("ORDERBOOK_HEADER: {:#018x}", orderbook);
        println!("TRADER: {:#018x}", trader);
    }
}
