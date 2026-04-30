//! Global Trader Index cache.
//!
//! CLOB resting orders reference traders by a `u32` sokoban node pointer into
//! the on-chain `GlobalTraderIndex`. That index is keyed by *trader PDA*, not
//! by wallet authority, so resolving the pointer alone yields the PDA pubkey —
//! which differs from what the user recognizes as "their wallet". To display a
//! human-recognizable identity, we do a second hop: batch-fetch every trader
//! account and pull the `authority` field (`DynamicTraderHeader` offset
//! 56..88). The cache stores `node_addr -> authority` for CLOB rows and also
//! `trader_pda -> authority` so spline rows (which carry the PDA directly)
//! can be rendered against the same wallet-authority namespace.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use phoenix_eternal_types::discriminant::accounts::TRADER as TRADER_DISCRIMINANT;
use phoenix_eternal_types::{program_ids, GlobalTraderIndexTree};
use solana_account_decoder_client_types::{UiAccountEncoding, UiDataSliceConfig};
use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey as PhoenixPubkey;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_types::config::RpcAccountInfoConfig;
use tokio::sync::{Notify, RwLock};
use tracing::warn;

/// Minimum wall time between GTI refreshes triggered by unresolved-pointer
/// misses. Prevents a burst of unresolved rows from hammering RPC.
const REFRESH_MIN_INTERVAL: Duration = Duration::from_secs(10);

/// Size of the account-level `GlobalTraderIndexHeader` that precedes the
/// sokoban data in arena 0. Matches the crate's
/// `std::mem::size_of::<GlobalTraderIndexHeader>()` constant (hard-coded here
/// to avoid reaching into the private `accounts` module).
const GTI_HEADER_SIZE: usize = 48;

/// Byte offset of `Superblock.num_arenas` within the arena-0 account data. The
/// superblock lives immediately after the `GlobalTraderIndexHeader`; within the
/// superblock `num_arenas: u16` sits 4 bytes in (after `size: u32`).
const NUM_ARENAS_OFFSET: usize = GTI_HEADER_SIZE + 4;

/// Byte range of `DynamicTraderHeader.authority` — a 32-byte pubkey at offset
/// 56. (See `phoenix_eternal_types::accounts::trader::DynamicTraderHeader`
/// layout.)
const TRADER_AUTHORITY_OFFSET: usize = 56;
const TRADER_AUTHORITY_END: usize = TRADER_AUTHORITY_OFFSET + 32;

/// Max accounts per `getMultipleAccounts` RPC call.
const RPC_BATCH_SIZE: usize = 100;

/// Cached mapping from `FIFORestingOrder.trader_position_id.trader_id()` (a
/// sokoban node pointer into the GTI) to the owning trader's wallet authority
/// pubkey.
pub struct GtiCache {
    authorities: HashMap<u32, PhoenixPubkey>,
    /// Trader-PDA → wallet authority map, populated in the same refresh pass
    /// as `authorities`. Spline rows carry the PDA directly (not the GTI node
    /// pointer), so resolving their display identity goes through this map
    /// instead of `resolve`.
    pda_to_authority: HashMap<PhoenixPubkey, PhoenixPubkey>,
    loaded_at: Instant,
}

impl GtiCache {
    /// Resolve a GTI node pointer to the owning wallet authority. Returns
    /// `None` for sentinel/null pointers or for traders not in this cached
    /// snapshot (caller should trigger a refresh on miss).
    pub fn resolve(&self, addr: u32) -> Option<PhoenixPubkey> {
        if addr == 0 {
            return None;
        }
        self.authorities.get(&addr).copied()
    }

    /// Resolve a Phoenix trader PDA to its wallet authority. Used by spline
    /// rows, which carry the PDA directly in the on-chain payload. Returns
    /// `None` for PDAs not yet loaded (caller should fall back and trigger a
    /// refresh).
    pub fn resolve_pda(&self, pda: &PhoenixPubkey) -> Option<PhoenixPubkey> {
        self.pda_to_authority.get(pda).copied()
    }

    fn is_fresh_enough(&self) -> bool {
        self.loaded_at.elapsed() < REFRESH_MIN_INTERVAL
    }
}

