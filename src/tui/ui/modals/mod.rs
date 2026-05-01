//! Modal dialogs grouped by screen.

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

mod chrome;
mod config;
mod ledger;
mod liquidation_feed;
mod market_selector;
mod orders;
mod position_leaderboard;
mod positions;
mod quit;
mod wallet_path;

pub(in crate::tui::ui) use chrome::*;
pub(in crate::tui::ui) use config::*;
pub(in crate::tui::ui) use ledger::*;
pub(in crate::tui::ui) use liquidation_feed::*;
pub(in crate::tui::ui) use market_selector::*;
pub(in crate::tui::ui) use orders::*;
pub(in crate::tui::ui) use position_leaderboard::*;
pub(in crate::tui::ui) use positions::*;
pub(in crate::tui::ui) use quit::*;
pub(in crate::tui::ui) use wallet_path::*;
