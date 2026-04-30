//! Order-book table rendering (ask side, bid side, spread row).

use phoenix_rise::MarketStatsUpdate;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::super::config::SplineConfig;
use super::super::constants::{ASK_BORDER, BID_BORDER, FIRE_ORANGE, TOP_N};
use super::super::format::{fmt_compact_prec, fmt_price, fmt_size};
use super::super::i18n::strings;
use super::super::state::{BookRow, MergedBook, RowSource};

pub(super) fn render_orderbook(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    cfg: &SplineConfig,
    merged: &MergedBook,
    wss_slot: u64,
    market_stats: &Option<MarketStatsUpdate>,
    _chart_data: &[(f64, f64)],
    user_trader_prefix: Option<&str>,
) {
    let data_symmetric =
        (merged.ask_rows.len().min(TOP_N) as u16).min(merged.bid_rows.len().min(TOP_N) as u16);
    let max_rows = area.height.saturating_sub(4).saturating_sub(6) / 2;
    let symmetric_count = data_symmetric.min(max_rows).max(1);

    let side_height = symmetric_count + 3;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(side_height),
            Constraint::Length(1),
            Constraint::Length(side_height),
        ])
        .split(area);

    let header_left = {
        let mut spans = vec![
            Span::styled(
                " 🐦‍🔥 Phoenix ",
                Style::default()
                    .fg(FIRE_ORANGE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cfg.symbol.as_str(), Style::default().fg(Color::White)),
        ];
        if let Some(stats) = market_stats {
            let change_notional = stats.mark_price - stats.prev_day_mark_price;
            let change_pct = if stats.prev_day_mark_price != 0.0 {
                (change_notional / stats.prev_day_mark_price) * 100.0
            } else {
                0.0
            };
            let change_color = if change_notional >= 0.0 {
                Color::LightGreen
            } else {
                Color::LightRed
            };
            spans.push(Span::styled(
                format!(" {:+.2}%", change_pct),
                Style::default().fg(change_color),
            ));

            let oi_usd = stats.open_interest * stats.mark_price;
            let s = strings();
            spans.push(Span::styled(
                format!(" {} ", s.vol),
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                format!("${}", fmt_price(stats.day_volume_usd, 0)),
                Style::default().fg(Color::White),
            ));
            spans.push(Span::styled(
                format!(" {} ", s.oi),
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                format!("${} ", fmt_price(oi_usd, 0)),
                Style::default().fg(Color::White),
            ));
        }
        Line::from(spans)
    };

    let ob_s = strings();
    let mut info_spans = Vec::new();
    if let Some(stats) = market_stats {
        info_spans.push(Span::styled(
            format!(" {} ", ob_s.mark),
            Style::default().fg(Color::DarkGray),
        ));
        info_spans.push(Span::styled(
            format!("${} ", fmt_price(stats.mark_price, cfg.price_decimals)),
            Style::default().fg(Color::White),
        ));

        info_spans.push(Span::styled(
            format!(" {} ", ob_s.index_price),
            Style::default().fg(Color::DarkGray),
        ));
        info_spans.push(Span::styled(
            format!("${} ", fmt_price(stats.oracle_price, cfg.price_decimals)),
            Style::default().fg(Color::White),
        ));

        let funding_annual_pct = stats.funding_rate * 8760.0;
        let rate_color = if funding_annual_pct >= 0.0 {
            Color::LightGreen
        } else {
            Color::LightRed
        };

        info_spans.push(Span::styled(
            format!(" {} ", ob_s.funding),
            Style::default().fg(Color::DarkGray),
        ));
        info_spans.push(Span::styled(
            format!("{:+.2}%", funding_annual_pct),
            Style::default().fg(rate_color),
        ));
        info_spans.push(Span::styled(
            " APR".to_string(),
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        info_spans.push(Span::styled(
            format!(" {}", ob_s.waiting_data),
            Style::default().fg(Color::DarkGray),
        ));
    }

    let header_block = Block::default()
        .title(header_left)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let header = Paragraph::new(Line::from(info_spans)).block(header_block);
    f.render_widget(header, chunks[0]);

    let n = symmetric_count as usize;
    let ask_visible_depth: f64 = merged.ask_rows.iter().take(n).map(|r| r.size).sum();
    let bid_visible_depth: f64 = merged.bid_rows.iter().take(n).map(|r| r.size).sum();
    let shared_max_depth = ask_visible_depth.max(bid_visible_depth).max(1.0);
    let ask_notional_usd: f64 = merged
        .ask_rows
        .iter()
        .take(n)
        .map(|r| r.size * (r.price_start + r.price_end) / 2.0)
        .sum();
    let bid_notional_usd: f64 = merged
        .bid_rows
        .iter()
        .take(n)
        .map(|r| r.size * (r.price_start + r.price_end) / 2.0)
        .sum();

    render_side_table(
        f,
        chunks[1],
        &merged.ask_rows,
        true,
        wss_slot,
        shared_max_depth,
        n,
        merged.best_ask,
        cfg.price_decimals,
        cfg.size_decimals,
        ask_notional_usd,
        user_trader_prefix,
    );

    let spread_line = match merged.spread {
        Some(sp) => {
            let bid_str = merged
                .best_bid
                .map_or_else(|| "-".to_string(), |b| fmt_price(b, cfg.price_decimals));
            let ask_str = merged
                .best_ask
                .map_or_else(|| "-".to_string(), |a| fmt_price(a, cfg.price_decimals));
            let pct = match merged.best_bid {
                Some(b) if b > 0.0 => format!(" ({:.2}%)", (sp / b) * 100.0),
                _ => String::new(),
            };
            Line::from(vec![
                Span::styled(
                    format!("{} ", ob_s.bid_abbrev),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(bid_str, Style::default().fg(Color::LightGreen)),
                Span::styled(
                    format!(" {} ", ob_s.ask_abbrev),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(ask_str, Style::default().fg(Color::LightRed)),
                Span::styled(
                    format!(" | ${}{}", fmt_price(sp, cfg.price_decimals), pct),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        }
        None => Line::from(Span::styled(
            format!("── {} ──", ob_s.no_spread),
            Style::default().fg(Color::DarkGray),
        )),
    };
    let spread = Paragraph::new(spread_line)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    f.render_widget(spread, chunks[2]);

    render_side_table(
        f,
        chunks[3],
        &merged.bid_rows,
        false,
        wss_slot,
        shared_max_depth,
        n,
        merged.best_bid,
        cfg.price_decimals,
        cfg.size_decimals,
        bid_notional_usd,
        user_trader_prefix,
    );
}

/// True when this CLOB row is owned by the user's wallet (matched by authority
/// pubkey prefix). Avoids the false positives a price-level match produces when
/// another trader quotes the same tick as the user. Spline rows never get the
/// arrow.
pub(super) fn user_order_at_book_row(user_trader_prefix: Option<&str>, row: &BookRow) -> bool {
    if row.source != RowSource::Clob {
        return false;
    }
    let Some(prefix) = user_trader_prefix else {
        return false;
    };
    row.trader == prefix
}

/// Renders either the Bid or Ask side of the orderbook table.
fn render_side_table(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    rows: &[BookRow],
    is_ask: bool,
    wss_slot: u64,
    max_depth: f64,
    visible_n: usize,
    best_price: Option<f64>,
    price_decimals: usize,
    size_decimals: usize,
    side_notional_usd: f64,
    user_trader_prefix: Option<&str>,
) {
    // When terminal is narrow, hide Size column to preserve the depth bar.
    let borders: u16 = 2;
    let fixed_with_size: u16 = 7 + 1 + 22 + 1 + 10 + 1 + 10 + 1;
    let fixed_without_size: u16 = 7 + 1 + 22 + 1 + 10 + 1;
    let bar_with_size = area.width.saturating_sub(fixed_with_size + borders) as usize;
    let show_size_col = bar_with_size >= 4;
    let fixed_cols = if show_size_col {
        fixed_with_size
    } else {
        fixed_without_size
    };
    let max_depth_bar = area.width.saturating_sub(fixed_cols + borders) as usize;
    let max_depth_bar = max_depth_bar.max(4);

    let display_rows: Vec<(usize, &BookRow, f64)> = {
        let mut agg = 0.0;
        rows.iter()
            .take(visible_n)
            .enumerate()
            .map(|(i, row)| {
                agg += row.size;
                (i, row, agg)
            })
            .collect()
    };

    let count = display_rows.len();
    let total = rows.len();

    let st_s = strings();
    let (label, border_color, price_color) = if is_ask {
        (st_s.asks, ASK_BORDER, Color::LightRed)
    } else {
        (st_s.bids, BID_BORDER, Color::LightGreen)
    };

    let best_str = best_price
        .map(|p| format!(" ${}", fmt_price(p, price_decimals)))
        .unwrap_or_default();

    let title_left = Line::from(vec![
        Span::styled(format!(" {}", label), Style::default().fg(Color::White)),
        Span::styled(best_str, Style::default().fg(price_color)),
        Span::styled(
            format!(" ({}/{}) ", count, total),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let title_right = Line::from(vec![
        Span::styled(
            format!(" {} ", st_s.slot),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("{} ", wss_slot),
            Style::default().fg(Color::DarkGray),
        ),
    ])
    .alignment(Alignment::Right);

    let block = Block::default()
        .title(title_left)
        .title(title_right)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let mut header_cells = vec![
        Cell::from(Line::from(st_s.trader).alignment(Alignment::Right)),
        Cell::from(Line::from(st_s.price_range).alignment(Alignment::Right)),
    ];
    if show_size_col {
        header_cells.push(Cell::from(
            Line::from(st_s.size).alignment(Alignment::Right),
        ));
    }
    header_cells.push(Cell::from(
        Line::from(st_s.depth).alignment(Alignment::Right),
    ));
    header_cells.push(Cell::from(""));
    let header_row = Row::new(header_cells).style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let outermost_idx = count.saturating_sub(1);
    let table_rows: Vec<Row> = if is_ask {
        display_rows
            .iter()
            .rev()
            .map(|&(idx, row, depth)| {
                let notional = (idx == outermost_idx).then_some(side_notional_usd);
                let has_mine = user_order_at_book_row(user_trader_prefix, row);
                build_row(
                    row,
                    depth,
                    max_depth,
                    max_depth_bar,
                    true,
                    price_decimals,
                    size_decimals,
                    show_size_col,
                    notional,
                    has_mine,
                )
            })
            .collect()
    } else {
        display_rows
            .iter()
            .map(|&(idx, row, depth)| {
                let notional = (idx == outermost_idx).then_some(side_notional_usd);
                let has_mine = user_order_at_book_row(user_trader_prefix, row);
                build_row(
                    row,
                    depth,
                    max_depth,
                    max_depth_bar,
                    false,
                    price_decimals,
                    size_decimals,
                    show_size_col,
                    notional,
                    has_mine,
                )
            })
            .collect()
    };

    let widths: Vec<Constraint> = if show_size_col {
        vec![
            Constraint::Length(7),
            Constraint::Length(22),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Fill(1),
        ]
    } else {
        vec![
            Constraint::Length(7),
            Constraint::Length(22),
            Constraint::Length(10),
            Constraint::Fill(1),
        ]
    };

    let table = Table::new(table_rows, widths)
        .header(header_row)
        .block(block)
        .column_spacing(1);

    f.render_widget(table, area);
}

fn build_row<'a>(
    row: &BookRow,
    depth: f64,
    max_depth: f64,
    max_bar: usize,
    is_ask: bool,
    price_decimals: usize,
    size_decimals: usize,
    show_size_col: bool,
    outermost_notional_usd: Option<f64>,
    has_user_order_here: bool,
) -> Row<'a> {
    let bar_len = ((depth / max_depth) * max_bar as f64).round().max(1.0) as usize;
    let bar_len = bar_len.min(max_bar);
    let intensity = ((depth / max_depth) * 140.0).min(140.0) as u8;
    let color = if is_ask {
        Color::Rgb(80 + intensity, 40, 40)
    } else {
        Color::Rgb(40, 80 + intensity, 40)
    };

    let bar_cell = match outermost_notional_usd {
        Some(notional) => {
            let label = format!(" ${} ", fmt_compact_prec(notional, 1));
            let label_len = label.chars().count();
            if label_len <= bar_len {
                let rest = "\u{2593}".repeat(bar_len - label_len);
                let label_fg = if is_ask {
                    Color::Rgb(240, 200, 200)
                } else {
                    Color::Rgb(200, 240, 200)
                };
                Cell::from(Line::from(vec![
                    Span::styled(label, Style::default().fg(label_fg).bg(color)),
                    Span::styled(rest, Style::default().fg(color)),
                ]))
            } else {
                Cell::from("\u{2593}".repeat(bar_len)).style(Style::default().fg(color))
            }
        }
        None => Cell::from("\u{2593}".repeat(bar_len)).style(Style::default().fg(color)),
    };

    // CLOB rows are point levels (single price); splines span a range.
    let price_str = if matches!(row.source, RowSource::Clob) {
        format!("${}", fmt_price(row.price_start, price_decimals))
    } else {
        format!(
            "${} → ${}",
            fmt_price(row.price_start, price_decimals),
            fmt_price(row.price_end, price_decimals)
        )
    };
    let trader_color = match row.source {
        RowSource::Spline => FIRE_ORANGE,
        RowSource::Clob => Color::Cyan,
    };

    let trader_cell = if has_user_order_here {
        Cell::from(
            Line::from(vec![
                Span::styled(
                    ">",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", row.trader),
                    Style::default().fg(trader_color),
                ),
            ])
            .alignment(Alignment::Right),
        )
    } else {
        Cell::from(Line::from(row.trader.clone()).alignment(Alignment::Right))
            .style(Style::default().fg(trader_color))
    };

    let mut cells = vec![
        trader_cell,
        Cell::from(Line::from(price_str).alignment(Alignment::Right)),
    ];
    if show_size_col {
        cells.push(
            Cell::from(Line::from(fmt_size(row.size, size_decimals)).alignment(Alignment::Right))
                .style(Style::default().fg(color)),
        );
    }
    cells.push(
        Cell::from(Line::from(fmt_size(depth, size_decimals)).alignment(Alignment::Right))
            .style(Style::default().fg(color)),
    );
    cells.push(bar_cell);

    Row::new(cells)
}
