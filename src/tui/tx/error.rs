//! Transaction error helpers — file logging plus mapping of Phoenix custom
//! program errors to user-facing messages.

use std::path::PathBuf;

use super::super::i18n::{Strings, strings};

const COMPUTATIONAL_BUDGET_EXCEEDED: &str = "computational budget exceeded";
const COMPUTATIONAL_BUDGET_EXCEEDED_VARIANT: &str = "computationalbudgetexceeded";
const PROGRAM_FAILED_TO_COMPLETE: &str = "programfailedtocomplete";
const EXCEEDED_CUS_METER_AT_BPF: &str = "exceeded cus meter at bpf instruction";

/// Resolve the path for the transaction error log file.
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

/// Appends a structured error entry to the Cinder transaction error log on a
/// blocking thread so async workers never stall on filesystem I/O.
pub(super) fn log_tx_error(sig: Option<&str>, context: &str, error: &str) {
    use chrono::Utc;
    let mut entry = format!(
        "[{}] {}\n",
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        context
    );
    if let Some(sig) = sig {
        entry.push_str(&format!("txid: {sig}\n"));
    }
    entry.push_str(error);
    entry.push('\n');
    let path = tx_log_path();
    drop(tokio::task::spawn_blocking(move || {
        use std::io::Write;
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
    }));
}

/// True when the confirmation pipeline saw a definitive on-chain/program error
/// (`transaction failed: ...`), as opposed to timeout or disconnect.
pub(super) fn not_confirmed_is_onchain_execution_failure(err: &str) -> bool {
    err.to_lowercase().contains("transaction failed:")
}

/// Match Phoenix/Solana error substrings shared between simulation rejections
/// and confirmation errors. Expects `lower` to be `raw.to_lowercase()`.
fn match_shared_phoenix_error(raw: &str, lower: &str, s: &Strings) -> Option<&'static str> {
    if lower.contains(COMPUTATIONAL_BUDGET_EXCEEDED)
        || lower.contains(COMPUTATIONAL_BUDGET_EXCEEDED_VARIANT)
    {
        return Some(s.tx_err_computational_budget_exceeded);
    }
    if lower.contains(PROGRAM_FAILED_TO_COMPLETE) {
        return Some(s.tx_err_program_failed_to_complete);
    }
    if lower.contains(EXCEEDED_CUS_METER_AT_BPF) {
        return Some(s.tx_err_insufficient_compute_units);
    }
    if lower.contains("custom program error: 0x1") {
        return Some(s.tx_err_not_enough_sol);
    }
    // Phoenix program error 7002: stop-loss direction conflicts with position.
    if raw.contains("Custom(7002)") || lower.contains("custom program error: 0x1b5a") {
        return Some(s.tx_err_stop_opposite_direction);
    }
    if lower.contains("postonlycross")
        || lower.contains("postonly does not satisfy cross")
        || (lower.contains("postonly") && lower.contains("satisfy cross"))
    {
        return Some(s.tx_err_post_only_no_cross);
    }
    if lower.contains("isolated-only markets reject cross-margin trader accounts")
        || (lower.contains("isolated-only")
            && lower.contains("cross-margin")
            && lower.contains("reject"))
    {
        return Some(s.tx_err_isolated_only_cross_margin);
    }
    None
}

/// User-facing text for `ConfirmError::NotConfirmed` in the status title.
pub(super) fn format_not_confirmed_error(e: &str) -> String {
    let s = strings();
    let lower = e.to_lowercase();
    if let Some(mapped) = match_shared_phoenix_error(e, &lower, s) {
        return mapped.to_string();
    }
    if lower.contains("confirmation timeout") {
        return "confirmation is still pending; check the txid before retrying".to_string();
    }
    if e.contains("InsufficientFunds") {
        s.tx_err_balance_too_low.to_string()
    } else {
        e.to_string()
    }
}

