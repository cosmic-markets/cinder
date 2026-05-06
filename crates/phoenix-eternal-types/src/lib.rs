//! Read-only account types for Phoenix Eternal perpetuals.
//!
//! This crate provides minimal, read-only types for deserializing Phoenix Eternal
//! on-chain accounts without exposing mutation methods or full implementation details.
//!
//! # Example
//!
//! ```ignore
//! use cosmic_phoenix_eternal_types::program_ids;
//! use cosmic_phoenix_eternal_types::{GlobalConfiguration, discriminant};
//!
//! // Derive the global configuration address
//! let (global_config_key, _) = program_ids::get_global_config_address_default();
//!
//! // Fetch account data from RPC and deserialize
//! let data: &[u8] = /* fetch from RPC */;
//! let global_config: &GlobalConfiguration = bytemuck::from_bytes(data);
//!
//! // Verify discriminant
//! assert_eq!(global_config.discriminant(), *discriminant::GLOBAL_CONFIGURATION);
//! ```

pub mod accounts;
pub mod discriminant;
pub mod events;
pub mod helpers;
pub mod instructions;
pub mod orderbook;
pub mod program_ids;
pub mod quantities;
pub mod sokoban;

// Re-export commonly used types
pub use accounts::{
    ActiveTraderBuffer, ActiveTraderBufferArenaHeader, ActiveTraderBufferHeader,
    ActiveTraderBufferTree, DynamicTraderHeader, GlobalConfiguration, GlobalTraderIndex,
    GlobalTraderIndexArenaHeader, GlobalTraderIndexHeader, GlobalTraderIndexRef,
    GlobalTraderIndexTree, Orderbook, OrderbookHeader, PerpAssetEntry, PerpAssetMapHeader,
    PerpAssetMapRef, PerpAssetMetadata, Spline, SplineCollectionHeader, SplineCollectionRef,
    TickRegion, TraderPosition, TraderPositionState, TraderState, GTC_LIFESPAN,
    MAX_NUMBER_OF_PERP_ASSETS, SPLINE_REGION_CAPACITY,
};
pub use events::{
    parse_events_from_inner_instructions, parse_events_from_inner_instructions_strict,
    parse_events_from_inner_instructions_with_context,
    parse_events_from_inner_instructions_with_context_strict, EventParseError,
    InnerInstructionContext, LiquidationEvent, LogHeader, MarketEvent, MarketEventHeader,
    MarketStatusChangedEvent, MarketSummaryEvent, OffChainMarketEventLengths, OrderFilledEvent,
    OrderModificationReason, OrderModifiedEvent, OrderPacket, OrderPacketEvent, OrderPlacedEvent,
    OrderRejectedEvent, PnLEvent, PricesUpdatedEvent, SelfTradeBehavior, SlotContextEvent,
    SplineFilledEvent, TradeSummaryEvent, TraderFundingSettledEvent, TraderFundsDepositedEvent,
    TraderFundsWithdrawnEvent, LOG_DISCRIMINANT, LOG_EVENT_LENGTHS_DISCRIMINANT,
};
pub use instructions::{
    get_log_authority, update_spline_parameters, update_spline_parameters_with_ordering,
    update_spline_position_limits_config, update_spline_price, update_spline_price_with_ordering,
    PositionSizeLimit, PositionSizeLimits, TickRegionParams, UpdateSplineParametersParams,
    UpdateSplineParametersParamsWithOrdering, UpdateSplinePositionLimitsConfigParams,
    UpdateSplinePriceParams, UpdateSplinePriceParamsWithOrdering,
};
pub use orderbook::{
    FIFOOrderId, FIFORestingOrder, NodePointer, OrderFlags, Side, TraderPositionId,
};
pub use program_ids::{
    get_active_trader_buffer_address, get_active_trader_buffer_address_default,
    get_global_config_address, get_global_config_address_default, get_global_trader_index_address,
    get_global_trader_index_address_default, get_spline_collection_address,
    get_spline_collection_address_default, get_trader_address, get_trader_address_default,
    PHOENIX_ETERNAL_BETA_PROGRAM_ID, PHOENIX_ETERNAL_PROGRAM_ID,
};
pub use quantities::{
    AssetIndex, AssetIndexU64, BaseLots, BaseLotsPerTick, BaseLotsPerTickU32, BaseLotsU32,
    BasisPointsU32, CapabilityAccess, ExchangeSequenceNumber, FundingRateUnitInSeconds,
    OptionalNonZeroU64, QuoteLots, QuoteLotsPerBaseLot, QuoteLotsPerBaseLotPerTick, SequenceNumber,
    SignedBaseLots, SignedFeeRateMicro, SignedQuoteLots, SignedQuoteLotsPerBaseLot,
    SignedQuoteLotsPerBaseLotUpcasted, Slot, Symbol, Ticks, TraderCapabilities,
    TraderCapabilityFlags, TraderCapabilityFlagsError, TraderCapabilityKind,
    ALL_TRADER_CAPABILITY_KINDS,
};
