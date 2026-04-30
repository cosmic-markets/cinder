//! Concrete quantity type definitions.

use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use std::fmt::{Debug, Display};
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// Slot number (u64 wrapper).
pub type Slot = u64;

// ============================================================================
// Macro to define simple u64 wrapper types
// ============================================================================

macro_rules! define_u64_type {
    ($name:ident) => {
        #[repr(C)]
        #[derive(
            Copy,
            Clone,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Pod,
            Zeroable,
            BorshDeserialize,
            BorshSerialize,
        )]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name {
            inner: u64,
        }

        impl $name {
            pub const ZERO: Self = Self { inner: 0 };

            #[inline(always)]
            pub const fn new(value: u64) -> Self {
                Self { inner: value }
            }

            #[inline(always)]
            pub const fn as_inner(&self) -> u64 {
                self.inner
            }

            #[inline(always)]
            pub fn as_signed(&self) -> i64 {
                self.inner as i64
            }

            pub fn checked_add(self, rhs: Self) -> Option<Self> {
                self.inner.checked_add(rhs.inner).map(Self::new)
            }

            pub fn checked_sub(self, rhs: Self) -> Option<Self> {
                self.inner.checked_sub(rhs.inner).map(Self::new)
            }

            pub fn saturating_add(self, rhs: Self) -> Self {
                Self::new(self.inner.saturating_add(rhs.inner))
            }

            pub fn saturating_sub(self, rhs: Self) -> Self {
                Self::new(self.inner.saturating_sub(rhs.inner))
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.inner)
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self::new(value)
            }
        }

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.inner
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self::Output {
                Self::new(self.inner + rhs.inner)
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) {
                self.inner += rhs.inner;
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self::Output {
                Self::new(self.inner - rhs.inner)
            }
        }

        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: Self) {
                self.inner -= rhs.inner;
            }
        }
    };
}

macro_rules! define_u32_type {
    ($name:ident) => {
        #[repr(C)]
        #[derive(
            Copy,
            Clone,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Pod,
            Zeroable,
            BorshDeserialize,
            BorshSerialize,
        )]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name {
            inner: u32,
        }

        impl $name {
            pub const ZERO: Self = Self { inner: 0 };

            #[inline(always)]
            pub const fn new(value: u32) -> Self {
                Self { inner: value }
            }

            #[inline(always)]
            pub const fn as_inner(&self) -> u32 {
                self.inner
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.inner)
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }

        impl From<u32> for $name {
            fn from(value: u32) -> Self {
                Self::new(value)
            }
        }

        impl From<$name> for u32 {
            fn from(value: $name) -> Self {
                value.inner
            }
        }
    };
}

macro_rules! define_i64_type {
    ($name:ident) => {
        #[repr(C)]
        #[derive(
            Copy,
            Clone,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Pod,
            Zeroable,
            BorshDeserialize,
            BorshSerialize,
        )]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name {
            inner: i64,
        }

        impl $name {
            pub const ZERO: Self = Self { inner: 0 };

            #[inline(always)]
            pub const fn new(value: i64) -> Self {
                Self { inner: value }
            }

            #[inline(always)]
            pub const fn as_inner(&self) -> i64 {
                self.inner
            }

            #[inline(always)]
            pub fn abs(&self) -> Self {
                Self::new(self.inner.abs())
            }

            #[inline(always)]
            pub fn abs_as_unsigned(&self) -> u64 {
                self.inner.unsigned_abs()
            }

            pub fn checked_add(self, rhs: Self) -> Option<Self> {
                self.inner.checked_add(rhs.inner).map(Self::new)
            }

            pub fn checked_sub(self, rhs: Self) -> Option<Self> {
                self.inner.checked_sub(rhs.inner).map(Self::new)
            }

            pub fn saturating_add(self, rhs: Self) -> Self {
                Self::new(self.inner.saturating_add(rhs.inner))
            }

            pub fn saturating_sub(self, rhs: Self) -> Self {
                Self::new(self.inner.saturating_sub(rhs.inner))
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}({})", stringify!($name), self.inner)
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self::new(value)
            }
        }

        impl From<$name> for i64 {
            fn from(value: $name) -> Self {
                value.inner
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self::Output {
                Self::new(self.inner + rhs.inner)
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) {
                self.inner += rhs.inner;
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self::Output {
                Self::new(self.inner - rhs.inner)
            }
        }

        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: Self) {
                self.inner -= rhs.inner;
            }
        }

        impl std::ops::Neg for $name {
            type Output = Self;
            fn neg(self) -> Self::Output {
                Self::new(-self.inner)
            }
        }
    };
}

