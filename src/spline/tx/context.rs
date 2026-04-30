//! Per-market transaction context — caches RPC clients, the trader PDA, the
//! market account addresses, a warm blockhash pool, and a shared
//! signatureSubscribe pubsub client used by every submission flow.

use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_signer::Signer;

use super::super::config::{rpc_http_url_from_env, ws_url_from_env};

/// Public Solana mainnet-beta RPC. Every send is fanned out here in addition
/// to the configured primary RPC (unless the primary already is this URL).
const PUBLIC_SOLANA_RPC_URL: &str = "https://api.mainnet-beta.solana.com";

/// Bound transaction preparation on slow/stalled RPCs. Without this, an empty
/// warm blockhash pool can leave the UI stuck at "Broadcasting ..." forever.
pub(super) const BLOCKHASH_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Returns true when `url` points at the public mainnet-beta endpoint, so we
/// avoid double-sending to the same host.
pub(super) fn is_public_mainnet_rpc(url: &str) -> bool {
    url.contains("api.mainnet-beta.solana.com")
}

pub struct MarketAddrs {
    pub perp_asset_map: solana_pubkey::Pubkey,
    pub orderbook: solana_pubkey::Pubkey,
    pub spline_collection: solana_pubkey::Pubkey,
    pub global_trader_index: Vec<solana_pubkey::Pubkey>,
    pub active_trader_buffer: Vec<solana_pubkey::Pubkey>,
}

pub struct TxContext {
    pub rpc_client: solana_rpc_client::nonblocking::rpc_client::RpcClient,
    /// Optional secondary RPC used purely for `send_transaction` fan-out.
    /// Confirmation still listens exclusively on `rpc_client`. `None` when the
    /// primary already targets the public mainnet-beta endpoint.
    pub secondary_send_rpc: Option<Arc<solana_rpc_client::nonblocking::rpc_client::RpcClient>>,
    pub metadata: phoenix_rise::PhoenixMetadata,
    pub authority_v2: solana_pubkey::Pubkey,
    pub trader_pda_v2: solana_pubkey::Pubkey,
    pub market_addrs: MarketAddrs,
    pub trader_registered: std::sync::atomic::AtomicBool,
    pub blockhash_pool: tokio::sync::Mutex<VecDeque<[u8; 32]>>,
    /// Cached WS URL for signature confirmations.
    pub(super) ws_url: String,
    /// Shared WSS client for signature confirmations — all orders multiplex
    /// through a single WebSocket instead of opening one per transaction.
    pub(super) sig_pubsub: tokio::sync::Mutex<Option<Arc<PubsubClient>>>,
}

impl TxContext {
    /// Initializes a new transaction context with RPC hooks and loaded static
    /// margin configurations. Accepts an existing `PhoenixHttpClient` to
    /// avoid opening a redundant SDK connection.
    pub async fn new(
        keypair: &Keypair,
        symbol: &str,
        http: &phoenix_rise::PhoenixHttpClient,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use solana_rpc_client::nonblocking::rpc_client::RpcClient;

        let rpc_url = rpc_http_url_from_env();
        let secondary_send_rpc = if is_public_mainnet_rpc(&rpc_url) {
            None
        } else {
            Some(Arc::new(RpcClient::new_with_commitment(
                PUBLIC_SOLANA_RPC_URL.to_string(),
                CommitmentConfig::processed(),
            )))
        };
        let rpc_client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::processed());

        let exchange = http.get_exchange().await?;
        let metadata = phoenix_rise::PhoenixMetadata::new(exchange.into());

        let keys = metadata.keys();
        let market = metadata
            .get_market(symbol)
            .ok_or_else(|| format!("{} market not found in exchange metadata", symbol))?;
        let market_addrs = MarketAddrs {
            perp_asset_map: solana_pubkey::Pubkey::from_str(&keys.perp_asset_map)?,
            orderbook: solana_pubkey::Pubkey::from_str(&market.market_pubkey)?,
            spline_collection: solana_pubkey::Pubkey::from_str(&market.spline_pubkey)?,
            global_trader_index: keys
                .global_trader_index
                .iter()
                .map(|s| solana_pubkey::Pubkey::from_str(s))
                .collect::<Result<Vec<_>, _>>()?,
            active_trader_buffer: keys
                .active_trader_buffer
                .iter()
                .map(|s| solana_pubkey::Pubkey::from_str(s))
                .collect::<Result<Vec<_>, _>>()?,
        };

        let authority_v2 = solana_pubkey::Pubkey::from_str(&keypair.pubkey().to_string())?;
        let trader_pda_v2 = phoenix_rise::TraderKey::derive_pda(&authority_v2, 0, 0);

        let registered = matches!(
            rpc_client.get_account(&trader_pda_v2).await,
            Ok(acc) if !acc.data.is_empty()
        );

        Ok(Self {
            rpc_client,
            secondary_send_rpc,
            metadata,
            authority_v2,
            trader_pda_v2,
            market_addrs,
            trader_registered: std::sync::atomic::AtomicBool::new(registered),
            blockhash_pool: tokio::sync::Mutex::new(VecDeque::with_capacity(30)),
            ws_url: ws_url_from_env(),
            sig_pubsub: tokio::sync::Mutex::new(None),
        })
    }

    /// Pushes the latest blockhash from the network into the rotating memory
    /// pool.
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

    /// Removes and returns the newest warm blockhash from the pool.
    /// Each blockhash is consumed so it is never reused across transactions.
    /// Using the newest entry maximises remaining validity (~150 blocks on
    /// Solana). Falls back to an HTTP fetch if the pool is empty.
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
            .map_err(|e| format!("{}", e))
        }
    }
}
