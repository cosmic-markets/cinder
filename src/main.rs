//! Binary entry: library holds all ingest logic; this file only starts the
//! runtime.

use std::env;

use tracing::{info, warn};

/// Install default ring provider; required for TLS (WSS) when rustls has no
/// default backend.
fn init_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

// Default multi-thread runtime spawns ~N OS threads (≈N × reserved stack); cap
// workers for lower RSS.
#[tokio::main(worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load `.env` before tracing init so `RUST_LOG` from the file applies.
    let dotenv_outcome = dotenvy::dotenv().map_err(|e| e.to_string());

    let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let level = match log_level.to_lowercase().as_str() {
        "debug" => tracing::Level::DEBUG,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        "trace" => tracing::Level::TRACE,
        _ => tracing::Level::INFO,
    };

    // Route tracing output to a sink so it doesn't corrupt the TUI or create
    // arbitrary files. RPC errors are still written manually to error_logs.txt
    // by tx.rs
    tracing_subscriber::fmt()
        .with_writer(std::io::sink)
        .with_max_level(level)
        .init();

    match dotenv_outcome {
        Ok(path) => info!(?path, "loaded .env"),
        Err(e) => warn!(error = %e, "failed to load .env"),
    }
    init_rustls_crypto_provider();
    cinder::run().await
}
