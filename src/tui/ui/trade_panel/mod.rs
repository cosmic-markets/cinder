//! Trading panel widget (order side/type/size, position summary).

use phoenix_rise::MarketStatsUpdate;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::super::format::{fmt_pnl_compact, fmt_price, truncate_pubkey};
use super::super::i18n::strings;
use super::super::state::TradingState;
use super::super::trading::{InputMode, OrderKind, PendingAction, TradingSide};

mod actions;
mod layout;
mod order_entry;
mod position;

pub(super) fn render_trading_panel(
    f: &mut Frame,
    area: ratatui::layout::Rect,
    trading: &TradingState,
    symbol: &str,
    _market_stats: &Option<MarketStatsUpdate>,
    price_decimals: usize,
    _market_pubkey: &str,
) {
    let rows = layout::render_panel_frame(f, area, trading, symbol);
    order_entry::render_order_entry(f, &rows, trading, symbol, price_decimals);
    actions::render_actions(f, &rows, trading);
    position::render_position_context(f, area, &rows, trading, symbol, price_decimals);
}
