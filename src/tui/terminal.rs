//! Crossterm / ratatui terminal setup and teardown.

use std::io::{self, Stdout};

use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn setup_terminal() -> io::Result<TuiTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen, cursor::Hide) {
        let _ = disable_raw_mode();
        return Err(err);
    }

    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(err) => {
            let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
            let _ = disable_raw_mode();
            Err(err)
        }
    }
}

pub fn restore_terminal(terminal: &mut TuiTerminal) {
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show);
}

/// Best-effort terminal restore without access to the Terminal handle.
/// Called from `app::run` on shutdown.
pub fn cleanup_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
}
