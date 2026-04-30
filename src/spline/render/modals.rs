//! Modal dialogs: market selector, positions, orders, tx, config, quit.

use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::super::constants::FIRE_ORANGE;
use super::super::format::{fmt_compact, fmt_price, fmt_size};
use super::super::i18n::strings;
use super::super::state::{
    MarketSelector, OrdersView, PositionsView, TopPositionsView, TradingState,
};
use super::super::trading::{InputMode, OrderInfo, TradingSide};
use super::{MODAL_BORDER, MODAL_HIGHLIGHT_BG};

pub(super) fn render_market_selector(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    selector: &MarketSelector,
    active_symbol: &str,
) {
    let list_height = selector.markets.len() as u16 + 4;
    let max_width: u16 = 72;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = list_height.min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let ms_s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", ms_s.markets),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", selector.markets.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let total_vol: f64 = selector.markets.iter().map(|m| m.volume_24h).sum();
    let total_oi: f64 = selector.markets.iter().map(|m| m.open_interest_usd).sum();
    let vol_title = Line::from(vec![
        Span::styled(
            format!("{} ", ms_s.vol_24h),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("${} ", fmt_price(total_vol, 0)),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!(" {} ", ms_s.oi),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("${} ", fmt_price(total_oi, 0)),
            Style::default().fg(Color::White),
        ),
    ])
    .right_aligned();

    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", ms_s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", ms_s.confirm),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", ms_s.back),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title(vol_title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let price_w: u16 = 12;
    let chg_w: u16 = 10;
    let lev_w: u16 = 10;
    let vol_w: u16 = 10;
    let oi_w: u16 = 10;

    let visible_slots = inner.height.saturating_sub(1) as usize; // -1 for header row
    let scroll_offset = if selector.selected_index >= visible_slots {
        selector.selected_index - visible_slots + 1
    } else {
        0
    };

    let table_rows: Vec<Row> = selector
        .markets
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, m)| {
            let is_selected = i == selector.selected_index;
            let is_active = m.symbol == active_symbol;

            let cursor_str = if is_selected { "▸" } else { " " };
            let sym_str = if is_active {
                format!("{} ●", m.symbol)
            } else {
                m.symbol.clone()
            };
            let price_str = if m.price > 0.0 {
                format!("${}", fmt_price(m.price, m.price_decimals))
            } else {
                "—".to_string()
            };
            let chg_str = if m.change_24h != 0.0 {
                format!("{:+.1}%", m.change_24h)
            } else {
                "—".to_string()
            };
            let chg_color = if is_selected {
                Color::White
            } else if m.change_24h > 0.0 {
                Color::LightGreen
            } else if m.change_24h < 0.0 {
                Color::LightRed
            } else {
                Color::DarkGray
            };
            let lev_str = format!("{:.0}x", m.max_leverage);
            let vol_str = if m.volume_24h > 0.0 {
                format!("${}", fmt_compact(m.volume_24h))
            } else {
                "—".to_string()
            };
            let oi_str = if m.open_interest_usd > 0.0 {
                format!("${}", fmt_compact(m.open_interest_usd))
            } else {
                "—".to_string()
            };

            let row_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(MODAL_HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            // Per-cell colors only apply to unselected, non-active rows.
            // Selected rows stay all-white on the highlight bg; active rows stay cyan.
            let (price_cell_style, lev_cell_style, vol_cell_style, oi_cell_style) =
                if is_selected || is_active {
                    (
                        Style::default(),
                        Style::default().fg(FIRE_ORANGE),
                        Style::default(),
                        Style::default(),
                    )
                } else {
                    (
                        Style::default().fg(Color::White),
                        Style::default().fg(FIRE_ORANGE),
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::DarkGray),
                    )
                };

            Row::new(vec![
                Cell::from(cursor_str),
                Cell::from(sym_str).style(Style::default().fg(FIRE_ORANGE)),
                Cell::from(Line::from(price_str).alignment(Alignment::Right))
                    .style(price_cell_style),
                Cell::from(Line::from(chg_str).alignment(Alignment::Right))
                    .style(Style::default().fg(chg_color)),
                Cell::from(Line::from(lev_str).alignment(Alignment::Right)).style(lev_cell_style),
                Cell::from(Line::from(vol_str).alignment(Alignment::Right)).style(vol_cell_style),
                Cell::from(Line::from(oi_str).alignment(Alignment::Right)).style(oi_cell_style),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(ms_s.market),
        Cell::from(Line::from(ms_s.price).alignment(Alignment::Right)),
        Cell::from(Line::from(ms_s.pct_change).alignment(Alignment::Right)),
        Cell::from(Line::from(ms_s.leverage).alignment(Alignment::Right)),
        Cell::from(Line::from(ms_s.vol_24h).alignment(Alignment::Right)),
        Cell::from(Line::from(ms_s.oi).alignment(Alignment::Right)),
    ])
    .style(header_style);

    let widths = [
        Constraint::Length(1),
        Constraint::Max(10),
        Constraint::Length(price_w),
        Constraint::Length(chg_w),
        Constraint::Length(lev_w),
        Constraint::Length(vol_w),
        Constraint::Length(oi_w),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);

    f.render_widget(table, inner);
}

