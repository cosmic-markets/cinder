//! Decode on-chain spline collection bytes into row-oriented book state.

use phoenix_rise::types::accounts::{
    FifoOrderId, Orderbook, OrderbookRestingOrder, SplineCollection,
};
use solana_pubkey::Pubkey as PhoenixPubkey;

use super::super::math::{base_lots_to_units, ticks_to_price};

/// One row at a single tick: trader PDA (the spline's owning trader account;
/// resolve to the wallet authority via `GtiCache::resolve_pda` at display
/// time), tick price, and the available size at that tick (density minus any
/// per-region fill that's already consumed this tick or the ones in front of
/// it). Splines are pre-expanded into one row per tick inside their regions
/// so the displayed book reads as a normal CLOB.
pub type SplineRow = (PhoenixPubkey, f64, f64);

#[derive(Clone)]
pub struct ParsedSplineData {
    pub bid_rows: Vec<SplineRow>,
    pub ask_rows: Vec<SplineRow>,
    /// Prices at which a 🧊 iceberg marker should be painted. One entry per
    /// active spline region with `top_level_hidden_take_size > 0`, positioned
    /// at `price_at_offset(end_offset)` — i.e., one tick further from mid than
    /// the region's worst visible tick. That price typically coincides with
    /// the worst tick of the next-outer region, so the marker lands on a real
    /// row; orphan markers (no row at that price) are dropped at merge time.
    pub bid_iceberg_prices: Vec<f64>,
    pub ask_iceberg_prices: Vec<f64>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
}

/// `SplineCollection::try_from_account_bytes` inside `catch_unwind` so bad data
/// cannot unwind callers.
#[inline]
fn load_collection(data: &[u8]) -> Option<SplineCollection> {
    std::panic::catch_unwind(|| SplineCollection::try_from_account_bytes(data).ok()).ok()?
}

#[inline]
fn region_is_active(
    region: &phoenix_rise::types::accounts::TickRegion,
    current_slot: u64,
    last_updated_slot: u64,
) -> bool {
    // Mirror the on-chain `TickRegion::is_active` predicate: a region is live
    // only if it still has unfilled visible capacity AND its lifespan window
    // (relative to the spline's last user update) hasn't elapsed. Skipping the
    // lifespan half left expired non-GTC regions painted as ghost depth at the
    // back end of the curve. GTC regions use `lifespan = u64::MAX` so the
    // saturating add keeps them permanently active.
    if region.total_size <= region.filled_size {
        return false;
    }
    region.lifespan.saturating_add(last_updated_slot) >= current_slot
}

pub fn parse_spline_sequence(data: &[u8]) -> Option<(u64, u64)> {
    let collection = load_collection(data)?;
    Some((
        collection.sequence_number.sequence_number,
        collection.sequence_number.last_update_slot,
    ))
}

/// Expand a [`TickRegion`] into one `(trader, price, size)` row per tick.
///
/// Each tick within `[start_offset, end_offset)` shows the per-tick `density`
/// (in base lots), with the unfilled budget (`total_size - filled_size`)
/// allocated from the rear (least-aggressive) tick inward. Phoenix matches
/// splines at the most-aggressive end first, so the front (closest to mid)
/// ticks are the ones already consumed; the unfilled remainder lives in the
/// rear ticks.
///
/// We deliberately do *not* subtract `top_level_hidden_take_size` from the
/// visible size — direct comparison against the public Phoenix frontend on
/// live SOL splines showed those values stay visible at full density (e.g.
/// at the touch where a maker has top_hidden_take ≈ 5× density, the
/// reference still shows the density). The hidden-take parameter appears to
/// affect matching behaviour rather than displayed depth.
///
/// `price_at_offset` builds the displayed price for a tick offset (mid minus
/// for bids, mid plus for asks).
fn expand_region<F>(
    region: &phoenix_rise::types::accounts::TickRegion,
    trader: solana_pubkey::Pubkey,
    bld: i8,
    price_at_offset: F,
    out: &mut Vec<SplineRow>,
) where
    F: Fn(u64) -> f64,
{
    if region.start_offset >= region.end_offset {
        return;
    }
    let unfilled_lots = region.total_size.saturating_sub(region.filled_size);
    if unfilled_lots == 0 || region.density == 0 {
        return;
    }
    let mut remaining = unfilled_lots;
    for offset in (region.start_offset..region.end_offset).rev() {
        if remaining == 0 {
            break;
        }
        let take = remaining.min(region.density);
        remaining -= take;
        out.push((
            trader,
            price_at_offset(offset),
            base_lots_to_units(take, bld),
        ));
    }
}

