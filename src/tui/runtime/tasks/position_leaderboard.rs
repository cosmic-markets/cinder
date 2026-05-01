//! Top-position refresh task spawner.

use super::*;

/// Spawn a one-shot top-positions refresh. The refresh scans the on-chain
/// `ActiveTraderBuffer`, converts every active position to display units
/// against the caller-supplied market configs + mark prices, and posts the
/// top-N (sorted by notional) through `tx`. Silent on empty/error — failures
/// just leave the existing list on screen.
pub(in crate::tui::runtime) fn spawn_top_positions_refresh(
    rpc_url: String,
    configs: std::collections::HashMap<String, SplineConfig>,
    marks: std::collections::HashMap<String, f64>,
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
                // If any entry has an unresolved trader (e.g. a brand-new
                // authority registered between GTI refreshes), nudge the GTI
                // loader so the next cycle shows the full pubkey.
                let had_miss = entries.iter().any(|e| e.trader.is_none());
                if had_miss {
                    gti_refresh.notify_one();
                }
                let _ = tx.send(entries);
            }
            Ok(Err(e)) => {
                warn!(error = %e, "top positions refresh failed");
            }
            Err(_) => {
                warn!("top positions refresh timed out");
            }
        }
    })
}
