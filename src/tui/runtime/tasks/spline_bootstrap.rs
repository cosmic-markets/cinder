//! One-shot HTTP fetch of the spline collection account at market-switch time.
//!
//! `accountSubscribe` only pushes when the account changes, so a quiet market
//! (e.g. an isolated-only market with no recent writes) would leave the
//! "Switching to … market…" modal stuck until the next on-chain spline write.
//! This bootstrap closes the gap by pushing the current account state through
//! the same handler the WSS path uses.

use super::*;

const SPLINE_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(3);

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