pub fn parse_spline_data(
    data: &[u8],
    tick_size: u64,
    bld: i8,
    current_slot: u64,
) -> Option<ParsedSplineData> {
    let collection = load_collection(data)?;
    if std::env::var_os("CINDER_SPLINE_DEBUG").is_some() {
        dump_spline_collection_debug(&collection, tick_size, bld);
    }
    let mut bid_rows: Vec<SplineRow> = Vec::new();
    let mut ask_rows: Vec<SplineRow> = Vec::new();
    let mut bid_iceberg_prices: Vec<f64> = Vec::new();
    let mut ask_iceberg_prices: Vec<f64> = Vec::new();

    for spline in collection.active_splines() {
        let trader = spline.trader;
        let mid_ticks = spline.mid_price;
        let mid = ticks_to_price(mid_ticks, tick_size, bld);
        let last_updated_slot = spline.user_update_slot;

        // Skip exhausted regions: as a spline rolls, `bid_offset` advances past
        // filled regions whose stored prices are stale. Including them here was
        // making the displayed book appear crossed.
        let bid_start = (spline.bid_offset as usize).min(spline.bid_regions.len());
        let bid_end = (spline.bid_num_regions as usize)
            .min(spline.bid_regions.len())
            .max(bid_start);
        for region in &spline.bid_regions[bid_start..bid_end] {
            if !region_is_active(region, current_slot, last_updated_slot) {
                continue;
            }
            let price_at_offset = |offset| mid - ticks_to_price(offset, tick_size, bld);
            if region.top_level_hidden_take_size > 0 {
                bid_iceberg_prices.push(price_at_offset(region.end_offset));
            }
            expand_region(region, trader, bld, price_at_offset, &mut bid_rows);
        }

        let ask_start = (spline.ask_offset as usize).min(spline.ask_regions.len());
        let ask_end = (spline.ask_num_regions as usize)
            .min(spline.ask_regions.len())
            .max(ask_start);
        for region in &spline.ask_regions[ask_start..ask_end] {
            if !region_is_active(region, current_slot, last_updated_slot) {
                continue;
            }
            let price_at_offset = |offset| mid + ticks_to_price(offset, tick_size, bld);
            if region.top_level_hidden_take_size > 0 {
                ask_iceberg_prices.push(price_at_offset(region.end_offset));
            }
            expand_region(region, trader, bld, price_at_offset, &mut ask_rows);
        }
    }

    bid_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ask_rows.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Trim crossed rows at the touch. Phoenix splines don't auto-match
    // maker-vs-maker, so two splines with different `mid_price` can sit
    // crossed until a taker resolves them. Heuristic: stale-ghost quotes are
    // usually tiny next to the genuine touch, so when the bid/ask cross we
    // drop whichever side has less per-tick size and re-check. The dropped
    // rows are also pulled out of `bid_rows`/`ask_rows` so the chart's
    // `best_bid`/`best_ask` reflect the cleaned touch (otherwise the
    // mid-price would jitter as a tiny stale region flickered in and out).
    let mut bid_skip = 0usize;
    let mut ask_skip = 0usize;
    while let (Some(b), Some(a)) = (bid_rows.get(bid_skip), ask_rows.get(ask_skip)) {
        if b.1 < a.1 {
            break;
        }
        if b.2 < a.2 {
            bid_skip += 1;
        } else {
            ask_skip += 1;
        }
    }
    let bid_rows: Vec<SplineRow> = bid_rows.into_iter().skip(bid_skip).collect();
    let ask_rows: Vec<SplineRow> = ask_rows.into_iter().skip(ask_skip).collect();

    let best_bid = bid_rows.first().map(|r| r.1);
    let best_ask = ask_rows.first().map(|r| r.1);

    Some(ParsedSplineData {
        bid_rows,
        ask_rows,
        bid_iceberg_prices,
        ask_iceberg_prices,
        best_bid,
        best_ask,
    })
}

