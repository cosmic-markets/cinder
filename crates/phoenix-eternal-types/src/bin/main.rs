//! Phoenix Eternal CLI - Inspect Phoenix Eternal accounts on Solana.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand};
use cosmic_phoenix_eternal_types::{
    discriminant::accounts as disc, events, program_ids, sokoban::Superblock,
    ActiveTraderBufferTree, DynamicTraderHeader, FIFORestingOrder, GlobalConfiguration,
    GlobalTraderIndexTree, MarketEvent, Orderbook, PerpAssetMapRef, SplineCollectionRef,
};
use serde::{Deserialize, Serialize};
use solana_account_decoder::UiAccountEncoding;
use solana_client::pubsub_client::PubsubClient;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_pubkey::Pubkey;
use solana_sdk::account::Account;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::Signature;
use solana_transaction_status::{
    EncodedTransaction, UiInnerInstructions, UiInstruction, UiMessage, UiTransactionEncoding,
};

const MAINNET_RPC: &str = "https://api.mainnet-beta.solana.com";
const LOCALNET_RPC: &str = "http://127.0.0.1:8899";
const DEFAULT_PROGRAM_ID: &str = "EtrnLzgbS7nMMy5fbD42kXiUzGg8XQzJ972Xtk1cjWih";
pub const FIRE_ORANGE: &str = "\x1b[38;2;255;165;90m";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredInnerInstruction {
    instruction_index: u8,
    stack_height: Option<u32>,
    program_id: String,
    data_base58: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTransaction {
    signature: String,
    instructions: Vec<StoredInnerInstruction>,
}

#[derive(Parser)]
#[command(name = "phoenix-eternal")]
#[command(about = "Inspect Phoenix Eternal accounts on Solana", long_about = None)]
struct Cli {
    /// RPC URL: 'm' for mainnet, 'l' for localnet, or custom URL (default: from solana config)
    #[arg(short = 'u', long = "url", global = true)]
    url: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show global configuration
    Config,

    /// Show all markets (perp asset map)
    Markets,

    /// Show orderbook for a market
    Orderbook {
        /// Market symbol (SOL, BTC, etc.) or market account pubkey
        market: String,
    },

    /// Show spline collections
    Splines {
        /// Market symbol or pubkey (default: SOL only)
        market: Option<String>,

        /// Single fetch instead of polling every second
        #[arg(long)]
        once: bool,
    },

    /// Show trader account details
    Trader {
        /// Trader authority wallet pubkey OR trader PDA key
        pubkey: String,

        /// PDA index (default: 0, only used if pubkey is authority)
        #[arg(long, default_value = "0")]
        pda_index: u8,

        /// Subaccount index (default: 0, only used if pubkey is authority)
        #[arg(long, default_value = "0")]
        subaccount: u8,
    },

    /// Parse and display events from a transaction
    Events {
        /// Transaction signature or Solscan URL
        tx: String,
    },

    /// Show trader positions from GlobalTraderIndex and ActiveTraderBuffer
    Positions {
        /// Trader authority wallet pubkey OR trader PDA key
        pubkey: String,

        /// PDA index (default: 0, only used if pubkey is authority)
        #[arg(long, default_value = "0")]
        pda_index: u8,

        /// Subaccount index (default: 0, only used if pubkey is authority)
        #[arg(long, default_value = "0")]
        subaccount: u8,
    },

    /// Fetch recent transactions touching a program and save inner instructions to disk
    FetchTxs {
        /// Program ID to query
        #[arg(long, default_value = DEFAULT_PROGRAM_ID)]
        program_id: String,

        /// Number of recent transactions to fetch
        #[arg(long, default_value_t = 1000)]
        limit: usize,

        /// Output directory for saved transaction JSON files
        #[arg(long)]
        out_dir: String,
    },

    /// Iterate dumped transactions and report any event parse failures
    VerifyEventDumps {
        /// Program ID used for event parsing
        #[arg(long, default_value = DEFAULT_PROGRAM_ID)]
        program_id: String,

        /// Directory containing JSON tx dumps from `fetch-txs`
        #[arg(long)]
        dir: String,
    },
}

fn main() {
    let cli = Cli::parse();
    init_tracing(&cli.command);

    let rpc_url = resolve_rpc_url(cli.url.as_deref());
    println!("Using RPC: {}\n", rpc_url);

    let client = RpcClient::new(rpc_url.clone());

    let result = match cli.command {
        Commands::Config => cmd_config(&client),
        Commands::Markets => cmd_markets(&client),
        Commands::Orderbook { market } => cmd_orderbook(&client, &market),
        Commands::Splines { market, once } => {
            cmd_splines(&client, &rpc_url, market.as_deref(), !once)
        }
        Commands::Events { tx } => cmd_events(&client, &tx),
        Commands::Trader {
            pubkey,
            pda_index,
            subaccount,
        } => cmd_trader(&client, &pubkey, pda_index, subaccount),
        Commands::Positions {
            pubkey,
            pda_index,
            subaccount,
        } => cmd_positions(&client, &pubkey, pda_index, subaccount),
        Commands::FetchTxs {
            program_id,
            limit,
            out_dir,
        } => cmd_fetch_txs(&client, &program_id, limit, &out_dir),
        Commands::VerifyEventDumps { program_id, dir } => cmd_verify_event_dumps(&program_id, &dir),
    };

    if let Err(e) = result {
        eprintln!("\x1b[31mError: {}\x1b[0m", e);
        std::process::exit(1);
    }
}

fn init_tracing(command: &Commands) {
    let default_filter = match command {
        Commands::VerifyEventDumps { .. } => "cosmic_phoenix_eternal_types::events::parser=debug",
        _ => "info",
    };

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .try_init();
}

/// Resolve RPC URL from flags or solana config
/// Accepts 'm' for mainnet, 'l' for localnet, or a custom URL
fn resolve_rpc_url(url: Option<&str>) -> String {
    if let Some(url) = url {
        match url {
            "m" | "mainnet" => return MAINNET_RPC.to_string(),
            "l" | "localnet" => return LOCALNET_RPC.to_string(),
            _ => return url.to_string(),
        }
    }

    // Try reading from solana config
    if let Some(config_url) = read_solana_config_url() {
        return config_url;
    }

    // Fallback to mainnet
    MAINNET_RPC.to_string()
}

fn rpc_url_to_ws_url(rpc_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(rest) = rpc_url.strip_prefix("https://") {
        return Ok(format!("wss://{}", rest));
    }
    if let Some(rest) = rpc_url.strip_prefix("http://") {
        return Ok(format!("ws://{}", rest));
    }
    if rpc_url.starts_with("ws://") || rpc_url.starts_with("wss://") {
        return Ok(rpc_url.to_string());
    }
    Err(format!(
        "unsupported RPC URL scheme for websocket conversion: {}",
        rpc_url
    )
    .into())
}

/// Read RPC URL from ~/.config/solana/cli/config.yml
fn read_solana_config_url() -> Option<String> {
    let home = dirs_next::home_dir()?;
    let config_path = home.join(".config/solana/cli/config.yml");

    if !config_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&config_path).ok()?;

    // Simple YAML parsing for json_rpc_url
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("json_rpc_url:") {
            let url = line.trim_start_matches("json_rpc_url:").trim();
            // Remove quotes if present
            let url = url.trim_matches('"').trim_matches('\'');
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }

    None
}