pub(super) fn render_positions_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &PositionsView,
    active_symbol: &str,
    markets: &MarketSelector,
) {
    let row_count = view.positions.len().max(1) as u16;
    let max_width: u16 = 90;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (row_count + 6).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    // `fmt_price` with 2 decimals renders anything in (-0.005, 0) as "-0.00", which
    // produces "$-0.00" when there are no positions (and -0.0 can sneak in from
    // float accumulation). Snap sub-cent magnitudes to +0.0 so the header reads
    // "$0.00" cleanly.
    let snap_zero = |v: f64| if v.abs() < 0.005 { 0.0 } else { v };
    let agg_notional = snap_zero(view.aggregate_notional());
    let agg_pnl = snap_zero(view.aggregate_pnl());
    let pnl_color = if agg_pnl >= 0.0 {
        Color::LightGreen
    } else {
        Color::LightRed
    };
    let pnl_prefix = if agg_pnl >= 0.0 { "+$" } else { "-$" };

    let pm_s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", pm_s.positions),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", view.positions.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let pnl_title = Line::from(vec![
        Span::styled(
            format!(" {} ", pm_s.notional_col),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("${} ", fmt_price(agg_notional, 2)),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!(" {} ", pm_s.upnl),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("{}{} ", pnl_prefix, fmt_price(agg_pnl.abs(), 2)),
            Style::default().fg(pnl_color),
        ),
    ])
    .right_aligned();

    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", pm_s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", pm_s.view_market),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "x ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", pm_s.close_pos),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "u ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", pm_s.close_all),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", pm_s.back),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title(pnl_title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if view.positions.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", pm_s.no_open_positions),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, inner);
        return;
    }

    let visible_slots = inner.height.saturating_sub(1) as usize;
    let scroll_offset = if view.selected_index >= visible_slots {
        view.selected_index - visible_slots + 1
    } else {
        0
    };

    let table_rows: Vec<Row> = view
        .positions
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, p)| {
            let is_selected = i == view.selected_index;
            let is_active = p.symbol == active_symbol;
            let cursor_str = if is_selected { "▸" } else { " " };
            let sym_str = if is_active {
                format!("{} ●", p.symbol)
            } else {
                p.symbol.clone()
            };

            let price_decimals = markets.price_decimals_for_symbol(&p.symbol);

            let pnl_color = if p.unrealized_pnl >= 0.0 {
                Color::LightGreen
            } else {
                Color::LightRed
            };
            let pnl_prefix = if p.unrealized_pnl >= 0.0 { "+$" } else { "-$" };
            let pnl_str = format!("{}{}", pnl_prefix, fmt_price(p.unrealized_pnl.abs(), 2));

            let liq_str = p
                .liquidation_price
                .map(|l| format!("${}", fmt_price(l, price_decimals)))
                .unwrap_or_else(|| "—".to_string());

            let lev_str = p
                .leverage
                .map(|l| format!("{:.1}x", l))
                .unwrap_or_else(|| "—".to_string());

            let row_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(MODAL_HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            Row::new(vec![
                Cell::from(cursor_str),
                Cell::from(sym_str).style(Style::default().fg(FIRE_ORANGE)),
                Cell::from(Span::styled(
                    match p.side {
                        TradingSide::Long => pm_s.long_label,
                        TradingSide::Short => pm_s.short_label,
                    },
                    Style::default()
                        .fg(p.side.color())
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Line::from(format!("{}", p.size)).alignment(Alignment::Right)),
                Cell::from(
                    Line::from(format!("${}", fmt_price(p.entry_price, price_decimals)))
                        .alignment(Alignment::Right),
                ),
                Cell::from(
                    Line::from(format!("${}", fmt_price(p.notional, 2)))
                        .alignment(Alignment::Right),
                ),
                Cell::from(Line::from(pnl_str).alignment(Alignment::Right))
                    .style(Style::default().fg(pnl_color)),
                Cell::from(Line::from(liq_str).alignment(Alignment::Right)),
                Cell::from(Line::from(lev_str).alignment(Alignment::Right))
                    .style(Style::default().fg(FIRE_ORANGE)),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(pm_s.market),
        Cell::from(pm_s.side),
        Cell::from(Line::from(pm_s.size).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.entry).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.notional_col).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.pnl).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.liq_col).alignment(Alignment::Right)),
        Cell::from(Line::from(pm_s.lev_col).alignment(Alignment::Right)),
    ])
    .style(header_style);

    let widths = [
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(7),
        Constraint::Length(9),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Length(10),
        Constraint::Length(7),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);

    f.render_widget(table, inner);
}

