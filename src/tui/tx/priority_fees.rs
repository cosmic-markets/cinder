//! Auto-derived `SetComputeUnitPrice` fee, refreshed in the background from
//! `getRecentPrioritizationFees` and used as the dynamic default when the
//! user has not set an override.
//!
//! A single global task polls the configured RPC every
//! `REFRESH_INTERVAL` with the Phoenix Eternal program plus its hot global
//! accounts as the `accounts` filter — without that filter the RPC samples
//! across every recent tx on Solana and the percentile collapses to zero
//! during normal operation. With the filter we see the fees that landed
//! against accounts our own transactions also lock, which is what we
//! actually need to outbid.
//!
//! `current_auto_priority_fee` returns the latest cached value (or `None`
//! if no successful query has completed yet, e.g. early in startup or
//! while the RPC is unreachable). The stored value is floored at
//! [`MIN_AUTO_PRIORITY_FEE`] so the cache never advertises 0 even when
//! the network has a quiet window.
//!
//! The task re-reads `rpc_http_url_from_env()` on every tick so that an
//! in-app RPC URL change (config modal) is picked up without restart.

use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use tracing::warn;

use crate::tui::config::{rpc_http_url_from_env, DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS};

/// How often the background task refreshes the cached auto fee.
const REFRESH_INTERVAL: Duration = Duration::from_secs(10);

/// Hard ceiling on a single `getRecentPrioritizationFees` call. Skip the tick
/// if the RPC is too slow rather than letting the refresh loop wedge.
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

/// Percentile (0-100) of returned per-slot fees that the auto value tracks.
/// p90 leans aggressive: the cached value reflects the fee that beat 90% of
/// recent Phoenix-touching txs in the network's priority queue, which is
/// where you want to be if you actually care about landing.
const FEE_PERCENTILE: usize = 90;

/// Floor for the cached auto fee. Even if every recent Phoenix-account tx
/// paid 0 priority, we publish at least this so users on the dynamic default
/// always pay *something* — matches the static `DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS`.
const MIN_AUTO_PRIORITY_FEE: u64 = DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS;

/// Phoenix Eternal program + the hot global accounts every Phoenix trade
/// touches. `getRecentPrioritizationFees` filters to recent txs that
/// referenced any of these (writable or read-only); the returned fees are
/// what those txs paid for priority.
const PHOENIX_HOT_ACCOUNTS: &[&str] = &[
    // Phoenix Eternal program
    "EtrnLzgbS7nMMy5fbD42kXiUzGg8XQzJ972Xtk1cjWih",
    // Phoenix log authority
    "GdxfTLSsdSY37G6fZoYtdGDSfgFnbT2EmRpuePZxWShS",
    // Global Configuration
    "2zskx2iyCvb6Stg7RBZkt1f6MrF4dpYtMG3yMvKwqtUZ",
    // Perp Asset Map
    "2nHGAaEw3D5dd4hVueaUNoygkQFmoeKqRQWnSPqSMFUC",
    // Global Trader Index
    "HCrPXLByGqRh2szQi3gj7oRdRVBNi1gccAyn4CQCT3HK",
    // Active Trader Buffer
    "2U32rSzzrQS3eVmGHsnbw5kcqKF3wQXpHGd3hMq5YJok",
];

fn phoenix_hot_accounts() -> &'static [Pubkey] {
    static PARSED: OnceLock<Vec<Pubkey>> = OnceLock::new();
    PARSED.get_or_init(|| {
        PHOENIX_HOT_ACCOUNTS
            .iter()
            .filter_map(|s| match s.parse::<Pubkey>() {
                Ok(pk) => Some(pk),
                Err(e) => {
                    warn!(account = %s, error = %e, "failed to parse Phoenix hot account");
                    None
                }
            })
            .collect()
    })
}

fn cache() -> &'static RwLock<Option<u64>> {
    static CACHE: OnceLock<RwLock<Option<u64>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(None))
}

/// Returns the latest auto-derived priority fee (microlamports per CU), or
/// `None` if the background task hasn't produced a value yet.
pub fn current_auto_priority_fee() -> Option<u64> {
    cache().read().ok().and_then(|g| *g)
}

