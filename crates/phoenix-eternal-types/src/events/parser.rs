//! Event parser for Phoenix Eternal inner instructions.

use std::collections::HashMap;
use std::fmt;

use borsh::BorshDeserialize;
use solana_pubkey::Pubkey;
use tracing::debug;

use super::market_event::{LogHeader, MarketEvent, OffChainMarketEventLengths};

const DISCRIMINANT_SIZE: usize = 8;
const HEADER_SIZE: usize = std::mem::size_of::<LogHeader>();

/// Discriminant for the `Log` instruction: SHA256("global:log")[..8].
pub const LOG_DISCRIMINANT: &[u8; 8] = &[141, 230, 214, 242, 9, 209, 207, 170];

/// Discriminant for the `LogEventLengths` instruction:
/// SHA256("global:log_event_lengths")[..8].
pub const LOG_EVENT_LENGTHS_DISCRIMINANT: &[u8; 8] = &[247, 7, 134, 203, 181, 71, 153, 71];

/// Context key for matching `LogEventLengths` and `Log` instructions.
///
/// Values come from Solana RPC `UiInnerInstructions`:
/// - `instruction_index`: top-level transaction instruction index
/// - `stack_height`: invocation stack height for this inner instruction
pub type InnerInstructionContext = (u8, Option<u32>);

/// Error returned by strict event parsing when decoding fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventParseError {
    /// Top-level instruction index and optional stack height of the inner instruction.
    pub context: InnerInstructionContext,
    /// Log batch index when known.
    pub batch_index: Option<u32>,
    /// Event index inside the batch when known.
    pub event_index: Option<usize>,
    /// First event byte (borsh enum variant discriminant) when known.
    pub discriminator: Option<u8>,
    /// Event chunk length in bytes when known.
    pub event_len: Option<usize>,
    /// Human-readable error message.
    pub message: String,
}

impl fmt::Display for EventParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (instruction_index={}, stack_height={:?}, batch_index={:?}, event_index={:?}, discriminator={:?}, event_len={:?})",
            self.message,
            self.context.0,
            self.context.1,
            self.batch_index,
            self.event_index,
            self.discriminator,
            self.event_len
        )
    }
}

impl std::error::Error for EventParseError {}

/// Parse market events from transaction inner instructions.
///
/// This parser is intentionally resilient:
/// - malformed or unknown events are skipped
/// - malformed length/header entries are skipped
/// - failures are logged with `tracing::debug!`
pub fn parse_events_from_inner_instructions(
    program_id: &Pubkey,
    inner_ixs: &[(Pubkey, Vec<u8>)],
) -> Vec<MarketEvent> {
    let contextual_inner_ixs: Vec<(InnerInstructionContext, Pubkey, Vec<u8>)> = inner_ixs
        .iter()
        .map(|(ix_program_id, data)| ((0, None), *ix_program_id, data.clone()))
        .collect();

    parse_events_from_inner_instructions_with_context(program_id, &contextual_inner_ixs)
}

/// Parse market events from transaction inner instructions keyed by context.
///
/// Use this when decoding Solana RPC `inner_instructions`, where multiple
/// contexts may reuse the same `batch_index`.
pub fn parse_events_from_inner_instructions_with_context(
    program_id: &Pubkey,
    inner_ixs: &[(InnerInstructionContext, Pubkey, Vec<u8>)],
) -> Vec<MarketEvent> {
    match parse_events_from_inner_instructions_with_context_impl(program_id, inner_ixs, false) {
        Ok(events) => events,
        Err(err) => {
            debug!(
                error = %err,
                "lenient event parser encountered an unexpected internal error; returning empty event list"
            );
            Vec::new()
        }
    }
}

/// Parse market events from transaction inner instructions and fail fast on the
/// first decode issue.
pub fn parse_events_from_inner_instructions_strict(
    program_id: &Pubkey,
    inner_ixs: &[(Pubkey, Vec<u8>)],
) -> Result<Vec<MarketEvent>, EventParseError> {
    let contextual_inner_ixs: Vec<(InnerInstructionContext, Pubkey, Vec<u8>)> = inner_ixs
        .iter()
        .map(|(ix_program_id, data)| ((0, None), *ix_program_id, data.clone()))
        .collect();

    parse_events_from_inner_instructions_with_context_impl(program_id, &contextual_inner_ixs, true)
}

