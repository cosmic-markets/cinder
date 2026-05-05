//! Transaction compile/sign/broadcast pipeline — builds a
//! `VersionedTransaction` from a recent blockhash, broadcasts it, and waits
//! for confirmation via the `signatureSubscribe` WebSocket feed.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use solana_commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_keypair::Keypair;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client_types::config::{RpcSendTransactionConfig, RpcSignatureSubscribeConfig};
use solana_rpc_client_types::response::{ProcessedSignatureResult, Response, RpcSignatureResult};
use solana_signature::Signature;
use solana_signer::Signer;

use super::context::TxContext;

pub(super) const SEND_CFG: RpcSendTransactionConfig = RpcSendTransactionConfig {
    skip_preflight: false,
    preflight_commitment: Some(CommitmentLevel::Processed),
    encoding: None,
    max_retries: Some(2),
    min_context_slot: None,
};

/// Hard ceiling on how long the RPC send may block.
pub(super) const SEND_TIMEOUT: Duration = Duration::from_secs(8);

/// How long to wait for `processed` commitment after the transaction is sent.
pub(super) const CONFIRM_TIMEOUT: Duration = Duration::from_secs(8);

/// Bound signature WebSocket connection and subscription setup.
const SIGNATURE_SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Fallback interval between HTTP `get_signature_status` polls while confirming
/// (WSS path races this loop).
const SIGNATURE_HTTP_POLL_INTERVAL: Duration = Duration::from_millis(350);

/// Compiles a versioned v0 transaction from `ixs` and signs it with `keypair`.
/// Returns the signed transaction and its signature so callers can subscribe
/// to confirmation *before* broadcasting.
pub(super) async fn compile_and_sign(
    ctx: &TxContext,
    keypair: &Keypair,
    ixs: &[solana_instruction::Instruction],
) -> Result<
    (
        solana_transaction::versioned::VersionedTransaction,
        Signature,
    ),
    String,
> {
    use solana_message::{v0, VersionedMessage};
    use solana_transaction::versioned::VersionedTransaction;

    let blockhash = ctx.pop_blockhash().await?;
    let message = v0::Message::try_compile(&keypair.pubkey(), ixs, &[], blockhash)
        .map_err(|e| format!("{}", e))?;
    let tx = VersionedTransaction::try_new(VersionedMessage::V0(message), &[keypair])
        .map_err(|e| format!("{}", e))?;
    let sig = tx.signatures[0];
    Ok((tx, sig))
}

/// Distinguishes between RPC rejection and on-chain confirmation failure.
pub(super) enum ConfirmError {
    /// The RPC refused the transaction (or we failed to subscribe/connect).
    Rejected(String),
    /// The transaction was broadcast but its final status is still unknown to
    /// this client. It may still land; callers should show the signature and
    /// avoid implying failure.
    NotConfirmed(String),
}

/// Returns the shared `PubsubClient`, creating or reconnecting as needed.
/// The returned `Arc` keeps the client alive even if a later caller replaces
/// it.
pub(super) async fn get_or_connect_sig_pubsub(
    ctx: &TxContext,
) -> Result<Arc<PubsubClient>, String> {
    let mut guard = ctx.sig_pubsub.lock().await;
    if let Some(ref client) = *guard {
        return Ok(Arc::clone(client));
    }
    let client = tokio::time::timeout(SIGNATURE_SUBSCRIBE_TIMEOUT, PubsubClient::new(&ctx.ws_url))
        .await
        .map_err(|_| "signature WSS connect timed out after 5s".to_string())?
        .map_err(|e| format!("signature WSS connect: {}", e))?;
    let client = Arc::new(client);
    *guard = Some(Arc::clone(&client));
    Ok(client)
}

