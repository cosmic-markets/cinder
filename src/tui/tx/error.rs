//! Transaction error helpers — file logging plus mapping of Phoenix custom
//! program errors to user-facing messages.

use std::path::PathBuf;

use super::super::i18n::{strings, Strings};

fn tx_log_path() -> PathBuf {
    if let Ok(dir) = std::env::var("CINDER_LOG_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("cinder-error.log");
        }
    }

    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    home.join(".config")
        .join("phoenix-cinder")
        .join("logs")
        .join("cinder-error.log")
}

/// Appends a structured error entry to the Cinder transaction error log.
///
/// Format:
/// ```text
/// [2026-04-16T12:34:56Z] <context>
/// txid: <sig>          ← omitted when no signature (send failed)
/// <full raw error>
/// ```
pub(super) fn log_tx_error(sig: Option<&str>, context: &str, error: &str) {
    use std::io::Write;

    use chrono::Utc;
    let mut entry = format!(
        "[{}] {}\n",
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        context
    );
    if let Some(sig) = sig {
        entry.push_str(&format!("txid: {}\n", sig));
    }
    entry.push_str(error);
    entry.push('\n');
    let path = tx_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(entry.as_bytes());
    }
}

/// User-facing text for `ConfirmError::NotConfirmed` in the status title (after
/// the em dash).
pub(super) fn format_not_confirmed_error(e: &str) -> String {
    let s = strings();
    let lower = e.to_lowercase();
    if lower.contains("custom program error: 0x1") {
        return s.tx_err_not_enough_sol.to_string();
    }
    if lower.contains("confirmation timeout") {
        return "confirmation is still pending; check the txid before retrying".to_string();
    }
    // Phoenix program error 7002: stop-loss direction conflicts with position.
    if e.contains("Custom(7002)") || lower.contains("custom program error: 0x1b5a") {
        return s.tx_err_stop_opposite_direction.to_string();
    }
    if e.contains("InsufficientFunds") {
        s.tx_err_balance_too_low.to_string()
    } else if is_post_only_cross_error(e) {
        s.tx_err_post_only_no_cross.to_string()
    } else {
        e.to_string()
    }
}

/// Simulation / program logs use slightly different casing; some paths emit `PostOnlyCross`
/// instead of the full sentence.
fn is_post_only_cross_error(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("postonlycross") {
        return true;
    }
    lower.contains("postonly does not satisfy cross")
        || (lower.contains("postonly") && lower.contains("satisfy cross"))
}

fn parse_phoenix_tx_error_with_table(error: &str, s: &Strings) -> String {
    let lower = error.to_lowercase();

    if lower.contains("custom program error: 0x1") {
        return s.tx_err_not_enough_sol.to_string();
    }
    if error.contains("Custom(7002)") || lower.contains("custom program error: 0x1b5a") {
        return s.tx_err_stop_opposite_direction.to_string();
    }
    if lower.contains("order size must be non-zero") {
        return s.tx_err_order_size_nonzero.to_string();
    }
    if is_post_only_cross_error(error) {
        return s.tx_err_post_only_no_cross.to_string();
    }
    if error.contains("CapabilityDenied") {
        return s.tx_err_capability_denied.to_string();
    }
    if error.contains("TraderFrozen") {
        return s.tx_err_trader_frozen.to_string();
    }
    if lower.contains("withdrawal request rejected") && lower.contains("insufficientmargin") {
        return s.tx_err_withdraw_insufficient_margin.to_string();
    }
    // Substring of Phoenix on-chain `EmberError` in some RPC log lines (unchanged in protocol).
    if lower.contains("embererror")
        && (lower.contains("6028") || lower.contains("insufficient balance"))
    {
        return s.tx_err_insufficient_balance.to_string();
    }
    if lower.contains("insufficientfunds")
        || lower.contains("insufficient funds")
        || error.contains("MarginError")
        || error.contains("validate_margin_state_change failed")
    {
        return s.tx_err_insufficient_funds.to_string();
    }
    let msg = format!("{}{}", s.tx_err_failed_prefix, error);
    // `msg[..200]` panics if byte 200 lands inside a multi-byte UTF-8 scalar (RPC
    // errors can contain non-ASCII). Iterate by char to keep the boundary
    // valid.
    if msg.chars().count() > 200 {
        let head: String = msg.chars().take(200).collect();
        format!("{}...", head)
    } else {
        msg
    }
}

/// Parses an RPC error for known Phoenix-specific issues and returns a
/// friendlier message.
pub fn parse_phoenix_tx_error(error: &str) -> String {
    parse_phoenix_tx_error_with_table(error, strings())
}

#[cfg(test)]
mod tests {
    use super::super::super::i18n::EN;
    use super::*;

    #[test]
    fn parse_phoenix_tx_error_maps_custom_program_0x1() {
        let raw = r#"Simulation failed: Custom program error: 0x1"#;
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_not_enough_sol
        );
    }

    #[test]
    fn parse_phoenix_tx_error_maps_post_only_cross_case_insensitive() {
        let raw = r#"RpcError( RpcResponseError { message: "Program log: postonly does not satisfy cross" })"#;
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_post_only_no_cross
        );
    }

    #[test]
    fn parse_phoenix_tx_error_maps_postonlycross_token() {
        let raw = "InstructionError(0, Custom(AccountCustomError { code: 6001, message: \"PostOnlyCross\" }))";
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_post_only_no_cross
        );
    }
}
