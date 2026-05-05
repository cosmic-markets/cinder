//! Configuration modal.

use super::*;

pub(in crate::tui::ui) fn render_config_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
) {
    let cfg_s = strings();
    const LABEL_W: u16 = 20;
    let popup_w: u16 = 80.min(area.width.saturating_sub(4));
    let popup_h: u16 = 9.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![Span::styled(
        format!(" ⚙  {} ", cfg_s.config),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]);

    let footer = match trading.input_mode {
        InputMode::EditingRpcUrl => Line::from(vec![
            Span::styled(
                " Enter ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", cfg_s.save_reconnect),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "Esc ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} ", cfg_s.cancel),
                Style::default().fg(Color::DarkGray),
            ),
        ])
        .left_aligned(),
        _ => Line::from(vec![
            Span::styled(
                " ↑↓ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", cfg_s.select),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "←→ ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", cfg_s.toggle),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "Enter ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", cfg_s.edit),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "Esc ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} ", cfg_s.cancel),
                Style::default().fg(Color::DarkGray),
            ),
        ])
        .left_aligned(),
    };

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacer
            Constraint::Length(1), // rpc url
            Constraint::Length(1), // language
            Constraint::Length(1), // clob orders
            Constraint::Length(1), // public RPC fan-out
            Constraint::Min(0),
        ])
        .split(inner);

    let editing_rpc = trading.input_mode == InputMode::EditingRpcUrl;
    let rpc_selected = trading.config_selected_field == 0 || editing_rpc;
    let lang_selected = trading.config_selected_field == 1 && !editing_rpc;
    let clob_selected = trading.config_selected_field == 2 && !editing_rpc;
    let fanout_selected = trading.config_selected_field == 3 && !editing_rpc;

    let rpc_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LABEL_W), Constraint::Min(0)])
        .split(rows[1]);

    let rpc_cursor = if rpc_selected { "▸ " } else { "  " };
    let rpc_label_style = if rpc_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(rpc_cursor, rpc_label_style),
            Span::styled(cfg_s.rpc_url, rpc_label_style),
        ])),
        rpc_cols[0],
    );

    let rpc_value_line = if editing_rpc {
        Line::from(vec![Span::styled(
            format!("{}_", trading.input_buffer),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )])
    } else {
        if trading.config.rpc_url.is_empty() {
            let resolved = std::env::var("RPC_URL")
                .or_else(|_| std::env::var("SOLANA_RPC_URL"))
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "https://api.mainnet-beta.solana.com".to_string());
            let host = super::super::rpc_host_from_urlish(&resolved);
            Line::from(vec![
                Span::styled(
                    cfg_s.rpc_default.to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(host, Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(Span::styled(
                trading.config.rpc_url.clone(),
                Style::default().fg(Color::White),
            ))
        }
    };
    f.render_widget(Paragraph::new(rpc_value_line), rpc_cols[1]);

    let lang_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LABEL_W), Constraint::Min(0)])
        .split(rows[2]);

    let lang_cursor = if lang_selected { "▸ " } else { "  " };
    let lang_label_style = if lang_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(lang_cursor, lang_label_style),
            Span::styled(cfg_s.language, lang_label_style),
        ])),
        lang_cols[0],
    );

    let (arrow_style, value_style) = if lang_selected {
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
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◀ ", arrow_style),
            Span::styled(trading.config.language.label(), value_style),
            Span::styled(" ▶", arrow_style),
        ])),
        lang_cols[1],
    );

    let clob_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LABEL_W), Constraint::Min(0)])
        .split(rows[3]);

    let clob_cursor = if clob_selected { "▸ " } else { "  " };
    let clob_label_style = if clob_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(clob_cursor, clob_label_style),
            Span::styled(cfg_s.clob_orders, clob_label_style),
        ])),
        clob_cols[0],
    );

    let (clob_arrow_style, clob_value_style) = if clob_selected {
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
    let clob_label = if trading.config.show_clob {
        cfg_s.on
    } else {
        cfg_s.off
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◀ ", clob_arrow_style),
            Span::styled(clob_label, clob_value_style),
            Span::styled(" ▶", clob_arrow_style),
            Span::styled(
                format!("  ({})", cfg_s.clob_orders_note),
                Style::default().fg(Color::DarkGray),
            ),
        ])),
        clob_cols[1],
    );

    let fanout_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LABEL_W), Constraint::Min(0)])
        .split(rows[4]);

    let fanout_cursor = if fanout_selected { "▸ " } else { "  " };
    let fanout_label_style = if fanout_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(fanout_cursor, fanout_label_style),
            Span::styled(cfg_s.fanout_public_rpc, fanout_label_style),
        ])),
        fanout_cols[0],
    );

    let (fanout_arrow_style, fanout_value_style) = if fanout_selected {
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
    let fanout_label = if trading.config.fanout_public_rpc {
        cfg_s.on
    } else {
        cfg_s.off
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◀ ", fanout_arrow_style),
            Span::styled(fanout_label, fanout_value_style),
            Span::styled(" ▶", fanout_arrow_style),
            Span::styled(
                format!("  ({})", cfg_s.fanout_public_rpc_note),
                Style::default().fg(Color::DarkGray),
            ),
        ])),
        fanout_cols[1],
    );
}