// Define the quantity types
define_u64_type!(QuoteLots);
define_u64_type!(BaseLots);
define_u64_type!(Ticks);
define_u64_type!(QuoteLotsPerBaseLot);
define_u64_type!(QuoteLotsPerBaseLotPerTick);
define_u64_type!(BaseLotsPerTick);

define_u32_type!(BaseLotsU32);
define_u32_type!(BaseLotsPerTickU32);
define_u32_type!(BasisPointsU32);

define_i64_type!(SignedBaseLots);
define_i64_type!(SignedQuoteLots);
define_i64_type!(SignedQuoteLotsPerBaseLot);
define_i64_type!(SignedTicks);

// ============================================================================
// SequenceNumber - 16-byte sequence number with slot tracking
// ============================================================================

#[repr(C)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Pod, Zeroable, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SequenceNumber {
    pub sequence_number: u64,
    pub last_update_slot: u64,
}

impl SequenceNumber {
    pub const fn new(sequence_number: u64, last_update_slot: u64) -> Self {
        Self {
            sequence_number,
            last_update_slot,
        }
    }

    pub fn sequence_number(&self) -> u64 {
        self.sequence_number
    }

    pub fn last_update_slot(&self) -> u64 {
        self.last_update_slot
    }
}

impl Debug for SequenceNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SequenceNumber")
            .field("sequence_number", &self.sequence_number)
            .field("last_update_slot", &self.last_update_slot)
            .finish()
    }
}

impl PartialOrd for SequenceNumber {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SequenceNumber {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sequence_number
            .cmp(&other.sequence_number)
            .then_with(|| self.last_update_slot.cmp(&other.last_update_slot))
    }
}

#[cfg(test)]
static_assertions::const_assert_eq!(std::mem::size_of::<SequenceNumber>(), 16);

/// Backwards-compatible alias.
pub type ExchangeSequenceNumber = SequenceNumber;

// ============================================================================
// SequenceNumberU8 - 8-bit sequence number
// ============================================================================

#[repr(C)]
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Pod,
    Zeroable,
    BorshDeserialize,
    BorshSerialize,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SequenceNumberU8 {
    inner: u8,
}

impl SequenceNumberU8 {
    pub const fn new(value: u8) -> Self {
        Self { inner: value }
    }

    pub const fn as_inner(&self) -> u8 {
        self.inner
    }

    pub fn wrapping_add(&self, other: u8) -> Self {
        Self::new(self.inner.wrapping_add(other))
    }
}

impl Debug for SequenceNumberU8 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SequenceNumberU8({})", self.inner)
    }
}

// ============================================================================
// SignedQuoteLotsI56 - 56-bit signed quote lots (7 bytes)
// ============================================================================

#[repr(C)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Pod, Zeroable, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignedQuoteLotsI56 {
    bytes: [u8; 7],
}

impl SignedQuoteLotsI56 {
    pub const ZERO: Self = Self { bytes: [0; 7] };

    pub fn new(value: i64) -> Self {
        let bytes_8 = value.to_le_bytes();
        let mut bytes = [0u8; 7];
        bytes.copy_from_slice(&bytes_8[..7]);
        Self { bytes }
    }

    pub fn as_i64(&self) -> i64 {
        let mut bytes_8 = [0u8; 8];
        bytes_8[..7].copy_from_slice(&self.bytes);
        // Sign extend if negative
        if self.bytes[6] & 0x80 != 0 {
            bytes_8[7] = 0xFF;
        }
        i64::from_le_bytes(bytes_8)
    }
}

impl Debug for SignedQuoteLotsI56 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SignedQuoteLotsI56({})", self.as_i64())
    }
}

// ============================================================================
// AssetIndex - 32-bit asset identifier
// ============================================================================

#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Pod,
    Zeroable,
    BorshDeserialize,
    BorshSerialize,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AssetIndex(u32);

impl AssetIndex {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_inner(&self) -> u32 {
        self.0
    }
}

impl Debug for AssetIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AssetIndex({})", self.0)
    }
}

impl Display for AssetIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for AssetIndex {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<AssetIndex> for u32 {
    fn from(value: AssetIndex) -> Self {
        value.0
    }
}

// ============================================================================
// AssetIndexU64 - 64-bit wrapper for AssetIndex (with padding)
// ============================================================================

#[repr(C)]
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Pod,
    Zeroable,
    BorshDeserialize,
    BorshSerialize,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AssetIndexU64 {
    index: AssetIndex,
    _padding: [u8; 4],
}

static_assertions::const_assert_eq!(std::mem::size_of::<AssetIndexU64>(), 8);

impl AssetIndexU64 {
    pub const fn new(index: AssetIndex) -> Self {
        Self {
            index,
            _padding: [0; 4],
        }
    }

