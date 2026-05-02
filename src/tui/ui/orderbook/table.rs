//! Bid and ask side table rendering helpers.

use super::*;

/// Renders either the Bid or Ask side of the orderbook table.
pub(super) fn render_side_table(
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
    // Trader col fits up to four merged initials with the user-arrow prefix
    // ("> m/x/y/z" = 9 chars). Price drops from a 22-wide range to a single
    // value ("$12345.67" ≈ 9 chars).
    let borders: u16 = 2;
    let fixed_with_size: u16 = 9 + 1 + 10 + 1 + 10 + 1 + 10 + 1;
    let fixed_without_size: u16 = 9 + 1 + 10 + 1 + 10 + 1;
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
        Cell::from(Line::from(st_s.price).alignment(Alignment::Right)),
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
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Fill(1),
        ]
    } else {
        vec![
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Fill(1),
        ]
    };

    let table = Table::new(table_rows, widths)
        .header(header_row)
        .column_spacing(1)
        .block(block);

    f.render_widget(table, area);
}

/// Build the trader display string. A single quoter at a price level shows
/// the first 4 chars of the pubkey prefix (e.g. "mmmb"). When multiple
/// traders share the level, collapse to first-letter initials joined by "/"
/// with a hard cap of 4 traders (3 slashes) — extra traders past the cap are
/// silently dropped to keep the column inside its 9-char allotment.
/// Multi-trader rows are sorted alphabetically by prefix so the rendered
/// order is stable across frames.
fn render_trader_initials(traders: &[(String, RowSource)]) -> String {
    if traders.len() == 1 {
        return traders[0].0.chars().take(4).collect();
    }
    let mut sorted: Vec<&(String, RowSource)> = traders.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    for (i, (prefix, _)) in sorted.iter().take(4).enumerate() {
        if i > 0 {
            out.push('/');
        }
        if let Some(c) = prefix.chars().next() {
            out.push(c);
        }
    }
    out
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

    let price_str = format!("${}", fmt_price(row.price, price_decimals));

    let trader_color = FIRE_ORANGE;
    let display_traders = render_trader_initials(&row.traders);

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
                    format!(" {}", display_traders),
                    Style::default().fg(trader_color),
                ),
            ])
            .alignment(Alignment::Right),
        )
    } else {
        Cell::from(Line::from(display_traders).alignment(Alignment::Right))
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
