//! Phoenix balance fetch task spawner.

use super::*;

pub(in crate::tui::runtime) fn spawn_balance_fetch(
    http: Arc<PhoenixHttpClient>,
    kp: Arc<Keypair>,
    symbol: String,
    balance_tx: UnboundedSender<BalanceUpdate>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let authority_v2 = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
            Ok(pk) => pk,
            Err(e) => {
                warn!(error = %e, "failed to convert wallet pubkey for phoenix fetch");
                return;
            }
        };
        let Ok((phoenix_bal, position, all_positions)) = tokio::time::timeout(
            BALANCE_FETCH_TIMEOUT,
            fetch_phoenix_balance_and_position(&http, &authority_v2, &symbol),
        )
        .await
        else {
            warn!(symbol = %symbol, "phoenix balance fetch timed out");
            return;
        };
        let _ = balance_tx.send(BalanceUpdate {
            phoenix_collateral: phoenix_bal,
            position,
            all_positions,
        });
    })
}