    pub const fn as_asset_index(&self) -> AssetIndex {
        self.index
    }
}

impl Debug for AssetIndexU64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AssetIndexU64({})", self.index.0)
    }
}

impl From<AssetIndex> for AssetIndexU64 {
    fn from(value: AssetIndex) -> Self {
        Self::new(value)
    }
}

impl From<u32> for AssetIndexU64 {
    fn from(value: u32) -> Self {
        Self::new(AssetIndex::new(value))
    }
}

impl From<AssetIndexU64> for AssetIndex {
    fn from(value: AssetIndexU64) -> Self {
        value.index
    }
}

impl From<AssetIndexU64> for u32 {
    fn from(value: AssetIndexU64) -> Self {
        value.index.0
    }
}

// ============================================================================
// Symbol - 16-byte asset symbol
// ============================================================================

#[repr(C)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Pod, Zeroable, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Symbol {
    bytes: [u8; 16],
}

impl Symbol {
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    pub fn as_str(&self) -> &str {
        let len = self.bytes.iter().position(|&b| b == 0).unwrap_or(16);
        std::str::from_utf8(&self.bytes[..len]).unwrap_or("")
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
}

impl Debug for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Symbol(\"{}\")", self.as_str())
    }
}

impl Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// SignedFeeRateMicro - i32 newtype for maker fee rates in events
// ============================================================================

#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Pod,
    Zeroable,
    BorshDeserialize,
    BorshSerialize,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignedFeeRateMicro {
    inner: i32,
}

impl SignedFeeRateMicro {
    pub const fn new(value: i32) -> Self {
        Self { inner: value }
    }

    pub const fn as_inner(&self) -> i32 {
        self.inner
    }
}

impl Debug for SignedFeeRateMicro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SignedFeeRateMicro({})", self.inner)
    }
}

impl Display for SignedFeeRateMicro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

// ============================================================================
// OptionalNonZeroU64 - u64 newtype for optional slot values (0 = None)
// ============================================================================

#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Pod,
    Zeroable,
    BorshDeserialize,
    BorshSerialize,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OptionalNonZeroU64 {
    inner: u64,
}

impl OptionalNonZeroU64 {
    pub const fn new(value: u64) -> Self {
        Self { inner: value }
    }

    pub const fn as_inner(&self) -> u64 {
        self.inner
    }

    pub fn is_none(&self) -> bool {
        self.inner == 0
    }

    pub fn is_some(&self) -> bool {
        self.inner != 0
    }

    pub fn get(&self) -> Option<u64> {
        if self.inner == 0 {
            None
        } else {
            Some(self.inner)
        }
    }
}

impl Debug for OptionalNonZeroU64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.get() {
            Some(v) => write!(f, "Some({})", v),
            None => write!(f, "None"),
        }
    }
}

impl Display for OptionalNonZeroU64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

// ============================================================================
// SignedQuoteLotsPerBaseLotUpcasted - i128 high-precision funding accumulator
// ============================================================================

#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Pod,
    Zeroable,
    BorshDeserialize,
    BorshSerialize,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignedQuoteLotsPerBaseLotUpcasted {
    inner: i128,
}

impl SignedQuoteLotsPerBaseLotUpcasted {
    pub const fn new(value: i128) -> Self {
        Self { inner: value }
    }

    pub const fn as_inner(&self) -> i128 {
        self.inner
    }
}

impl Debug for SignedQuoteLotsPerBaseLotUpcasted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SignedQuoteLotsPerBaseLotUpcasted({})", self.inner)
    }
}

impl Display for SignedQuoteLotsPerBaseLotUpcasted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

// ============================================================================
// FundingRateUnitInSeconds - u64 funding interval parameter
// ============================================================================

define_u64_type!(FundingRateUnitInSeconds);

// ============================================================================
// TraderCapabilityFlags - u32 bitfield for trader capabilities
// ============================================================================

/// Enumerates the feature switches that gate trader behavior.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TraderCapabilityKind {
    PlaceLimitOrder,
    PlaceMarketOrder,
    RiskIncreasingTrade,
    RiskReducingTrade,
    DepositCollateral,
    WithdrawCollateral,
}

