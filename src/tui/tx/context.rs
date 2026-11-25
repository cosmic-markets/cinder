//! Per-market transaction context — caches RPC clients, the trader PDA, the
//! market account addresses, a warm blockhash pool, and a shared
//! signatureSubscribe pubsub client used by every submission flow.

use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use phoenix_eternal_types::program_ids;
use phoenix_rise::api::{PhoenixMetadata, Trader, TraderKey};
use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_signer::Signer;
use tracing::warn;

use super::super::config::{
    DEFAULT_PUBLIC_SOLANA_RPC_URL, current_user_config, is_public_mainnet_rpc,
    rpc_http_url_from_env, ws_url_from_env,
};

/// Bound transaction preparation on slow/stalled RPCs.
const BLOCKHASH_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Shared wallet `Trader` mirror with a hydration flag so isolated-margin order
/// builders never build against an un-hydrated empty `Trader`.
pub struct TraderMirror {
    pub trader: Trader,
    pub hydrated: bool,
}

impl TraderMirror {
    /// Construct an empty (un-hydrated) trader mirror for `authority`.
    pub fn empty(authority: Pubkey) -> Self {
        Self {
            trader: Trader::new(TraderKey::new(authority)),
            hydrated: false,
        }
    }
}

/// Derive the canonical spline-collection PDA from the market account,
/// preferring the derived address over any stale API value.
fn derive_spline_collection(
    symbol: &str,
    market_pubkey: &str,
    api_spline_pubkey: &str,
) -> Result<Pubkey, solana_pubkey::ParsePubkeyError> {
    let market_pk = Pubkey::from_str(market_pubkey)?;
    let (derived, _) = program_ids::get_spline_collection_address_default(&market_pk);
    if let Ok(api_pk) = Pubkey::from_str(api_spline_pubkey)
        && api_pk != derived
    {
        warn!(
            api = %api_pk,
            derived = %derived,
            symbol = %symbol,
            "spline pubkey mismatch; using derived address"
        );
    }
    Ok(derived)
}

