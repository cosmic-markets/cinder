//! On-chain data loaders and decoders that feed the TUI.

use solana_pubkey::Pubkey;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use tracing::warn;

pub mod position_leaderboard;
pub mod spline_book;
pub mod trader_index;

pub use position_leaderboard::fetch_top_positions;
pub use spline_book::{
    L2Level, ParsedSplineData, parse_l2_book_from_market_account, parse_spline_data,
    parse_spline_sequence,
};
pub use trader_index::{GtiCache, GtiHandle, spawn_gti_loader};

/// Fetch arena-0 header and all contiguous overflow arenas for a sokoban-backed
/// on-chain tree (used by both `ActiveTraderBuffer` and `GlobalTraderIndex`).
pub(super) async fn fetch_sokoban_arenas(
    client: &RpcClient,
    num_arenas_offset: usize,
    label: &str,
    derive_pda: fn(u16) -> (Pubkey, u8),
) -> Result<Vec<Vec<u8>>, String> {
    let (header_key, _) = derive_pda(0);
    let header_account = client
        .get_account(&header_key)
        .await
        .map_err(|e| format!("fetch {label} header: {e}"))?;

    if header_account.data.len() < num_arenas_offset + 2 {
        return Err(format!("{label} header account too small for superblock"));
    }
    // Superblock.num_arenas: u16 little-endian immediately after the account header.
    let num_arenas = u16::from_le_bytes([
        header_account.data[num_arenas_offset],
        header_account.data[num_arenas_offset + 1],
    ]);

    let mut buffers: Vec<Vec<u8>> = Vec::with_capacity(num_arenas.max(1) as usize);
    buffers.push(header_account.data);
    for i in 1..num_arenas {
        let (arena_key, _) = derive_pda(i);
        match client.get_account(&arena_key).await {
            Ok(acc) => buffers.push(acc.data),
            Err(e) => {
                // Stop early rather than inserting a misaligned buffer; the next
                // refresh will retry.
                warn!(arena = i, error = %e, "{label} arena fetch failed; truncating");
                break;
            }
        }
    }
    Ok(buffers)
}