// =============================================================================
// Command Implementations
// =============================================================================

fn cmd_config(client: &RpcClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Global Configuration ===\n");

    let (global_config_key, _) = program_ids::get_global_config_address_default();
    println!("Address: {}", global_config_key);

    let account = client.get_account(&global_config_key)?;
    let global_config: &GlobalConfiguration = bytemuck::from_bytes(&account.data);

    println!(
        "discriminant: {} (valid: {})",
        global_config.discriminant,
        global_config.discriminant == *disc::GLOBAL_CONFIGURATION
    );
    println!("account_key: {}", global_config.account_key);
    println!(
        "canonical_token_mint_key: {}",
        global_config.canonical_token_mint_key
    );
    println!("global_vault_key: {}", global_config.global_vault_key);
    println!("perp_asset_map_key: {}", global_config.perp_asset_map_key);
    println!(
        "global_trader_index_header_key: {}",
        global_config.global_trader_index_header_key
    );
    println!(
        "active_trader_buffer_header_key: {}",
        global_config.active_trader_buffer_header_key
    );
    println!("withdraw_queue_key: {}", global_config.withdraw_queue_key);
    println!(
        "exchange_status: {:?}",
        global_config.exchange_status.as_u8()
    );
    println!("quote_decimals: {}", global_config.quote_decimals());
    println!(
        "withdrawal_margin_factor_bps: {}",
        global_config.withdrawal_margin_factor_bps()
    );
    println!(
        "deposit_cooldown_period_in_slots: {}",
        global_config.deposit_cooldown_period_in_slots()
    );

    Ok(())
}

fn cmd_markets(client: &RpcClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Markets (Perp Asset Map) ===\n");

    let (global_config, _) = load_global_config(client)?;
    let perp_asset_map_data = client.get_account(&global_config.perp_asset_map_key)?.data;
    let perp_asset_map = PerpAssetMapRef::load_from_buffer(&perp_asset_map_data);

    println!("Address: {}", global_config.perp_asset_map_key);
    println!("Active markets: {}\n", perp_asset_map.len());

    for (i, (symbol, metadata)) in perp_asset_map.iter().enumerate() {
        let symbol_str = String::from_utf8_lossy(symbol.as_bytes())
            .trim_end_matches('\0')
            .to_string();

        let tick_size_raw = metadata.tick_size().as_inner();
        let bld = metadata.base_lot_decimals();
        let base_lot_to_units = base_lots_to_units(1, bld);
        let tick_size_dollars_per_unit = ticks_to_price(1, tick_size_raw, bld);
        let mark_price_in_ticks = metadata.mark_price();
        let mark_price_ui = mark_price_in_ticks as f64 * tick_size_dollars_per_unit;

        let oi_lots = metadata.open_interest().as_inner();
        let oi_cap_lots = metadata.open_interest_cap().as_inner();
        let oi_units = base_lots_to_units(oi_lots, bld);
        let oi_cap_units = base_lots_to_units(oi_cap_lots, bld);

        println!(
            "[{}] \x1b[1m{}\x1b[0m (asset_id: {})",
            i,
            symbol_str.trim(),
            metadata.asset_id().as_inner()
        );
        println!("    market_account: {}", metadata.market_account());
        println!("    mark_price: {:.4}", mark_price_ui,);
        println!(
            "    base_lot_size: {} {} (decimals: {})",
            base_lot_to_units,
            symbol_str.trim(),
            metadata.base_lot_decimals()
        );
        println!(
            "    tick_size: ${:.6}/{}/tick ({} quote_lots/base_lot/tick)",
            tick_size_dollars_per_unit,
            symbol_str.trim(),
            tick_size_raw
        );
        println!(
            "    open_interest: {:.4} {} ({} lots)",
            oi_units,
            symbol_str.trim(),
            oi_lots
        );
        println!(
            "    open_interest_cap: {:.4} {} ({} lots)",
            oi_cap_units,
            symbol_str.trim(),
            oi_cap_lots
        );
        println!();
    }

    Ok(())
}

fn cmd_events(client: &RpcClient, tx: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Transaction Events ===\n");

    let sig = parse_tx_signature(tx)?;
    println!("Signature: {}\n", sig);

    let config = solana_client::rpc_config::RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    let tx_response = client.get_transaction_with_config(&sig, config)?;

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

    let phoenix_ix_count = parsed_ixs
        .iter()
        .filter(|(_, pid, _)| *pid == program_id)
        .count();
    println!(
        "Inner instructions: {} total, {} from Phoenix Eternal\n",
        parsed_ixs.len(),
        phoenix_ix_count
    );

    let market_events =
        events::parse_events_from_inner_instructions_with_context(&program_id, &parsed_ixs);

    if market_events.is_empty() {
        println!("No events found.");
        return Ok(());
    }

    println!("Parsed {} event(s):\n", market_events.len());
    println!("{}", serde_json::to_string_pretty(&market_events)?);

    Ok(())
}

fn cmd_fetch_txs(
    client: &RpcClient,
    program_id: &str,
    limit: usize,
    out_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Fetch Program Transactions ===\n");

    let program = Pubkey::from_str(program_id)?;
    println!("Program ID: {}", program);
    println!("Target tx count: {}", limit);
    println!("Output dir: {}\n", out_dir);

    fs::create_dir_all(out_dir)?;

    let signatures = fetch_recent_signatures(client, &program, limit)?;
    println!("Fetched {} signature(s)\n", signatures.len());

    let tx_config = solana_client::rpc_config::RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    for (i, sig) in signatures.iter().enumerate() {
        let tx_response = client.get_transaction_with_config(sig, tx_config)?;
        let dumped = dump_transaction(
            sig,
            &tx_response.transaction.transaction,
            tx_response.transaction.meta,
        )?;
        let path = Path::new(out_dir).join(format!("{}.json", sig));
        fs::write(path, serde_json::to_vec_pretty(&dumped)?)?;

        if (i + 1) % 50 == 0 || i + 1 == signatures.len() {
            println!("Saved {}/{} tx dumps", i + 1, signatures.len());
        }
    }

    println!("\nDone.");
    Ok(())
}

fn cmd_verify_event_dumps(program_id: &str, dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Verify Event Dumps ===\n");

    let program = Pubkey::from_str(program_id)?;
    println!("Program ID: {}", program);
    println!("Input dir: {}\n", dir);

    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut files_checked = 0usize;
    let mut total_events = 0usize;

    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let bytes = fs::read(&path)?;
        let dumped: StoredTransaction = serde_json::from_slice(&bytes)?;

        let parsed_ixs: Result<Vec<_>, Box<dyn std::error::Error>> = dumped
            .instructions
            .iter()
            .map(|ix| {
                let program_id = Pubkey::from_str(&ix.program_id)?;
                let data = bs58::decode(&ix.data_base58).into_vec()?;
                Ok::<_, Box<dyn std::error::Error>>((
                    (ix.instruction_index, ix.stack_height),
                    program_id,
                    data,
                ))
            })
            .collect();

        let parsed_ixs = parsed_ixs?;
        let events =
            events::parse_events_from_inner_instructions_with_context_strict(&program, &parsed_ixs)
                .map_err(|err| {
                    format!(
                        "failed to parse events in {} ({}): {}",
                        dumped.signature,
                        path.display(),
                        err
                    )
                })?;

        files_checked += 1;
        total_events += events.len();
    }

    println!(
        "Verified {} file(s), parsed {} event(s).",
        files_checked, total_events
    );
    Ok(())
}