/// Load raw GTI account buffers (header + overflow arenas).
async fn fetch_gti_buffers(client: &RpcClient) -> Result<Vec<Vec<u8>>, String> {
    let (header_key, _) = program_ids::get_global_trader_index_address_default(0);
    let header_account = client
        .get_account(&header_key)
        .await
        .map_err(|e| format!("fetch GTI header: {e}"))?;

    if header_account.data.len() < NUM_ARENAS_OFFSET + 2 {
        return Err("GTI header account too small for superblock".to_string());
    }
    // Superblock.num_arenas: u16 little-endian. Reading bytes directly avoids
    // pulling `bytemuck` into this crate just for one field.
    let num_arenas = u16::from_le_bytes([
        header_account.data[NUM_ARENAS_OFFSET],
        header_account.data[NUM_ARENAS_OFFSET + 1],
    ]);

    let mut buffers: Vec<Vec<u8>> = Vec::with_capacity(num_arenas.max(1) as usize);
    buffers.push(header_account.data);
    for i in 1..num_arenas {
        let (arena_key, _) = program_ids::get_global_trader_index_address_default(i);
        match client.get_account(&arena_key).await {
            Ok(acc) => buffers.push(acc.data),
            // Arena accounts are contiguous; a missing one means `num_arenas` is stale
            // or the layout is malformed. Log and stop — later lookups will just miss
            // and trigger another refresh.
            Err(e) => {
                warn!(arena = i, error = %e, "GTI arena fetch failed; truncating cache");
                break;
            }
        }
    }
    Ok(buffers)
}

/// Walk the GTI tree and collect `(node_addr, trader_pda)` pairs. Panics inside
/// the sokoban layer are caught so a corrupt cache doesn't unwind the loader.
fn collect_tree_pairs(buffers: &[Vec<u8>]) -> Vec<(u32, PhoenixPubkey)> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tree = GlobalTraderIndexTree::load_from_buffers(buffers.iter().map(|b| b.as_slice()));
        let mut pairs: Vec<(u32, PhoenixPubkey)> = Vec::with_capacity(tree.tree.len());
        for (pda, _state) in tree.tree.iter() {
            let addr = tree.tree.get_addr(pda);
            if addr != 0 {
                pairs.push((addr, *pda));
            }
        }
        pairs
    }))
    .unwrap_or_default()
}

/// Batch-fetch the listed trader PDAs and pull the 32-byte `authority` field
/// from each. Returns both an `(addr → authority)` map (for CLOB resolve) and
/// a `(PDA → authority)` map (for spline rows). Entries whose accounts are
/// missing or too small are skipped — callers fall back to a placeholder when
/// either lookup returns `None`.
async fn fetch_authorities(
    client: &RpcClient,
    pairs: &[(u32, PhoenixPubkey)],
) -> (
    HashMap<u32, PhoenixPubkey>,
    HashMap<PhoenixPubkey, PhoenixPubkey>,
) {
    // Defensive dedup: the GTI tree iterator yields unique keys, but if anything
    // ever produces duplicates we'd otherwise waste bandwidth fetching the same
    // pubkey more than once per refresh. Keep the first occurrence per `(addr,
    // pda)`.
    let mut seen_addrs: HashSet<u32> = HashSet::with_capacity(pairs.len());
    let mut seen_pdas: HashSet<[u8; 32]> = HashSet::with_capacity(pairs.len());
    let deduped: Vec<(u32, PhoenixPubkey)> = pairs
        .iter()
        .filter(|(addr, pda)| seen_addrs.insert(*addr) && seen_pdas.insert(pda.to_bytes()))
        .copied()
        .collect();
    if deduped.len() != pairs.len() {
        warn!(
            dropped = pairs.len() - deduped.len(),
            "GTI tree iter yielded duplicate entries; deduped before RPC"
        );
    }
    let mut out: HashMap<u32, PhoenixPubkey> = HashMap::with_capacity(deduped.len());
    let mut pda_out: HashMap<PhoenixPubkey, PhoenixPubkey> = HashMap::with_capacity(deduped.len());
    for chunk in deduped.chunks(RPC_BATCH_SIZE) {
        let pairs_aligned: Vec<(u32, PhoenixPubkey)> = chunk.to_vec();
        let pks: Vec<PhoenixPubkey> = pairs_aligned.iter().map(|(_, p)| *p).collect();
        // Only the first `TRADER_AUTHORITY_END` bytes are needed to read the authority
        // field. Without `data_slice`, each trader account comes back in full (position
        // buffers + padding, often several KB); slicing shrinks each reply to the fixed
        // header prefix and removes a ~100x bandwidth multiplier on every refresh
        // cycle.
        let cfg = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::processed()),
            data_slice: Some(UiDataSliceConfig {
                offset: 0,
                length: TRADER_AUTHORITY_END,
            }),
            min_context_slot: None,
        };
        let resp = match client.get_multiple_accounts_with_config(&pks, cfg).await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "getMultipleAccounts failed for trader batch; skipping");
                continue;
            }
        };
        let accounts = resp.value;
        for ((addr, pda), acc) in pairs_aligned.iter().zip(accounts) {
            let Some(acc) = acc else { continue };
            let data = acc.data.as_slice();
            if data.len() < TRADER_AUTHORITY_END {
                continue;
            }
            // Guard against reading from a closed-and-reused account slot: the discriminant
            // in the first 8 bytes must match `account:trader`. Mismatches get skipped
            // (rows will render as unresolved and retry on the next refresh).
            let disc = u64::from_le_bytes(data[..8].try_into().unwrap_or([0u8; 8]));
            if disc != *TRADER_DISCRIMINANT {
                warn!(
                    addr = *addr,
                    disc = disc,
                    "trader account discriminant mismatch; skipping"
                );
                continue;
            }
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&data[TRADER_AUTHORITY_OFFSET..TRADER_AUTHORITY_END]);
            let authority = PhoenixPubkey::from(bytes);
            out.insert(*addr, authority);
            pda_out.insert(*pda, authority);
        }
    }
    (out, pda_out)
}

