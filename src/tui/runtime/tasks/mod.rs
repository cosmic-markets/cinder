//! Background task spawners grouped by stream or workload.

//! Background task spawners: blockhash refresh, wallet WSS, balance fetch,
//! trader orders WS, and the Phoenix L2 book RPC subscription.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use phoenix_rise::accounts::{
    ConditionalOrderCollection, ConditionalOrderTrigger, StopLossDirection, StopLossOrderKind,
    StopLossTradeSide,
};
use phoenix_rise::types::{
    TraderStatePayload, TraderStateRowChangeKind, TraderStateStopLossTrigger,
};
use phoenix_rise::{
    get_conditional_orders_address, Direction, PhoenixHttpClient, PhoenixWSClient, Trader,
    TraderKey,
};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_types::config::RpcAccountInfoConfig;
use solana_signer::Signer;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tracing::warn;

use super::super::config::{rpc_http_url_from_env, ws_url_from_env, SplineConfig};
use super::super::data::fetch_top_positions;
use super::super::data::GtiHandle;
use super::super::data::{parse_l2_book_from_market_account, L2Level};
use super::super::format::pubkey_trader_prefix;
use super::super::state::{BalanceUpdate, ClobLevel, L2BookStreamMsg, SplineBootstrapMsg, TxStatusMsg};
use super::super::trading::{
    fetch_phoenix_balance_and_position, OrderInfo, TopPositionEntry, TradingSide,
};
use super::super::tx::TxContext;
use super::{TxCtxMsg, L2_EMIT_MIN_INTERVAL, L2_SNAPSHOT_DEPTH, WSS_RETRY_CAP, WSS_RETRY_INIT};

mod balances;
mod connect_flow;
mod l2_book;
mod liquidations;
mod orders;
mod position_leaderboard;
mod spline_bootstrap;
mod tx_context;
mod wallet_stream;

pub(in crate::tui::runtime) use balances::*;
pub(in crate::tui::runtime) use connect_flow::*;
pub(in crate::tui::runtime) use l2_book::*;
pub(in crate::tui::runtime) use liquidations::*;
pub(in crate::tui::runtime) use orders::*;
pub(in crate::tui::runtime) use position_leaderboard::*;
pub(in crate::tui::runtime) use spline_bootstrap::*;
pub(in crate::tui::runtime) use tx_context::*;
pub(in crate::tui::runtime) use wallet_stream::*;
