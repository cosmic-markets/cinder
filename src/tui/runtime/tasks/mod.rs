//! Background task spawners: wallet lifecycle, market snapshots, L2 book,
//! trader orders WS, wallet WSS, and liquidation feed.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use phoenix_rise::accounts::owned::{
    ConditionalOrderCollection, ConditionalOrderTrigger, StopLossDirection, StopLossOrderKind,
    StopLossTradeSide,
};
use phoenix_rise::api::{PhoenixHttpClient, PhoenixWSClient, Trader, TraderKey};
use phoenix_rise::ix::constants::get_conditional_orders_address;
use phoenix_rise::ix::types::Direction;
use phoenix_rise::types::trader::{
    TraderStatePayload, TraderStateRowChangeKind, TraderStateStopLossTrigger,
};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::config::RpcAccountInfoConfig;
use solana_signer::Signer;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tracing::warn;

use super::super::config::{SplineConfig, rpc_http_url_from_env};
use super::super::data::{
    GtiHandle, L2Level, fetch_top_positions, parse_l2_book_from_market_account,
};
use super::super::format::pubkey_trader_prefix;
use super::super::state::{
    BalanceUpdate, ClobLevel, L2BookStreamMsg, SplineBootstrapMsg, TxStatusMsg,
};
use super::super::trading::{
    OrderInfo, TopPositionEntry, TradingSide, fetch_phoenix_balance_and_position,
};
use super::super::tx::TxContext;
use super::{L2_SNAPSHOT_DEPTH, TxCtxMsg, WSS_RETRY_CAP, WSS_RETRY_INIT};

mod l2_book;
mod liquidations;
mod market;
mod orders;
mod wallet;
mod wallet_stream;

pub(in crate::tui::runtime) use l2_book::*;
pub(in crate::tui::runtime) use liquidations::*;
pub(in crate::tui::runtime) use market::*;
pub(in crate::tui::runtime) use orders::*;
pub(in crate::tui::runtime) use wallet::*;
pub(in crate::tui::runtime) use wallet_stream::*;