/// Dump the raw on-chain spline regions for the active splines into
/// `cinder_spline_debug.txt` (file is overwritten on each parse so the latest
/// snapshot is always there). Gated by the `CINDER_SPLINE_DEBUG` env var so
/// production runs don't pay the I/O cost. Writes prices alongside the raw
/// offsets/lots so the file can be read directly without doing the math by
/// hand.
fn dump_spline_collection_debug(collection: &SplineCollection, tick_size: u64, bld: i8) {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "asset={} num_splines={} num_active={} seq={} slot={}",
        collection.asset_symbol,
        collection.num_splines,
        collection.num_active,
        collection.sequence_number.sequence_number,
        collection.sequence_number.last_update_slot,
    );
    for (i, spline) in collection.splines.iter().enumerate() {
        if !spline.is_active {
            continue;
        }
        let mid = ticks_to_price(spline.mid_price, tick_size, bld);
        let trader_short: String = spline.trader.to_string().chars().take(8).collect();
        let _ = writeln!(
            s,
            "spline[{i}] trader={trader_short} mid_ticks={} mid=${mid:.6} \
             bid_offset={} bid_num_regions={} bid_filled={} \
             ask_offset={} ask_num_regions={} ask_filled={}",
            spline.mid_price,
            spline.bid_offset,
            spline.bid_num_regions,
            spline.bid_filled_amount,
            spline.ask_offset,
            spline.ask_num_regions,
            spline.ask_filled_amount,
        );
        let bid_end = (spline.bid_num_regions as usize).min(spline.bid_regions.len());
        for (j, r) in spline.bid_regions.iter().enumerate().take(bid_end) {
            let active = j >= spline.bid_offset as usize;
            let p_start = mid - ticks_to_price(r.start_offset, tick_size, bld);
            let p_end = mid - ticks_to_price(r.end_offset, tick_size, bld);
            let _ = writeln!(
                s,
                "  bid[{j}]{} start_off={} end_off={} ${p_start:.6}..${p_end:.6} \
                 density={} total={} filled={} hidden_filled={} top_hidden_take={} lifespan={}",
                if active { "*" } else { " " },
                r.start_offset,
                r.end_offset,
                r.density,
                r.total_size,
                r.filled_size,
                r.hidden_filled_size,
                r.top_level_hidden_take_size,
                r.lifespan,
            );
        }
        let ask_end = (spline.ask_num_regions as usize).min(spline.ask_regions.len());
        for (j, r) in spline.ask_regions.iter().enumerate().take(ask_end) {
            let active = j >= spline.ask_offset as usize;
            let p_start = mid + ticks_to_price(r.start_offset, tick_size, bld);
            let p_end = mid + ticks_to_price(r.end_offset, tick_size, bld);
            let _ = writeln!(
                s,
                "  ask[{j}]{} start_off={} end_off={} ${p_start:.6}..${p_end:.6} \
                 density={} total={} filled={} hidden_filled={} top_hidden_take={} lifespan={}",
                if active { "*" } else { " " },
                r.start_offset,
                r.end_offset,
                r.density,
                r.total_size,
                r.filled_size,
                r.hidden_filled_size,
                r.top_level_hidden_take_size,
                r.lifespan,
            );
        }
    }
    let _ = std::fs::write("cinder_spline_debug.txt", s);
}

/// One aggregated L2 level for a single trader at a single price.
#[derive(Copy, Clone, Debug)]
pub struct L2Level {
    pub price: f64,
    pub qty: f64,
    /// Sokoban node pointer into the `GlobalTraderIndex` tree — resolves to a
    /// pubkey via `GtiCache::resolve`. `0` means the order's
    /// `trader_position_id` was null/sentinel.
    pub trader_id: u32,
}