/// Map a raw RPC/program error string using an explicit [`Strings`] table.
fn parse_phoenix_tx_error_with_table(error: &str, s: &Strings) -> String {
    let lower = error.to_lowercase();
    if let Some(mapped) = match_shared_phoenix_error(error, &lower, s) {
        return mapped.to_string();
    }
    if lower.contains("order size must be non-zero") {
        return s.tx_err_order_size_nonzero.to_string();
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
    if lower.contains("embererror")
        && (lower.contains("6028") || lower.contains("insufficient balance"))
    {
        return s.tx_err_insufficient_balance.to_string();
    }
    if lower.contains("insufficientfunds")
        || lower.contains("insufficient funds")
        || lower.contains("insufficient transferable funds")
        || error.contains("MarginError")
        || error.contains("validate_margin_state_change failed")
    {
        return s.tx_err_insufficient_funds.to_string();
    }

    let msg = format!("{}{}", s.tx_err_failed_prefix, error);
    // Truncate by char boundary to avoid panicking on multi-byte UTF-8.
    if msg.chars().count() > 200 {
        let head: String = msg.chars().take(200).collect();
        format!("{head}...")
    } else {
        msg
    }
}

/// Parses an RPC error for known Phoenix-specific issues and returns a
/// friendlier message.
pub(super) fn parse_phoenix_tx_error(error: &str) -> String {
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

    #[test]
    fn parse_phoenix_tx_error_maps_isolated_only_cross_margin() {
        let raw = "Program log: isolated-only markets reject cross-margin trader accounts";
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_isolated_only_cross_margin
        );
    }

    #[test]
    fn parse_phoenix_tx_error_maps_insufficient_transferable_funds() {
        let raw = r#"API error: 400 - {"error":"Insufficient transferable funds in source account. Total Balance: $0.24, Transferable: $0.00, Requested: $0.01."}"#;
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_insufficient_funds
        );
    }

    #[test]
    fn parse_phoenix_tx_error_maps_compute_units_meter_when_no_budget_variant() {
        let raw = r#"Program log: ... exceeded CUs meter at BPF instruction ..."#;
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_insufficient_compute_units
        );
    }

    #[test]
    fn parse_phoenix_tx_error_maps_computational_budget_variant_token() {
        let raw = r#"Simulation failed: InstructionError(3, ComputationalBudgetExceeded)"#;
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_computational_budget_exceeded
        );
    }

    #[test]
    fn parse_phoenix_tx_error_prefers_budget_variant_when_also_cu_meter_logged() {
        let raw = r#"InstructionError(0, Custom(ProgramErrorWithOrigin { program_error: InstructionError(0, ComputationalBudgetExceeded), origin: Some("... exceeded CUs meter at BPF instruction ...") }))"#;
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_computational_budget_exceeded
        );
    }

    #[test]
    fn parse_phoenix_tx_error_maps_computational_budget_exceeded() {
        let raw = r#"Simulation failed: Transaction simulation failed: Error processing Instruction 0: Computational budget exceeded"#;
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_computational_budget_exceeded
        );
    }

    #[test]
    fn parse_phoenix_tx_error_prefers_computational_budget_when_both_substrings_present() {
        let raw = "Computational budget exceeded … exceeded CUs meter at BPF instruction";
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_computational_budget_exceeded
        );
    }

    #[test]
    fn parse_phoenix_tx_error_maps_program_failed_to_complete() {
        let raw = r#"transaction failed: Some(InstructionError(0, ProgramFailedToComplete))"#;
        assert_eq!(
            parse_phoenix_tx_error_with_table(raw, &EN),
            EN.tx_err_program_failed_to_complete
        );
    }

    #[test]
    fn not_confirmed_onchain_detection() {
        assert!(not_confirmed_is_onchain_execution_failure(
            "transaction failed: Some(InstructionError(0, ProgramFailedToComplete))",
        ));
        assert!(!not_confirmed_is_onchain_execution_failure(
            "confirmation timeout"
        ));
    }
}
