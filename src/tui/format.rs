//! Display helpers for the TUI.

use solana_pubkey::Pubkey as PhoenixPubkey;

/// USD price with thousands separators and `decimals` fractional digits.
pub fn fmt_price(v: f64, decimals: usize) -> String {
    let s = format!("{:.prec$}", v, prec = decimals);
    let (integer, decimal) = s.split_once('.').unwrap_or((&s, ""));
    let negative = integer.starts_with('-');
    let digits: &str = if negative { &integer[1..] } else { integer };
    let with_commas: String = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).expect("integer part of formatted float is ASCII"))
        .collect::<Vec<_>>()
        .join(",");
    if decimal.is_empty() {
        if negative {
            format!("-{}", with_commas)
        } else {
            with_commas
        }
    } else if negative {
        format!("-{}.{}", with_commas, decimal)
    } else {
        format!("{}.{}", with_commas, decimal)
    }
}

pub fn fmt_size(v: f64, decimals: usize) -> String {
    format!("{:.prec$}", v, prec = decimals)
}

/// Compact notation (K / M / B) with configurable fractional digits.
pub fn fmt_compact_prec(v: f64, prec: usize) -> String {
    let abs = v.abs();
    let (scaled, suffix) = if abs >= 1_000_000_000.0 {
        (v / 1_000_000_000.0, "B")
    } else if abs >= 1_000_000.0 {
        (v / 1_000_000.0, "M")
    } else if abs >= 1_000.0 {
        (v / 1_000.0, "K")
    } else {
        (v, "")
    };
    format!("{:.prec$}{}", scaled, suffix, prec = prec)
}

/// Compact notation (K / M / B) with two fractional digits.
pub fn fmt_compact(v: f64) -> String {
    fmt_compact_prec(v, 2)
}

/// Trade-panel unrealized PnL: `K` / `M` / `B` with two decimals from 1k
/// upward; below that, plain two-decimal USD (no commas) to keep the row short.
pub fn fmt_pnl_compact(abs_usd: f64) -> String {
    fmt_compact(abs_usd)
}

/// Compact "time since" formatter for relative timestamps. Picks the largest
/// unit that fits without going to a fractional value: `5s`, `42m`, `3h`,
/// `2d`, `4w`. Future timestamps (clock skew) clamp to `0s`. Used by the
/// liquidation feed's age column, which redraws on the runtime's 1Hz tick.
pub fn fmt_time_since_secs(elapsed_secs: i64) -> String {
    let s = elapsed_secs.max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86_400 {
        format!("{}h", s / 3600)
    } else if s < 604_800 {
        format!("{}d", s / 86_400)
    } else {
        format!("{}w", s / 604_800)
    }
}

pub fn pubkey_trader_prefix(trader: &PhoenixPubkey) -> String {
    let s = trader.to_string();
    s[..4.min(s.len())].to_owned()
}

/// 4+4 abbreviated pubkey for wider columns: `ABCD\u{2026}WXYZ`. Falls back to
/// the full string if the pubkey is somehow shorter than 8 chars (which it
/// won't be — base58 pubkeys are always 32 bytes / 43–44 chars).
pub fn pubkey_trader_short(trader: &PhoenixPubkey) -> String {
    let s = trader.to_string();
    if s.len() <= 8 {
        return s;
    }
    format!("{}\u{2026}{}", &s[..4], &s[s.len() - 4..])
}

/// Truncate toward zero to 2 dp so the UI never shows more USDC than on-chain
/// (e.g. rounding 0.29999 up to 0.30 would break tiny withdrawals).
pub fn truncate_balance(v: f64) -> f64 {
    (v * 100.0).floor() / 100.0
}

pub fn fmt_balance(v: f64) -> String {
    format!("{:.2}", truncate_balance(v))
}

pub fn truncate_pubkey(pk: &str) -> String {
    if pk.len() <= 8 {
        pk.to_string()
    } else {
        format!("{}...{}", &pk[..4], &pk[pk.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_formatting() {
        assert_eq!(fmt_balance(0.009), "0.00");
        assert_eq!(fmt_balance(1.999), "1.99");
        assert_eq!(fmt_balance(10.555), "10.55");
    }

    #[test]
    fn pnl_compact_abbreviates_from_thousands() {
        assert_eq!(fmt_pnl_compact(999.0), "999.00");
        assert_eq!(fmt_pnl_compact(1_000.0), "1.00K");
        assert_eq!(fmt_pnl_compact(12_345.67), "12.35K");
        assert_eq!(fmt_pnl_compact(1_000_000.0), "1.00M");
        assert_eq!(fmt_pnl_compact(1_500_000_000.0), "1.50B");
    }

    #[test]
    fn time_since_picks_largest_fitting_unit() {
        assert_eq!(fmt_time_since_secs(0), "0s");
        assert_eq!(fmt_time_since_secs(-5), "0s"); // clock skew clamps
        assert_eq!(fmt_time_since_secs(59), "59s");
        assert_eq!(fmt_time_since_secs(60), "1m");
        assert_eq!(fmt_time_since_secs(3599), "59m");
        assert_eq!(fmt_time_since_secs(3600), "1h");
        assert_eq!(fmt_time_since_secs(86_399), "23h");
        assert_eq!(fmt_time_since_secs(86_400), "1d");
        assert_eq!(fmt_time_since_secs(604_799), "6d");
        assert_eq!(fmt_time_since_secs(604_800), "1w");
        assert_eq!(fmt_time_since_secs(2_419_200), "4w");
    }

    #[test]
    fn truncate_pubkey_short_and_long() {
        assert_eq!(truncate_pubkey("123"), "123");
        assert_eq!(truncate_pubkey("12345678"), "12345678");
        assert_eq!(truncate_pubkey("123456789"), "1234...6789");
        assert_eq!(truncate_pubkey("SomeLongAddressTokenHere"), "Some...Here");
    }
}
