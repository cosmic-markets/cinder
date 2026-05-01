//! Top-N positions across the entire Phoenix protocol.
//!
//! Data source: the on-chain `ActiveTraderBuffer` — one account (plus zero or
//! more overflow-arena accounts) that holds a sokoban red-black tree keyed
//! `TraderPositionId -> TraderPositionState`. Iterating the tree yields every
//! active position in the protocol in a single pass, no `getProgramAccounts`
//! scan required.
//!
//! To resolve a position's trader back to a wallet authority we piggy-back on
//! the existing [`GtiCache`], which is already maintained by the CLOB rendering
//! path. `trader_id` in `TraderPositionId` is the sokoban *node pointer* into
//! the `GlobalTraderIndex` — the same u32 the cache is keyed by.

use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use std::collections::HashMap;
use tracing::warn;

use phoenix_eternal_types::quantities::{SignedBaseLots, SignedQuoteLots};
use phoenix_eternal_types::{program_ids, ActiveTraderBufferTree};

use super::super::config::SplineConfig;
use super::super::data::GtiCache;
use super::super::format::pubkey_trader_short;
use super::super::trading::{TopPositionEntry, TradingSide};

const ATB_HEADER_SIZE: usize =
    std::mem::size_of::<phoenix_eternal_types::ActiveTraderBufferHeader>();

/// Byte offset of `Superblock.num_arenas: u16` within the arena-0 account data. The
/// superblock lives immediately after the ATB header — same layout pattern as
/// the GlobalTraderIndex. See `gti::NUM_ARENAS_OFFSET`.
const ATB_NUM_ARENAS_OFFSET: usize = ATB_HEADER_SIZE + 4;

/// Quote lots are in micro-USD (6 decimals) — same constant used throughout
/// the math crate (see `MarketCalculator::quote_lot_decimals`).
const QUOTE_DECIMALS: i32 = 6;

/// Number of rows the modal shows. The fetch task still iterates every
/// position in the buffer, but only the top-N by notional are kept. The modal
/// scrolls when the popup can't render every row at the current terminal size.
pub const TOP_N_POSITIONS: usize = 50;

async fn fetch_atb_buffers(client: &RpcClient) -> Result<Vec<Vec<u8>>, String> {
    let (header_key, _) = program_ids::get_active_trader_buffer_address_default(0);
    let header_account = client
        .get_account(&header_key)
        .await
        .map_err(|e| format!("fetch ATB header: {e}"))?;

    if header_account.data.len() < ATB_NUM_ARENAS_OFFSET + 2 {
        return Err("ATB header account too small for superblock".to_string());
    }
    let num_arenas = u16::from_le_bytes([
        header_account.data[ATB_NUM_ARENAS_OFFSET],
        header_account.data[ATB_NUM_ARENAS_OFFSET + 1],
    ]);

    let mut buffers: Vec<Vec<u8>> = Vec::with_capacity(num_arenas.max(1) as usize);
    buffers.push(header_account.data);
    for i in 1..num_arenas {
        let (arena_key, _) = program_ids::get_active_trader_buffer_address_default(i);
        match client.get_account(&arena_key).await {
            Ok(acc) => buffers.push(acc.data),
            Err(e) => {
                // Stop early rather than inserting a misaligned buffer. The
                // next refresh will try again; the top list just reflects
                // fewer positions in the meantime.
                warn!(arena = i, error = %e, "ATB arena fetch failed; truncating");
                break;
            }
        }
    }
    Ok(buffers)
}

/// One raw position pulled out of the tree before symbol/mark-price resolution.
/// Kept separate from [`TopPositionEntry`] so the decode step doesn't need to
/// know about `SplineConfig` or mark prices.
struct RawPosition {
    trader_id: u32,
    asset_id: u32,
    /// Signed base lots as stored on-chain — positive = long.
    base_lots: i64,
    /// Signed virtual quote lots as stored on-chain. Sign convention is the
    /// negative of `base_lots` (opening a long pays quote → virtual is < 0).
    virtual_quote_lots: i64,
}

/// Walk the ATB tree and extract every non-zero position.
///
/// Wrapped in `catch_unwind` for the same reason as the GTI loader — sokoban
/// allocates from raw bytes and a malformed buffer can panic deep inside
/// `bytemuck`. Swallowing that keeps the refresh loop alive; the next tick
/// will retry against fresh bytes.
fn decode_positions(buffers: &[Vec<u8>]) -> Vec<RawPosition> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tree = ActiveTraderBufferTree::load_from_buffers(buffers.iter().map(|b| b.as_slice()));
        let mut out: Vec<RawPosition> = Vec::with_capacity(tree.tree.len());
        for (id, state) in tree.tree.iter() {
            let base_lots: SignedBaseLots = state.base_lot_position;
            if base_lots.as_inner() == 0 {
                continue;
            }
            let virtual_quote_lots: SignedQuoteLots = state.virtual_quote_lot_position;
            let Some(trader_id) = id.trader_id().as_u32_checked() else {
                continue;
            };
            out.push(RawPosition {
                trader_id,
                asset_id: id.asset_id().as_inner(),
                base_lots: base_lots.as_inner(),
                virtual_quote_lots: virtual_quote_lots.as_inner(),
            });
        }
        out
    }))
    .unwrap_or_default()
}