/// Aggregate a side's resting orders (yielded best-first by the tree iterator)
/// into per- `(price, trader)` levels. Sizes from orders sharing both a tick
/// and a trader are summed.
///
/// Iteration stops once `max_prices` unique prices have been produced. Within a
/// price, rows are emitted in the order traders first appear (which follows
/// FIFO insertion for that tick). Different traders at the same tick produce
/// separate rows.
#[inline]
fn aggregate_side<'a, I>(iter: I, tick_size: u64, bld: i8, max_prices: usize) -> Vec<L2Level>
where
    I: Iterator<Item = (&'a FifoOrderId, &'a OrderbookRestingOrder)>,
{
    let mut out: Vec<L2Level> = Vec::with_capacity(max_prices);
    let mut cur_ticks: Option<u64> = None;
    // Running per-trader totals for the current price level. Small Vec keeps
    // ordering stable and avoids a HashMap allocation for the typical handful
    // of traders per tick.
    let mut cur_traders: Vec<(u32, f64)> = Vec::new();
    let mut prices_seen: usize = 0;

    let flush = |ticks: u64, traders: &mut Vec<(u32, f64)>, out: &mut Vec<L2Level>| {
        let price = ticks_to_price(ticks, tick_size, bld);
        for (trader_id, qty) in traders.drain(..) {
            out.push(L2Level {
                price,
                qty,
                trader_id,
            });
        }
    };

    for (order_id, order) in iter {
        let ticks = order_id.price_in_ticks;
        let trader_id = order.trader_position_id.trader_id.unwrap_or(0);
        let qty = base_lots_to_units(order.num_base_lots_remaining, bld);

        match cur_ticks {
            Some(t) if t == ticks => {
                if let Some(entry) = cur_traders.iter_mut().find(|(id, _)| *id == trader_id) {
                    entry.1 += qty;
                } else {
                    cur_traders.push((trader_id, qty));
                }
            }
            Some(t) => {
                flush(t, &mut cur_traders, &mut out);
                prices_seen += 1;
                if prices_seen >= max_prices {
                    return out;
                }
                cur_ticks = Some(ticks);
                cur_traders.push((trader_id, qty));
            }
            None => {
                cur_ticks = Some(ticks);
                cur_traders.push((trader_id, qty));
            }
        }
    }
    if let Some(t) = cur_ticks {
        if prices_seen < max_prices {
            flush(t, &mut cur_traders, &mut out);
        }
    }
    out
}

fn resting_order_cmp_bid(
    a: &OrderbookRestingOrder,
    b: &OrderbookRestingOrder,
) -> std::cmp::Ordering {
    b.initial_slot
        .cmp(&a.initial_slot)
        .then_with(|| a.next_node.cmp(&b.next_node))
}

fn resting_order_cmp_ask(
    a: &OrderbookRestingOrder,
    b: &OrderbookRestingOrder,
) -> std::cmp::Ordering {
    a.initial_slot
        .cmp(&b.initial_slot)
        .then_with(|| a.next_node.cmp(&b.next_node))
}

/// Decode the Phoenix market (orderbook) account bytes into per-trader L2
/// levels.
///
/// Returns `(bids, asks)` sorted best-first (bids descending, asks ascending).
/// Each side contains at most `max_prices` unique price points; within a price,
/// rows are split by trader so every resting order's identity is preserved for
/// rendering.
pub fn parse_l2_book_from_market_account(
    data: Vec<u8>,
    tick_size: u64,
    bld: i8,
    max_prices: usize,
) -> Option<(Vec<L2Level>, Vec<L2Level>)> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ob = Orderbook::try_from_account_bytes(&data).ok()?;
        let mut bid_entries: Vec<(&FifoOrderId, &OrderbookRestingOrder)> =
            ob.bids.iter().map(|e| (&e.order_id, &e.order)).collect();
        bid_entries.sort_by(|(ida, oa), (idb, ob)| {
            idb.price_in_ticks
                .cmp(&ida.price_in_ticks)
                .then_with(|| resting_order_cmp_bid(oa, ob))
        });
        let mut ask_entries: Vec<(&FifoOrderId, &OrderbookRestingOrder)> =
            ob.asks.iter().map(|e| (&e.order_id, &e.order)).collect();
        ask_entries.sort_by(|(ida, oa), (idb, ob)| {
            ida.price_in_ticks
                .cmp(&idb.price_in_ticks)
                .then_with(|| resting_order_cmp_ask(oa, ob))
        });
        let bids = aggregate_side(bid_entries.into_iter(), tick_size, bld, max_prices);
        let asks = aggregate_side(ask_entries.into_iter(), tick_size, bld, max_prices);
        Some((bids, asks))
    }))
    .ok()?
}
