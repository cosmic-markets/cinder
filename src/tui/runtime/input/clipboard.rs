//! Clipboard helper used by modal actions.

/// Writes `text` to the OS clipboard. Returns a short error string on failure
/// (e.g. no display server) — callers surface it in the status line.
pub(in crate::tui::runtime) fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}