async fn fetch_cache(rpc_url: String) -> Result<GtiCache, String> {
    let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::processed());
    let buffers = fetch_gti_buffers(&client).await?;
    let pairs = collect_tree_pairs(&buffers);
    let (authorities, pda_to_authority) = fetch_authorities(&client, &pairs).await;
    Ok(GtiCache {
        authorities,
        pda_to_authority,
        loaded_at: Instant::now(),
    })
}

/// Shared handle over the cached GTI. `None` before the first load completes.
pub type GtiHandle = Arc<RwLock<Option<GtiCache>>>;

/// Spawn a task that keeps `cache` loaded. Triggers a refresh on startup and
/// then whenever `refresh` is notified, subject to `REFRESH_MIN_INTERVAL`
/// debouncing. `rpc_url_fn` is called at each refresh so [c] RPC-URL changes
/// take effect on the next notify.
pub fn spawn_gti_loader<F>(
    cache: GtiHandle,
    refresh: Arc<Notify>,
    rpc_url_fn: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> String + Send + 'static,
{
    tokio::spawn(async move {
        // Kick off an initial load immediately.
        refresh.notify_one();
        loop {
            refresh.notified().await;
            // Debounce: if the current cache was loaded very recently, skip this refresh.
            if let Some(existing) = cache.read().await.as_ref() {
                if existing.is_fresh_enough() {
                    continue;
                }
            }
            let url = rpc_url_fn();
            match fetch_cache(url).await {
                Ok(new_cache) => {
                    *cache.write().await = Some(new_cache);
                }
                Err(e) => {
                    warn!(error = %e, "GTI refresh failed; will retry on next notify");
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cache(entries: &[(u32, [u8; 32])], age: Duration) -> GtiCache {
        let mut authorities = HashMap::new();
        for (addr, bytes) in entries {
            authorities.insert(*addr, PhoenixPubkey::from(*bytes));
        }
        GtiCache {
            authorities,
            pda_to_authority: HashMap::new(),
            loaded_at: Instant::now()
                .checked_sub(age)
                .expect("test age fits in Instant arithmetic"),
        }
    }

    #[test]
    fn resolve_returns_cached_authority() {
        let key = [7u8; 32];
        let cache = make_cache(&[(42, key)], Duration::from_secs(0));
        assert_eq!(cache.resolve(42), Some(PhoenixPubkey::from(key)));
    }

    #[test]
    fn resolve_returns_none_for_missing_addr() {
        let cache = make_cache(&[(1, [0u8; 32])], Duration::from_secs(0));
        assert_eq!(cache.resolve(999), None);
    }

    #[test]
    fn resolve_treats_zero_as_sentinel_null() {
        let cache = make_cache(&[(0, [0u8; 32])], Duration::from_secs(0));
        assert_eq!(cache.resolve(0), None);
    }

    #[test]
    fn is_fresh_enough_is_true_for_recent_load() {
        let cache = make_cache(&[], Duration::from_secs(0));
        assert!(cache.is_fresh_enough());
    }

    #[test]
    fn is_fresh_enough_is_false_after_refresh_interval() {
        let cache = make_cache(&[], REFRESH_MIN_INTERVAL + Duration::from_secs(1));
        assert!(!cache.is_fresh_enough());
    }
}
