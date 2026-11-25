//! Configuration modal.

use ratatui::layout::Rect;

use super::*;
use crate::tui::config::{
    DEFAULT_COMPUTE_UNIT_LIMIT_PER_POSITION, DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS,
    DEFAULT_PUBLIC_SOLANA_RPC_URL,
};
use crate::tui::tx::current_auto_priority_fee;

const LABEL_W: u16 = 22;

/// Render a two-column settings row (`▸ Label` on left, `value_line` on right).
fn render_setting_row(f: &mut Frame, row: Rect, label: &str, selected: bool, value_line: Line<'_>) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LABEL_W), Constraint::Min(0)])
        .split(row);

    let cursor = if selected { "▸ " } else { "  " };
    let label_style = if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(cursor, label_style),
            Span::styled(label.to_string(), label_style),
        ])),
        cols[0],
    );
    f.render_widget(Paragraph::new(value_line), cols[1]);
}

/// Render a `◀ value ▶` selector/toggle row with an optional dim note.
fn render_toggle_row(
    f: &mut Frame,
    row: Rect,
    label: &str,
    selected: bool,
    value: &str,
    note: Option<&str>,
) {
    let (arrow_style, value_style) = if selected {
        (
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::White),
        )
    };
    let mut spans = vec![
        Span::styled("◀ ", arrow_style),
        Span::styled(value.to_string(), value_style),
        Span::styled(" ▶", arrow_style),
    ];
    if let Some(note) = note {
        spans.push(Span::styled(
            format!("  ({note})"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    render_setting_row(f, row, label, selected, Line::from(spans));
}

/// Render an inline-editable compute-unit setting row.
#[allow(clippy::too_many_arguments)]
fn render_cu_field_row(
    f: &mut Frame,
    row: Rect,
    label: &str,
    note: &str,
    selected: bool,
    editing: bool,
    buffer: &str,
    override_value: Option<String>,
    default_display: String,
    default_label: &str,
) {
    let value_line = if editing {
        Line::from(Span::styled(
            format!("{buffer}_"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ))
    } else if let Some(v) = override_value {
        Line::from(vec![
            Span::styled(v, Style::default().fg(Color::White)),
            Span::styled(format!("  ({note})"), Style::default().fg(Color::DarkGray)),
        ])
    } else {
        let mut spans = vec![Span::styled(
            default_label.to_string(),
            Style::default().fg(Color::DarkGray),
        )];
        if !default_display.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                default_display,
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.push(Span::styled(
            format!("  ({note})"),
            Style::default().fg(Color::DarkGray),
        ));
        Line::from(spans)
    };
    render_setting_row(f, row, label, selected, value_line);
}

/// Render the settings modal (`[c]`).
pub(in crate::tui::ui) fn render_config_modal(f: &mut Frame, area: Rect, trading: &TradingState) {
    let cfg_s = strings();
    let title = Line::from(Span::styled(
        format!(" ⚙  {} ", cfg_s.config),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));

    let editing_rpc = trading.input_mode == InputMode::EditingRpcUrl;
    let editing_cu_price = trading.input_mode == InputMode::EditingComputeUnitPrice;
    let editing_cu_limit = trading.input_mode == InputMode::EditingComputeUnitLimit;
    let any_text_edit = editing_rpc || editing_cu_price || editing_cu_limit;

    let footer = if any_text_edit {
        let action_label = if editing_rpc {
            cfg_s.save_reconnect
        } else {
            cfg_s.set
        };
        footer_hints(&[
            ("Enter", Color::Cyan, action_label),
            ("Esc", Color::Cyan, cfg_s.cancel),
        ])
    } else {
        footer_hints(&[
            ("↑↓", Color::Cyan, cfg_s.select),
            ("←→", Color::Cyan, cfg_s.toggle),
            ("Enter", Color::Cyan, cfg_s.edit),
            ("Esc", Color::Cyan, cfg_s.cancel),
        ])
    };

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));
    let inner = centered_modal(f, area, 88, 13, block);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacer
            Constraint::Length(1), // rpc url
            Constraint::Length(1), // language
            Constraint::Length(1), // clob orders
            Constraint::Length(1), // public RPC fan-out
            Constraint::Length(1), // skip order confirmation
            Constraint::Length(1), // skip preflight
            Constraint::Length(1), // CU price
            Constraint::Length(1), // CU limit
            Constraint::Min(0),
        ])
        .split(inner);

    let sel = |idx: usize| trading.config_selected_field == idx && !any_text_edit;
    let on_off = |b: bool| if b { cfg_s.on } else { cfg_s.off };

    // Row 0: RPC URL
    let rpc_value_line = if editing_rpc {
        Line::from(Span::styled(
            format!("{}_", trading.input_buffer),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ))
    } else if trading.config.rpc_url.is_empty() {
        let resolved = std::env::var("RPC_URL")
            .or_else(|_| std::env::var("SOLANA_RPC_URL"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PUBLIC_SOLANA_RPC_URL.to_string());
        let host = super::super::rpc_host_from_urlish(&resolved);
        Line::from(vec![
            Span::styled(cfg_s.rpc_default, Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(host, Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(Span::styled(
            trading.config.rpc_url.clone(),
            Style::default().fg(Color::White),
        ))
    };
    render_setting_row(
        f,
        rows[1],
        cfg_s.rpc_url,
        sel(0) || editing_rpc,
        rpc_value_line,
    );

    // Rows 1..=5: Language + boolean toggles
    render_toggle_row(
        f,
        rows[2],
        cfg_s.language,
        sel(1),
        trading.config.language.label(),
        None,
    );
    render_toggle_row(
        f,
        rows[3],
        cfg_s.clob_orders,
        sel(2),
        on_off(trading.config.show_clob),
        Some(cfg_s.clob_orders_note),
    );
    render_toggle_row(
        f,
        rows[4],
        cfg_s.fanout_public_rpc,
        sel(3),
        on_off(trading.config.fanout_public_rpc),
        Some(cfg_s.fanout_public_rpc_note),
    );
    render_toggle_row(
        f,
        rows[5],
        cfg_s.skip_order_confirmation,
        sel(4),
        on_off(trading.config.skip_order_confirmation),
        Some(cfg_s.skip_order_confirmation_note),
    );
    render_toggle_row(
        f,
        rows[6],
        cfg_s.skip_preflight,
        sel(5),
        on_off(trading.config.skip_preflight),
        Some(cfg_s.skip_preflight_note),
    );

    // Rows 6..=7: Compute unit price & limit
    let cu_auto_label;
    let (cu_default_value, cu_default_label) = match current_auto_priority_fee() {
        Some(fee) => {
            cu_auto_label = format!("<{} / {fee}>", cfg_s.cu_auto);
            (String::new(), cu_auto_label.as_str())
        }
        None => (
            DEFAULT_COMPUTE_UNIT_PRICE_MICRO_LAMPORTS.to_string(),
            cfg_s.cu_default,
        ),
    };
    render_cu_field_row(
        f,
        rows[7],
        cfg_s.cu_price,
        cfg_s.cu_price_note,
        sel(6) || editing_cu_price,
        editing_cu_price,
        &trading.input_buffer,
        trading
            .config
            .compute_unit_price_micro_lamports
            .map(|v| v.to_string()),
        cu_default_value,
        cu_default_label,
    );
    render_cu_field_row(
        f,
        rows[8],
        cfg_s.cu_limit,
        cfg_s.cu_limit_note,
        sel(7) || editing_cu_limit,
        editing_cu_limit,
        &trading.input_buffer,
        trading
            .config
            .compute_unit_limit_per_position
            .map(|v| v.to_string()),
        DEFAULT_COMPUTE_UNIT_LIMIT_PER_POSITION.to_string(),
        cfg_s.cu_default,
    );
}
