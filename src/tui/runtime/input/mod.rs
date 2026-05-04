//! Keyboard input handlers grouped by input mode.

//! Keyboard input handlers for each input mode.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use phoenix_rise::PhoenixHttpClient;

use super::super::config::{
    default_wallet_path, resolve_wallet_modal_input, save_user_config, SplineConfig,
};
use super::super::format::{fmt_size, truncate_balance};
use super::super::i18n::strings;
use super::super::math::{ui_size_to_num_base_lots, LotConversionError, MAX_UI_ORDER_SIZE_UNITS};
use super::super::state::{TradingState, TuiState};
use super::super::trading::{InputMode, OrderKind, PendingAction, TradingSide};
use super::wallet::{connect_wallet_with_keypair, disconnect_wallet};
use super::{Channels, KeyAction};

const MAX_USDC_TRANSFER_AMOUNT: f64 = 1_000_000_000.0;

mod amounts;
mod clipboard;
mod forms;
mod market;
mod normal;
mod referral;
mod settings;
mod views;

pub(in crate::tui::runtime) use amounts::*;
pub(in crate::tui::runtime) use clipboard::*;
pub(in crate::tui::runtime) use forms::*;
pub(in crate::tui::runtime) use market::*;
pub(in crate::tui::runtime) use normal::*;
pub(in crate::tui::runtime) use referral::*;
pub(in crate::tui::runtime) use settings::*;
pub(in crate::tui::runtime) use views::*;