/// Parse market events from contextual inner instructions and fail fast on the
/// first decode issue.
pub fn parse_events_from_inner_instructions_with_context_strict(
    program_id: &Pubkey,
    inner_ixs: &[(InnerInstructionContext, Pubkey, Vec<u8>)],
) -> Result<Vec<MarketEvent>, EventParseError> {
    parse_events_from_inner_instructions_with_context_impl(program_id, inner_ixs, true)
}

fn parse_events_from_inner_instructions_with_context_impl(
    program_id: &Pubkey,
    inner_ixs: &[(InnerInstructionContext, Pubkey, Vec<u8>)],
    strict: bool,
) -> Result<Vec<MarketEvent>, EventParseError> {
    let mut lengths_by_batch: HashMap<(InnerInstructionContext, u32), Vec<u16>> = HashMap::new();
    let mut log_entries: Vec<((InnerInstructionContext, u32), Vec<u8>)> = Vec::new();

    for (context, ix_program_id, data) in inner_ixs {
        if ix_program_id != program_id {
            continue;
        }

        if data.len() < DISCRIMINANT_SIZE {
            let parse_error = EventParseError {
                context: *context,
                batch_index: None,
                event_index: None,
                discriminator: None,
                event_len: None,
                message: "inner instruction too short for discriminant".to_string(),
            };
            if strict {
                return Err(parse_error);
            }
            debug!(
                instruction_index = context.0,
                stack_height = ?context.1,
                data_len = data.len(),
                "inner instruction too short for discriminant; skipping"
            );
            continue;
        }

        let disc = &data[..DISCRIMINANT_SIZE];
        let payload = &data[DISCRIMINANT_SIZE..];

        if disc == LOG_EVENT_LENGTHS_DISCRIMINANT {
            match OffChainMarketEventLengths::try_from_slice(payload) {
                Ok(parsed) => {
                    lengths_by_batch.insert((*context, parsed.batch_index), parsed.lengths);
                }
                Err(err) => {
                    let parse_error = EventParseError {
                        context: *context,
                        batch_index: None,
                        event_index: None,
                        discriminator: None,
                        event_len: None,
                        message: format!("failed to deserialize LogEventLengths payload: {}", err),
                    };
                    if strict {
                        return Err(parse_error);
                    }
                    debug!(
                        instruction_index = context.0,
                        stack_height = ?context.1,
                        data_len = data.len(),
                        error = %err,
                        "failed to deserialize LogEventLengths payload; skipping"
                    );
                }
            }
        } else if disc == LOG_DISCRIMINANT {
            if payload.len() < HEADER_SIZE {
                let parse_error = EventParseError {
                    context: *context,
                    batch_index: None,
                    event_index: None,
                    discriminator: None,
                    event_len: None,
                    message: format!(
                        "log payload too short for LogHeader (got {}, need {})",
                        payload.len(),
                        HEADER_SIZE
                    ),
                };
                if strict {
                    return Err(parse_error);
                }
                debug!(
                    instruction_index = context.0,
                    stack_height = ?context.1,
                    payload_len = payload.len(),
                    expected_header_size = HEADER_SIZE,
                    "log payload too short for LogHeader; skipping"
                );
                continue;
            }

            match LogHeader::try_from_slice(&payload[..HEADER_SIZE]) {
                Ok(header) => {
                    let event_data = payload[HEADER_SIZE..].to_vec();
                    log_entries.push(((*context, header.log_batch_index), event_data));
                }
                Err(err) => {
                    let parse_error = EventParseError {
                        context: *context,
                        batch_index: None,
                        event_index: None,
                        discriminator: None,
                        event_len: None,
                        message: format!("failed to deserialize LogHeader: {}", err),
                    };
                    if strict {
                        return Err(parse_error);
                    }
                    debug!(
                        instruction_index = context.0,
                        stack_height = ?context.1,
                        payload_len = payload.len(),
                        error = %err,
                        "failed to deserialize LogHeader; skipping"
                    );
                }
            }
        }
    }

    let mut events = Vec::new();

    for ((context, batch_index), event_data) in log_entries {
        let Some(lengths) = lengths_by_batch.get(&(context, batch_index)) else {
            let parse_error = EventParseError {
                context,
                batch_index: Some(batch_index),
                event_index: None,
                discriminator: None,
                event_len: None,
                message: "missing LogEventLengths for log batch".to_string(),
            };
            if strict {
                return Err(parse_error);
            }
            debug!(
                instruction_index = context.0,
                stack_height = ?context.1,
                batch_index,
                event_data_len = event_data.len(),
                "missing LogEventLengths for log batch; skipping"
            );
            continue;
        };

        let mut offset = 0usize;
        for (event_index, &event_len_u16) in lengths.iter().enumerate() {
            let event_len = event_len_u16 as usize;

            let Some(next_offset) = offset.checked_add(event_len) else {
                let parse_error = EventParseError {
                    context,
                    batch_index: Some(batch_index),
                    event_index: Some(event_index),
                    discriminator: None,
                    event_len: Some(event_len),
                    message: "event length overflow while walking batch".to_string(),
                };
                if strict {
                    return Err(parse_error);
                }
                debug!(
                    instruction_index = context.0,
                    stack_height = ?context.1,
                    batch_index,
                    event_index,
                    event_len,
                    offset,
                    event_data_len = event_data.len(),
                    "event length overflow while walking batch; stopping batch parse"
                );
                break;
            };

            if next_offset > event_data.len() {
                let parse_error = EventParseError {
                    context,
                    batch_index: Some(batch_index),
                    event_index: Some(event_index),
                    discriminator: None,
                    event_len: Some(event_len),
                    message: "event length exceeds remaining payload".to_string(),
                };
                if strict {
                    return Err(parse_error);
                }
                debug!(
                    instruction_index = context.0,
                    stack_height = ?context.1,
                    batch_index,
                    event_index,
                    event_len,
                    offset,
                    event_data_len = event_data.len(),
                    "event length exceeds remaining payload; stopping batch parse"
                );
                break;
            }

            let chunk = &event_data[offset..next_offset];
            let discriminator = chunk.first().copied();

            if let Some(discriminator) = discriminator {
                match MarketEvent::try_from_slice(chunk) {
                    Ok(event) => events.push(event),
                    Err(err) => {
                        let parse_error = EventParseError {
                            context,
                            batch_index: Some(batch_index),
                            event_index: Some(event_index),
                            discriminator: Some(discriminator),
                            event_len: Some(event_len),
                            message: format!("failed to deserialize market event: {}", err),
                        };
                        if strict {
                            return Err(parse_error);
                        }
                        debug!(
                            instruction_index = context.0,
                            stack_height = ?context.1,
                            batch_index,
                            event_index,
                            discriminator,
                            event_len,
                            error = %err,
                            "failed to deserialize market event; skipping"
                        );
                    }
                }
            } else {
                let parse_error = EventParseError {
                    context,
                    batch_index: Some(batch_index),
                    event_index: Some(event_index),
                    discriminator: None,
                    event_len: Some(event_len),
                    message: "empty event chunk".to_string(),
                };
                if strict {
                    return Err(parse_error);
                }
                debug!(
                    instruction_index = context.0,
                    stack_height = ?context.1,
                    batch_index,
                    event_index,
                    event_len,
                    "empty event chunk; skipping"
                );
            }

            offset = next_offset;
        }

        if offset < event_data.len() {
            let parse_error = EventParseError {
                context,
                batch_index: Some(batch_index),
                event_index: None,
                discriminator: None,
                event_len: Some(event_data.len() - offset),
                message: "trailing bytes remain after consuming declared event lengths".to_string(),
            };
            if strict {
                return Err(parse_error);
            }
            debug!(
                instruction_index = context.0,
                stack_height = ?context.1,
                batch_index,
                parsed_bytes = offset,
                event_data_len = event_data.len(),
                "trailing bytes remain after consuming declared event lengths"
            );
        }
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    use super::super::market_event::{
        OrderModificationReason, OrderModifiedEvent, SlotContextEvent,
    };

    fn build_log_event_lengths_data(batch_index: u32, lengths: &[u16]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(LOG_EVENT_LENGTHS_DISCRIMINANT);
        let payload = OffChainMarketEventLengths {
            batch_index,
            lengths: lengths.to_vec(),
        };
        payload.serialize(&mut data).unwrap();
        data
    }

    fn build_log_data(batch_index: u32, total_events: u32, events_payload: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(LOG_DISCRIMINANT);
        let header = LogHeader {
            log_batch_index: batch_index,
            total_events,
        };
        header.serialize(&mut data).unwrap();
        data.extend_from_slice(events_payload);
        data
    }

    fn serialize_event(event: &MarketEvent) -> Vec<u8> {
        borsh::to_vec(event).unwrap()
    }

    #[test]
    fn test_discriminant_constants() {
        assert_eq!(*LOG_DISCRIMINANT, [141, 230, 214, 242, 9, 209, 207, 170]);
        assert_eq!(
            *LOG_EVENT_LENGTHS_DISCRIMINANT,
            [247, 7, 134, 203, 181, 71, 153, 71]
        );
    }

    #[test]
    fn test_parse_single_batch() {
        let program_id = Pubkey::new_unique();

        let event1 = MarketEvent::SlotContext(SlotContextEvent {
            timestamp: 100,
            slot: 1,
        });
        let event2 = MarketEvent::SlotContext(SlotContextEvent {
            timestamp: 200,
            slot: 2,
        });

        let serialized1 = serialize_event(&event1);
        let serialized2 = serialize_event(&event2);

        let lengths_data =
            build_log_event_lengths_data(0, &[serialized1.len() as u16, serialized2.len() as u16]);

        let mut events_payload = Vec::new();
        events_payload.extend_from_slice(&serialized1);
        events_payload.extend_from_slice(&serialized2);
        let log_data = build_log_data(0, 2, &events_payload);

        let inner_ixs = vec![(program_id, lengths_data), (program_id, log_data)];

        let events = parse_events_from_inner_instructions(&program_id, &inner_ixs);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], MarketEvent::SlotContext(_)));
        assert!(matches!(events[1], MarketEvent::SlotContext(_)));
    }

    #[test]
    fn test_parse_unknown_events_are_skipped() {
        let program_id = Pubkey::new_unique();

        let unknown_bytes = vec![250, 1, 2, 3, 4];
        let known_event = MarketEvent::SlotContext(SlotContextEvent {
            timestamp: 123,
            slot: 456,
        });
        let known_bytes = serialize_event(&known_event);

        let lengths_data = build_log_event_lengths_data(
            0,
            &[unknown_bytes.len() as u16, known_bytes.len() as u16],
        );

        let mut payload = Vec::new();
        payload.extend_from_slice(&unknown_bytes);
        payload.extend_from_slice(&known_bytes);
        let log_data = build_log_data(0, 2, &payload);

        let inner_ixs = vec![(program_id, lengths_data), (program_id, log_data)];

        let events = parse_events_from_inner_instructions(&program_id, &inner_ixs);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], MarketEvent::SlotContext(_)));
    }

    #[test]
    fn test_parse_unknown_events_strict_returns_error() {
        let program_id = Pubkey::new_unique();

        let unknown_bytes = vec![250, 1, 2, 3, 4];
        let known_event = MarketEvent::SlotContext(SlotContextEvent {
            timestamp: 123,
            slot: 456,
        });
        let known_bytes = serialize_event(&known_event);

        let lengths_data = build_log_event_lengths_data(
            0,
            &[unknown_bytes.len() as u16, known_bytes.len() as u16],
        );

        let mut payload = Vec::new();
        payload.extend_from_slice(&unknown_bytes);
        payload.extend_from_slice(&known_bytes);
        let log_data = build_log_data(0, 2, &payload);

        let inner_ixs = vec![(program_id, lengths_data), (program_id, log_data)];

        let err = parse_events_from_inner_instructions_strict(&program_id, &inner_ixs)
            .expect_err("strict parser should fail on unknown discriminator");
        assert_eq!(err.batch_index, Some(0));
        assert_eq!(err.event_index, Some(0));
        assert_eq!(err.discriminator, Some(250));
        assert_eq!(err.event_len, Some(5));
    }

    #[test]
    fn test_missing_lengths_is_non_fatal() {
        let program_id = Pubkey::new_unique();

        let known_event = MarketEvent::SlotContext(SlotContextEvent {
            timestamp: 123,
            slot: 456,
        });
        let known_bytes = serialize_event(&known_event);
        let log_data = build_log_data(42, 1, &known_bytes);

        let inner_ixs = vec![(program_id, log_data)];

        let events = parse_events_from_inner_instructions(&program_id, &inner_ixs);
        assert!(events.is_empty());
    }

    #[test]
    fn test_missing_lengths_is_fatal_in_strict_mode() {
        let program_id = Pubkey::new_unique();

        let known_event = MarketEvent::SlotContext(SlotContextEvent {
            timestamp: 123,
            slot: 456,
        });
        let known_bytes = serialize_event(&known_event);
        let log_data = build_log_data(42, 1, &known_bytes);

        let inner_ixs = vec![(program_id, log_data)];

        let err = parse_events_from_inner_instructions_strict(&program_id, &inner_ixs)
            .expect_err("strict parser should fail when lengths are missing");
        assert_eq!(err.batch_index, Some(42));
        assert!(err.message.contains("missing LogEventLengths"));
    }

    #[test]
    fn test_filters_by_program_id() {
        let program_id = Pubkey::new_unique();
        let other_program = Pubkey::new_unique();

        let event = MarketEvent::SlotContext(SlotContextEvent {
            timestamp: 100,
            slot: 1,
        });
        let serialized = serialize_event(&event);

        let lengths_data = build_log_event_lengths_data(0, &[serialized.len() as u16]);
        let log_data = build_log_data(0, 1, &serialized);

        let inner_ixs = vec![(other_program, lengths_data), (other_program, log_data)];

        let events = parse_events_from_inner_instructions(&program_id, &inner_ixs);
        assert!(events.is_empty());
    }

    #[test]
    fn test_context_prevents_batch_index_collisions() {
        let program_id = Pubkey::new_unique();

        let event1 = MarketEvent::SlotContext(SlotContextEvent {
            timestamp: 100,
            slot: 1,
        });
        let event2 = MarketEvent::OrderModified(OrderModifiedEvent {
            order_sequence_number: 7,
            price: 100u64.into(),
            base_lots_released: 1i64.into(),
            quote_lots_released: (-2i64).into(),
            base_lots_remaining: 3u64.into(),
            reason: OrderModificationReason::Expired,
        });

        let ser1 = serialize_event(&event1);
        let ser2 = serialize_event(&event2);

        let lengths_a = build_log_event_lengths_data(0, &[ser1.len() as u16]);
        let log_a = build_log_data(0, 1, &ser1);

        let lengths_b = build_log_event_lengths_data(0, &[ser2.len() as u16]);
        let log_b = build_log_data(0, 1, &ser2);

        let contextual_inner_ixs = vec![
            ((0, Some(2)), program_id, lengths_a),
            ((0, Some(2)), program_id, log_a),
            ((1, Some(2)), program_id, lengths_b),
            ((1, Some(2)), program_id, log_b),
        ];

        let events =
            parse_events_from_inner_instructions_with_context(&program_id, &contextual_inner_ixs);

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], MarketEvent::SlotContext(_)));
        assert!(matches!(events[1], MarketEvent::OrderModified(_)));
    }
}
