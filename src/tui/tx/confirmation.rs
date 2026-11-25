//! Transaction compile/sign/broadcast/confirm pipeline and compute-budget
//! instruction builders.

use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use futures_util::StreamExt;
use solana_commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::{VersionedMessage, v0};
use solana_pubkey::Pubkey;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client_api::config::{RpcSendTransactionConfig, RpcSignatureSubscribeConfig};
use solana_rpc_client_api::response::{ProcessedSignatureResult, Response, RpcSignatureResult};
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use tokio::sync::{mpsc, oneshot};

use super::super::config::{
    current_compute_unit_limit_per_position, current_compute_unit_price_micro_lamports,
    current_user_config,
};
use super::super::i18n::strings;
use super::super::state::{SliceOutcome, TxStatusMsg};
use super::context::TxContext;
use super::error::{
    format_not_confirmed_error, log_tx_error, not_confirmed_is_onchain_execution_failure,
    parse_phoenix_tx_error,
};

static COMPUTE_BUDGET_PROGRAM_ID: LazyLock<Pubkey> = LazyLock::new(|| {
    Pubkey::from_str("ComputeBudget111111111111111111111111111111")
        .expect("hardcoded ComputeBudget pubkey")
});

/// Hard ceiling on how long the RPC send may block.
const SEND_TIMEOUT: Duration = Duration::from_secs(8);

/// How long to wait for `processed` commitment after the transaction is sent.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(8);

/// Bound signature WebSocket connection and subscription setup.
const SIGNATURE_SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Fallback interval between HTTP `get_signature_status` polls while confirming.
const SIGNATURE_HTTP_POLL_INTERVAL: Duration = Duration::from_millis(350);

/// Cadence at which the same signed transaction is re-broadcast while confirming.
const REBROADCAST_INTERVAL: Duration = Duration::from_millis(300);

/// Per-send timeout used inside the rebroadcast loop.
const REBROADCAST_SEND_TIMEOUT: Duration = Duration::from_secs(1);

/// Maximum Phoenix instructions bundled into a single batch transaction.
const BATCH_SIZE: usize = 5;

/// Stagger between consecutive batch transactions.
const BATCH_STAGGER: Duration = Duration::from_millis(500);

/// Compute-unit budget selection appended to every outgoing transaction.
pub(super) enum CuBudget {
    /// Multiply the configured per-position CU limit by `num_positions`.
    Scaled(u32),
    /// Use an explicit raw CU limit (e.g. deposit/withdraw).
    Raw(u32),
}

/// Status strings and optional callbacks for a single transaction execution.
pub(super) struct TxReport {
    pub broadcasting_title: String,
    pub prepare_failed_title: String,
    pub confirming_title: String,
    pub confirmed_title: String,
    pub rejected_title: String,
    pub exec_failed_title: String,
    pub not_confirmed_prefix: String,
    pub log_context: String,
    pub trade_markers: Vec<(String, bool)>,
    pub silent_status: bool,
    pub record_ledger_on_silent: bool,
    pub outcome_tx: Option<oneshot::Sender<SliceOutcome>>,
}

/// One instruction entry fed into [`execute_batched_txs`].
#[derive(Clone)]
pub(super) struct BatchItem {
    pub ix: Instruction,
    pub label: String,
    pub trade_marker: Option<(String, bool)>,
}

/// Localized label templates for [`execute_batched_txs`].
pub(super) struct BatchLabels {
    pub broadcasting: &'static str,
    pub failed_prepare: &'static str,
    pub confirming: &'static str,
    pub batch_noun: &'static str,
    pub log_noun: &'static str,
}

/// Builds the two ComputeBudget instructions scaled by `num_positions`.
pub(super) fn build_compute_budget_ixs(num_positions: u32) -> Vec<Instruction> {
    let limit = current_compute_unit_limit_per_position().saturating_mul(num_positions);
    build_compute_budget_ixs_raw(limit)
}

