use std::str::FromStr;

use clap::{Parser, Subcommand};
use cosmic_phoenix_eternal_types::events::{InnerInstructionContext, MarketEvent, MarketEventHeader};
use cosmic_phoenix_eternal_types::{program_ids, Side};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::{
    EncodedTransaction, UiInnerInstructions, UiInstruction, UiMessage, UiTransactionEncoding,
};
#[cfg(unix)]
use std::collections::HashMap;

pub const QUOTE_LOT_DECIMALS: u8 = 6;

const MAINNET_RPC: &str = "https://api.mainnet-beta.solana.com";
const LOCALNET_RPC: &str = "http://127.0.0.1:8899";

// =============================================================================
// CLI
// =============================================================================

#[derive(Parser)]
#[command(name = "spline-fill-subscriber")]
#[command(about = "Parse and subscribe to spline fills on Phoenix Eternal")]
struct Cli {
    /// RPC URL: 'm' for mainnet, 'l' for localnet, or custom URL
    #[arg(short = 'u', long = "url", global = true)]
    url: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse spline fills from a single transaction
    Tx {
        /// Transaction signature or Solscan URL
        signature: String,

        /// Spline trader pubkey to filter fills for
        #[arg(long)]
        spline: String,
    },

    /// Subscribe to live spline fills via Yellowstone gRPC
    Subscribe {
        /// Spline trader pubkey to filter fills for
        #[arg(long)]
        spline: String,

        /// Yellowstone gRPC endpoint URL
        #[arg(long)]
        grpc_url: String,

        /// Optional x-token for gRPC authentication
        #[arg(long)]
        x_token: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let rpc_url = resolve_rpc_url(cli.url.as_deref());

    eprintln!("Using RPC: {}\n", rpc_url);

    let result = match cli.command {
        Commands::Tx { signature, spline } => cmd_tx(&rpc_url, &signature, &spline).await,
        Commands::Subscribe {
            spline,
            grpc_url,
            x_token,
        } => cmd_subscribe(&rpc_url, &spline, &grpc_url, x_token.as_deref()).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        let mut source = e.source();
        while let Some(cause) = source {
            eprintln!("  caused by: {}", cause);
            source = cause.source();
        }
        std::process::exit(1);
    }
}

// =============================================================================
// Commands
// =============================================================================

async fn cmd_tx(
    rpc_url: &str,
    tx_input: &str,
    spline_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = RpcClient::new(rpc_url.to_string());
    let spline_trader = Pubkey::from_str(spline_str)?;
    let sig = parse_tx_signature(tx_input)?;

    let fills = fetch_and_parse_tx(&client, &sig, spline_trader).await?;
    print_spline_fills(&fills, spline_trader);
    Ok(())
}

#[cfg(unix)]
async fn cmd_subscribe(
    rpc_url: &str,
    spline_str: &str,
    grpc_url: &str,
    x_token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    use futures::{SinkExt, StreamExt};
    use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
    use yellowstone_grpc_proto::geyser::{
        subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
        SubscribeRequestFilterTransactions, SubscribeRequestPing,
    };

    let spline_trader = Pubkey::from_str(spline_str)?;
    let program_id = program_ids::PHOENIX_ETERNAL_PROGRAM_ID;

    eprintln!("RPC:  {}", rpc_url);
    eprintln!("gRPC: {}", grpc_url);
    eprintln!("Spline trader: {}", spline_trader);
    eprintln!("Subscribing to transactions for {}...\n", program_id);

    // Extract token from URL path if not provided explicitly via --x-token.
    // e.g. https://ellipsis.rpcpool.com/7ba0a839-... → endpoint=https://ellipsis.rpcpool.com, token=7ba0a839-...
    let (endpoint, token) = if let Some(t) = x_token {
        (grpc_url.to_string(), Some(t.to_string()))
    } else if let Some(idx) = grpc_url.rfind('/') {
        let path_segment = &grpc_url[idx + 1..];
        if !path_segment.is_empty() && path_segment != "/" {
            (grpc_url[..idx].to_string(), Some(path_segment.to_string()))
        } else {
            (grpc_url.to_string(), None)
        }
    } else {
        (grpc_url.to_string(), None)
    };

    let mut grpc_client = GeyserGrpcClient::build_from_shared(endpoint.clone())?
        .x_token(token)?
        .tls_config(ClientTlsConfig::new().with_enabled_roots())?
        .max_decoding_message_size(33_554_432) // 8x the limit of 4MB
        .connect()
        .await?;

    let mut transactions = HashMap::new();
    transactions.insert(
        "phoenix_eternal".to_string(),
        SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: vec![program_id.to_string()],
            ..Default::default()
        },
    );

