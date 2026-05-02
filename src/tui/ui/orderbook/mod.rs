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

mod table;

use table::render_side_table;

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
        .map(|r| r.size * r.price)
        .sum();
    let bid_notional_usd: f64 = merged
        .bid_rows
        .iter()
        .take(n)
        .map(|r| r.size * r.price)
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

/// True when one of the constituent traders at this price level is the user's
/// wallet. Restricted to `RowSource::Clob` matches because the user only places
/// CLOB orders — a spline trader whose pubkey prefix collides with the user's
/// wallet shouldn't get the arrow.
pub(super) fn user_order_at_book_row(user_trader_prefix: Option<&str>, row: &BookRow) -> bool {
    let Some(prefix) = user_trader_prefix else {
        return false;
    };
    row.traders
        .iter()
        .any(|(trader, source)| *source == RowSource::Clob && trader == prefix)
}