/// Builds ComputeBudget instructions with an explicit CU limit.
pub(super) fn build_compute_budget_ixs_raw(compute_unit_limit: u32) -> Vec<Instruction> {
    let program_id = *COMPUTE_BUDGET_PROGRAM_ID;

    let mut limit_data = Vec::with_capacity(5);
    limit_data.push(2u8);
    limit_data.extend_from_slice(&compute_unit_limit.to_le_bytes());

    let cu_price = current_compute_unit_price_micro_lamports();
    let mut price_data = Vec::with_capacity(9);
    price_data.push(3u8);
    price_data.extend_from_slice(&cu_price.to_le_bytes());

    vec![
        Instruction {
            program_id,
            accounts: vec![],
            data: limit_data,
        },
        Instruction {
            program_id,
            accounts: vec![],
            data: price_data,
        },
    ]
}

/// Build the per-send RPC config, honoring the user's `skip_preflight` toggle.
fn send_cfg() -> RpcSendTransactionConfig {
    RpcSendTransactionConfig {
        skip_preflight: current_user_config().skip_preflight,
        preflight_commitment: Some(CommitmentLevel::Processed),
        encoding: None,
        max_retries: Some(2),
        min_context_slot: None,
    }
}

/// Compiles a versioned v0 transaction from `ixs` and signs it with `keypair`.
async fn compile_and_sign(
    ctx: &TxContext,
    keypair: &Keypair,
    ixs: &[Instruction],
) -> Result<(VersionedTransaction, Signature), String> {
    let blockhash = ctx.pop_blockhash().await?;
    let message = v0::Message::try_compile(&keypair.pubkey(), ixs, &[], blockhash)
        .map_err(|e| format!("{e}"))?;
    let tx = VersionedTransaction::try_new(VersionedMessage::V0(message), &[keypair])
        .map_err(|e| format!("{e}"))?;
    let sig = tx.signatures[0];
    Ok((tx, sig))
}

/// Distinguishes between RPC rejection and on-chain confirmation failure.
enum ConfirmError {
    /// The RPC refused the transaction (or subscription setup failed).
    Rejected(String),
    /// The transaction was broadcast but its final status failed or timed out.
    NotConfirmed(String),
}

/// Returns the shared `PubsubClient`, creating or reconnecting as needed.
async fn get_or_connect_sig_pubsub(ctx: &TxContext) -> Result<Arc<PubsubClient>, String> {
    let mut guard = ctx.sig_pubsub.lock().await;
    if let Some(ref client) = *guard {
        return Ok(Arc::clone(client));
    }
    let client = tokio::time::timeout(SIGNATURE_SUBSCRIBE_TIMEOUT, PubsubClient::new(&ctx.ws_url))
        .await
        .map_err(|_| "signature WSS connect timed out after 5s".to_string())?
        .map_err(|e| format!("signature WSS connect: {e}"))?;
    let client = Arc::new(client);
    *guard = Some(Arc::clone(&client));
    Ok(client)
}