    let request = SubscribeRequest {
        transactions,
        commitment: Some(CommitmentLevel::Confirmed as i32),
        ..Default::default()
    };

    let (mut subscribe_tx, mut stream) = grpc_client.subscribe_with_request(Some(request)).await?;

    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Stream error: {}", e);
                continue;
            }
        };

        let update = match msg.update_oneof {
            Some(UpdateOneof::Transaction(tx_update)) => tx_update,
            Some(UpdateOneof::Ping(_)) => {
                // Respond to server pings to keep the connection alive
                let _ = subscribe_tx
                    .send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    })
                    .await;
                continue;
            }
            _ => continue,
        };

        let tx_update = update;

        let Some(ref tx_info) = tx_update.transaction else {
            continue;
        };

        let fills = match parse_yellowstone_tx(tx_info, spline_trader) {
            Ok(f) => f,
            Err(_) => {
                continue;
            }
        };

        if !fills.spline_fills.is_empty() || !fills.crossing_fills.is_empty() {
            let sig = bs58::encode(&tx_info.signature).into_string();
            eprintln!("--- tx: {} ---", sig);
            print_spline_fills(&fills, spline_trader);
            eprintln!();
        }
    }

    Ok(())
}

#[cfg(not(unix))]
async fn cmd_subscribe(
    _rpc_url: &str,
    _spline_str: &str,
    _grpc_url: &str,
    _x_token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    Err("Yellowstone gRPC subscription support is only available on Unix targets".into())
}

// =============================================================================
// Data types
// =============================================================================

#[derive(Debug)]
pub struct SplineFills {
    pub asset: String,
    pub crossing_fills: Vec<RetailOrderFill>,
    pub spline_fills: Vec<SplineMakerFill>,
}

