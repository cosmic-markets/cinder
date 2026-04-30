//! Global Configuration account type.

use bytemuck::{Pod, Zeroable};
use solana_pubkey::Pubkey;

use crate::quantities::Slot;

/// Exchange status bits.
#[repr(C)]
#[derive(Copy, Clone, Default, Pod, Zeroable)]
pub struct ExchangeStatusBits {
    bits: u8,
}

impl ExchangeStatusBits {
    pub fn as_u8(&self) -> u8 {
        self.bits
    }
}

impl std::fmt::Debug for ExchangeStatusBits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExchangeStatusBits({:#04x})", self.bits)
    }
}

/// Global configuration account for Phoenix Eternal.
///
/// Size: 2560 bytes
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GlobalConfiguration {
    pub discriminant: u64,
    pub account_key: Pubkey,
    /// Reserved (256 bytes)
    _padding_a: [u8; 256],
    pub canonical_token_mint_key: Pubkey,
    pub global_vault_key: Pubkey,
    pub perp_asset_map_key: Pubkey,
    pub global_trader_index_header_key: Pubkey,
    pub active_trader_buffer_header_key: Pubkey,
    _reserved0: [u8; 8],
    _reserved1: [u8; 8],
    pub withdraw_queue_key: Pubkey,
    pub exchange_status: ExchangeStatusBits,
    pub quote_decimals: u8,
    pub withdrawal_margin_factor_bps: u16,
    _padding0: [u8; 4],
    pub deposit_cooldown_period_in_slots: Slot,
    /// Reserved (256 bytes)
    _padding_b: [u8; 256],

    // Reserved bytes
    _padding1: [u64; 31],
    _padding2: [u64; 32],
    _padding3: [u64; 32],
    _padding4: [u64; 32],
    _padding5: [u64; 32],
    _padding6: [u64; 32],
    _padding7: [u64; 32],
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<GlobalConfiguration>(), 2560);

impl GlobalConfiguration {
    /// Get the quote token decimals (e.g., 6 for USDC).
    pub fn quote_decimals(&self) -> u8 {
        self.quote_decimals
    }

    /// Get the withdrawal margin factor in basis points.
    pub fn withdrawal_margin_factor_bps(&self) -> u16 {
        self.withdrawal_margin_factor_bps
    }

    /// Get the deposit cooldown period in slots.
    pub fn deposit_cooldown_period_in_slots(&self) -> Slot {
        self.deposit_cooldown_period_in_slots
    }

    /// Get the exchange status bits.
    pub fn exchange_status(&self) -> ExchangeStatusBits {
        self.exchange_status
    }

    /// Get the canonical token mint (quote token) key.
    pub fn canonical_token_mint_key(&self) -> &Pubkey {
        &self.canonical_token_mint_key
    }

    /// Get the global vault key.
    pub fn global_vault_key(&self) -> &Pubkey {
        &self.global_vault_key
    }

    /// Get the perp asset map key.
    pub fn perp_asset_map_key(&self) -> &Pubkey {
        &self.perp_asset_map_key
    }

    /// Get the global trader index header key.
    pub fn global_trader_index_header_key(&self) -> &Pubkey {
        &self.global_trader_index_header_key
    }

    /// Get the active trader buffer header key.
    pub fn active_trader_buffer_header_key(&self) -> &Pubkey {
        &self.active_trader_buffer_header_key
    }

    /// Get the withdraw queue key.
    pub fn withdraw_queue_key(&self) -> &Pubkey {
        &self.withdraw_queue_key
    }
}

impl std::fmt::Debug for GlobalConfiguration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalConfiguration")
            .field("discriminant", &self.discriminant)
            .field("account_key", &self.account_key)
            .field("canonical_token_mint_key", &self.canonical_token_mint_key)
            .field("global_vault_key", &self.global_vault_key)
            .field("perp_asset_map_key", &self.perp_asset_map_key)
            .field(
                "global_trader_index_header_key",
                &self.global_trader_index_header_key,
            )
            .field(
                "active_trader_buffer_header_key",
                &self.active_trader_buffer_header_key,
            )
            .field("withdraw_queue_key", &self.withdraw_queue_key)
            .field("exchange_status", &self.exchange_status)
            .field("quote_decimals", &self.quote_decimals)
            .field(
                "withdrawal_margin_factor_bps",
                &self.withdrawal_margin_factor_bps,
            )
            .field(
                "deposit_cooldown_period_in_slots",
                &self.deposit_cooldown_period_in_slots,
            )
            .finish()
    }
}