/// "Top positions on Phoenix" modal. Table columns: rank, market, trader,
/// side, size, entry, notional, PnL. Sized similar to the positions modal
/// but wider to fit the trader column.
pub(super) fn render_top_positions_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &TopPositionsView,
    markets: &MarketSelector,
) {
    // Cap visible rows so the popup doesn't grow to fit the full top-N (50).
    // Anything beyond `MAX_VISIBLE_ROWS` becomes scrollable via the existing
    // selected_index → scroll_offset path below.
    const MAX_VISIBLE_ROWS: u16 = 15;
    let row_count = (view.positions.len() as u16).clamp(1, MAX_VISIBLE_ROWS);
    // Sum of column widths + column spacing + borders. Picked tightly so the
    // modal doesn't sprawl on wide terminals.
    let max_width: u16 = 78;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (row_count + 6).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.top_positions_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", view.positions.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.top_positions_copy_hint),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{} ", s.back), Style::default().fg(Color::DarkGray)),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if view.positions.is_empty() {
        // Cold-open placeholder vs genuine empty list — the task flips
        // `loaded` after the first refresh completes.
        let msg = if view.loaded {
            s.top_positions_empty
        } else {
            s.top_positions_loading
        };
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", msg),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, inner);
        return;
    }

    let visible_slots = inner.height.saturating_sub(1) as usize;
    let scroll_offset = if view.selected_index >= visible_slots {
        view.selected_index - visible_slots + 1
    } else {
        0
    };

    let table_rows: Vec<Row> = view
        .positions
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, p)| {
            let is_selected = i == view.selected_index;
            let cursor_str = if is_selected { "▸" } else { " " };
            let rank_str = format!("{}", i + 1);

            let price_decimals = markets.price_decimals_for_symbol(&p.symbol);

            let pnl_color = if p.unrealized_pnl >= 0.0 {
                Color::LightGreen
            } else {
                Color::LightRed
            };
            let pnl_prefix = if p.unrealized_pnl >= 0.0 { "+$" } else { "-$" };
            let pnl_str = format!("{}{}", pnl_prefix, fmt_price(p.unrealized_pnl.abs(), 2));

            let row_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(MODAL_HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            Row::new(vec![
                Cell::from(cursor_str),
                Cell::from(Line::from(rank_str).alignment(Alignment::Right))
                    .style(Style::default().fg(Color::DarkGray)),
                Cell::from(p.symbol.clone()).style(Style::default().fg(FIRE_ORANGE)),
                Cell::from(p.trader_display.clone()).style(Style::default().fg(Color::White)),
                Cell::from(Span::styled(
                    match p.side {
                        TradingSide::Long => s.long_label,
                        TradingSide::Short => s.short_label,
                    },
                    Style::default()
                        .fg(p.side.color())
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Line::from(fmt_compact(p.size)).alignment(Alignment::Right)),
                Cell::from(
                    Line::from(format!("${}", fmt_price(p.entry_price, price_decimals)))
                        .alignment(Alignment::Right),
                ),
                Cell::from(
                    Line::from(format!("${}", fmt_compact(p.notional))).alignment(Alignment::Right),
                ),
                Cell::from(Line::from(pnl_str).alignment(Alignment::Right))
                    .style(Style::default().fg(pnl_color)),
            ])
            .style(row_style)
        })
        .collect();

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(Line::from(s.top_positions_rank).alignment(Alignment::Right)),
        Cell::from(s.market),
        Cell::from(s.top_positions_trader),
        Cell::from(s.side),
        Cell::from(Line::from(s.size).alignment(Alignment::Right)),
        Cell::from(Line::from(s.entry).alignment(Alignment::Right)),
        Cell::from(Line::from(s.notional_col).alignment(Alignment::Right)),
        Cell::from(Line::from(s.pnl).alignment(Alignment::Right)),
    ])
    .style(header_style);

    let widths = [
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(8),
        // Trader column: 4 + ellipsis + 4 = 9 visible chars, plus a small
        // cushion so the cell isn't flush with its neighbour.
        Constraint::Length(10),
        Constraint::Length(5),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);

    f.render_widget(table, inner);
}