impl TraderCapabilityKind {
    /// Stable machine-friendly capability key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::PlaceLimitOrder => "place_limit_order",
            Self::PlaceMarketOrder => "place_market_order",
            Self::RiskIncreasingTrade => "risk_increasing_trade",
            Self::RiskReducingTrade => "risk_reducing_trade",
            Self::DepositCollateral => "deposit_collateral",
            Self::WithdrawCollateral => "withdraw_collateral",
        }
    }

    /// Human-readable capability description.
    pub const fn description(self) -> &'static str {
        match self {
            Self::PlaceLimitOrder => "Place resting limit orders on the book.",
            Self::PlaceMarketOrder => "Submit market/IOC style order flow.",
            Self::RiskIncreasingTrade => "Increase position risk via trade execution.",
            Self::RiskReducingTrade => "Reduce position risk via trade execution/cancels.",
            Self::DepositCollateral => "Deposit collateral into trader account.",
            Self::WithdrawCollateral => "Withdraw collateral from trader account.",
        }
    }
}

impl Display for TraderCapabilityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.key())
    }
}

/// Ordered list of supported capability kinds.
pub const ALL_TRADER_CAPABILITY_KINDS: [TraderCapabilityKind; 6] = [
    TraderCapabilityKind::PlaceLimitOrder,
    TraderCapabilityKind::PlaceMarketOrder,
    TraderCapabilityKind::RiskIncreasingTrade,
    TraderCapabilityKind::RiskReducingTrade,
    TraderCapabilityKind::DepositCollateral,
    TraderCapabilityKind::WithdrawCollateral,
];

/// Describes how a capability may be exercised for the current trader state.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilityAccess {
    immediate: bool,
    via_cold_activation: bool,
}

impl CapabilityAccess {
    pub const fn new(immediate: bool, via_cold_activation: bool) -> Self {
        Self {
            immediate,
            via_cold_activation,
        }
    }

    pub const fn denied() -> Self {
        Self::new(false, false)
    }

    pub const fn immediate() -> Self {
        Self::new(true, false)
    }

    pub const fn via_cold_activation() -> Self {
        Self::new(false, true)
    }

    #[inline(always)]
    pub const fn allows_immediate(self) -> bool {
        self.immediate
    }

    #[inline(always)]
    pub const fn allows_via_cold_activation(self) -> bool {
        self.immediate || self.via_cold_activation
    }

    #[inline(always)]
    pub const fn requires_cold_activation(self) -> bool {
        self.via_cold_activation && !self.immediate
    }
}

/// Capability matrix derived from [`TraderCapabilityFlags`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderCapabilities {
    place_limit_order: CapabilityAccess,
    place_market_order: CapabilityAccess,
    risk_increasing_trade: CapabilityAccess,
    risk_reducing_trade: CapabilityAccess,
    deposit_collateral: CapabilityAccess,
    withdraw_collateral: CapabilityAccess,
}

impl TraderCapabilities {
    pub const fn new(
        place_limit_order: CapabilityAccess,
        place_market_order: CapabilityAccess,
        risk_increasing_trade: CapabilityAccess,
        risk_reducing_trade: CapabilityAccess,
        deposit_collateral: CapabilityAccess,
        withdraw_collateral: CapabilityAccess,
    ) -> Self {
        Self {
            place_limit_order,
            place_market_order,
            risk_increasing_trade,
            risk_reducing_trade,
            deposit_collateral,
            withdraw_collateral,
        }
    }

    #[inline(always)]
    pub const fn access(&self, capability: TraderCapabilityKind) -> CapabilityAccess {
        match capability {
            TraderCapabilityKind::PlaceLimitOrder => self.place_limit_order,
            TraderCapabilityKind::PlaceMarketOrder => self.place_market_order,
            TraderCapabilityKind::RiskIncreasingTrade => self.risk_increasing_trade,
            TraderCapabilityKind::RiskReducingTrade => self.risk_reducing_trade,
            TraderCapabilityKind::DepositCollateral => self.deposit_collateral,
            TraderCapabilityKind::WithdrawCollateral => self.withdraw_collateral,
        }
    }

    #[inline(always)]
    pub const fn allows(&self, capability: TraderCapabilityKind) -> bool {
        self.access(capability).allows_immediate()
    }

    #[inline(always)]
    pub const fn allows_with_cold_activation(&self, capability: TraderCapabilityKind) -> bool {
        self.access(capability).allows_via_cold_activation()
    }
}

/// Errors returned by strict conversion from raw capability bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraderCapabilityFlagsError {
    ReservedBitsSet(u32),
}

impl Display for TraderCapabilityFlagsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReservedBitsSet(bits) => write!(f, "reserved capability bits set: 0x{bits:08X}"),
        }
    }
}

impl std::error::Error for TraderCapabilityFlagsError {}

