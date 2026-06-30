#![allow(deprecated)]

//! Event deserialization for Phoenix Eternal.
//!
//! Phoenix Eternal emits on-chain events as self-CPIs. Each event batch produces
//! two inner instructions:
//! 1. **LogEventLengths** - contains a list of u16 byte sizes for upcoming events
//! 2. **Log** - contains the actual concatenated borsh-serialized event data
//!
//! This module provides types and a parser to deserialize these events from
//! transaction inner instructions.

mod market_event;
mod parser;

pub use market_event::*;
pub use parser::*;