pub(super) fn render_switching_modal(f: &mut Frame, area: ratatui::layout::Rect, symbol: &str) {
    let label = format!("🐦‍🔥 Switching to {} market… ", symbol);
    let popup_w: u16 = (label.len() as u16 + 4).min(area.width.saturating_sub(4));
    let popup_h: u16 = 3.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let text = Line::from(vec![
        Span::styled("🐦‍🔥 Switching to ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            symbol,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" market…", Style::default().fg(Color::DarkGray)),
    ])
    .centered();

    f.render_widget(
        Paragraph::new(text).alignment(ratatui::layout::Alignment::Center),
        inner,
    );
}

pub(super) fn render_orders_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    view: &OrdersView,
    active_symbol: &str,
) {
    let row_count = view.orders.len().max(1) as u16;
    let max_width: u16 = 96;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (row_count + 6).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let om_s = strings();
    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", om_s.orders),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", view.orders.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", om_s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "x ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", om_s.cxl_order),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "u ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", om_s.cxl_all),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", om_s.back),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if view.orders.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", om_s.no_open_orders),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, inner);
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled(
            format!("  {}", om_s.market),
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.side,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.order_type,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.price,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.size,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.filled,
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            om_s.flags,
            Style::default().fg(Color::DarkGray),
        )),
    ]);

    let visible_slots = inner.height.saturating_sub(1) as usize;
    let scroll_offset = if view.selected_index >= visible_slots {
        view.selected_index - visible_slots + 1
    } else {
        0
    };

    let table_rows: Vec<Row> = view
        .orders
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_slots)
        .map(|(i, o)| order_row(o, active_symbol, i == view.selected_index))
        .collect();

    let widths = [
        Constraint::Length(13),
        Constraint::Length(5),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Min(0),
    ];
    let table = Table::new(table_rows, widths)
        .header(header)
        .column_spacing(1);
    f.render_widget(table, inner);
}

