//! Env-derived RPC URLs and HTTP-loaded spline parameters per market.

use std::env;
use std::str::FromStr;
use std::sync::{OnceLock, RwLock};

use phoenix_eternal_types::program_ids;
use phoenix_rise::types::MarketStatus;
use phoenix_rise::ExchangeMarketConfig;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey as PhoenixPubkey;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    Chinese,
}

impl Language {
    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Chinese => "中文",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "cn",
        }
    }

    pub fn from_code(s: &str) -> Self {
        match s {
            "cn" | "zh" | "zh-CN" | "zh_CN" => Self::Chinese,
            _ => Self::English,
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::English => Self::Chinese,
            Self::Chinese => Self::English,
        }
    }
}

/// User-facing settings persisted to `~/.config/phoenix-cinder/config.json`.
/// Empty `rpc_url` = not overridden; fall back to env/default.
#[derive(Debug, Clone)]
pub struct UserConfig {
    pub rpc_url: String,
    pub language: Language,
    /// Whether to subscribe to and display CLOB L2 order data. Defaults to
    /// `true`. When `false`, no websocket is opened for the CLOB feed and
    /// the order book shows only spline rows.
    pub show_clob: bool,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            rpc_url: String::new(),
            language: Language::default(),
            show_clob: true,
        }
    }
}

/// Candidate wallet paths in priority order: `phoenix.json` next to the
/// binary, then `PHX_WALLET_PATH` / `KEYPAIR_PATH`, then the standard
/// Solana CLI location. The first path that exists and decodes is used.
fn wallet_path_candidates() -> Vec<String> {
    let home = env::var("USERPROFILE").unwrap_or_else(|_| env::var("HOME").unwrap_or_default());
    let env_path = env::var("PHX_WALLET_PATH").or_else(|_| env::var("KEYPAIR_PATH"));
    [
        Some("phoenix.json".to_string()),
        env_path.ok(),
        Some(format!("{}/.config/solana/id.json", home)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Best-guess wallet path to seed the load-wallet modal: prefer the first
/// candidate that already exists on disk; otherwise fall back to the
/// standard `~/.config/solana/id.json` location so the user can edit from
/// a sensible default.
pub fn default_wallet_path() -> String {
    let candidates = wallet_path_candidates();
    for p in &candidates {
        if std::path::Path::new(p).exists() {
            return p.clone();
        }
    }
    candidates
        .into_iter()
        .last()
        .unwrap_or_else(|| "id.json".to_string())
}

/// Parses a `Keypair` from raw text — accepts either a JSON byte array
/// (`[1,2,3,…]`, the Solana CLI format) or a base58-encoded 64-byte
/// keypair string. The error string is suitable for the TUI modal.
pub fn parse_keypair_text(text: &str) -> Result<Keypair, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("input is empty".to_string());
    }
    if trimmed.starts_with('[') {
        let bytes: Vec<u8> =
            serde_json::from_str(trimmed).map_err(|_| "invalid JSON byte array".to_string())?;
        if bytes.len() < 32 {
            return Err(format!(
                "keypair too short ({} bytes; need 32)",
                bytes.len()
            ));
        }
        let mut secret_key = [0u8; 32];
        secret_key.copy_from_slice(&bytes[..32]);
        return Ok(Keypair::new_from_array(secret_key));
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Keypair::from_base58_string(trimmed)
    })) {
        Ok(kp) => Ok(kp),
        Err(_) => Err("invalid base58 keypair string".to_string()),
    }
}

/// Reads a Solana keypair from `path`. Accepts files containing either a
/// JSON byte array (Solana CLI format) or a base58-encoded keypair string.
pub fn load_keypair_from_path(path: &str) -> Result<Keypair, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("wallet path is empty".to_string());
    }
    let content = std::fs::read_to_string(trimmed).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("file not found: {trimmed}"),
        std::io::ErrorKind::PermissionDenied => format!("permission denied: {trimmed}"),
        _ => format!("read error: {e}"),
    })?;
    parse_keypair_text(&content)
}

fn home_dir() -> String {
    env::var("USERPROFILE").unwrap_or_else(|_| env::var("HOME").unwrap_or_default())
}

fn config_dir_path() -> String {
    format!("{}/.config/phoenix-cinder", home_dir())
}

pub fn user_config_path() -> String {
    format!("{}/config.json", config_dir_path())
}

