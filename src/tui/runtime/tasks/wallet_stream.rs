//! Wallet account websocket task spawner.

use super::*;

/// Subscribes to both the USDC ATA and the SOL wallet account on a single
/// shared `PubsubClient` connection. Both initial balances are fetched with one
/// `RpcClient`.
pub(in crate::tui::runtime) fn spawn_wallet_wss(
    pubkey_bytes: [u8; 32],
    ws_url: String,
    usdc_tx: UnboundedSender<f64>,
    sol_tx: UnboundedSender<f64>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use solana_rpc_client::nonblocking::rpc_client::RpcClient;

        // Derive USDC ATA.
        let token_program_id =
            solana_pubkey::Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
                .expect("valid token program id");
        let ata_program_id =
            solana_pubkey::Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
                .expect("valid ata program id");
        let usdc_mint =
            solana_pubkey::Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
                .expect("valid usdc mint id");
        let wallet_pk = solana_pubkey::Pubkey::from(pubkey_bytes);
        let (ata, _) = solana_pubkey::Pubkey::find_program_address(
            &[
                pubkey_bytes.as_ref(),
                token_program_id.as_ref(),
                usdc_mint.as_ref(),
            ],
            &ata_program_id,
        );

        let account_cfg = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::processed()),
            ..Default::default()
        };

        // One RPC client for both initial balance fetches.
        {
            let rpc_client = RpcClient::new_with_commitment(
                rpc_http_url_from_env(),
                CommitmentConfig::processed(),
            );
            if let Ok(res) = rpc_client.get_token_account_balance(&ata).await {
                let _ = usdc_tx.send(res.ui_amount.unwrap_or(0.0));
            }
            if let Ok(lamports) = rpc_client.get_balance(&wallet_pk).await {
                let _ = sol_tx.send(lamports as f64 / 1_000_000_000.0);
            }
        }

        // One shared PubsubClient for both USDC and SOL subscriptions.
        let mut backoff = WSS_RETRY_INIT;
        loop {
            let pubsub = match PubsubClient::new(&ws_url).await {
                Ok(c) => c,
                Err(_) => {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };

            let (mut usdc_stream, usdc_unsub) = match pubsub
                .account_subscribe(&ata, Some(account_cfg.clone()))
                .await
            {
                Ok(s) => s,
                Err(_) => {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };

            let (mut sol_stream, sol_unsub) = match pubsub
                .account_subscribe(&wallet_pk, Some(account_cfg.clone()))
                .await
            {
                Ok(s) => s,
                Err(_) => {
                    usdc_unsub().await;
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WSS_RETRY_CAP);
                    continue;
                }
            };

            backoff = WSS_RETRY_INIT;

            // Drive both streams on the same connection.
            loop {
                tokio::select! {
                    msg = usdc_stream.next() => {
                        let Some(resp) = msg else { break };
                        if let Some(data) = resp.value.data.decode() {
                            if data.len() >= 72 {
                                let raw = u64::from_le_bytes(
                                    data[64..72].try_into().unwrap_or_default(),
                                );
                                let _ = usdc_tx.send(raw as f64 / 1_000_000.0);
                            }
                        }
                    }
                    msg = sol_stream.next() => {
                        let Some(resp) = msg else { break };
                        let _ = sol_tx.send(resp.value.lamports as f64 / 1_000_000_000.0);
                    }
                }
            }

            usdc_unsub().await;
            sol_unsub().await;
            tokio::time::sleep(WSS_RETRY_INIT).await;
        }
    })
}

/// If the HTTP fetch outlives this deadline the task exits and the periodic
/// `balance_interval` tick will schedule a fresh attempt. Prevents a stuck
/// upstream from pinning `balance_fetch_handle` to an unfinished state forever.
pub(super) const BALANCE_FETCH_TIMEOUT: Duration = Duration::from_millis(1500);

/// Upper bound on a single top-positions refresh cycle. The ActiveTraderBuffer
/// plus overflow arenas are a handful of sequential `getAccount` calls, so a
/// well-behaved RPC finishes in well under a second; this is a safety net for
/// a stalled endpoint so the refresh ticker can respawn cleanly.
pub(super) const TOP_POSITIONS_TIMEOUT: Duration = Duration::from_secs(5);