fn order_row<'a>(o: &'a OrderInfo, active_symbol: &str, is_selected: bool) -> Row<'a> {
    let is_active = o.symbol == active_symbol;
    let cursor_str = if is_selected { "▸" } else { " " };
    let sym_str = if is_active {
        format!("{} {} ●", cursor_str, o.symbol)
    } else {
        format!("{} {}", cursor_str, o.symbol)
    };

    let side_color = o.side.color();
    let or_s = strings();
    let side_label = match o.side {
        TradingSide::Long => or_s.buy,
        TradingSide::Short => or_s.sell,
    };

    let filled = if o.initial_size > 0.0 {
        100.0 * (1.0 - (o.size_remaining / o.initial_size).clamp(0.0, 1.0))
    } else {
        0.0
    };

    let mut flag_parts: Vec<&str> = Vec::new();
    if o.reduce_only {
        flag_parts.push("RO");
    }
    if o.is_stop_loss {
        flag_parts.push("SL");
    }
    let flags = flag_parts.join(" ");

    let row_style = if is_selected {
        Style::default()
            .fg(Color::White)
            .bg(MODAL_HIGHLIGHT_BG)
            .add_modifier(Modifier::BOLD)
    } else if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };

    Row::new(vec![
        Cell::from(Span::styled(
            sym_str,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            side_label,
            Style::default().fg(side_color).add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            if o.is_stop_loss {
                or_s.stp.to_string()
            } else {
                // The SDK returns "Limit" / "Market" / etc. Map the two we
                // know to the 3-letter abbreviation used elsewhere in the UI;
                // anything else (future order types) renders uppercased.
                match o.order_type.as_str() {
                    "Limit" => or_s.lmt.to_string(),
                    "Market" => or_s.mkt.to_string(),
                    other => other.to_uppercase(),
                }
            },
            Style::default().fg(Color::White),
        )),
        Cell::from(Span::styled(
            format!("${}", fmt_price(o.price_usd, 4)),
            Style::default().fg(Color::White),
        )),
        Cell::from(Span::styled(
            fmt_size(o.size_remaining, 4),
            Style::default().fg(Color::White),
        )),
        Cell::from(Span::styled(
            format!("{:.1}%", filled),
            Style::default().fg(Color::DarkGray),
        )),
        Cell::from(Span::styled(
            flags,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
    ])
    .style(row_style)
}

pub(super) fn render_ledger_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
) {
    let lg_s = strings();
    // Each entry takes 2 lines (header row + signature row), plus one header line,
    // plus borders/padding.
    let body_lines = (trading.ledger.len().max(1) as u16) * 2 + 1;
    let max_width: u16 = 110;
    let popup_w = max_width.min(area.width.saturating_sub(4));
    let popup_h = (body_lines + 4).min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![
        Span::styled(
            " 🐦‍🔥 Phoenix ",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", lg_s.ledger_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({}) ", trading.ledger.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let footer = Line::from(vec![
        Span::styled(
            " ↑↓ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", lg_s.select),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", lg_s.close),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if trading.ledger.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            format!(" {}", lg_s.ledger_empty),
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(empty, inner);
        return;
    }

    // Header line: column labels for the first (summary) row of each entry.
    // The second row per entry holds only the txid and spans the full width,
    // so it doesn't get a column label.
    let header_line = Line::from(vec![
        Span::styled(
            format!("  {:<11}", lg_s.ledger_col_time),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(lg_s.ledger_col_action, Style::default().fg(Color::DarkGray)),
    ]);
    let header_rect = ratatui::layout::Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(header_line), header_rect);

    let body_rect = ratatui::layout::Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    let visible_entries = (body_rect.height / 2) as usize;
    if visible_entries == 0 {
        return;
    }
    let selected = trading
        .ledger_selected
        .min(trading.ledger.len().saturating_sub(1));
    let scroll_offset = if selected >= visible_entries {
        selected - visible_entries + 1
    } else {
        0
    };

    for (slot, (i, entry)) in trading
        .ledger
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_entries)
        .enumerate()
    {
        let is_selected = i == selected;
        let cursor_str = if is_selected { "▸" } else { " " };
        let entry_style = if is_selected {
            Style::default()
                .bg(MODAL_HIGHLIGHT_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let summary_line = Line::from(vec![
            Span::styled(
                format!(" {}", cursor_str),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:<11}", entry.timestamp),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(entry.title.clone(), Style::default().fg(Color::White)),
        ]);
        let sig_line = Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(entry.txid.clone(), Style::default().fg(Color::Cyan)),
        ]);

        let entry_rect = ratatui::layout::Rect {
            x: body_rect.x,
            y: body_rect.y + (slot as u16) * 2,
            width: body_rect.width,
            height: 2,
        };
        let para = Paragraph::new(vec![summary_line, sig_line]).style(entry_style);
        f.render_widget(para, entry_rect);
    }
}

pub(super) fn render_config_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
) {
    let cfg_s = strings();
    const LABEL_W: u16 = 20;
    let popup_w: u16 = 80.min(area.width.saturating_sub(4));
    let popup_h: u16 = 8.min(area.height.saturating_sub(2));
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
            Constraint::Min(0),
        ])
        .split(inner);

    let editing_rpc = trading.input_mode == InputMode::EditingRpcUrl;
    let rpc_selected = trading.config_selected_field == 0 || editing_rpc;
    let lang_selected = trading.config_selected_field == 1 && !editing_rpc;
    let clob_selected = trading.config_selected_field == 2 && !editing_rpc;

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
            let host = super::rpc_host_from_urlish(&resolved);
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
}