/// Parse all on-chain account addresses required to build instructions for `symbol`.
fn build_market_addrs(metadata: &PhoenixMetadata, symbol: &str) -> Result<MarketAddrs, String> {
    let keys = metadata.keys();
    let market = metadata
        .get_market(symbol)
        .ok_or_else(|| format!("{symbol} market not found in exchange metadata"))?;
    Ok(MarketAddrs {
        perp_asset_map: Pubkey::from_str(&keys.perp_asset_map).map_err(|e| e.to_string())?,
        orderbook: Pubkey::from_str(&market.market_pubkey).map_err(|e| e.to_string())?,
        spline_collection: derive_spline_collection(
            &market.symbol,
            &market.market_pubkey,
            &market.spline_pubkey,
        )
        .map_err(|e| e.to_string())?,
        global_trader_index: keys
            .global_trader_index
            .iter()
            .map(|s| Pubkey::from_str(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
        active_trader_buffer: keys
            .active_trader_buffer
            .iter()
            .map(|s| Pubkey::from_str(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
    })
}

/// Resolved on-chain account addresses for a single Phoenix market.
pub struct MarketAddrs {
    pub perp_asset_map: Pubkey,
    pub orderbook: Pubkey,
    pub spline_collection: Pubkey,
    pub global_trader_index: Vec<Pubkey>,
    pub active_trader_buffer: Vec<Pubkey>,
}

/// Per-session transaction state shared across all outgoing submission tasks.
pub struct TxContext {
    pub rpc_client: RpcClient,
    /// Optional secondary RPC used purely for `send_transaction` fan-out.
    pub secondary_send_rpc: Option<Arc<RpcClient>>,
    pub metadata: PhoenixMetadata,
    pub authority_v2: Pubkey,
    pub trader_pda_v2: Pubkey,
    pub market_addrs: MarketAddrs,
    /// Symbol the `market_addrs` were built for.
    pub active_symbol: String,
    pub blockhash_pool: tokio::sync::Mutex<VecDeque<[u8; 32]>>,
    pub(super) ws_url: String,
    pub(super) sig_pubsub: tokio::sync::Mutex<Option<Arc<PubsubClient>>>,
    pub shared_trader: Arc<RwLock<TraderMirror>>,
}

impl TxContext {
    /// Initializes a new transaction context with RPC hooks and exchange metadata.
    pub async fn new(
        keypair: &Keypair,
        symbol: &str,
        http: Arc<phoenix_rise::api::PhoenixHttpClient>,
        shared_trader: Arc<RwLock<TraderMirror>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let rpc_url = rpc_http_url_from_env();
        let fanout_enabled = current_user_config().fanout_public_rpc;
        let secondary_send_rpc = if !fanout_enabled || is_public_mainnet_rpc(&rpc_url) {
            None
        } else {
            Some(Arc::new(RpcClient::new_with_commitment(
                DEFAULT_PUBLIC_SOLANA_RPC_URL.to_string(),
                CommitmentConfig::processed(),
            )))
        };
        let rpc_client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::processed());

        let exchange = http.exchange().get_exchange().await?;
        let metadata = PhoenixMetadata::new(exchange.into());
        let market_addrs = build_market_addrs(&metadata, symbol)?;

        let authority_v2 = keypair.pubkey();
        let trader_pda_v2 = TraderKey::derive_pda(&authority_v2, 0, 0);

        Ok(Self {
            rpc_client,
            secondary_send_rpc,
            metadata,
            authority_v2,
            trader_pda_v2,
            market_addrs,
            active_symbol: symbol.to_string(),
            blockhash_pool: tokio::sync::Mutex::new(VecDeque::with_capacity(30)),
            ws_url: ws_url_from_env(),
            sig_pubsub: tokio::sync::Mutex::new(None),
            shared_trader,
        })
    }

    /// Snapshot the shared `Trader` state if the WS task has hydrated it.
    pub fn snapshot_trader(&self) -> Option<Trader> {
        let guard = match self.shared_trader.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.hydrated.then(|| guard.trader.clone())
    }

    /// Construct an empty (un-hydrated) trader mirror for `authority`.
    pub fn empty_trader_mirror(authority: Pubkey) -> TraderMirror {
        TraderMirror::empty(authority)
    }

    /// Pushes the latest blockhash from the network into the rotating memory pool.
    pub async fn push_blockhash(&self) {
        if let Ok(Ok((bh, _))) = tokio::time::timeout(
            BLOCKHASH_FETCH_TIMEOUT,
            self.rpc_client
                .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed()),
        )
        .await
        {
            let mut pool = self.blockhash_pool.lock().await;
            let bytes = bh.to_bytes();
            if pool.back() != Some(&bytes) {
                pool.push_back(bytes);
                if pool.len() > 30 {
                    pool.pop_front();
                }
            }
        }
    }

    /// Removes and returns the newest warm blockhash from the pool, falling back
    /// to an HTTP fetch if the pool is empty.
    pub async fn pop_blockhash(&self) -> Result<solana_hash::Hash, String> {
        let mut pool = self.blockhash_pool.lock().await;
        if let Some(bytes) = pool.pop_back() {
            Ok(solana_hash::Hash::new_from_array(bytes))
        } else {
            drop(pool);
            tokio::time::timeout(
                BLOCKHASH_FETCH_TIMEOUT,
                self.rpc_client
                    .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed()),
            )
            .await
            .map_err(|_| "blockhash fetch timed out after 5s; check RPC health".to_string())?
            .map(|(hash, _)| hash)
            .map_err(|e| format!("{e}"))
        }
    }

    /// Derive the Phoenix trader PDA for `subaccount_index`.
    pub fn trader_pda_for_subaccount(&self, subaccount_index: u8) -> Pubkey {
        TraderKey::derive_pda(&self.authority_v2, 0, subaccount_index)
    }

    /// Returns whether `symbol` is an isolated-only market in exchange metadata.
    pub fn market_isolated_only(&self, symbol: &str) -> bool {
        self.metadata
            .get_market(symbol)
            .map(|market| market.isolated_only)
            .unwrap_or(false)
    }

    /// Returns the first-tier max leverage for `symbol`, if known.
    pub fn max_leverage_for_symbol(&self, symbol: &str) -> Option<f64> {
        self.metadata
            .get_market(symbol)?
            .leverage_tiers
            .first()
            .map(|tier| tier.max_leverage)
    }

    /// Resolve on-chain account addresses for an arbitrary market `symbol`.
    pub fn market_addrs_for_symbol(&self, symbol: &str) -> Option<MarketAddrs> {
        build_market_addrs(&self.metadata, symbol).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_spline_collection_overrides_stale_api_value() {
        let market_pubkey = "11111111111111111111111111111112";
        let market_pk = Pubkey::from_str(market_pubkey).unwrap();
        let (canonical, _) = program_ids::get_spline_collection_address_default(&market_pk);

        let stale_api = Pubkey::new_unique().to_string();
        assert_ne!(stale_api, canonical.to_string());

        let resolved = derive_spline_collection("TEST-PERP", market_pubkey, &stale_api).unwrap();
        assert_eq!(resolved, canonical);
    }

    #[test]
    fn derive_spline_collection_returns_canonical_when_api_matches() {
        let market_pubkey = "11111111111111111111111111111112";
        let market_pk = Pubkey::from_str(market_pubkey).unwrap();
        let (canonical, _) = program_ids::get_spline_collection_address_default(&market_pk);

        let resolved =
            derive_spline_collection("TEST-PERP", market_pubkey, &canonical.to_string()).unwrap();
        assert_eq!(resolved, canonical);
    }
}