fn fetch_recent_signatures(
    client: &RpcClient,
    program_id: &Pubkey,
    limit: usize,
) -> Result<Vec<Signature>, Box<dyn std::error::Error>> {
    let mut collected = Vec::new();
    let mut before: Option<Signature> = None;

    while collected.len() < limit {
        let request_limit = (limit - collected.len()).min(1_000);
        let config = solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
            before,
            until: None,
            limit: Some(request_limit),
            commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
        };

        let page = client.get_signatures_for_address_with_config(program_id, config)?;
        if page.is_empty() {
            break;
        }

        for sig_info in &page {
            let sig = Signature::from_str(&sig_info.signature)?;
            collected.push(sig);
            if collected.len() >= limit {
                break;
            }
        }

        before = page
            .last()
            .and_then(|sig_info| Signature::from_str(&sig_info.signature).ok());
        if before.is_none() {
            break;
        }
    }

    Ok(collected)
}

fn dump_transaction(
    sig: &Signature,
    encoded_tx: &EncodedTransaction,
    meta: Option<solana_transaction_status::UiTransactionStatusMeta>,
) -> Result<StoredTransaction, Box<dyn std::error::Error>> {
    let account_keys = extract_account_keys(encoded_tx)?;

    let mut instructions = Vec::new();
    if let Some(meta) = meta {
        if let solana_transaction_status::option_serializer::OptionSerializer::Some(inner_ixs) =
            meta.inner_instructions
        {
            for group in inner_ixs {
                for ix in &group.instructions {
                    if let UiInstruction::Compiled(compiled) = ix {
                        let pid = *account_keys
                            .get(compiled.program_id_index as usize)
                            .ok_or("program_id_index out of bounds")?;

                        instructions.push(StoredInnerInstruction {
                            instruction_index: group.index,
                            stack_height: compiled.stack_height,
                            program_id: pid.to_string(),
                            data_base58: compiled.data.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(StoredTransaction {
        signature: sig.to_string(),
        instructions,
    })
}

/// Extract a tx signature from either a raw base58 string or a Solscan URL.
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

/// Pull account keys out of the encoded transaction.
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

/// Flatten inner instructions into ((instruction_index, stack_height), program_id, data) tuples.
fn flatten_inner_instructions(
    inner_ixs: &[UiInnerInstructions],
    account_keys: &[Pubkey],
) -> Result<Vec<(events::InnerInstructionContext, Pubkey, Vec<u8>)>, Box<dyn std::error::Error>> {
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
                UiInstruction::Parsed(_) => {
                    // Skip parsed instructions (system/token program etc.)
                }
            }
        }
    }

    Ok(result)
}

/// Convert a number of price ticks to a dollar price.
///
/// Formula: `ticks * tick_size * 10^base_lot_decimals / 10^6`
/// (quote lot decimals are fixed at 6)
fn ticks_to_price(ticks: u64, tick_size: u64, base_lot_decimals: i8) -> f64 {
    ticks as f64 * tick_size as f64 * 10_f64.powi(base_lot_decimals as i32) / 1_000_000.0
}

/// Convert base lots to human-readable base units.
///
/// Formula: `lots / 10^base_lot_decimals`
fn base_lots_to_units(lots: u64, base_lot_decimals: i8) -> f64 {
    let divisor = 10_f64.powi(base_lot_decimals as i32);
    if divisor == 0.0 {
        return 0.0;
    }
    lots as f64 / divisor
}

/// First four characters of the base58 pubkey (short trader label in CLI output).
fn pubkey_trader_prefix(trader: &Pubkey) -> String {
    trader.to_string().chars().take(4).collect()
}

#[allow(dead_code)]
fn print_event(index: usize, event: &MarketEvent) {
    match event {
        MarketEvent::SlotContext(e) => {
            println!("[{}] \x1b[1mSlotContext\x1b[0m", index);
            println!("    slot: {}, timestamp: {}", e.slot, e.timestamp);
        }
        MarketEvent::Header(e) => {
            println!("[{}] \x1b[1mHeader\x1b[0m", index);
            println!(
                "    asset: {} (id: {}), tick_size: {}",
                e.asset_symbol, e.asset_id, e.tick_size
            );
            println!("    seq: {}, signer: {}", e.sequence_number, e.signer);
            println!("    trader: {}", e.trader_account);
        }
        MarketEvent::OrderPlaced(e) => {
            println!("[{}] \x1b[1mOrderPlaced\x1b[0m", index);
            println!(
                "    order_id: {}, price: {}, qty: {}",
                e.order_id, e.price, e.quantity
            );
            println!(
                "    seq: {}, flags: {:?}",
                e.order_sequence_number, e.order_flags
            );
        }
        MarketEvent::OrderFilled(e) => {
            println!("[{}] \x1b[1mOrderFilled\x1b[0m", index);
            println!(
                "    side: {:?}, price: {}, base_filled: {}, quote_filled: {}",
                e.side, e.price, e.base_lots_filled, e.quote_lots_filled
            );
            println!(
                "    maker: {}, remaining: {}",
                e.maker, e.quantity_remaining
            );
        }
        MarketEvent::OrderRejected(e) => {
            println!("[{}] \x1b[1mOrderRejected\x1b[0m", index);
            println!(
                "    side: {:?}, price: {}, lots: {}",
                e.side, e.price, e.num_base_lots
            );
            println!("    reason: {}", e.reason_str());
        }
        MarketEvent::SplineFilled(e) => {
            println!("[{}] \x1b[1mSplineFilled\x1b[0m", index);
            println!(
                "    side: {:?}, price: {}, base_filled: {}, quote_filled: {}",
                e.side, e.price, e.base_lots_filled, e.quote_lots_filled
            );
            println!("    maker: {}", e.maker);
        }
        MarketEvent::TradeSummary(e) => {
            println!("[{}] \x1b[1mTradeSummary\x1b[0m", index);
            println!("    trader: {}, side: {:?}", e.trader, e.side);
            println!(
                "    base_filled: {}, quote_filled: {}, fee: {}",
                e.base_lots_filled, e.quote_lots_filled, e.fee_in_quote_lots
            );
        }
        MarketEvent::OrderModified(e) => {
            println!("[{}] \x1b[1mOrderModified\x1b[0m", index);
            println!(
                "    seq: {}, price: {}, reason: {:?}",
                e.order_sequence_number, e.price, e.reason
            );
            println!(
                "    base_released: {}, remaining: {}",
                e.base_lots_released, e.base_lots_remaining
            );
        }
        MarketEvent::MarketSummary(e) => {
            println!("[{}] \x1b[1mMarketSummary\x1b[0m", index);
            println!(
                "    asset: {} (id: {}), OI: {}",
                e.asset_symbol, e.asset_id, e.open_interest
            );
            println!("    mark: {}, spot: {}", e.mark_price, e.spot_price);
        }
        MarketEvent::TraderFundsDeposited(e) => {
            println!("[{}] \x1b[1mTraderFundsDeposited\x1b[0m", index);
            println!("    trader: {}, amount: {}", e.trader, e.amount);
            println!("    new_balance: {}", e.new_collateral_balance);
        }
        MarketEvent::TraderFundsWithdrawn(e) => {
            println!("[{}] \x1b[1mTraderFundsWithdrawn\x1b[0m", index);
            println!("    trader: {}, amount: {}", e.trader, e.amount);
        }
        MarketEvent::TraderFundingSettled(e) => {
            println!("[{}] \x1b[1mTraderFundingSettled\x1b[0m", index);
            println!(
                "    trader: {}, asset: {} (id: {})",
                e.trader, e.asset_symbol, e.asset_id
            );
            println!(
                "    payment: {}, new_balance: {}",
                e.funding_payment, e.new_collateral_balance
            );
        }
        MarketEvent::MarketStatusChanged(e) => {
            println!("[{}] \x1b[1mMarketStatusChanged\x1b[0m", index);
            println!(
                "    {:?} -> {:?}",
                e.previous_market_status, e.new_market_status
            );
        }
        MarketEvent::PricesUpdated(e) => {
            println!("[{}] \x1b[1mPricesUpdated\x1b[0m", index);
            println!(
                "    asset: {} (id: {}), mark: {}",
                e.asset_symbol, e.asset_id, e.new_mark_price
            );
            if let Some(bid) = &e.new_best_bid {
                print!("    bid: {}", bid);
            }
            if let Some(ask) = &e.new_best_ask {
                print!(", ask: {}", ask);
            }
            println!();
        }
        MarketEvent::Liquidation(e) => {
            println!("[{}] \x1b[1mLiquidation\x1b[0m", index);
            println!(
                "    liquidator: {}, trader: {}",
                e.liquidator, e.liquidated_trader
            );
            println!(
                "    asset_id: {}, size: {}, mark: {}",
                e.asset_id, e.liquidation_size, e.mark_price
            );
        }
        MarketEvent::PnL(e) => {
            println!("[{}] \x1b[1mPnL\x1b[0m", index);
            println!(
                "    trader: {}, asset: {} (id: {})",
                e.trader, e.asset_symbol, e.asset_id
            );
            println!(
                "    realized_pnl: {}, funding: {}",
                e.realized_pnl, e.funding_payment
            );
            println!(
                "    base: {} -> {}, vquote: {} -> {}",
                e.base_lots_before,
                e.base_lots_after,
                e.virtual_quote_lots_before,
                e.virtual_quote_lots_after
            );
        }
        other => {
            println!("[{}] \x1b[33mUnhandled\x1b[0m {:?}", index, other);
        }
    }
}

fn cmd_orderbook(client: &RpcClient, market: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Orderbook ===\n");

    let (global_config, _) = load_global_config(client)?;
    let perp_asset_map_data = client.get_account(&global_config.perp_asset_map_key)?.data;
    let perp_asset_map = PerpAssetMapRef::load_from_buffer(&perp_asset_map_data);

    let (market_key, symbol_str, bld, tick_size_raw) = resolve_market(&perp_asset_map, market)?;

    println!("Market: {} ({})", symbol_str, market_key);

    let mut market_data = client.get_account(&market_key)?.data;
    let orderbook = Orderbook::load_from_buffer(&mut market_data);
    let header = orderbook.header;

    println!(
        "sequence_number: {}",
        header.sequence_number().sequence_number()
    );
    println!(
        "order_sequence_number: {}",
        header.order_sequence_number.sequence_number()
    );
    println!("num_bids: {}", orderbook.num_bids());
    println!("num_asks: {}", orderbook.num_asks());
    println!();

    // Load GTI for trader pubkey resolution
    let gti_tree = load_global_trader_index(client)?;

    // Helper to resolve trader_position_id to pubkey
    let resolve_pubkey = |order: &FIFORestingOrder| -> String {
        let addr = order.trader_position_id().trader_id().to_u32_or_sentinel();
        gti_tree
            .tree
            .get_node_from_pointer(addr)
            .map(|(pk, _)| pk.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    };

    // Show asks (reversed to show lowest first at bottom)
    let asks: Vec<_> = orderbook.asks_tree.iter().take(10).collect();
    println!("--- Asks (top 10) ---");
    for (order_id, order) in asks.into_iter().rev() {
        let price_ticks = order_id.price_in_ticks.as_inner();
        let price_dollars = ticks_to_price(price_ticks, tick_size_raw, bld);
        let size_lots = order.num_base_lots_remaining().as_inner();
        let size_units = base_lots_to_units(size_lots, bld);
        let trader_pubkey = resolve_pubkey(order);
        println!(
            "\x1b[31m  ${:>12.4}  {:>12.6} {}\x1b[0m",
            price_dollars, size_units, trader_pubkey
        );
    }

    println!("--- Spread ---");

    // Show bids
    println!("--- Bids (top 10) ---");
    for (order_id, order) in orderbook.bids_tree.iter().take(10) {
        let price_ticks = order_id.price_in_ticks.as_inner();
        let price_dollars = ticks_to_price(price_ticks, tick_size_raw, bld);
        let size_lots = order.num_base_lots_remaining().as_inner();
        let size_units = base_lots_to_units(size_lots, bld);
        let trader_pubkey = resolve_pubkey(order);
        println!(
            "\x1b[32m  ${:>12.4}  {:>12.6} {}\x1b[0m",
            price_dollars, size_units, trader_pubkey
        );
    }

    Ok(())
}

fn cmd_splines(
    client: &RpcClient,
    rpc_url: &str,
    market: Option<&str>,
    watch: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let effective = market.unwrap_or("SOL");
    if watch && effective.trim().eq_ignore_ascii_case("SOL") {
        return cmd_splines_ws(client, rpc_url, effective);
    }

    loop {
        if watch {
            print!("\x1b[2J\x1b[H");
        }

        println!("=== Spline Collections ===\n");

        let (global_config, _) = match load_global_config(client) {
            Ok(value) => value,
            Err(e) => {
                println!("  Failed to load global config: {}\n", e);
                if watch {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                break;
            }
        };

        let perp_asset_map_data = match client.get_account_with_commitment(
            &global_config.perp_asset_map_key,
            CommitmentConfig::processed(),
        ) {
            Ok(resp) => match resp.value {
                Some(acc) => acc.data,
                None => {
                    println!(
                        "  Perp asset map account not found: {}\n",
                        global_config.perp_asset_map_key
                    );
                    if watch {
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                    break;
                }
            },
            Err(e) => {
                println!(
                    "  Failed to fetch perp asset map account {}: {}\n",
                    global_config.perp_asset_map_key, e
                );
                if watch {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                break;
            }
        };
        let perp_asset_map = PerpAssetMapRef::load_from_buffer(&perp_asset_map_data);

        let (market_key, symbol_str, bld, tick_size_raw) =
            resolve_market(&perp_asset_map, effective)?;

        let (spline_key, _) = program_ids::get_spline_collection_address_default(&market_key);

        let spline_data =
            match client.get_account_with_commitment(&spline_key, CommitmentConfig::processed()) {
                Ok(resp) => match resp.value {
                    Some(acc) => acc.data,
                    None => {
                        println!("--- {} ---", symbol_str);
                        println!("  Spline collection not found: account does not exist\n");
                        if watch {
                            thread::sleep(Duration::from_secs(1));
                            continue;
                        }
                        break;
                    }
                },
                Err(e) => {
                    println!("--- {} ---", symbol_str);
                    println!("  Spline collection not found: {}\n", e);
                    if watch {
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                    break;
                }
            };

        let mut buf = String::new();
        print_spline_collection(
            &mut buf,
            &symbol_str,
            &spline_key,
            bld,
            tick_size_raw,
            &spline_data,
        );
        print!("{}", buf);

        if !watch {
            break;
        }
        thread::sleep(Duration::from_secs(2));
    }

    Ok(())
}

fn cmd_splines_ws(
    client: &RpcClient,
    rpc_url: &str,
    market: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (global_config, _) = load_global_config(client)?;
    let perp_asset_map_data = client
        .get_account_with_commitment(
            &global_config.perp_asset_map_key,
            CommitmentConfig::processed(),
        )?
        .value
        .ok_or("perp asset map account not found")?
        .data;
    let perp_asset_map = PerpAssetMapRef::load_from_buffer(&perp_asset_map_data);
    let (market_key, symbol_str, bld, tick_size_raw) = resolve_market(&perp_asset_map, market)?;
    let (spline_key, _) = program_ids::get_spline_collection_address_default(&market_key);
    let ws_url = rpc_url_to_ws_url(rpc_url)?;

    loop {
        match client.get_account_with_commitment(&spline_key, CommitmentConfig::processed()) {
            Ok(resp) => {
                if let Some(acc) = resp.value {
                    let mut buf = String::from("=== Spline Collections ===\n\n");
                    print_spline_collection(
                        &mut buf,
                        &symbol_str,
                        &spline_key,
                        bld,
                        tick_size_raw,
                        &acc.data,
                    );
                    print!("{}", frame_output(&buf));
                }
            }
            Err(e) => {
                print!("{}", frame_output(&format!("=== Spline Collections ===\n\n--- {} ---\n  Failed initial spline fetch: {}\n\n", symbol_str, e)));
            }
        }

        println!("  Watching account updates via websocket: {}", ws_url);
        let sub_config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::processed()),
            ..RpcAccountInfoConfig::default()
        };
        let (_subscription, receiver) =
            PubsubClient::account_subscribe(&ws_url, &spline_key, Some(sub_config))?;

        while let Ok(notification) = receiver.recv() {
            if let Some(account) = notification.value.decode::<Account>() {
                let mut buf = String::from("=== Spline Collections ===\n\n");
                print_spline_collection(
                    &mut buf,
                    &symbol_str,
                    &spline_key,
                    bld,
                    tick_size_raw,
                    &account.data,
                );
                print!("{}", frame_output(&buf));
            }
        }

        println!("Websocket disconnected; reconnecting in 1s...");
        thread::sleep(Duration::from_secs(1));
    }
}

/// Format a float with 2 decimal places and comma separators.
fn frame_output(buf: &str) -> String {
    let mut out = String::with_capacity(buf.len() + 256);
    out.push_str("\x1b[?25l\x1b[H");
    for line in buf.split('\n') {
        out.push_str(line);
        out.push_str("\x1b[K\n");
    }
    out.push_str("\x1b[J\x1b[?25h");
    out
}

fn fmt_price(v: f64) -> String {
    let s = format!("{:.2}", v);
    let (integer, decimal) = s.split_once('.').unwrap_or((&s, "00"));
    let negative = integer.starts_with('-');
    let digits: &str = if negative { &integer[1..] } else { integer };
    let with_commas: String = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join(",");
    if negative {
        format!("-{}.{}", with_commas, decimal)
    } else {
        format!("{}.{}", with_commas, decimal)
    }
}

fn print_spline_collection(
    buf: &mut String,
    symbol_str: &str,
    spline_key: &Pubkey,
    bld: i8,
    tick_size_raw: u64,
    spline_data: &[u8],
) {
    let spline_collection = SplineCollectionRef::load_from_buffer(spline_data);

    // ── Header ──
    let key_str = spline_key.to_string();
    let short_key = format!("{}...{}", &key_str[..4], &key_str[key_str.len() - 4..]);
    writeln!(buf).ok();
    writeln!(
        buf,
        "  \x1b[1m🐦‍🔥 Phoenix: {} \x1b[90m({})\x1b[0m",
        symbol_str, short_key
    )
    .ok();
    writeln!(buf).ok();

    // ── Collect rows ──
    // (trader prefix, start $, end $, density / tick, filled, size, trader_color_seed)
    let mut bid_rows: Vec<(String, f64, f64, f64, f64, f64)> = Vec::new();
    let mut ask_rows: Vec<(String, f64, f64, f64, f64, f64)> = Vec::new();

    for spline in spline_collection.iter() {
        let trader_prefix = pubkey_trader_prefix(spline.trader());
        let mid_price_ticks = spline.mid_price().as_inner();
        let mid_price_dollars = ticks_to_price(mid_price_ticks, tick_size_raw, bld);

        for region in spline.active_bid_regions().iter() {
            if region.is_empty() {
                continue;
            }
            let start_price = mid_price_dollars
                - ticks_to_price(region.start_offset.as_inner(), tick_size_raw, bld);
            let end_price = mid_price_dollars
                - ticks_to_price(region.end_offset.as_inner(), tick_size_raw, bld);
            let density_units = base_lots_to_units(region.density().as_inner() as u64, bld);
            let filled = base_lots_to_units(region.filled_size().as_inner() as u64, bld);
            let total = base_lots_to_units(region.total_size.as_inner(), bld);
            bid_rows.push((
                trader_prefix.clone(),
                start_price,
                end_price,
                density_units,
                filled,
                total,
            ));
        }

        for region in spline.active_ask_regions().iter() {
            if region.is_empty() {
                continue;
            }
            let start_price = mid_price_dollars
                + ticks_to_price(region.start_offset.as_inner(), tick_size_raw, bld);
            let end_price = mid_price_dollars
                + ticks_to_price(region.end_offset.as_inner(), tick_size_raw, bld);
            let density_units = base_lots_to_units(region.density().as_inner() as u64, bld);
            let filled = base_lots_to_units(region.filled_size().as_inner() as u64, bld);
            let total = base_lots_to_units(region.total_size.as_inner(), bld);
            ask_rows.push((
                trader_prefix.clone(),
                start_price,
                end_price,
                density_units,
                filled,
                total,
            ));
        }
    }

    // Bids: best (highest) first
    bid_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    // Asks: best (lowest) first
    ask_rows.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let top_n = 5usize;

    let best_bid = bid_rows.first().map(|r| r.1);
    let best_ask = ask_rows.first().map(|r| r.1);
    let spread = match (best_bid, best_ask) {
        (Some(b), Some(a)) => Some(a - b),
        _ => None,
    };

    // ── Render helpers ──
    let max_depth_bar = 12usize;

    // Pre-compute depth values for scaling the bars
    let bid_display: Vec<(usize, &(String, f64, f64, f64, f64, f64), f64)> = {
        let mut agg = 0.0f64;
        bid_rows
            .iter()
            .take(top_n)
            .enumerate()
            .map(|(i, row)| {
                agg += row.5;
                (i, row, agg)
            })
            .collect()
    };
    let ask_display: Vec<(usize, &(String, f64, f64, f64, f64, f64), f64)> = {
        let mut agg = 0.0f64;
        ask_rows
            .iter()
            .take(top_n)
            .enumerate()
            .map(|(i, row)| {
                agg += row.5;
                (i, row, agg)
            })
            .collect()
    };

    let bid_max_depth = bid_display.last().map(|r| r.2).unwrap_or(1.0);
    let ask_max_depth = ask_display.last().map(|r| r.2).unwrap_or(1.0);

    // Column widths — pre-format all values to find maxima
    let format_row_strings = |rows: &[(usize, &(String, f64, f64, f64, f64, f64), f64)]|
        -> Vec<(String, String, String, String, String)> {
        rows.iter().map(|(_, row, depth)| {
            let price_range = format!("${} → ${}", fmt_price(row.1), fmt_price(row.2));
            let size = fmt_price(row.5);
            let depth_s = fmt_price(*depth);
            (row.0.clone(), price_range, size, depth_s, String::new())
        }).collect()
    };

    let bid_strings = format_row_strings(&bid_display);
    let ask_strings = format_row_strings(&ask_display);

    // Find max widths across both sides for consistent alignment
    let all_strings: Vec<&(String, String, String, String, String)> =
        bid_strings.iter().chain(ask_strings.iter()).collect();
    let w_idx = format!("{}", top_n.saturating_sub(1)).len().max(1);
    let w_src = all_strings
        .iter()
        .map(|s| s.0.len())
        .max()
        .unwrap_or(4)
        .max(6);
    let w_price = all_strings
        .iter()
        .map(|s| s.1.len())
        .max()
        .unwrap_or(20)
        .max(11);
    let w_size = all_strings
        .iter()
        .map(|s| s.2.len())
        .max()
        .unwrap_or(8)
        .max(4);
    let w_depth = all_strings
        .iter()
        .map(|s| s.3.len())
        .max()
        .unwrap_or(8)
        .max(5);

    // Visible width between │ delimiters:
    // (1 space) idx (3 spaces) src (2) price (2) size (2) depth (2) bar+pad(max_depth_bar+1)
    let inner_width =
        1 + w_idx + 3 + w_src + 2 + w_price + 2 + w_size + 2 + w_depth + 2 + max_depth_bar + 1;

    // ── Print table ──
    // Layout: Asks on top (worst→best going top→bottom), spread, Bids below (best→worst going top→bottom)

    // Asks table — reversed so worst ask is at top, best ask at bottom
    let ask_count = ask_display.len();
    let ask_total = ask_rows.len();
    let ask_header = format!("─ \x1b[31mAsks\x1b[0m ({} of {}) ", ask_count, ask_total);
    let ask_header_visible_len = 1 + format!(" Asks ({} of {}) ", ask_count, ask_total).len();
    let ask_pad = inner_width.saturating_sub(ask_header_visible_len);
    writeln!(buf, "  ┌{}{}┐", ask_header, "─".repeat(ask_pad)).ok();

    // Column header
    writeln!(
        buf,
        "  │ {:>w_idx$}   {:>w_src$}  {:>w_price$}  {:>w_size$}  {:>w_depth$}  {:>bar_w$}│",
        "#",
        "Trader",
        "Price Range",
        "Size",
        "Depth",
        "",
        w_idx = w_idx,
        w_src = w_src,
        w_price = w_price,
        w_size = w_size,
        w_depth = w_depth,
        bar_w = max_depth_bar + 1,
    )
    .ok();

    // Print asks in reverse order (worst first → best last)
    for (display_idx, &(orig_idx, row, depth)) in ask_display.iter().rev().enumerate() {
        let _ = display_idx;
        let bar_len = ((depth / ask_max_depth) * max_depth_bar as f64).ceil() as usize;
        let bar: String = "▓".repeat(bar_len.min(max_depth_bar));
        let intensity = ((row.5 / ask_max_depth) * 200.0).min(255.0) as u8;
        let red = 100 + intensity.min(155);
        let color = format!("\x1b[38;2;{};50;50m", red);
        writeln!(
            buf,
            "  │ {:>w_idx$}   {}{:>w_src$}\x1b[0m  {:>w_price$}  {}{:>w_size$}  {:>w_depth$}\x1b[0m  {}{}\x1b[0m{}│",
            orig_idx,
            FIRE_ORANGE, row.0,
            ask_strings[orig_idx].1,
            color,
            ask_strings[orig_idx].2,
            ask_strings[orig_idx].3,
            color,
            bar,
            " ".repeat(max_depth_bar - bar_len.min(max_depth_bar) + 1),
            w_idx = w_idx, w_src = w_src, w_price = w_price, w_size = w_size, w_depth = w_depth,
        ).ok();
    }
    writeln!(buf, "  └{}┘", "─".repeat(inner_width)).ok();

    // ── Spread indicator ──
    match spread {
        Some(s) => {
            let pct = match best_bid {
                Some(b) if b > 0.0 => format!(" ({:.2}%)", (s / b) * 100.0),
                _ => String::new(),
            };
            let spread_text = format!(" spread: ${}{} ", fmt_price(s), pct);
            let total_pad = inner_width.saturating_sub(spread_text.len());
            let left = total_pad / 2;
            let right = total_pad - left;
            writeln!(
                buf,
                "  \x1b[90m {}──{}──{}\x1b[0m",
                " ".repeat(left),
                spread_text,
                " ".repeat(right),
            )
            .ok();
        }
        None => {
            writeln!(buf, "  \x1b[90m  ── no spread ──\x1b[0m").ok();
        }
    }

    // Bids table — best bid at top, worst at bottom
    let bid_count = bid_display.len();
    let bid_total = bid_rows.len();
    let bid_header = format!("─ \x1b[32mBids\x1b[0m ({} of {}) ", bid_count, bid_total);
    let bid_header_visible_len = 1 + format!(" Bids ({} of {}) ", bid_count, bid_total).len();
    let bid_pad = inner_width.saturating_sub(bid_header_visible_len);
    writeln!(buf, "  ┌{}{}┐", bid_header, "─".repeat(bid_pad)).ok();

    // Column header
    writeln!(
        buf,
        "  │ {:>w_idx$}   {:>w_src$}  {:>w_price$}  {:>w_size$}  {:>w_depth$}  {:>bar_w$}│",
        "#",
        "Trader",
        "Price Range",
        "Size",
        "Depth",
        "",
        w_idx = w_idx,
        w_src = w_src,
        w_price = w_price,
        w_size = w_size,
        w_depth = w_depth,
        bar_w = max_depth_bar + 1,
    )
    .ok();

    for (orig_idx, row, depth) in &bid_display {
        let bar_len = ((*depth / bid_max_depth) * max_depth_bar as f64).ceil() as usize;
        let bar: String = "▓".repeat(bar_len.min(max_depth_bar));
        let intensity = ((row.5 / bid_max_depth) * 200.0).min(255.0) as u8;
        let green = 100 + intensity.min(155);
        let color = format!("\x1b[38;2;50;{};50m", green);
        writeln!(
            buf,
            "  │ {:>w_idx$}   {}{:>w_src$}\x1b[0m  {:>w_price$}  {}{:>w_size$}  {:>w_depth$}\x1b[0m  {}{}\x1b[0m{}│",
            orig_idx,
            FIRE_ORANGE, row.0,
            bid_strings[*orig_idx].1,
            color,
            bid_strings[*orig_idx].2,
            bid_strings[*orig_idx].3,
            color,
            bar,
            " ".repeat(max_depth_bar - bar_len.min(max_depth_bar) + 1),
            w_idx = w_idx, w_src = w_src, w_price = w_price, w_size = w_size, w_depth = w_depth,
        ).ok();
    }
    writeln!(buf, "  └{}┘", "─".repeat(inner_width)).ok();

    // ── Footer ──
    writeln!(buf).ok();
    if let Some(b) = best_bid {
        write!(buf, "  💧 Best bid: \x1b[32m${}\x1b[0m", fmt_price(b)).ok();
    }
    if let Some(a) = best_ask {
        if best_bid.is_some() {
            write!(buf, "  ·  ").ok();
        }
        write!(buf, "Best ask: \x1b[31m${}\x1b[0m", fmt_price(a)).ok();
    }
    if best_bid.is_some() || best_ask.is_some() {
        writeln!(buf).ok();
    }
    writeln!(buf).ok();
}

fn cmd_trader(
    client: &RpcClient,
    pubkey: &str,
    pda_index: u8,
    subaccount: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Trader Account ===\n");

    let input_pubkey =
        Pubkey::from_str(pubkey).map_err(|_| format!("Invalid pubkey: {}", pubkey))?;

    // Try to determine if this is a trader PDA or an authority
    let (trader_key, is_direct) = resolve_trader_key(client, &input_pubkey, pda_index, subaccount)?;

    if is_direct {
        println!("Trader PDA: {}\n", trader_key);
    } else {
        println!("Authority: {}", input_pubkey);
        println!("PDA Index: {}, Subaccount: {}", pda_index, subaccount);
        println!("Trader PDA: {}\n", trader_key);
    }

    let account = client.get_account(&trader_key)?;
    let header: &DynamicTraderHeader =
        bytemuck::from_bytes(&account.data[..std::mem::size_of::<DynamicTraderHeader>()]);

    let (global_config, _) = load_global_config(client)?;
    let quote_decimals = global_config.quote_decimals() as f64;
    let collateral_dollars = header.collateral().as_inner() as f64 / 10_f64.powf(quote_decimals);

    println!(
        "discriminant: {} (valid: {})",
        header.discriminant(),
        header.discriminant() == *disc::TRADER
    );
    println!(
        "sequence_number: {}",
        header.sequence_number().sequence_number
    );
    println!("key: {}", header.key());
    println!("authority: {}", header.authority());
    println!("position_authority: {}", header.position_authority());
    println!("max_positions: {}", header.max_positions());
    println!("trader_pda_index: {}", header.trader_pda_index());
    println!(
        "trader_subaccount_index: {}",
        header.trader_subaccount_index()
    );
    println!("last_deposit_slot: {}", header.last_deposit_slot());
    println!(
        "collateral: \x1b[1m${:.2}\x1b[0m ({} quote lots)",
        collateral_dollars,
        header.collateral().as_inner()
    );
    println!(
        "has_pending_withdrawal: {}",
        header.has_pending_withdrawal()
    );

    Ok(())
}

fn cmd_positions(
    client: &RpcClient,
    pubkey: &str,
    pda_index: u8,
    subaccount: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Trader Positions ===\n");

    let input_pubkey =
        Pubkey::from_str(pubkey).map_err(|_| format!("Invalid pubkey: {}", pubkey))?;

    // Try to determine if this is a trader PDA or an authority
    let (trader_pubkey, is_direct) =
        resolve_trader_key(client, &input_pubkey, pda_index, subaccount)?;

    if is_direct {
        println!("Trader PDA: {}\n", trader_pubkey);
    } else {
        println!("Authority: {}", input_pubkey);
        println!("PDA Index: {}, Subaccount: {}", pda_index, subaccount);
        println!("Trader PDA: {}\n", trader_pubkey);
    }

    let (global_config, _) = load_global_config(client)?;
    let quote_decimals = global_config.quote_decimals() as f64;

    // Load perp asset map for symbol lookup
    let perp_asset_map_data = client.get_account(&global_config.perp_asset_map_key)?.data;
    let perp_asset_map = PerpAssetMapRef::load_from_buffer(&perp_asset_map_data);

    // Build asset_id -> (symbol, base_lot_decimals, tick_size) map
    let mut asset_info: HashMap<u32, (String, f64, f64)> = HashMap::new();
    for (symbol, metadata) in perp_asset_map.iter() {
        let symbol_str = String::from_utf8_lossy(symbol.as_bytes())
            .trim_end_matches('\0')
            .to_string();
        let asset_id = metadata.asset_id().as_inner();
        let base_lot_decimals = metadata.base_lot_decimals() as f64;
        let tick_size = metadata.tick_size().as_inner() as f64;
        asset_info.insert(asset_id, (symbol_str, base_lot_decimals, tick_size));
    }

    // Load GlobalTraderIndex
    println!("Loading GlobalTraderIndex...");
    let gti_tree = load_global_trader_index(client)?;
    println!("  {} traders loaded\n", gti_tree.len());

    // Look up trader in GTI
    match gti_tree.get(&trader_pubkey) {
        Some(trader_state) => {
            let trader_node_addr = gti_tree.tree.get_addr(&trader_pubkey);

            // Show trader state
            let collateral_dollars =
                trader_state.collateral().as_inner() as f64 / 10_f64.powf(quote_decimals);
            println!("Trader: {}", trader_pubkey);
            println!("  trader_id (node): {}", trader_node_addr);
            println!(
                "  collateral: \x1b[1m${:.2}\x1b[0m ({} quote lots)",
                collateral_dollars,
                trader_state.collateral().as_inner()
            );
            println!();

            // Load ActiveTraderBuffer
            println!("Loading ActiveTraderBuffer...");
            let atb_tree = load_active_trader_buffer(client)?;
            println!("  {} positions loaded\n", atb_tree.len());

            // Find positions for this trader
            let target_trader_id = trader_node_addr;
            let mut found_positions = 0;

            println!("--- Positions ---\n");

            for (position_id, position_state) in atb_tree.iter() {
                if position_id.trader_id().to_u32_or_sentinel() == target_trader_id {
                    found_positions += 1;
                    let asset_id = position_id.asset_id().as_inner();

                    let (symbol, base_lot_decimals, _tick_size) = asset_info
                        .get(&asset_id)
                        .cloned()
                        .unwrap_or_else(|| (format!("ASSET_{}", asset_id), 0.0, 1.0));

                    let base_lot_to_units = 10_f64.powf(-base_lot_decimals);

                    let base_lots = position_state.position.base_lot_position.as_inner();
                    let base_units = base_lots as f64 * base_lot_to_units;

                    let virtual_quote_lots = position_state
                        .position
                        .virtual_quote_lot_position
                        .as_inner();
                    let virtual_quote_dollars =
                        virtual_quote_lots as f64 / 10_f64.powf(quote_decimals);

                    let entry_price_dollars = if base_lots != 0 {
                        -virtual_quote_dollars / base_units
                    } else {
                        0.0
                    };

                    let direction = if base_lots > 0 {
                        "\x1b[32mLONG\x1b[0m"
                    } else if base_lots < 0 {
                        "\x1b[31mSHORT\x1b[0m"
                    } else {
                        "NEUTRAL"
                    };

                    println!("[{}] {} {}", found_positions, symbol, direction);
                    println!(
                        "    size: {:.6} {} ({} lots)",
                        base_units.abs(),
                        symbol,
                        base_lots.abs()
                    );
                    println!("    entry_price: ${:.4}", entry_price_dollars.abs());
                    println!(
                        "    virtual_quote: ${:.2} ({} lots)",
                        virtual_quote_dollars, virtual_quote_lots
                    );
                    println!(
                        "    bid_orders: {}, ask_orders: {}",
                        position_state.num_bids(),
                        position_state.num_asks()
                    );
                    println!();
                }
            }

            if found_positions == 0 {
                println!("No active positions found.");
            } else {
                println!("Total: {} position(s)", found_positions);
            }
        }
        None => {
            // Trader not in GTI - they are a cold trader
            // Fetch their state directly from the trader account
            println!("\x1b[33mTrader not in GlobalTraderIndex (cold trader)\x1b[0m\n");

            let account = client
                .get_account(&trader_pubkey)
                .map_err(|_| format!("Trader account not found: {}", trader_pubkey))?;

            let header: &DynamicTraderHeader =
                bytemuck::from_bytes(&account.data[..std::mem::size_of::<DynamicTraderHeader>()]);

            let collateral_dollars =
                header.collateral().as_inner() as f64 / 10_f64.powf(quote_decimals);

            println!("Trader: {}", trader_pubkey);
            println!(
                "  collateral: \x1b[1m${:.2}\x1b[0m ({} quote lots)",
                collateral_dollars,
                header.collateral().as_inner()
            );
            println!("  authority: {}", header.authority());
            println!();
            println!("No active positions (cold trader).");
        }
    }

    Ok(())
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Resolve whether input is a trader PDA or an authority.
/// Returns (trader_key, is_direct) where is_direct=true if the input was already a trader PDA.
fn resolve_trader_key(
    client: &RpcClient,
    input: &Pubkey,
    pda_index: u8,
    subaccount: u8,
) -> Result<(Pubkey, bool), Box<dyn std::error::Error>> {
    // First, try to fetch the account directly and check if it's a trader
    if let Ok(account) = client.get_account(input) {
        if account.data.len() >= std::mem::size_of::<DynamicTraderHeader>() {
            let header: &DynamicTraderHeader =
                bytemuck::from_bytes(&account.data[..std::mem::size_of::<DynamicTraderHeader>()]);
            if header.discriminant() == *disc::TRADER {
                // It's a direct trader PDA
                return Ok((*input, true));
            }
        }
    }

    // Not a trader account, treat as authority and derive PDA
    let (trader_key, _) = program_ids::get_trader_address_default(input, pda_index, subaccount);
    Ok((trader_key, false))
}

fn load_global_config(
    client: &RpcClient,
) -> Result<(GlobalConfiguration, Pubkey), Box<dyn std::error::Error>> {
    let (global_config_key, _) = program_ids::get_global_config_address_default();
    let account = client.get_account(&global_config_key)?;
    let global_config: GlobalConfiguration = *bytemuck::from_bytes(&account.data);
    Ok((global_config, global_config_key))
}

fn resolve_market(
    perp_asset_map: &PerpAssetMapRef,
    market: &str,
) -> Result<(Pubkey, String, i8, u64), Box<dyn std::error::Error>> {
    // Try parsing as pubkey first
    if let Ok(pubkey) = Pubkey::from_str(market) {
        // Find by market account
        for (symbol, metadata) in perp_asset_map.iter() {
            if metadata.market_account() == pubkey {
                let symbol_str = String::from_utf8_lossy(symbol.as_bytes())
                    .trim_end_matches('\0')
                    .to_string();
                return Ok((
                    pubkey,
                    symbol_str,
                    metadata.base_lot_decimals(),
                    metadata.tick_size().as_inner(),
                ));
            }
        }
        return Err(format!("Market not found: {}", market).into());
    }

    // Try matching by symbol (case-insensitive, trimmed)
    let market_upper = market.trim().to_uppercase();
    for (symbol, metadata) in perp_asset_map.iter() {
        let symbol_str = String::from_utf8_lossy(symbol.as_bytes())
            .trim_end_matches('\0')
            .to_string();
        let symbol_trimmed = symbol_str.trim().to_uppercase();
        // Match exact or prefix (e.g., "SOL" matches "SOL-PERP")
        if symbol_trimmed == market_upper
            || symbol_trimmed.starts_with(&market_upper)
            || symbol_trimmed.ends_with(&market_upper)
        {
            return Ok((
                metadata.market_account(),
                symbol_str,
                metadata.base_lot_decimals(),
                metadata.tick_size().as_inner(),
            ));
        }
    }

    // List available markets in error
    let available: Vec<_> = perp_asset_map
        .iter()
        .map(|(s, _)| {
            String::from_utf8_lossy(s.as_bytes())
                .trim_end_matches('\0')
                .trim()
                .to_string()
        })
        .collect();
    Err(format!("Market '{}' not found. Available: {:?}", market, available).into())
}

fn load_global_trader_index(
    client: &RpcClient,
) -> Result<GlobalTraderIndexTree<'static>, Box<dyn std::error::Error>> {
    let (header_key, _) = program_ids::get_global_trader_index_address_default(0);
    let header_account = client.get_account(&header_key)?;

    // Parse superblock to get num_arenas (located after the GlobalTraderIndexHeader)
    // Layout: GlobalTraderIndexHeader (48 bytes) | Superblock (32 bytes) | ...
    let header_size = 48; // GlobalTraderIndexHeader size
    let superblock: &Superblock = bytemuck::from_bytes(
        &header_account.data[header_size..header_size + std::mem::size_of::<Superblock>()],
    );
    let num_arenas = superblock.num_arenas;

    // Collect buffers - leak to get 'static lifetime for tree
    let mut buffers: Vec<Vec<u8>> = vec![header_account.data];

    // Fetch additional arenas (indices 1 through num_arenas - 1)
    for i in 1..num_arenas {
        let (arena_key, _) = program_ids::get_global_trader_index_address_default(i);
        if let Ok(acc) = client.get_account(&arena_key) {
            buffers.push(acc.data);
        }
    }

    // Leak buffers to get 'static lifetime
    let buffers: &'static [Vec<u8>] = Box::leak(buffers.into_boxed_slice());
    let tree = GlobalTraderIndexTree::load_from_buffers(buffers.iter().map(|b| b.as_slice()));

    Ok(tree)
}

fn load_active_trader_buffer(
    client: &RpcClient,
) -> Result<ActiveTraderBufferTree<'static>, Box<dyn std::error::Error>> {
    let (header_key, _) = program_ids::get_active_trader_buffer_address_default(0);
    let header_account = client.get_account(&header_key)?;

    // Parse superblock to get num_arenas (located after the ActiveTraderBufferHeader)
    // Layout: ActiveTraderBufferHeader (48 bytes) | Superblock (32 bytes) | ...
    let header_size = 48; // ActiveTraderBufferHeader size
    let superblock: &Superblock = bytemuck::from_bytes(
        &header_account.data[header_size..header_size + std::mem::size_of::<Superblock>()],
    );
    let num_arenas = superblock.num_arenas;

    let mut buffers: Vec<Vec<u8>> = vec![header_account.data];

    // Fetch additional arenas (indices 1 through num_arenas - 1)
    for i in 1..num_arenas {
        let (arena_key, _) = program_ids::get_active_trader_buffer_address_default(i);
        if let Ok(acc) = client.get_account(&arena_key) {
            buffers.push(acc.data);
        }
    }

    // Leak buffers to get 'static lifetime
    let buffers: &'static [Vec<u8>] = Box::leak(buffers.into_boxed_slice());
    let tree = ActiveTraderBufferTree::load_from_buffers(buffers.iter().map(|b| b.as_slice()));

    Ok(tree)
}