/// "Load Wallet" modal — single-line editable path with a hint row and an
/// optional error row when a previous Enter failed.
pub(super) fn render_wallet_path_modal(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
) {
    let s = strings();
    let has_error = trading.wallet_path_error.is_some();
    let desired_h: u16 = if has_error { 7 } else { 6 };
    let popup_h: u16 = desired_h.min(area.height.saturating_sub(2));
    let popup_w: u16 = 80.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![
        Span::raw(" 🐦‍🔥 "),
        Span::styled(
            format!("{} ", s.st_load_wallet_title),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let footer = Line::from(vec![
        Span::styled(
            " Enter ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", s.st_load_wallet_action),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", s.cancel),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .left_aligned();

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let mut constraints = vec![
        Constraint::Length(1), // label
        Constraint::Length(1), // input
        Constraint::Length(1), // spacer
    ];
    if has_error {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(0));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", s.st_wallet_path_label),
            Style::default().fg(Color::DarkGray),
        ))),
        rows[0],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{}_", trading.wallet_path_buffer),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        ])),
        rows[1],
    );

    if has_error {
        let err = trading.wallet_path_error.as_deref().unwrap_or("");
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", s.st_wallet_load_failed),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(err.to_string(), Style::default().fg(Color::LightRed)),
            ])),
            rows[3],
        );
    }
}

pub(super) fn render_quit_modal(f: &mut Frame, area: ratatui::layout::Rect) {
    let popup_w: u16 = 32.min(area.width.saturating_sub(4));
    let popup_h: u16 = 3.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

    f.render_widget(ratatui::widgets::Clear, popup_area);

    let title = Line::from(vec![
        Span::raw("🐦‍🔥 "),
        Span::styled(
            "Phoenix",
            Style::default()
                .fg(FIRE_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MODAL_BORDER));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let q_s = strings();
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", q_s.quit_confirm),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            "[Y]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "[N]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    ])
    .centered();

    f.render_widget(Paragraph::new(line), inner);
}
