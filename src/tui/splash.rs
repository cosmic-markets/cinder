//! Animated fire-colored "CINDER" splash shown while initial network setup is
//! in flight. Owns the terminal for the duration of the animation and hands it
//! back so the TUI can take over without leaving/re-entering the alt screen.

use std::io::Stdout;
use std::time::{Duration, Instant};

use chrono::Utc;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;
use tokio::sync::oneshot;

use super::i18n::strings;

const BANNER: &[&str] = &[
    " ██████╗██╗███╗   ██╗██████╗ ███████╗██████╗ ",
    "██╔════╝██║████╗  ██║██╔══██╗██╔════╝██╔══██╗",
    "██║     ██║██╔██╗ ██║██║  ██║█████╗  ██████╔╝",
    "██║     ██║██║╚██╗██║██║  ██║██╔══╝  ██╔══██╗",
    "╚██████╗██║██║ ╚████║██████╔╝███████╗██║  ██║",
    " ╚═════╝╚═╝╚═╝  ╚═══╝╚═════╝ ╚══════╝╚═╝  ╚═╝",
];

const FRAME_INTERVAL: Duration = Duration::from_millis(55);
/// Show the animation for at least this long even if network setup finishes
/// quickly — otherwise the splash just flickers and the user can't tell what
/// they saw.
const MIN_SPLASH: Duration = Duration::from_millis(300);

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let v = a as f32 + (b as f32 - a as f32) * t;
    v.round().clamp(0.0, 255.0) as u8
}

fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    (
        lerp_u8(a.0, b.0, t),
        lerp_u8(a.1, b.1, t),
        lerp_u8(a.2, b.2, t),
    )
}

/// Sample the fire gradient. `row` is 0.0 at the top of the banner, 1.0 at the
/// bottom; `col` is the character column. `t` is animation seconds.
fn fire_color(t: f32, row: f32, col: f32) -> Color {
    // Two out-of-phase wobbles per column give the flame-flicker feel.
    let mut p = row;
    p += (t * 3.7 + col * 0.55).sin() * 0.13;
    p += (t * 2.1 + col * 0.31).cos() * 0.08;
    let p = p.clamp(0.0, 1.0);

    let (r, g, b) = if p < 0.35 {
        lerp_rgb((255, 190, 60), (255, 140, 20), p / 0.35)
    } else if p < 0.70 {
        lerp_rgb((255, 140, 20), (240, 90, 0), (p - 0.35) / 0.35)
    } else {
        lerp_rgb((240, 90, 0), (170, 40, 0), (p - 0.70) / 0.30)
    };
    Color::Rgb(r, g, b)
}

fn build_frame(time: f32) -> Vec<Line<'static>> {
    let total = (BANNER.len() as f32 - 1.0).max(1.0);
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(BANNER.len() + 2);
    for (row_idx, row) in BANNER.iter().enumerate() {
        let row_p = row_idx as f32 / total;
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(row.chars().count());
        for (col_idx, ch) in row.chars().enumerate() {
            let style = if ch == ' ' {
                Style::default()
            } else {
                Style::default()
                    .fg(fire_color(time, row_p, col_idx as f32))
                    .add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        lines.push(Line::from(spans));
    }

    // Current UTC clock + pulsing "loading" tagline beneath the banner.
    lines.push(Line::from(""));
    let now = Utc::now().format("%H:%M:%S").to_string();
    let dots = ((time * 2.5) as usize) % 4;
    let mut tagline = format!("{now}  Loading Phoenix markets");
    for _ in 0..dots {
        tagline.push('.');
    }
    let pulse = ((time * 1.8).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    let ember = lerp_rgb((180, 60, 0), (255, 170, 40), pulse);
    lines.push(Line::from(Span::styled(
        tagline,
        Style::default().fg(Color::Rgb(ember.0, ember.1, ember.2)),
    )));

    // Progress bar — we don't know the total setup duration up-front, so fill
    // asymptotically toward 100% (1 - exp(-t/τ)). Reads as steady forward
    // motion without ever falsely claiming "done".
    let banner_w = BANNER[0].chars().count();
    let progress = 1.0 - (-time / 1.0).exp();
    let filled = (progress * banner_w as f32).round() as usize;
    let filled = filled.min(banner_w);
    let mut bar_spans: Vec<Span<'static>> = Vec::with_capacity(banner_w);
    for col in 0..banner_w {
        if col < filled {
            let row_p = (col as f32 / banner_w.max(1) as f32).clamp(0.0, 1.0);
            bar_spans.push(Span::styled(
                "█",
                Style::default().fg(fire_color(time, row_p, col as f32)),
            ));
        } else {
            bar_spans.push(Span::styled(
                "░",
                Style::default().fg(Color::Rgb(60, 30, 15)),
            ));
        }
    }
    lines.push(Line::from(bar_spans));

    // Credit — right-aligned via leading padding so it lines up with the
    // banner's right edge regardless of banner width.
    let credit = "powered by Cosmic Markets";
    let pad = banner_w.saturating_sub(credit.chars().count());
    let mut credit_text = String::with_capacity(pad + credit.len());
    for _ in 0..pad {
        credit_text.push(' ');
    }
    credit_text.push_str(credit);
    lines.push(Line::from(Span::styled(
        credit_text,
        Style::default()
            .fg(Color::Rgb(170, 110, 70))
            .add_modifier(Modifier::ITALIC),
    )));

    // Risk disclaimer — dim, italic, left-aligned so it reads as fine print
    // rather than competing with the banner.
    let disclaimer = strings().splash_risk_disclaimer;
    lines.push(Line::from(Span::styled(
        disclaimer,
        Style::default()
            .fg(Color::Rgb(110, 70, 50))
            .add_modifier(Modifier::ITALIC),
    )));
    lines
}

fn draw_frame(terminal: &mut Terminal<CrosstermBackend<Stdout>>, time: f32) -> std::io::Result<()> {
    let lines = build_frame(time);
    terminal.draw(|f| {
        let area = f.area();
        let banner_w = BANNER[0].chars().count() as u16;
        let banner_h = lines.len() as u16;
        if area.width < banner_w || area.height < banner_h {
            return;
        }
        let x = (area.width - banner_w) / 2;
        let y = area.height.saturating_sub(banner_h) / 2;
        let target = Rect::new(x, y, banner_w, banner_h);
        f.render_widget(Paragraph::new(lines), target);
    })?;
    Ok(())
}

/// Run the splash on a blocking thread. Stops as soon as `stop` is signalled or
/// dropped, then returns the terminal so the caller can hand it to the real
/// TUI.
pub fn spawn(
    mut terminal: Terminal<CrosstermBackend<Stdout>>,
    mut stop: oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<Terminal<CrosstermBackend<Stdout>>> {
    tokio::task::spawn_blocking(move || {
        let _ = terminal.clear();
        let start = Instant::now();
        loop {
            let stopped = matches!(
                stop.try_recv(),
                Ok(_) | Err(oneshot::error::TryRecvError::Closed)
            );
            if stopped && start.elapsed() >= MIN_SPLASH {
                break;
            }
            let t = start.elapsed().as_secs_f32();
            let _ = draw_frame(&mut terminal, t);
            std::thread::sleep(FRAME_INTERVAL);
        }
        terminal
    })
}
