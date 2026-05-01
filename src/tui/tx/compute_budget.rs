//! Compute-budget helpers — builds the `SetComputeUnitLimit` and
//! `SetComputeUnitPrice` instructions that scale CU allowance with the
//! number of trader positions touched by a given tx.

use std::str::FromStr;

use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

/// Compute units allocated per position being closed/traded in a single tx.
pub(super) const COMPUTE_UNITS_PER_POSITION: u32 = 200_000;
/// Priority fee in microlamports per compute unit.
const PRIORITY_FEE_MICRO_LAMPORTS: u64 = 111;

/// Builds the two ComputeBudget instructions that must be appended to every
/// Solana transaction:
///   1. SetComputeUnitLimit  — 150 000  × `num_positions`
///   2. SetComputeUnitPrice  — 111 microLamports per CU
///
/// Uses the on-chain ComputeBudget program at
/// `ComputeBudget111111111111111111111111111111`.
pub(super) fn build_compute_budget_ixs(num_positions: u32) -> Vec<Instruction> {
    let limit = COMPUTE_UNITS_PER_POSITION.saturating_mul(num_positions);
    build_compute_budget_ixs_raw(limit)
}

/// Builds ComputeBudget instructions with an explicit CU limit (not scaled by
/// positions). Used for simpler operations like deposit/withdraw.
pub(super) fn build_compute_budget_ixs_raw(compute_unit_limit: u32) -> Vec<Instruction> {
    let program_id = Pubkey::from_str("ComputeBudget111111111111111111111111111111")
        .expect("hardcoded ComputeBudget pubkey");

    let mut limit_data = Vec::with_capacity(5);
    limit_data.push(2u8);
    limit_data.extend_from_slice(&compute_unit_limit.to_le_bytes());

    let mut price_data = Vec::with_capacity(9);
    price_data.push(3u8);
    price_data.extend_from_slice(&PRIORITY_FEE_MICRO_LAMPORTS.to_le_bytes());

    vec![
        Instruction {
            program_id,
            accounts: vec![],
            data: limit_data,
        },
        Instruction {
            program_id,
            accounts: vec![],
            data: price_data,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_compute_budget_ixs_raw() {
        let ixs = build_compute_budget_ixs_raw(100_000);
        assert_eq!(ixs.len(), 2);

        let limit_ix = &ixs[0];
        assert_eq!(
            limit_ix.program_id.to_string(),
            "ComputeBudget111111111111111111111111111111"
        );
        assert_eq!(limit_ix.data[0], 2);
        let limit_val =
            u32::from_le_bytes(limit_ix.data[1..5].try_into().expect("5-byte limit ix"));
        assert_eq!(limit_val, 100_000);

        let price_ix = &ixs[1];
        assert_eq!(
            price_ix.program_id.to_string(),
            "ComputeBudget111111111111111111111111111111"
        );
        assert_eq!(price_ix.data[0], 3);
        let price_val =
            u64::from_le_bytes(price_ix.data[1..9].try_into().expect("9-byte price ix"));
        assert_eq!(price_val, PRIORITY_FEE_MICRO_LAMPORTS);
    }

    #[test]
    fn test_build_compute_budget_ixs_scaled() {
        let ixs = build_compute_budget_ixs(2);
        assert_eq!(ixs.len(), 2);

        let limit_ix = &ixs[0];
        let limit_val =
            u32::from_le_bytes(limit_ix.data[1..5].try_into().expect("5-byte limit ix"));
        assert_eq!(limit_val, COMPUTE_UNITS_PER_POSITION * 2);
    }
}