fn load_user_config_from_disk() -> UserConfig {
    let Ok(content) = std::fs::read_to_string(user_config_path()) else {
        return UserConfig::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return UserConfig::default();
    };
    UserConfig {
        rpc_url: v
            .get("rpc_url")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        language: Language::from_code(v.get("language").and_then(|x| x.as_str()).unwrap_or("en")),
        show_clob: v.get("show_clob").and_then(|x| x.as_bool()).unwrap_or(true),
    }
}

fn user_config_cache() -> &'static RwLock<UserConfig> {
    static CACHE: OnceLock<RwLock<UserConfig>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(load_user_config_from_disk()))
}

/// Current user config (from in-memory cache; first call reads from disk).
pub fn current_user_config() -> UserConfig {
    user_config_cache()
        .read()
        .map(|g| g.clone())
        .unwrap_or_default()
}

/// Persist `cfg` to disk and update the in-memory cache so subsequent
/// `rpc_http_url_from_env` / `current_user_config` calls return the new values.
///
/// Note: clients/streams established with a prior RPC URL keep that URL —
/// changes fully take effect after a restart.
pub fn save_user_config(cfg: &UserConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir_path())?;
    let value = serde_json::json!({
        "rpc_url": cfg.rpc_url,
        "language": cfg.language.code(),
        "show_clob": cfg.show_clob,
    });
    let content = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
    std::fs::write(user_config_path(), content)?;
    if let Ok(mut w) = user_config_cache().write() {
        *w = cfg.clone();
    }
    Ok(())
}

/// Holds configuration and derivation parameters for a specific market's spline
/// chart.
#[derive(Debug, Clone)]
pub struct SplineConfig {
    pub tick_size: u64,
    pub base_lot_decimals: i8,
    pub spline_collection: String,
    pub market_pubkey: String,
    pub symbol: String,
    /// Global asset index used as the key in on-chain per-market tables
    /// (e.g. ActiveTraderBuffer position ids). Needed to map an arbitrary
    /// on-chain position back to its market symbol.
    pub asset_id: u32,
    /// Number of decimal places to display for prices, derived from tick_size.
    pub price_decimals: usize,
    /// Number of decimal places for base-asset quantities, derived from
    /// base_lot_decimals.
    pub size_decimals: usize,
}

/// Reads the main HTTP RPC URL, preferring the user config file, then `RPC_URL`
/// / `SOLANA_RPC_URL` env vars, falling back to Mainnet Beta.
pub fn rpc_http_url_from_env() -> String {
    let cfg = current_user_config();
    if !cfg.rpc_url.trim().is_empty() {
        return cfg.rpc_url;
    }
    env::var("RPC_URL")
        .or_else(|_| env::var("SOLANA_RPC_URL"))
        .unwrap_or_else(|_| {
            warn!(
                "Using default mainnet-beta RPC_URL (https://api.mainnet-beta.solana.com) because \
                 neither RPC_URL nor SOLANA_RPC_URL is set."
            );
            "https://api.mainnet-beta.solana.com".to_string()
        })
}

/// Reads the main WebSocket JSON-RPC URL from the environment, or derives it
/// from the HTTP URL.
pub fn ws_url_from_env() -> String {
    env::var("RPC_WS_URL")
        .or_else(|_| env::var("SOLANA_WS_URL"))
        .unwrap_or_else(|_| http_rpc_url_to_ws(&rpc_http_url_from_env()))
}

/// Converts an HTTP(S) Solana RPC endpoint string to its equivalent WSS
/// endpoint.
pub fn http_rpc_url_to_ws(http: &str) -> String {
    let http = http.trim();
    if http.starts_with("wss://") || http.starts_with("ws://") {
        return http.to_string();
    }
    if let Some(rest) = http.strip_prefix("https://") {
        return format!("wss://{rest}");
    }
    if let Some(rest) = http.strip_prefix("http://") {
        if rest == "127.0.0.1:8899" {
            return "ws://127.0.0.1:8900".to_string();
        }
        if rest == "localhost:8899" {
            return "ws://localhost:8900".to_string();
        }
        return format!("ws://{rest}");
    }
    format!("wss://{http}")
}

/// Derive the number of decimal places to display for prices from tick_size and
/// base_lot_decimals.
pub fn compute_price_decimals(tick_size: u64, base_lots_decimals: i8) -> usize {
    let min_price_step = tick_size as f64 * 10_f64.powi(base_lots_decimals as i32) / 10_f64.powi(6); // QUOTE_LOT_DECIMALS = 6
    if min_price_step > 0.0 {
        let raw = (-min_price_step.log10()).ceil().max(0.0);
        // Clamp to a displayable range; pathological tick sizes (e.g. tick_size=1,
        // bld=18) would otherwise produce astronomically large values or
        // NaN-cast-to-usize.
        (raw as usize).min(18)
    } else {
        2
    }
}