/// Continuously rebroadcasts the signed transaction until cancelled by `select!`.
async fn rebroadcast_forever(ctx: &TxContext, tx: &VersionedTransaction) -> ! {
    let mut interval = tokio::time::interval(REBROADCAST_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await; // skip immediate first tick
    loop {
        interval.tick().await;
        let primary = tokio::time::timeout(
            REBROADCAST_SEND_TIMEOUT,
            ctx.rpc_client.send_transaction_with_config(tx, send_cfg()),
        );
        let secondary = async {
            if let Some(secondary) = ctx.secondary_send_rpc.as_ref() {
                let _ = tokio::time::timeout(
                    REBROADCAST_SEND_TIMEOUT,
                    secondary.send_transaction_with_config(tx, send_cfg()),
                )
                .await;
            }
        };
        let _ = tokio::join!(primary, secondary);
    }
}

/// Sends the pre-signed transaction and waits for `processed` confirmation,
/// racing a WSS subscription stream against HTTP `get_signature_status` polls.
async fn send_and_confirm_on_stream(
    ctx: &TxContext,
    tx: &VersionedTransaction,
    sig: &Signature,
    stream: &mut (impl futures_util::Stream<Item = Response<RpcSignatureResult>> + Unpin),
) -> Result<(), ConfirmError> {
    // Fire-and-forget fan-out to the public mainnet-beta RPC when distinct from primary.
    if let Some(secondary) = ctx.secondary_send_rpc.as_ref() {
        let secondary = Arc::clone(secondary);
        let tx_clone = tx.clone();
        let cfg = send_cfg();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                SEND_TIMEOUT,
                secondary.send_transaction_with_config(&tx_clone, cfg),
            )
            .await;
        });
    }

    match tokio::time::timeout(
        SEND_TIMEOUT,
        ctx.rpc_client.send_transaction_with_config(tx, send_cfg()),
    )
    .await
    {
        Err(_) => {
            return Err(ConfirmError::Rejected(
                "Transaction timed out — RPC did not respond within 8s".into(),
            ));
        }
        Ok(Err(e)) => return Err(ConfirmError::Rejected(format!("{e:#?}"))),
        Ok(Ok(_)) => {}
    }

    // Race WSS subscription vs HTTP polling while rebroadcasting on a fixed cadence.
    let confirm = tokio::time::timeout(CONFIRM_TIMEOUT, async {
        let wss = async {
            while let Some(resp) = stream.next().await {
                match resp.value {
                    RpcSignatureResult::ProcessedSignature(ProcessedSignatureResult { err }) => {
                        return if err.is_none() {
                            Ok(())
                        } else {
                            Err(format!("transaction failed: {err:?}"))
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
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                match ctx.rpc_client.get_signature_status(sig).await {
                    Ok(Some(Ok(()))) => return Ok(()),
                    Ok(Some(Err(e))) => return Err(format!("transaction failed: {e:?}")),
                    _ => {}
                }
            }
        };

        tokio::select! {
            r = wss => r,
            r = http_poll => r,
        }
    });

    let result = tokio::select! {
        r = confirm => r,
        never = rebroadcast_forever(ctx, tx) => match never {},
    };

    match result {
        Ok(r) => r.map_err(ConfirmError::NotConfirmed),
        Err(_) => Err(ConfirmError::NotConfirmed("confirmation timeout".into())),
    }
}

/// Subscribes to a signature on the shared WebSocket (reconnecting once on
/// stale connection), sends the transaction, and awaits `processed` confirmation.
async fn subscribe_send_confirm(
    ctx: &TxContext,
    tx: &VersionedTransaction,
    sig: &Signature,
) -> Result<(), ConfirmError> {
    let sub_config = RpcSignatureSubscribeConfig {
        commitment: Some(CommitmentConfig::processed()),
        enable_received_notification: Some(false),
    };

    for attempt in 0..2 {
        let client = get_or_connect_sig_pubsub(ctx)
            .await
            .map_err(ConfirmError::Rejected)?;

        match tokio::time::timeout(
            SIGNATURE_SUBSCRIBE_TIMEOUT,
            client.signature_subscribe(sig, Some(sub_config.clone())),
        )
        .await
        {
            Ok(Ok((mut stream, unsubscribe))) => {
                let result = send_and_confirm_on_stream(ctx, tx, sig, &mut stream).await;
                unsubscribe().await;
                return result;
            }
            Err(_) if attempt == 1 => {
                return Err(ConfirmError::Rejected(
                    "signature_subscribe timed out after 5s".to_string(),
                ));
            }
            Ok(Err(e)) if attempt == 1 => {
                return Err(ConfirmError::Rejected(format!("signature_subscribe: {e}")));
            }
            _ => {}
        }
        // First attempt failed — drop the stale shared client and retry once.
        drop(client);
        ctx.sig_pubsub.lock().await.take();
    }
    unreachable!("loop always returns on attempt 1")
}

/// Append compute budget, compile & sign, broadcast, confirm, and report status/ledger/outcome.
/// Returns `Some(signature_string)` if the transaction was compiled and signed, or `None` if
/// preparation failed.
pub(super) async fn execute_tx(
    ctx: &TxContext,
    keypair: &Keypair,
    mut ixs: Vec<Instruction>,
    cu_budget: CuBudget,
    tx_status: &mpsc::UnboundedSender<TxStatusMsg>,
    mut report: TxReport,
) -> Option<String> {
    let send_status = |msg: TxStatusMsg| {
        if report.silent_status && msg.is_per_slice_chatter() {
            return;
        }
        let _ = tx_status.send(msg);
    };
    let record_ledger = |title: String, sig: String| {
        if report.silent_status && report.record_ledger_on_silent {
            let _ = tx_status.send(TxStatusMsg::LedgerOnly {
                title,
                signature: sig,
            });
        }
    };

    match cu_budget {
        CuBudget::Scaled(n) => ixs.extend(build_compute_budget_ixs(n)),
        CuBudget::Raw(limit) => ixs.extend(build_compute_budget_ixs_raw(limit)),
    }

    send_status(TxStatusMsg::SetStatus {
        title: report.broadcasting_title,
        detail: String::new(),
    });

    let (tx, sig) = match compile_and_sign(ctx, keypair, &ixs).await {
        Ok(pair) => pair,
        Err(e) => {
            send_status(TxStatusMsg::SetStatus {
                title: report.prepare_failed_title,
                detail: e.clone(),
            });
            if let Some(tx) = report.outcome_tx.take() {
                let _ = tx.send(SliceOutcome::Failed(e));
            }
            return None;
        }
    };

    let sig_str = sig.to_string();
    send_status(TxStatusMsg::SetStatus {
        title: report.confirming_title,
        detail: sig_str.clone(),
    });

    match subscribe_send_confirm(ctx, &tx, &sig).await {
        Ok(()) => {
            for (symbol, is_buy) in report.trade_markers {
                send_status(TxStatusMsg::TradeMarker { symbol, is_buy });
            }
            send_status(TxStatusMsg::SetStatus {
                title: report.confirmed_title.clone(),
                detail: sig_str.clone(),
            });
            record_ledger(report.confirmed_title, sig_str.clone());
            if let Some(tx) = report.outcome_tx.take() {
                let _ = tx.send(SliceOutcome::Confirmed);
            }
        }
        Err(ConfirmError::Rejected(e)) => {
            log_tx_error(None, &format!("{} rejected", report.log_context), &e);
            let detail = parse_phoenix_tx_error(&e);
            send_status(TxStatusMsg::SetStatus {
                title: report.rejected_title.clone(),
                detail: detail.clone(),
            });
            record_ledger(report.rejected_title, sig_str.clone());
            if let Some(tx) = report.outcome_tx.take() {
                let _ = tx.send(SliceOutcome::Failed(detail));
            }
        }
        Err(ConfirmError::NotConfirmed(e)) => {
            log_tx_error(
                Some(&sig_str),
                &format!("{} not confirmed", report.log_context),
                &e,
            );
            if not_confirmed_is_onchain_execution_failure(&e) {
                let detail = parse_phoenix_tx_error(&e);
                send_status(TxStatusMsg::SetStatus {
                    title: report.exec_failed_title.clone(),
                    detail: detail.clone(),
                });
                record_ledger(report.exec_failed_title, sig_str.clone());
                if let Some(tx) = report.outcome_tx.take() {
                    let _ = tx.send(SliceOutcome::Failed(detail));
                }
            } else {
                let title = format!(
                    "{} ({})",
                    report.not_confirmed_prefix,
                    format_not_confirmed_error(&e)
                );
                send_status(TxStatusMsg::SetStatus {
                    title: title.clone(),
                    detail: sig_str.clone(),
                });
                record_ledger(title, sig_str.clone());
                if let Some(tx) = report.outcome_tx.take() {
                    let _ = tx.send(SliceOutcome::Unknown(sig_str.clone()));
                }
            }
        }
    }

    Some(sig_str)
}

/// Chunk `items` into transactions of [`BATCH_SIZE`] instructions and execute
/// each batch with a [`BATCH_STAGGER`] delay between batches. Returns the last
/// signed signature (or empty string if preparation failed on the first batch).
pub(super) async fn execute_batched_txs(
    ctx: &TxContext,
    keypair: &Keypair,
    items: Vec<BatchItem>,
    labels: BatchLabels,
    tx_status: &mpsc::UnboundedSender<TxStatusMsg>,
) -> String {
    let s = strings();
    let batches: Vec<Vec<BatchItem>> = items.chunks(BATCH_SIZE).map(|c| c.to_vec()).collect();
    let num_batches = batches.len();
    let mut last_sig = String::new();

    for (batch_idx, batch) in batches.into_iter().enumerate() {
        let batch_num = batch_idx + 1;
        // Deduplicate display labels while preserving order.
        let mut seen_labels = HashSet::new();
        let batch_label = batch
            .iter()
            .filter(|item| seen_labels.insert(item.label.clone()))
            .map(|item| item.label.clone())
            .collect::<Vec<_>>()
            .join(", ");

        // Deduplicate per-symbol trade markers within the batch.
        let mut seen_markers = HashSet::new();
        let trade_markers: Vec<(String, bool)> = batch
            .iter()
            .filter_map(|item| {
                let (sym, is_buy) = item.trade_marker.as_ref()?;
                seen_markers
                    .insert(sym.clone())
                    .then(|| (sym.clone(), *is_buy))
            })
            .collect();

        let ixs: Vec<Instruction> = batch.into_iter().map(|item| item.ix).collect();
        let ops_in_batch = ixs.len().max(1) as u32;

        let report = TxReport {
            broadcasting_title: format!(
                "{} {batch_num}/{num_batches}: {batch_label}…",
                labels.broadcasting
            ),
            prepare_failed_title: format!("{} {batch_num}/{num_batches}", labels.failed_prepare),
            confirming_title: format!(
                "{} {batch_num}/{num_batches}: {batch_label}…",
                labels.confirming
            ),
            confirmed_title: format!(
                "✅ {} {batch_num}/{num_batches} {} {batch_label}",
                labels.batch_noun, s.tx_batch_confirmed_suf
            ),
            rejected_title: format!(
                "❌ {} {batch_num}/{num_batches} {}",
                labels.batch_noun, s.tx_batch_rejected_suf
            ),
            exec_failed_title: format!(
                "❌ {} {batch_num}/{num_batches} {}",
                labels.batch_noun, s.tx_batch_exec_failed_suf
            ),
            not_confirmed_prefix: format!(
                "{} {batch_num}/{num_batches} {}",
                labels.batch_noun, s.tx_batch_not_confirmed_suf
            ),
            log_context: format!("{} batch {batch_num}/{num_batches}", labels.log_noun),
            trade_markers,
            silent_status: false,
            record_ledger_on_silent: false,
            outcome_tx: None,
        };

        let Some(sig_str) = execute_tx(
            ctx,
            keypair,
            ixs,
            CuBudget::Scaled(ops_in_batch),
            tx_status,
            report,
        )
        .await
        else {
            break;
        };
        last_sig = sig_str;

        if batch_num < num_batches {
            tokio::time::sleep(BATCH_STAGGER).await;
        }
    }

    last_sig
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_compute_budget_ixs_raw() {
        let ixs = build_compute_budget_ixs_raw(100_000);
        assert_eq!(ixs.len(), 2);

        let limit_ix = &ixs[0];
        assert_eq!(
            limit_ix.program_id.to_string(),
            "ComputeBudget111111111111111111111111111111"
        );
        assert_eq!(limit_ix.data[0], 2);
        let limit_val =
            u32::from_le_bytes(limit_ix.data[1..5].try_into().expect("5-byte limit ix"));
        assert_eq!(limit_val, 100_000);

        let price_ix = &ixs[1];
        assert_eq!(
            price_ix.program_id.to_string(),
            "ComputeBudget111111111111111111111111111111"
        );
        assert_eq!(price_ix.data[0], 3);
        let price_val =
            u64::from_le_bytes(price_ix.data[1..9].try_into().expect("9-byte price ix"));
        assert_eq!(price_val, current_compute_unit_price_micro_lamports());
    }

    #[test]
    fn test_build_compute_budget_ixs_scaled() {
        let ixs = build_compute_budget_ixs(2);
        assert_eq!(ixs.len(), 2);

        let limit_ix = &ixs[0];
        let limit_val =
            u32::from_le_bytes(limit_ix.data[1..5].try_into().expect("5-byte limit ix"));
        assert_eq!(limit_val, current_compute_unit_limit_per_position() * 2);
    }
}