fn store(fee: u64) {
    if let Ok(mut g) = cache().write() {
        *g = Some(fee);
    }
}

/// Compute the configured percentile across `samples`. Returns `None` for an
/// empty slice. Mutates the input by sorting in place.
fn percentile(samples: &mut [u64], pct: usize) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    // Nearest-rank percentile: index = ceil(pct/100 * n) - 1, clamped.
    let n = samples.len();
    let idx = ((pct * n) + 99) / 100;
    let idx = idx.saturating_sub(1).min(n - 1);
    Some(samples[idx])
}

/// Returns the percentile of `samples` floored at [`MIN_AUTO_PRIORITY_FEE`].
/// `None` only when the input is empty (caller should leave the cache
/// untouched in that case so a transient empty response doesn't reset the
/// last good reading).
fn bounded_percentile(samples: &mut [u64]) -> Option<u64> {
    percentile(samples, FEE_PERCENTILE).map(|fee| fee.max(MIN_AUTO_PRIORITY_FEE))
}

/// Spawn the global refresh task. Idempotent: subsequent calls are no-ops.
pub fn spawn_auto_priority_fee_refresh() {
    static SPAWNED: OnceLock<()> = OnceLock::new();
    if SPAWNED.set(()).is_err() {
        return;
    }

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut current_url: Option<String> = None;
        let mut client: Option<RpcClient> = None;
        let accounts = phoenix_hot_accounts();
        loop {
            interval.tick().await;
            let url = rpc_http_url_from_env();
            if current_url.as_deref() != Some(url.as_str()) {
                client = Some(RpcClient::new_with_commitment(
                    url.clone(),
                    CommitmentConfig::processed(),
                ));
                current_url = Some(url);
            }
            let Some(rpc) = client.as_ref() else { continue };
            let fetch =
                tokio::time::timeout(QUERY_TIMEOUT, rpc.get_recent_prioritization_fees(accounts));
            let Ok(Ok(fees)) = fetch.await else { continue };
            let mut samples: Vec<u64> = fees.into_iter().map(|f| f.prioritization_fee).collect();
            if let Some(fee) = bounded_percentile(&mut samples) {
                store(fee);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_handles_empty_slice() {
        let mut empty: Vec<u64> = vec![];
        assert_eq!(percentile(&mut empty, 75), None);
    }

    #[test]
    fn percentile_p75_picks_nearest_rank() {
        // 100 values 1..=100; p75 nearest-rank = 75.
        let mut v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&mut v, 75), Some(75));
    }

    #[test]
    fn percentile_p100_picks_max() {
        let mut v = vec![10, 20, 30, 40, 50];
        assert_eq!(percentile(&mut v, 100), Some(50));
    }

    #[test]
    fn percentile_handles_single_element() {
        let mut v = vec![42];
        assert_eq!(percentile(&mut v, 75), Some(42));
    }

    #[test]
    fn percentile_handles_all_zeros() {
        let mut v = vec![0u64; 50];
        assert_eq!(percentile(&mut v, 75), Some(0));
    }

    #[test]
    fn bounded_percentile_floors_zero_at_min() {
        let mut v = vec![0u64; 100];
        assert_eq!(bounded_percentile(&mut v), Some(MIN_AUTO_PRIORITY_FEE));
    }

    #[test]
    fn bounded_percentile_returns_none_for_empty() {
        let mut v: Vec<u64> = vec![];
        assert_eq!(bounded_percentile(&mut v), None);
    }

    #[test]
    fn bounded_percentile_passes_through_high_values() {
        // 100 values 1_000..=100_000 (step 1_000); p90 nearest-rank index =
        // ceil(0.90 * 100) - 1 = 89, sample[89] = 90_000 — well above the
        // 111-microlamport floor, so the floor must not clamp it.
        let mut v: Vec<u64> = (1..=100).map(|i| i * 1_000).collect();
        assert_eq!(bounded_percentile(&mut v), Some(90_000));
    }

    #[test]
    fn phoenix_hot_accounts_all_parse() {
        // None of the hardcoded constants should be malformed.
        assert_eq!(phoenix_hot_accounts().len(), PHOENIX_HOT_ACCOUNTS.len());
    }
}