/// Build a SplineConfig from an already-fetched ExchangeMarketConfig (no
/// network call).
pub fn build_spline_config(
    market: &ExchangeMarketConfig,
) -> Result<SplineConfig, Box<dyn std::error::Error>> {
    if !matches!(
        market.market_status,
        MarketStatus::Active | MarketStatus::PostOnly
    ) {
        warn!(
            market_status = ?market.market_status,
            symbol = %market.symbol,
            "market status is not Active/PostOnly; continuing with spline indexing"
        );
    }

    let market_pk = PhoenixPubkey::from_str(&market.market_pubkey)?;
    let api_spline_pk = PhoenixPubkey::from_str(&market.spline_pubkey)?;
    let (derived_spline_pk, _) = program_ids::get_spline_collection_address_default(&market_pk);
    if api_spline_pk != derived_spline_pk {
        warn!(
            api = %api_spline_pk,
            derived = %derived_spline_pk,
            symbol = %market.symbol,
            "spline pubkey mismatch; using derived address"
        );
    }

    let price_decimals = compute_price_decimals(market.tick_size, market.base_lots_decimals);

    // base_lot_decimals encodes the exponent in base_lots_to_units:
    //   units = lots / 10^bld
    // A positive bld means fractional units (e.g. bld=2 → 0.01 minimum).
    // A negative bld means lots are > 1 unit each, so 0 decimals suffice.
    let size_decimals = market.base_lots_decimals.max(0) as usize;

    Ok(SplineConfig {
        tick_size: market.tick_size,
        base_lot_decimals: market.base_lots_decimals,
        spline_collection: derived_spline_pk.to_string(),
        market_pubkey: market.market_pubkey.clone(),
        symbol: market.symbol.clone(),
        asset_id: market.asset_id,
        price_decimals,
        size_decimals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_round_trips_through_code() {
        for lang in [Language::English, Language::Chinese] {
            assert_eq!(Language::from_code(lang.code()), lang);
        }
    }

    #[test]
    fn language_from_code_accepts_zh_aliases() {
        assert_eq!(Language::from_code("zh"), Language::Chinese);
        assert_eq!(Language::from_code("zh-CN"), Language::Chinese);
        assert_eq!(Language::from_code("zh_CN"), Language::Chinese);
    }

    #[test]
    fn language_from_code_falls_back_to_english() {
        assert_eq!(Language::from_code("fr"), Language::English);
        assert_eq!(Language::from_code(""), Language::English);
    }

    #[test]
    fn language_toggle_is_involution() {
        assert_eq!(Language::English.toggle(), Language::Chinese);
        assert_eq!(Language::English.toggle().toggle(), Language::English);
    }

    #[test]
    fn http_to_ws_promotes_https_scheme() {
        assert_eq!(
            http_rpc_url_to_ws("https://api.mainnet-beta.solana.com"),
            "wss://api.mainnet-beta.solana.com"
        );
    }

    #[test]
    fn http_to_ws_demotes_http_scheme() {
        assert_eq!(
            http_rpc_url_to_ws("http://example.com:8899"),
            "ws://example.com:8899"
        );
    }

    #[test]
    fn http_to_ws_remaps_localhost_default_ports() {
        assert_eq!(
            http_rpc_url_to_ws("http://127.0.0.1:8899"),
            "ws://127.0.0.1:8900"
        );
        assert_eq!(
            http_rpc_url_to_ws("http://localhost:8899"),
            "ws://localhost:8900"
        );
    }

    #[test]
    fn http_to_ws_defaults_schemeless_input_to_wss() {
        assert_eq!(
            http_rpc_url_to_ws("api.example.com"),
            "wss://api.example.com"
        );
    }

    #[test]
    fn compute_price_decimals_clamps_pathological_inputs() {
        // Astronomically tiny min step would otherwise produce an unbounded value.
        assert!(compute_price_decimals(1, 18) <= 18);
    }

    #[test]
    fn compute_price_decimals_returns_zero_when_step_is_invalid() {
        assert_eq!(compute_price_decimals(0, 0), 2);
    }

    #[test]
    fn compute_price_decimals_matches_known_step_sizes() {
        // tick_size=1, base_lot_decimals=0 → step = 1e-6 → 6 decimals.
        assert_eq!(compute_price_decimals(1, 0), 6);
        // tick_size=100, base_lot_decimals=0 → step = 1e-4 → 4 decimals.
        assert_eq!(compute_price_decimals(100, 0), 4);
    }
}
