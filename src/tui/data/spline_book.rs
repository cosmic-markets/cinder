//! Decode on-chain spline collection bytes into row-oriented book state.

use phoenix_rise::types::accounts::{
    FifoOrderId, Orderbook, OrderbookRestingOrder, SplineCollection,
};
use solana_pubkey::Pubkey as PhoenixPubkey;

use super::super::math::{base_lots_to_units, ticks_to_price};

/// One row: trader PDA (the spline's owning trader account; resolve to the
/// wallet authority via `GtiCache::resolve_pda` at display time), price
/// interval, density, filled, total size.
pub type SplineRow = (PhoenixPubkey, f64, f64, f64, f64, f64);

#[derive(Clone)]
pub struct ParsedSplineData {
    pub bid_rows: Vec<SplineRow>,
    pub ask_rows: Vec<SplineRow>,
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
fn checked_lots_to_units(lots: u64, base_lot_decimals: i8) -> Option<f64> {
    Some(base_lots_to_units(lots, base_lot_decimals))
}

#[inline]
fn region_has_liquidity(region: &phoenix_rise::types::accounts::TickRegion) -> bool {
    region.total_size > 0
}

pub fn parse_spline_sequence(data: &[u8]) -> Option<(u64, u64)> {
    let collection = load_collection(data)?;
    Some((
        collection.sequence_number.sequence_number,
        collection.sequence_number.last_update_slot,
    ))
}

pub fn parse_spline_data(data: &[u8], tick_size: u64, bld: i8) -> Option<ParsedSplineData> {
    let collection = load_collection(data)?;
    let mut bid_rows = Vec::new();
    let mut ask_rows = Vec::new();

    for spline in collection.active_splines() {
        let trader = spline.trader;
        let mid_ticks = spline.mid_price;
        let mid = ticks_to_price(mid_ticks, tick_size, bld);

        let n_bid = (spline.bid_num_regions as usize).min(spline.bid_regions.len());
        for region in spline.bid_regions.iter().take(n_bid) {
            if !region_has_liquidity(region) {
                continue;
            }
            let start = mid - ticks_to_price(region.start_offset, tick_size, bld);
            let end = mid - ticks_to_price(region.end_offset, tick_size, bld);
            let Some(density) = checked_lots_to_units(region.density, bld) else {
                continue;
            };
            let Some(filled) = checked_lots_to_units(region.filled_size, bld) else {
                continue;
            };
            let Some(total) = checked_lots_to_units(region.total_size, bld) else {
                continue;
            };
            bid_rows.push((trader, start, end, density, filled, total));
        }

        let n_ask = (spline.ask_num_regions as usize).min(spline.ask_regions.len());
        for region in spline.ask_regions.iter().take(n_ask) {
            if !region_has_liquidity(region) {
                continue;
            }
            let start = mid + ticks_to_price(region.start_offset, tick_size, bld);
            let end = mid + ticks_to_price(region.end_offset, tick_size, bld);
            let Some(density) = checked_lots_to_units(region.density, bld) else {
                continue;
            };
            let Some(filled) = checked_lots_to_units(region.filled_size, bld) else {
                continue;
            };
            let Some(total) = checked_lots_to_units(region.total_size, bld) else {
                continue;
            };
            ask_rows.push((trader, start, end, density, filled, total));
        }
    }

    bid_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ask_rows.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let best_bid = bid_rows.first().map(|r| r.1);
    let best_ask = ask_rows.first().map(|r| r.1);

    Some(ParsedSplineData {
        bid_rows,
        ask_rows,
        best_bid,
        best_ask,
    })
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