#[repr(transparent)]
#[derive(Copy, Clone, Default, PartialEq, Eq, Pod, Zeroable, BorshDeserialize, BorshSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraderCapabilityFlags {
    flags: u32,
}

impl TraderCapabilityFlags {
    /// Marks participation in hot trader index / immediate limit-order access.
    pub const HOT: u32 = 1 << 0;
    /// Allows placing resting limit orders.
    pub const CAN_PLACE_LIMIT: u32 = 1 << 1;
    /// Allows crossing-style order flow (market/IOC) and risk-reducing trades.
    pub const CAN_PLACE_MARKET: u32 = 1 << 2;
    /// Allows risk-increasing execution paths.
    pub const CAN_RISK_INCREASE: u32 = 1 << 3;
    /// Allows collateral deposits.
    pub const CAN_DEPOSIT: u32 = 1 << 4;
    /// Allows collateral withdrawals.
    pub const CAN_WITHDRAW: u32 = 1 << 5;

    pub const VALID_MASK: u32 = Self::HOT
        | Self::CAN_PLACE_LIMIT
        | Self::CAN_PLACE_MARKET
        | Self::CAN_RISK_INCREASE
        | Self::CAN_DEPOSIT
        | Self::CAN_WITHDRAW;
    pub const RESERVED_MASK: u32 = !Self::VALID_MASK;

    /// Creates from raw bits without validation.
    ///
    /// This is intentionally permissive to remain forward-compatible when
    /// newer on-chain programs introduce additional bits.
    pub const fn new(flags: u32) -> Self {
        Self { flags }
    }

    /// Strict conversion that rejects unknown/reserved bits.
    pub const fn try_new(flags: u32) -> Result<Self, TraderCapabilityFlagsError> {
        let reserved = flags & Self::RESERVED_MASK;
        if reserved != 0 {
            Err(TraderCapabilityFlagsError::ReservedBitsSet(reserved))
        } else {
            Ok(Self { flags })
        }
    }

    pub const fn as_u32(&self) -> u32 {
        self.flags
    }

    pub const fn bits(self) -> u32 {
        self.flags
    }

    pub const fn reserved_bits(self) -> u32 {
        self.flags & Self::RESERVED_MASK
    }

    pub const fn has_reserved_bits(self) -> bool {
        self.reserved_bits() != 0
    }

    pub const fn contains(self, mask: u32) -> bool {
        (self.flags & mask) == mask
    }

    #[inline(always)]
    pub const fn new_uninitialized() -> Self {
        Self::new(0)
    }

    #[inline(always)]
    pub const fn cold() -> Self {
        Self::new(
            Self::CAN_PLACE_LIMIT
                | Self::CAN_PLACE_MARKET
                | Self::CAN_RISK_INCREASE
                | Self::CAN_DEPOSIT
                | Self::CAN_WITHDRAW,
        )
    }

    #[inline(always)]
    pub const fn hot_active() -> Self {
        Self::new(Self::VALID_MASK)
    }

    #[inline(always)]
    const fn reduce_only_mask() -> u32 {
        Self::CAN_PLACE_LIMIT | Self::CAN_PLACE_MARKET | Self::CAN_DEPOSIT | Self::CAN_WITHDRAW
    }

    #[inline(always)]
    const fn frozen_mask() -> u32 {
        Self::CAN_PLACE_LIMIT | Self::CAN_PLACE_MARKET
    }

    #[inline(always)]
    pub const fn reduce_only() -> Self {
        Self::new(Self::reduce_only_mask())
    }

    #[inline(always)]
    pub const fn reduce_only_hot() -> Self {
        Self::new(Self::reduce_only_mask() | Self::HOT)
    }

    #[inline(always)]
    pub const fn frozen() -> Self {
        Self::new(Self::frozen_mask())
    }

    #[inline(always)]
    pub const fn frozen_hot() -> Self {
        Self::new(Self::frozen_mask() | Self::HOT)
    }

    #[inline(always)]
    pub const fn is_uninitialized(self) -> bool {
        self.flags == 0
    }

    #[inline(always)]
    pub const fn is_hot(self) -> bool {
        self.contains(Self::HOT)
    }

    #[inline(always)]
    pub const fn is_cold(self) -> bool {
        !self.is_uninitialized() && !self.is_hot()
    }

    #[inline(always)]
    pub const fn is_reduce_only(self) -> bool {
        self.contains(Self::CAN_PLACE_MARKET)
            && !self.contains(Self::CAN_RISK_INCREASE)
            && self.contains(Self::CAN_WITHDRAW)
            && self.contains(Self::CAN_DEPOSIT)
    }

