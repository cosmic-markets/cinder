//! Transaction context task spawners.

use super::*;

pub(in crate::tui::runtime) fn spawn_tx_context_task(
    kp: Arc<Keypair>,
    symbol: String,
    http: Arc<PhoenixHttpClient>,
    ctx_chan: UnboundedSender<TxCtxMsg>,
    status_chan: UnboundedSender<TxStatusMsg>,
) -> tokio::task::JoinHandle<()> {
    // `kp.pubkey()` is a v3 `Address`; the channel carries v2 `Pubkey` to stay
    // consistent with the rest of the Phoenix-side code. String bridge mirrors
    // the conversion done elsewhere.
    let wallet = match solana_pubkey::Pubkey::from_str(&kp.pubkey().to_string()) {
        Ok(pk) => pk,
        Err(e) => {
            warn!(error = %e, "failed to convert wallet pubkey for TxContext task");
            // Return a no-op task so the caller always has a handle — avoids an
            // `Option<JoinHandle>` everywhere for a pure-logic failure.
            return tokio::spawn(async {});
        }
    };
    tokio::spawn(async move {
        match TxContext::new(&kp, &symbol, &http).await {
            Ok(ctx) => {
                let ctx = Arc::new(ctx);
                let _ = ctx_chan.send((wallet, symbol, ctx));
            }
            Err(e) => {
                warn!(error = %e, "TxContext init failed");
                let _ = status_chan.send(TxStatusMsg::SetStatus {
                    title: format!("Failed to load trading context: {}", e),
                    detail: String::new(),
                });
            }
        }
    })
}

pub(in crate::tui::runtime) fn spawn_blockhash_refresh_task(
    tx_context: Arc<TxContext>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1800));
        loop {
            interval.tick().await;
            tx_context.push_blockhash().await;
        }
    })
}