impl SplineFills {
    pub fn new(asset: String) -> Self {
        Self {
            asset,
            crossing_fills: Vec::new(),
            spline_fills: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct SplineMakerFill {
    pub last_price_in_ticks: u64,
    pub was_multi_level: bool,
    /// Raw values
    pub base_lots_filled: u64,
    pub virtual_quote_lots_filled: u64,

    pub maker_side: Side,

    /// Human readable values
    pub quantity: f64,
    pub fill_price: f64,
    pub notional_size: f64,
    pub taker: Pubkey,
}

#[derive(Debug)]
pub struct RetailOrderFill {
    pub price_in_ticks: u64,
    /// Raw values
    pub base_lots_filled: u64,
    pub virtual_quote_lots_filled: u64,

    pub taker_side: Side,

    /// Human readable values
    pub quantity: f64,
    pub fill_price: f64,
    pub notional_size: f64,
    pub maker: Pubkey,
}

// =============================================================================
// Event parsing
// =============================================================================

pub fn parse_events(
    events: &[MarketEvent],
    spline_trader: Pubkey,
) -> Result<SplineFills, anyhow::Error> {
    let header = fetch_header(events)?;

    let asset = header.asset_symbol.to_string();
    let tick_size = header.tick_size as u64;
    let base_lot_decimals = header.base_lot_decimals;

    let quote_lots_per_quote_unit = 10f64.powi(QUOTE_LOT_DECIMALS as i32);
    let base_lots_per_base_unit = 10f64.powi(base_lot_decimals as i32);

    let mut spline_fills = SplineFills::new(asset);
    let mut retail_orders: Vec<RetailOrderFill> = Vec::new();
    // Track the current taker from TradeSummary events that precede spline fills.
    // The event ordering within a single match is: [OrderFilled|SplineFilled]* TradeSummary
    // But across multiple matches in a tx, a TradeSummary identifies the taker for the
    // preceding fills. We accumulate spline fills into a pending buffer and assign the
    // taker when the TradeSummary arrives.
    let mut pending_spline_fills: Vec<SplineMakerFill> = Vec::new();

    for event in events {
        match event {
            MarketEvent::SplineFilled(spline_filled) => {
                if spline_filled.maker == spline_trader {
                    let price_in_ticks = spline_filled.price.as_inner();
                    let base_lots_filled = spline_filled.base_lots_filled.as_inner();
                    let virtual_quote_lots_filled = spline_filled.quote_lots_filled.as_inner();

                    let quantity = base_lots_filled as f64 / base_lots_per_base_unit;

                    let last_tick_price =
                        price_in_ticks as f64 * tick_size as f64 / quote_lots_per_quote_unit;

                    let fill_price = (virtual_quote_lots_filled as f64 / quote_lots_per_quote_unit)
                        / (base_lots_filled as f64 / base_lots_per_base_unit);

                    let notional_size = quantity * fill_price;

                    pending_spline_fills.push(SplineMakerFill {
                        last_price_in_ticks: price_in_ticks,
                        was_multi_level: fill_price != last_tick_price,
                        base_lots_filled,
                        virtual_quote_lots_filled,
                        maker_side: spline_filled.side,
                        quantity,
                        fill_price,
                        notional_size,
                        taker: Pubkey::default(), // filled in on TradeSummary
                    });
                }
            }
            MarketEvent::OrderFilled(order_filled) => {
                // OrderFilled where the spline is the taker crossing the book
                if order_filled.maker != spline_trader {
                    // We don't know the taker yet — accumulate and assign on TradeSummary
                    let price_in_ticks = order_filled.price.as_inner();
                    let base_lots_filled = order_filled.base_lots_filled.as_inner();
                    let virtual_quote_lots_filled = order_filled.quote_lots_filled.as_inner();

                    let quantity = base_lots_filled as f64 / base_lots_per_base_unit;

                    let fill_price =
                        price_in_ticks as f64 * tick_size as f64 / quote_lots_per_quote_unit;

                    let notional_size = quantity * fill_price;

                    retail_orders.push(RetailOrderFill {
                        price_in_ticks,
                        base_lots_filled,
                        virtual_quote_lots_filled,
                        taker_side: order_filled.side,
                        quantity,
                        fill_price,
                        notional_size,
                        maker: order_filled.maker,
                    });
                }
            }
            MarketEvent::TradeSummary(trade_summary) => {
                let taker = trade_summary.trader;

                if taker != spline_trader {
                    // This taker is someone else — assign them to pending spline fills
                    for fill in pending_spline_fills.drain(..) {
                        spline_fills
                            .spline_fills
                            .push(SplineMakerFill { taker, ..fill });
                    }
                    pending_spline_fills.clear();
                } else {
                    // The spline trader is the taker — commit crossing fills
                    spline_fills.crossing_fills.append(&mut retail_orders);
                }
                // Always clear the retail orders after processing a trade summary
                retail_orders.clear();
            }
            _ => {}
        }
    }

    Ok(spline_fills)
}

pub fn fetch_header(events: &[MarketEvent]) -> Result<MarketEventHeader, anyhow::Error> {
    for event in events {
        if let MarketEvent::Header(header) = event {
            return Ok(*header);
        }
    }
    Err(anyhow::anyhow!("No header found"))
}

// =============================================================================
// Display
// =============================================================================

fn print_spline_fills(fills: &SplineFills, spline_trader: Pubkey) {
    if fills.spline_fills.is_empty() && fills.crossing_fills.is_empty() {
        println!("No fills for this spline trader.");
        return;
    }

    let green = "\x1b[32m";
    let red = "\x1b[31m";
    let reset = "\x1b[0m";

    // Crossing fills: spline is taker, taker_side is from the resting maker's perspective
    // Maker Bid = maker was buying, so spline was selling = SELL
    // Maker Ask = maker was selling, so spline was buying = BUY
    for f in &fills.crossing_fills {
        let (side_str, color) = match f.taker_side {
            Side::Bid => ("SELL", red),
            Side::Ask => ("BUY", green),
        };
        println!(
            "{}{:<6} | {:<4} | ${:<12.4} | {:<12.6} | ${:<10.2} | taker: {} | maker: {} | aggressor: true{}",
            color, fills.asset, side_str, f.fill_price, f.quantity, f.notional_size,
            spline_trader, f.maker, reset,
        );
    }

    // Spline maker fills: spline is maker, side is from spline's perspective
    // Bid = spline buying = BUY, Ask = spline selling = SELL
    for f in &fills.spline_fills {
        let (side_str, color) = match f.maker_side {
            Side::Bid => ("BUY", green),
            Side::Ask => ("SELL", red),
        };
        println!(
            "{}{:<6} | {:<4} | ${:<12.4} | {:<12.6} | ${:<10.2} | taker: {} | maker: {} | aggressor: false{}",
            color, fills.asset, side_str, f.fill_price, f.quantity, f.notional_size,
            f.taker, spline_trader, reset,
        );
    }
}

// =============================================================================
// Transaction helpers
// =============================================================================

async fn fetch_and_parse_tx(
    client: &RpcClient,
    sig: &Signature,
    spline_trader: Pubkey,
) -> Result<SplineFills, Box<dyn std::error::Error>> {
    let config = solana_client::rpc_config::RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    let tx_response = client.get_transaction_with_config(sig, config).await?;

    let meta = tx_response
        .transaction
        .meta
        .ok_or("transaction has no metadata")?;

    let account_keys = extract_account_keys(&tx_response.transaction.transaction)?;

    let inner_ixs = meta
        .inner_instructions
        .ok_or("transaction has no inner instructions")?;

    let program_id = program_ids::PHOENIX_ETERNAL_PROGRAM_ID;
    let parsed_ixs = flatten_inner_instructions(&inner_ixs, &account_keys)?;

    let market_events =
        cosmic_phoenix_eternal_types::events::parse_events_from_inner_instructions_with_context(
            &program_id,
            &parsed_ixs,
        );

    let fills = parse_events(&market_events, spline_trader)?;
    Ok(fills)
}

/// Parse a Yellowstone gRPC transaction directly into SplineFills without re-fetching via RPC.
#[cfg(unix)]
fn parse_yellowstone_tx(
    tx_info: &yellowstone_grpc_proto::geyser::SubscribeUpdateTransactionInfo,
    spline_trader: Pubkey,
) -> Result<SplineFills, Box<dyn std::error::Error>> {
    let tx = tx_info
        .transaction
        .as_ref()
        .ok_or("transaction missing from update")?;
    let meta = tx_info
        .meta
        .as_ref()
        .ok_or("transaction meta missing from update")?;
    let msg = tx.message.as_ref().ok_or("transaction message missing")?;

    // Build account keys: static keys from message + loaded addresses from meta
    let mut account_keys: Vec<Pubkey> = msg
        .account_keys
        .iter()
        .map(|k| Pubkey::try_from(k.as_slice()))
        .collect::<Result<Vec<_>, _>>()?;

    for addr in &meta.loaded_writable_addresses {
        account_keys.push(Pubkey::try_from(addr.as_slice())?);
    }
    for addr in &meta.loaded_readonly_addresses {
        account_keys.push(Pubkey::try_from(addr.as_slice())?);
    }

    // Flatten inner instructions into the format expected by the event parser
    let program_id = program_ids::PHOENIX_ETERNAL_PROGRAM_ID;
    let mut parsed_ixs: Vec<(InnerInstructionContext, Pubkey, Vec<u8>)> = Vec::new();

    for group in &meta.inner_instructions {
        for ix in &group.instructions {
            let pid = *account_keys
                .get(ix.program_id_index as usize)
                .ok_or("program_id_index out of bounds")?;
            let context = (group.index as u8, ix.stack_height);
            parsed_ixs.push((context, pid, ix.data.clone()));
        }
    }

    let market_events =
        cosmic_phoenix_eternal_types::events::parse_events_from_inner_instructions_with_context(
            &program_id,
            &parsed_ixs,
        );

    let fills = parse_events(&market_events, spline_trader)?;
    Ok(fills)
}

fn parse_tx_signature(input: &str) -> Result<Signature, Box<dyn std::error::Error>> {
    let sig_str = input
        .strip_prefix("https://solscan.io/tx/")
        .or_else(|| input.strip_prefix("http://solscan.io/tx/"))
        .unwrap_or(input)
        .split('?')
        .next()
        .unwrap_or(input);

    Signature::from_str(sig_str)
        .map_err(|e| format!("invalid signature '{}': {}", sig_str, e).into())
}

fn extract_account_keys(
    encoded_tx: &EncodedTransaction,
) -> Result<Vec<Pubkey>, Box<dyn std::error::Error>> {
    match encoded_tx {
        EncodedTransaction::Json(ui_tx) => match &ui_tx.message {
            UiMessage::Raw(raw) => raw
                .account_keys
                .iter()
                .map(|s| Pubkey::from_str(s).map_err(|e| e.into()))
                .collect(),
            UiMessage::Parsed(parsed) => parsed
                .account_keys
                .iter()
                .map(|k| Pubkey::from_str(&k.pubkey).map_err(|e| e.into()))
                .collect(),
        },
        _ => Err("unexpected transaction encoding".into()),
    }
}

fn flatten_inner_instructions(
    inner_ixs: &[UiInnerInstructions],
    account_keys: &[Pubkey],
) -> Result<Vec<(InnerInstructionContext, Pubkey, Vec<u8>)>, Box<dyn std::error::Error>> {
    let mut result = Vec::new();

    for group in inner_ixs {
        for ix in &group.instructions {
            match ix {
                UiInstruction::Compiled(compiled) => {
                    let pid = *account_keys
                        .get(compiled.program_id_index as usize)
                        .ok_or("program_id_index out of bounds")?;
                    let data = bs58::decode(&compiled.data).into_vec()?;
                    let context = (group.index, compiled.stack_height);
                    result.push((context, pid, data));
                }
                UiInstruction::Parsed(_) => {}
            }
        }
    }

    Ok(result)
}

// =============================================================================
// URL helpers
// =============================================================================

fn resolve_rpc_url(url: Option<&str>) -> String {
    if let Some(url) = url {
        match url {
            "m" | "mainnet" => return MAINNET_RPC.to_string(),
            "l" | "localnet" => return LOCALNET_RPC.to_string(),
            _ => return url.to_string(),
        }
    }

    if let Some(config_url) = read_solana_config_url() {
        return config_url;
    }

    MAINNET_RPC.to_string()
}

fn read_solana_config_url() -> Option<String> {
    let home = dirs_next::home_dir()?;
    let config_path = home.join(".config/solana/cli/config.yml");

    if !config_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&config_path).ok()?;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("json_rpc_url:") {
            let url = line.trim_start_matches("json_rpc_url:").trim();
            let url = url.trim_matches('"').trim_matches('\'');
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }

    None
}