    #[inline(always)]
    pub const fn is_frozen(self) -> bool {
        self.contains(Self::CAN_PLACE_MARKET)
            && !self.contains(Self::CAN_RISK_INCREASE)
            && !self.contains(Self::CAN_WITHDRAW)
            && !self.contains(Self::CAN_DEPOSIT)
    }

    #[inline(always)]
    pub const fn allows(self, capability: TraderCapabilityKind) -> bool {
        match capability {
            TraderCapabilityKind::PlaceLimitOrder => {
                self.is_hot() && self.contains(Self::CAN_PLACE_LIMIT)
            }
            TraderCapabilityKind::PlaceMarketOrder => self.contains(Self::CAN_PLACE_MARKET),
            TraderCapabilityKind::RiskIncreasingTrade => {
                self.contains(Self::CAN_PLACE_MARKET) && self.contains(Self::CAN_RISK_INCREASE)
            }
            TraderCapabilityKind::RiskReducingTrade => self.contains(Self::CAN_PLACE_MARKET),
            TraderCapabilityKind::DepositCollateral => self.contains(Self::CAN_DEPOSIT),
            TraderCapabilityKind::WithdrawCollateral => self.contains(Self::CAN_WITHDRAW),
        }
    }

    #[inline(always)]
    pub const fn allows_with_cold_activation(self, capability: TraderCapabilityKind) -> bool {
        match capability {
            TraderCapabilityKind::PlaceLimitOrder => {
                self.allows(capability) || (!self.is_hot() && self.contains(Self::CAN_PLACE_LIMIT))
            }
            _ => self.allows(capability),
        }
    }

    #[inline(always)]
    pub const fn capabilities(self) -> TraderCapabilities {
        let limit_immediate = self.allows(TraderCapabilityKind::PlaceLimitOrder);
        let limit_cold_activation = self
            .allows_with_cold_activation(TraderCapabilityKind::PlaceLimitOrder)
            && !limit_immediate;
        let market_immediate = self.allows(TraderCapabilityKind::PlaceMarketOrder);
        let risk_increase_immediate = self.allows(TraderCapabilityKind::RiskIncreasingTrade);
        let risk_reduce_immediate = self.allows(TraderCapabilityKind::RiskReducingTrade);
        let deposit_immediate = self.allows(TraderCapabilityKind::DepositCollateral);
        let withdraw_immediate = self.allows(TraderCapabilityKind::WithdrawCollateral);

        TraderCapabilities::new(
            CapabilityAccess::new(limit_immediate, limit_cold_activation),
            CapabilityAccess::new(market_immediate, false),
            CapabilityAccess::new(risk_increase_immediate, false),
            CapabilityAccess::new(risk_reduce_immediate, false),
            CapabilityAccess::new(deposit_immediate, false),
            CapabilityAccess::new(withdraw_immediate, false),
        )
    }

    fn set_hot(&mut self) {
        self.flags |= Self::HOT;
    }

    fn unset_hot(&mut self) {
        self.flags &= !Self::HOT;
    }

    fn set_can_place_limit(&mut self) {
        self.flags |= Self::CAN_PLACE_LIMIT;
    }

    fn unset_can_place_limit(&mut self) {
        self.flags &= !Self::CAN_PLACE_LIMIT;
    }

    fn set_can_place_market(&mut self) {
        self.flags |= Self::CAN_PLACE_MARKET;
    }

    fn unset_can_place_market(&mut self) {
        self.flags &= !Self::CAN_PLACE_MARKET;
    }

    fn set_can_risk_increase(&mut self) {
        self.flags |= Self::CAN_RISK_INCREASE;
    }

    fn unset_can_risk_increase(&mut self) {
        self.flags &= !Self::CAN_RISK_INCREASE;
    }

    fn set_can_deposit(&mut self) {
        self.flags |= Self::CAN_DEPOSIT;
    }

    fn unset_can_deposit(&mut self) {
        self.flags &= !Self::CAN_DEPOSIT;
    }

    fn set_can_withdraw(&mut self) {
        self.flags |= Self::CAN_WITHDRAW;
    }

    fn unset_can_withdraw(&mut self) {
        self.flags &= !Self::CAN_WITHDRAW;
    }

    fn ensure_invariants(&mut self) {
        self.flags &= Self::VALID_MASK;
    }

    fn update_hot(&mut self, hot: bool) {
        if hot {
            self.set_hot();
        } else {
            self.unset_hot();
        }
        self.ensure_invariants();
    }

    pub fn mark_hot(&mut self) {
        self.update_hot(true);
    }