/// Sends the pre-signed transaction and waits for `processed` confirmation,
/// racing a WSS subscription stream against HTTP `get_signature_status` polls.
/// Whichever source responds first wins; the other is abandoned.
pub(super) async fn send_and_confirm_on_stream(
    ctx: &TxContext,
    tx: &solana_transaction::versioned::VersionedTransaction,
    sig: &Signature,
    stream: &mut (impl futures_util::Stream<Item = Response<RpcSignatureResult>> + Unpin),
) -> Result<(), ConfirmError> {
    // --- send ---------------------------------------------------------------
    // Fire-and-forget fan-out to the public mainnet-beta RPC (if distinct from
    // the primary). We don't await or propagate its result — the primary RPC
    // remains authoritative for send success and confirmation.
    if let Some(secondary) = ctx.secondary_send_rpc.as_ref() {
        let secondary = Arc::clone(secondary);
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                SEND_TIMEOUT,
                secondary.send_transaction_with_config(&tx_clone, SEND_CFG),
            )
            .await;
        });
    }

    let send_result = tokio::time::timeout(SEND_TIMEOUT, {
        ctx.rpc_client.send_transaction_with_config(tx, SEND_CFG)
    })
    .await;

    match send_result {
        Err(_) => {
            return Err(ConfirmError::Rejected(
                "Transaction timed out — RPC did not respond within 8s".into(),
            ));
        }
        Ok(Err(e)) => {
            return Err(ConfirmError::Rejected(format!("{:#?}", e)));
        }
        Ok(Ok(_)) => {}
    }

    // --- confirm: race WSS stream vs HTTP polling ---------------------------
    let result = tokio::time::timeout(CONFIRM_TIMEOUT, async {
        let wss = async {
            while let Some(resp) = stream.next().await {
                match resp.value {
                    RpcSignatureResult::ProcessedSignature(ProcessedSignatureResult { err }) => {
                        return if err.is_none() {
                            Ok(())
                        } else {
                            Err(format!("transaction failed: {:?}", err))
                        };
                    }
                    RpcSignatureResult::ReceivedSignature(_) => continue,
                }
            }
            Err("signature subscription closed before confirmation".into())
        };

        let http_poll = async {
            let mut interval = tokio::time::interval(SIGNATURE_HTTP_POLL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await; // skip the immediate first tick
            loop {
                interval.tick().await;
                match ctx.rpc_client.get_signature_status(sig).await {
                    Ok(Some(Ok(()))) => return Ok(()),
                    Ok(Some(Err(e))) => return Err(format!("transaction failed: {:?}", e)),
                    _ => {} // not yet landed or transient RPC error — keep polling
                }
            }
        };

        tokio::select! {
            r = wss => r,
            r = http_poll => r,
        }
    })
    .await;

    match result {
        Ok(r) => r.map_err(ConfirmError::NotConfirmed),
        Err(_) => Err(ConfirmError::NotConfirmed("confirmation timeout".into())),
    }
}

/// Subscribes to a signature on the shared WebSocket, sends the transaction,
/// then waits for `processed` confirmation on the already-open stream.
///
/// Subscribing *before* sending eliminates the race where a fast validator
/// confirms the transaction before the WebSocket subscription is established.
pub(super) async fn subscribe_send_confirm(
    ctx: &TxContext,
    tx: &solana_transaction::versioned::VersionedTransaction,
    sig: &Signature,
) -> Result<(), ConfirmError> {
    let sub_config = RpcSignatureSubscribeConfig {
        commitment: Some(CommitmentConfig::processed()),
        enable_received_notification: Some(false),
    };

    // --- subscribe first ---------------------------------------------------
    let client = get_or_connect_sig_pubsub(ctx)
        .await
        .map_err(ConfirmError::Rejected)?;

    // Happy path: subscribe succeeds on the current shared connection.
    if let Ok(Ok((mut stream, unsubscribe))) = tokio::time::timeout(
        SIGNATURE_SUBSCRIBE_TIMEOUT,
        client.signature_subscribe(sig, Some(sub_config.clone())),
    )
    .await
    {
        let result = send_and_confirm_on_stream(ctx, tx, sig, &mut stream).await;
        unsubscribe().await;
        return result;
    }

    // Subscribe failed — connection likely stale. Reconnect once.
    drop(client);
    {
        ctx.sig_pubsub.lock().await.take();
    }
    let client = get_or_connect_sig_pubsub(ctx)
        .await
        .map_err(ConfirmError::Rejected)?;
    let (mut stream, unsubscribe) = tokio::time::timeout(
        SIGNATURE_SUBSCRIBE_TIMEOUT,
        client.signature_subscribe(sig, Some(sub_config)),
    )
    .await
    .map_err(|_| ConfirmError::Rejected("signature_subscribe timed out after 5s".to_string()))?
    .map_err(|e| ConfirmError::Rejected(format!("signature_subscribe: {}", e)))?;

    let result = send_and_confirm_on_stream(ctx, tx, sig, &mut stream).await;
    unsubscribe().await;
    result
}
