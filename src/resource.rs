use serde::{Deserialize, Serialize};

use crate::error::RangeError;

/// Parts per million. `1_000_000` is certainty.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Probability(u32);

impl Probability {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1_000_000);

    pub fn from_ppm(ppm: u32) -> Result<Self, RangeError> {
        if ppm > 1_000_000 {
            return Err(RangeError::Probability(ppm));
        }
        Ok(Self(ppm))
    }

    pub fn ppm(self) -> u32 {
        self.0
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0).min(1_000_000))
    }
}

impl TryFrom<u32> for Probability {
    type Error = RangeError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::from_ppm(value)
    }
}

impl From<Probability> for u32 {
    fn from(value: Probability) -> Self {
        value.0
    }
}

/// Capability score in the closed range `0..=1000`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct Score(u16);

impl Score {
    pub fn new(value: u16) -> Result<Self, RangeError> {
        if value > 1_000 {
            return Err(RangeError::Score(value));
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for Score {
    type Error = RangeError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Score> for u16 {
    fn from(value: Score) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceDim {
    Tokens,
    Context,
    ProviderQuota,
    AccountBudget,
    Time,
    ToolCalls,
}

impl ResourceDim {
    pub const ALL: [Self; 6] = [
        Self::Tokens,
        Self::Context,
        Self::ProviderQuota,
        Self::AccountBudget,
        Self::Time,
        Self::ToolCalls,
    ];
}

/// Component-wise budget. Arithmetic never borrows across dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceVector {
    pub tokens: u64,
    pub context: u64,
    pub provider_quota: u64,
    pub account_budget: u64,
    pub time_ms: u64,
    pub tool_calls: u64,
}

impl ResourceVector {
    pub const fn zero() -> Self {
        Self {
            tokens: 0,
            context: 0,
            provider_quota: 0,
            account_budget: 0,
            time_ms: 0,
            tool_calls: 0,
        }
    }

    pub const fn splat(value: u64) -> Self {
        Self {
            tokens: value,
            context: value,
            provider_quota: value,
            account_budget: value,
            time_ms: value,
            tool_calls: value,
        }
    }

    pub fn get(self, dim: ResourceDim) -> u64 {
        match dim {
            ResourceDim::Tokens => self.tokens,
            ResourceDim::Context => self.context,
            ResourceDim::ProviderQuota => self.provider_quota,
            ResourceDim::AccountBudget => self.account_budget,
            ResourceDim::Time => self.time_ms,
            ResourceDim::ToolCalls => self.tool_calls,
        }
    }

    pub fn set(&mut self, dim: ResourceDim, value: u64) {
        match dim {
            ResourceDim::Tokens => self.tokens = value,
            ResourceDim::Context => self.context = value,
            ResourceDim::ProviderQuota => self.provider_quota = value,
            ResourceDim::AccountBudget => self.account_budget = value,
            ResourceDim::Time => self.time_ms = value,
            ResourceDim::ToolCalls => self.tool_calls = value,
        }
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            tokens: self.tokens.saturating_add(other.tokens),
            context: self.context.saturating_add(other.context),
            provider_quota: self.provider_quota.saturating_add(other.provider_quota),
            account_budget: self.account_budget.saturating_add(other.account_budget),
            time_ms: self.time_ms.saturating_add(other.time_ms),
            tool_calls: self.tool_calls.saturating_add(other.tool_calls),
        }
    }

    pub fn saturating_sub(self, other: Self) -> Self {
        Self {
            tokens: self.tokens.saturating_sub(other.tokens),
            context: self.context.saturating_sub(other.context),
            provider_quota: self.provider_quota.saturating_sub(other.provider_quota),
            account_budget: self.account_budget.saturating_sub(other.account_budget),
            time_ms: self.time_ms.saturating_sub(other.time_ms),
            tool_calls: self.tool_calls.saturating_sub(other.tool_calls),
        }
    }

    pub fn covers(self, other: Self) -> bool {
        ResourceDim::ALL
            .into_iter()
            .all(|dim| self.get(dim) >= other.get(dim))
    }

    pub fn is_zero(self) -> bool {
        ResourceDim::ALL.into_iter().all(|dim| self.get(dim) == 0)
    }

    pub fn scale(self, factor: u64) -> Self {
        Self {
            tokens: self.tokens.saturating_mul(factor),
            context: self.context.saturating_mul(factor),
            provider_quota: self.provider_quota.saturating_mul(factor),
            account_budget: self.account_budget.saturating_mul(factor),
            time_ms: self.time_ms.saturating_mul(factor),
            tool_calls: self.tool_calls.saturating_mul(factor),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Distribution<T> {
    pub p50: T,
    pub p90: T,
}

/// Multiply a value by a probability in parts per million.
///
/// A non-zero probability applied to a non-zero value stays at least 1 so a
/// small objective is not truncated into a false "nothing to preserve".
pub fn scale_by_probability(probability: Probability, value: u64) -> u64 {
    if probability.ppm() == 0 || value == 0 {
        return 0;
    }
    let scaled = u64::from(probability.ppm()).saturating_mul(value) / 1_000_000;
    scaled.max(1)
}

/// Protected budget ordinary inference is not allowed to spend.
///
/// The fields are token-denominated. [`ContinuationReserve::to_hold`] maps
/// them onto the resource vector, including context that must remain to
/// serialise the field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationReserve {
    pub schema_version: u32,
    pub serialization_tokens: u64,
    pub verification_budget: u64,
    pub coordinator_budget: u64,
    pub persistence_budget: u64,
    pub safety_margin: u64,
}

impl ContinuationReserve {
    pub fn zero() -> Self {
        Self {
            schema_version: crate::schema::SCHEMA_VERSION,
            serialization_tokens: 0,
            verification_budget: 0,
            coordinator_budget: 0,
            persistence_budget: 0,
            safety_margin: 0,
        }
    }

    pub fn token_total(&self) -> u64 {
        self.serialization_tokens
            .saturating_add(self.verification_budget)
            .saturating_add(self.coordinator_budget)
            .saturating_add(self.persistence_budget)
            .saturating_add(self.safety_margin)
    }

    /// Map the reserve onto executable dimensions.
    ///
    /// Tokens and context both receive the full token total, because the
    /// incumbent needs residual context in which to write the continuation.
    /// Quota, money, time, and tool calls receive the safety margin so a
    /// pure token reserve cannot hide exhaustion on another axis.
    pub fn to_hold(&self) -> ResourceVector {
        let tokens = self.token_total();
        ResourceVector {
            tokens,
            context: tokens,
            provider_quota: self.safety_margin,
            account_budget: self.safety_margin,
            time_ms: self.safety_margin,
            tool_calls: self.safety_margin.min(1_000),
        }
    }
}