    pub fn unmark_hot(&mut self) {
        self.update_hot(false);
    }

    pub fn enable_capability(&mut self, capability: TraderCapabilityKind) {
        match capability {
            TraderCapabilityKind::PlaceLimitOrder => self.set_can_place_limit(),
            TraderCapabilityKind::PlaceMarketOrder | TraderCapabilityKind::RiskReducingTrade => {
                self.set_can_place_market()
            }
            TraderCapabilityKind::RiskIncreasingTrade => self.set_can_risk_increase(),
            TraderCapabilityKind::DepositCollateral => self.set_can_deposit(),
            TraderCapabilityKind::WithdrawCollateral => self.set_can_withdraw(),
        }
        self.ensure_invariants();
    }

    pub fn disable_capability(&mut self, capability: TraderCapabilityKind) {
        match capability {
            TraderCapabilityKind::PlaceLimitOrder => self.unset_can_place_limit(),
            TraderCapabilityKind::PlaceMarketOrder => self.unset_can_place_market(),
            TraderCapabilityKind::RiskReducingTrade => {
                self.unset_can_place_market();
                self.unset_can_place_limit();
            }
            TraderCapabilityKind::RiskIncreasingTrade => self.unset_can_risk_increase(),
            TraderCapabilityKind::DepositCollateral => self.unset_can_deposit(),
            TraderCapabilityKind::WithdrawCollateral => self.unset_can_withdraw(),
        }
        self.ensure_invariants();
    }

    pub fn mark_reduce_only(&mut self) {
        self.enable_capability(TraderCapabilityKind::PlaceLimitOrder);
        self.enable_capability(TraderCapabilityKind::PlaceMarketOrder);
        self.enable_capability(TraderCapabilityKind::DepositCollateral);
        self.enable_capability(TraderCapabilityKind::WithdrawCollateral);
        self.disable_capability(TraderCapabilityKind::RiskIncreasingTrade);
    }

    pub fn freeze(&mut self) {
        self.mark_reduce_only();
        self.disable_capability(TraderCapabilityKind::DepositCollateral);
        self.disable_capability(TraderCapabilityKind::WithdrawCollateral);
    }

    pub fn enable_all_capabilities(&mut self) {
        self.enable_capability(TraderCapabilityKind::PlaceLimitOrder);
        self.enable_capability(TraderCapabilityKind::PlaceMarketOrder);
        self.enable_capability(TraderCapabilityKind::RiskIncreasingTrade);
        self.enable_capability(TraderCapabilityKind::DepositCollateral);
        self.enable_capability(TraderCapabilityKind::WithdrawCollateral);
    }
}

impl Debug for TraderCapabilityFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = if self.is_uninitialized() {
            "uninitialized"
        } else if self.is_frozen() {
            if self.is_hot() {
                "frozen_hot"
            } else {
                "frozen_cold"
            }
        } else if self.is_reduce_only() {
            if self.is_hot() {
                "reduce_only_hot"
            } else {
                "reduce_only_cold"
            }
        } else if self.is_hot() {
            "hot_active"
        } else if self.is_cold() {
            "cold"
        } else {
            "custom"
        };

        f.debug_struct("TraderCapabilityFlags")
            .field("bits", &format_args!("0x{:08X}", self.flags))
            .field("state", &state)
            .field("capabilities", &format_args!("{self}"))
            .finish()
    }
}

impl Display for TraderCapabilityFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const CAPABILITIES: [(&str, u32); 6] = [
            ("HOT", TraderCapabilityFlags::HOT),
            ("CAN_PLACE_LIMIT", TraderCapabilityFlags::CAN_PLACE_LIMIT),
            ("CAN_PLACE_MARKET", TraderCapabilityFlags::CAN_PLACE_MARKET),
            (
                "CAN_RISK_INCREASE",
                TraderCapabilityFlags::CAN_RISK_INCREASE,
            ),
            ("CAN_DEPOSIT", TraderCapabilityFlags::CAN_DEPOSIT),
            ("CAN_WITHDRAW", TraderCapabilityFlags::CAN_WITHDRAW),
        ];

        let mut wrote_any = false;
        for (name, mask) in CAPABILITIES {
            if self.contains(mask) {
                if wrote_any {
                    write!(f, " | ")?;
                }
                write!(f, "{name}")?;
                wrote_any = true;
            }
        }

        let unknown = self.reserved_bits();
        if unknown != 0 {
            if wrote_any {
                write!(f, " | ")?;
            }
            write!(f, "UNKNOWN(0x{unknown:08X})")?;
            wrote_any = true;
        }

        if !wrote_any {
            write!(f, "NONE")?;
        }

        Ok(())
    }
}

