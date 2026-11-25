//! Wallet lifecycle background tasks: initial connect/referral check, periodic
//! Phoenix balance fetch, `TxContext` bootstrap, and blockhash pool refresh.

use super::*;

const GET_TRADER_RETRIES: usize = 4;
const GET_TRADER_RETRY_DELAY: Duration = Duration::from_millis(750);

/// Fetch Phoenix collateral & positions with timeout and push a [`BalanceUpdate`].
async fn fetch_and_send_balance(
    http: &PhoenixHttpClient,
    authority: &Pubkey,
    symbol: &str,
    balance_tx: &UnboundedSender<BalanceUpdate>,
    label: &str,
) {
    let Ok((phoenix_bal, position, all_positions)) = tokio::time::timeout(
        BALANCE_FETCH_TIMEOUT,
        fetch_phoenix_balance_and_position(http, authority, symbol),
    )
    .await
    else {
        warn!(symbol = %symbol, "{label} timed out");
        return;
    };
    let _ = balance_tx.send(BalanceUpdate {
        phoenix_collateral: phoenix_bal,
        position,
        all_positions,
    });
}

/// Spawn a periodic HTTP balance and positions fetch for `kp`.
pub(in crate::tui::runtime) fn spawn_balance_fetch(
    http: Arc<PhoenixHttpClient>,
    kp: Arc<Keypair>,
    symbol: String,
    balance_tx: UnboundedSender<BalanceUpdate>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let authority = kp.pubkey();
        fetch_and_send_balance(
            &http,
            &authority,
            &symbol,
            &balance_tx,
            "phoenix balance fetch",
        )
        .await;
    })
}

/// On wallet connect: check whether the authority already has a Phoenix account
/// (prompting the referral modal if not), then run the initial balance fetch.
pub(in crate::tui::runtime) fn spawn_initial_connect_flow(
    http: Arc<PhoenixHttpClient>,
    kp: Arc<Keypair>,
    symbol: String,
    balance_tx: UnboundedSender<BalanceUpdate>,
    tx_status: UnboundedSender<TxStatusMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let authority = kp.pubkey();
        let mut final_err: Option<String> = None;

        for attempt in 0..GET_TRADER_RETRIES {
            match http.traders().get_trader_subaccount(&authority, 0, 0).await {
                Ok(None) => {
                    let _ = tx_status.send(TxStatusMsg::PromptReferralChoice);
                    final_err = None;
                    break;
                }
                Ok(Some(_)) => {
                    final_err = None;
                    break;
                }
                Err(e) => {
                    final_err = Some(format!("{e}"));
                    if attempt + 1 < GET_TRADER_RETRIES {
                        tokio::time::sleep(GET_TRADER_RETRY_DELAY).await;
                    }
                }
            }
        }
        if let Some(err) = final_err {
            warn!(error = %err, "get_trader_subaccount failed after retries; skipping referral check");
        }

        fetch_and_send_balance(
            &http,
            &authority,
            &symbol,
            &balance_tx,
            "initial phoenix balance fetch",
        )
        .await;
    })
}

/// Spawn the one-shot [`TxContext`] bootstrap for `kp` and `symbol`.
pub(in crate::tui::runtime) fn spawn_tx_context_task(
    kp: Arc<Keypair>,
    symbol: String,
    http: Arc<PhoenixHttpClient>,
    shared_trader: Arc<std::sync::RwLock<crate::tui::tx::TraderMirror>>,
    ctx_chan: UnboundedSender<TxCtxMsg>,
    status_chan: UnboundedSender<TxStatusMsg>,
) -> tokio::task::JoinHandle<()> {
    let wallet = kp.pubkey();
    tokio::spawn(async move {
        match TxContext::new(&kp, &symbol, Arc::clone(&http), shared_trader).await {
            Ok(ctx) => {
                let _ = ctx_chan.send((wallet, symbol, Arc::new(ctx)));
            }
            Err(e) => {
                warn!(error = %e, "TxContext init failed");
                let _ = status_chan.send(TxStatusMsg::SetStatus {
                    title: format!("Failed to load trading context: {e}"),
                    detail: String::new(),
                });
            }
        }
    })
}

/// Continuously refresh the warm blockhash pool inside `tx_context`.
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
