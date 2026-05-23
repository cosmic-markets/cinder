//! Phoenix Flight builder routing — wraps order-placement instructions so
//! builder fees credit to the Cosmic Markets builder account.

use std::str::FromStr;
use std::sync::LazyLock;

use phoenix_rise::PhoenixFlightClient;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

/// Cosmic Markets registered Phoenix Flight builder authority.
const COSMIC_BUILDER_AUTHORITY: &str = "cosmiccdRa4xgCtPiawLwjJiDZNyukNXiXWa2gwRfyq";

static FLIGHT_CLIENT: LazyLock<PhoenixFlightClient> = LazyLock::new(|| {
    PhoenixFlightClient::new(
        Pubkey::from_str(COSMIC_BUILDER_AUTHORITY).expect("valid builder authority"),
        0,
        0,
    )
});

/// Wrap a single instruction through Flight when it is an order-placement ix.
pub fn wrap_order_ix(ix: Instruction, trader_wallet: Pubkey) -> Result<Instruction, String> {
    FLIGHT_CLIENT
        .try_wrap_order_instruction(ix, trader_wallet)
        .map_err(|e| e.to_string())
}

/// Wrap each instruction in `ixs`; non-routable ixs pass through unchanged.
pub fn wrap_order_ixs(
    ixs: Vec<Instruction>,
    trader_wallet: Pubkey,
) -> Result<Vec<Instruction>, String> {
    ixs.into_iter()
        .map(|ix| wrap_order_ix(ix, trader_wallet))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosmic_builder_authority_parses() {
        assert!(Pubkey::from_str(COSMIC_BUILDER_AUTHORITY).is_ok());
    }

    #[test]
    fn flight_client_uses_cosmic_builder_authority() {
        assert_eq!(
            FLIGHT_CLIENT.builder_authority,
            Pubkey::from_str(COSMIC_BUILDER_AUTHORITY).unwrap()
        );
    }
}