impl From<TraderCapabilityFlags> for u32 {
    fn from(flags: TraderCapabilityFlags) -> Self {
        flags.bits()
    }
}

impl TryFrom<u32> for TraderCapabilityFlags {
    type Error = TraderCapabilityFlagsError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

// ============================================================================
// Size assertions
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_sizes() {
        assert_eq!(std::mem::size_of::<QuoteLots>(), 8);
        assert_eq!(std::mem::size_of::<BaseLots>(), 8);
        assert_eq!(std::mem::size_of::<Ticks>(), 8);
        assert_eq!(std::mem::size_of::<SignedBaseLots>(), 8);
        assert_eq!(std::mem::size_of::<SignedQuoteLots>(), 8);
        assert_eq!(std::mem::size_of::<SequenceNumber>(), 16);
        assert_eq!(std::mem::size_of::<SequenceNumberU8>(), 1);
        assert_eq!(std::mem::size_of::<SignedQuoteLotsI56>(), 7);
        assert_eq!(std::mem::size_of::<AssetIndex>(), 4);
        assert_eq!(std::mem::size_of::<AssetIndexU64>(), 8);
        assert_eq!(std::mem::size_of::<Symbol>(), 16);
        assert_eq!(std::mem::size_of::<TraderCapabilityFlags>(), 4);
    }

    #[test]
    fn test_signed_quote_lots_i56() {
        let positive = SignedQuoteLotsI56::new(12345);
        assert_eq!(positive.as_i64(), 12345);

        let negative = SignedQuoteLotsI56::new(-12345);
        assert_eq!(negative.as_i64(), -12345);

        let zero = SignedQuoteLotsI56::ZERO;
        assert_eq!(zero.as_i64(), 0);
    }

    #[test]
    fn test_trader_capability_presets_and_access() {
        let cold = TraderCapabilityFlags::cold();
        assert!(!cold.is_hot());
        assert!(cold.allows(TraderCapabilityKind::PlaceMarketOrder));
        assert!(!cold.allows(TraderCapabilityKind::PlaceLimitOrder));
        assert!(cold.allows_with_cold_activation(TraderCapabilityKind::PlaceLimitOrder));

        let hot_active = TraderCapabilityFlags::hot_active();
        assert!(hot_active.is_hot());
        assert!(hot_active.allows(TraderCapabilityKind::PlaceLimitOrder));
        assert!(hot_active.allows(TraderCapabilityKind::RiskIncreasingTrade));

        let reduce_only_hot = TraderCapabilityFlags::reduce_only_hot();
        assert!(reduce_only_hot.is_hot());
        assert!(reduce_only_hot.is_reduce_only());
        assert!(!reduce_only_hot.allows(TraderCapabilityKind::RiskIncreasingTrade));
        assert!(reduce_only_hot.allows(TraderCapabilityKind::RiskReducingTrade));

        let frozen_hot = TraderCapabilityFlags::frozen_hot();
        assert!(frozen_hot.is_hot());
        assert!(frozen_hot.is_frozen());
        assert!(!frozen_hot.allows(TraderCapabilityKind::DepositCollateral));
        assert!(!frozen_hot.allows(TraderCapabilityKind::WithdrawCollateral));
    }

    #[test]
    fn test_trader_capability_display_and_reserved_bits() {
        let active = TraderCapabilityFlags::hot_active();
        let text = active.to_string();
        assert!(text.contains("HOT"));
        assert!(text.contains("CAN_PLACE_MARKET"));

        let with_unknown = TraderCapabilityFlags::new(TraderCapabilityFlags::HOT | (1 << 31));
        let unknown_text = with_unknown.to_string();
        assert!(unknown_text.contains("UNKNOWN(0x80000000)"));
        assert!(with_unknown.has_reserved_bits());

        let strict = TraderCapabilityFlags::try_new(1 << 31);
        assert!(matches!(
            strict,
            Err(TraderCapabilityFlagsError::ReservedBitsSet(0x80000000))
        ));
    }

    #[test]
    fn test_disable_risk_reducing_drops_limit_and_market() {
        let mut flags = TraderCapabilityFlags::hot_active();
        flags.disable_capability(TraderCapabilityKind::RiskReducingTrade);
        assert!(!flags.allows(TraderCapabilityKind::RiskReducingTrade));
        assert!(!flags.allows(TraderCapabilityKind::PlaceMarketOrder));
        assert!(!flags.allows(TraderCapabilityKind::PlaceLimitOrder));
    }
}
