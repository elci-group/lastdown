use serde::{Deserialize, Serialize};

use crate::error::RangeError;
use crate::ids::{FieldId, RouteId};
use crate::resource::Score;
use crate::schema::SCHEMA_VERSION;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustDomain {
    Local,
    SameProvider,
    ForeignProvider,
}

/// Static capability plus room for later empirical updates.
///
/// There is no vendor tag here. Two versions of a model are different profiles
/// and must not share a key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProfile {
    pub schema_version: u32,
    pub reasoning: Score,
    pub coding: Score,
    pub vision: Score,
    pub tool_use: Score,
    pub long_context: Score,
    pub structured_output: Score,
    pub latency_ms_p50: u64,
    pub latency_ms_p90: u64,
    pub cost_per_1k_tokens: u64,
}

impl CapabilityProfile {
    pub fn coding_default() -> Result<Self, RangeError> {
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            reasoning: Score::new(700)?,
            coding: Score::new(800)?,
            vision: Score::new(100)?,
            tool_use: Score::new(750)?,
            long_context: Score::new(600)?,
            structured_output: Score::new(800)?,
            latency_ms_p50: 2_000,
            latency_ms_p90: 8_000,
            cost_per_1k_tokens: 1,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRoute {
    pub route_id: RouteId,
    /// Opaque versioned key such as `model-x@2026-10`.
    pub model_key: String,
    pub capability: CapabilityProfile,
    pub trust_domain: TrustDomain,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastdownPolicy {
    pub schema_version: u32,
    pub redzone_ppm: u32,
    pub lastdown_ppm: u32,
    pub min_attempt_ppm: u32,
    pub fresh_context_success_ppm: u32,
    pub reset_bonus_ppm: u32,
}

impl Default for LastdownPolicy {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            redzone_ppm: 250_000,
            lastdown_ppm: 550_000,
            min_attempt_ppm: 550_000,
            fresh_context_success_ppm: 980_000,
            reset_bonus_ppm: 0,
        }
    }
}

impl LastdownPolicy {
    pub fn validated(&self) -> Result<(), RangeError> {
        for ppm in [
            self.redzone_ppm,
            self.lastdown_ppm,
            self.min_attempt_ppm,
            self.fresh_context_success_ppm,
        ] {
            if ppm > 1_000_000 {
                return Err(RangeError::Probability(ppm));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PossessionBudget {
    pub schema_version: u32,
    pub granted_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Possession {
    pub schema_version: u32,
    pub model: ModelRoute,
    pub started_at_ms: u64,
    pub starting_field: FieldId,
    pub started_completed: usize,
    pub budget: PossessionBudget,
    pub policy: LastdownPolicy,
}
