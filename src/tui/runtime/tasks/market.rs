//! One-shot market data tasks: top-positions leaderboard refresh and spline
//! account HTTP bootstrap on market switch.

use super::*;

const SPLINE_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(3);

/// Spawn a one-shot top-positions refresh scanning `ActiveTraderBuffer`.
pub(in crate::tui::runtime) fn spawn_top_positions_refresh(
    rpc_url: String,
    configs: HashMap<String, SplineConfig>,
    marks: HashMap<String, f64>,
    gti_cache: GtiHandle,
    gti_refresh: Arc<tokio::sync::Notify>,
    tx: UnboundedSender<Vec<TopPositionEntry>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let fut = async {
            let cache_guard = gti_cache.read().await;
            fetch_top_positions(&rpc_url, &configs, &marks, cache_guard.as_ref()).await
        };
        match tokio::time::timeout(TOP_POSITIONS_TIMEOUT, fut).await {
            Ok(Ok(entries)) => {
                if entries.iter().any(|e| e.trader.is_none()) {
                    gti_refresh.notify_one();
                }
                let _ = tx.send(entries);
            }
            Ok(Err(e)) => warn!(error = %e, "top positions refresh failed"),
            Err(_) => warn!("top positions refresh timed out"),
        }
    })
}

/// Spawn a one-shot HTTP `getAccountInfo` fetch for `spline_collection` so
/// quiet markets render immediately on market switch.
pub(in crate::tui::runtime) fn spawn_spline_bootstrap_fetch(
    symbol: String,
    spline_collection: String,
    rpc_url: String,
    tx: UnboundedSender<SplineBootstrapMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let pk = match Pubkey::from_str(&spline_collection) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(symbol = %symbol, error = %e, "spline bootstrap: invalid pubkey");
                return;
            }
        };
        let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::processed());
        let fetch = client.get_account_with_commitment(&pk, CommitmentConfig::processed());
        let resp = match tokio::time::timeout(SPLINE_BOOTSTRAP_TIMEOUT, fetch).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                warn!(symbol = %symbol, error = %e, "spline bootstrap fetch failed");
                return;
            }
            Err(_) => {
                warn!(symbol = %symbol, "spline bootstrap fetch timed out");
                return;
            }
        };
        let Some(account) = resp.value else {
            warn!(symbol = %symbol, "spline bootstrap: account not found");
            return;
        };
        let _ = tx.send(SplineBootstrapMsg {
            symbol,
            slot: resp.context.slot,
            data: account.data,
        });
    })
}