/// Convert one raw on-chain position into a display entry. Returns `None`
/// when `asset_id` isn't in the local config map (unknown market).
///
/// Notional/PnL are computed against `mark` when available, and fall back to
/// entry-price derived notional with zero PnL when it isn't.
fn to_entry(
    raw: &RawPosition,
    configs_by_asset: &HashMap<u32, &SplineConfig>,
    marks: &HashMap<String, f64>,
    gti: Option<&GtiCache>,
) -> Option<TopPositionEntry> {
    let cfg = configs_by_asset.get(&raw.asset_id).copied()?;

    // lots → base units: divide by 10^base_lot_decimals. Matches
    // `MarketCalculator::signed_base_lots_to_units`.
    let base_units_per_unit = 10f64.powi(cfg.base_lot_decimals as i32);
    if base_units_per_unit <= 0.0 {
        return None;
    }
    let size_signed = raw.base_lots as f64 / base_units_per_unit;
    let size = size_signed.abs();
    if size < 1e-12 {
        return None;
    }

    // virtual quote lots → USD: divide by 10^quote_decimals (= 6). The sign is
    // the negative of the position sign (opening a long costs USD → virtual
    // quote is negative for longs). Use absolute values to derive entry.
    let virtual_quote_usd_abs = (raw.virtual_quote_lots as f64).abs() / 10f64.powi(QUOTE_DECIMALS);
    let entry_price = if size > 0.0 {
        virtual_quote_usd_abs / size
    } else {
        0.0
    };

    let side = if raw.base_lots > 0 {
        TradingSide::Long
    } else {
        TradingSide::Short
    };

    let mark = marks.get(&cfg.symbol).copied().filter(|m| *m > 0.0);
    let (notional, unrealized_pnl) = match mark {
        Some(m) => {
            let pnl = match side {
                TradingSide::Long => size * (m - entry_price),
                TradingSide::Short => size * (entry_price - m),
            };
            (size * m, pnl)
        }
        None => (size * entry_price, 0.0),
    };

    let (trader, trader_display) = resolve_trader(raw.trader_id, gti);

    Some(TopPositionEntry {
        symbol: cfg.symbol.clone(),
        trader,
        trader_display,
        side,
        size,
        entry_price,
        notional,
        unrealized_pnl,
    })
}

fn resolve_trader(trader_id: u32, gti: Option<&GtiCache>) -> (Option<String>, String) {
    match gti.and_then(|g| g.resolve(trader_id)) {
        Some(authority) => {
            let full = authority.to_string();
            let display = pubkey_trader_short(&authority);
            (Some(full), display)
        }
        // Unresolved pointers happen on first cold open (GTI cache still
        // populating) and when a brand-new trader registered between refreshes.
        // Display a stable placeholder keyed on the node pointer so the row
        // doesn't jump around once the authority resolves on the next tick.
        None => (None, format!("#{}", trader_id)),
    }
}

/// Top-level snapshot refresh. Decodes every active position but does **not**
/// sort or truncate — the poller does both after re-applying the freshest
/// mark prices.
///
/// Why the split: marks passed into the task are a snapshot taken at spawn
/// time. Some markets may not have ticked yet, so `to_entry` falls back to
/// `size * entry_price` for those rows. If we sorted + truncated here, any
/// position whose `virtual_quote_lot_position` happens to be zero (can happen
/// around fills/flips) would get `notional = 0` and be ranked last — silently
/// dropped before the poller gets a chance to fix it with the live mark. By
/// deferring the ranking to the receive branch (which always has the latest
/// marks), every candidate stays in the running until the final truncate.
pub async fn fetch_top_positions(
    rpc_url: &str,
    configs: &HashMap<String, SplineConfig>,
    marks: &HashMap<String, f64>,
    gti: Option<&GtiCache>,
) -> Result<Vec<TopPositionEntry>, String> {
    use solana_commitment_config::CommitmentConfig;
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::processed());
    let buffers = fetch_atb_buffers(&client).await?;
    let raw = decode_positions(&buffers);

    let configs_by_asset: HashMap<u32, &SplineConfig> =
        configs.values().map(|c| (c.asset_id, c)).collect();

    Ok(raw
        .iter()
        .filter_map(|r| to_entry(r, &configs_by_asset, marks, gti))
        .collect())
}
