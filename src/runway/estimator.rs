use serde::{Deserialize, Serialize};

use crate::resource::{Distribution, Probability, ResourceDim, ResourceVector};
use crate::schema::{SCHEMA_VERSION, YTG_REFERENCE_MILLI};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunwayInputs {
    pub remaining: ResourceVector,
    pub reserve_hold: ResourceVector,
    pub observed_per_gate: ResourceVector,
    pub gates_remaining: u32,
    pub nominal_ytg_milli: u32,
    pub historical_ytg_error_milli: i32,
    pub recent_token_velocity_per_sec: u64,
    pub tool_call_cost: ResourceVector,
    pub expected_tool_latency_ms: u64,
    pub provider_health: Probability,
    pub rate_limit_headroom: Probability,
    pub next_cost: ResourceVector,
    pub fresh_grant: ResourceVector,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunwayEstimate {
    pub schema_version: u32,
    pub executable_remaining: ResourceVector,
    pub continuation_required: ResourceVector,
    pub completion_required: Distribution<ResourceVector>,
    pub exhaustion_probability: Probability,
    pub binding_dimension: Option<ResourceDim>,
}

/// Exhaustion of one dimension.
///
/// Comfortable headroom (`executable >= p90`) sits at 4%. The median sits at
/// 65%. An empty executable dimension that the work still needs sits at 100%.
pub fn dimension_exhaustion(executable: u64, p50: u64, p90: u64) -> u32 {
    if p50 == 0 {
        return 0;
    }
    if executable == 0 {
        return 1_000_000;
    }
    let p90 = p90.max(p50);
    if executable >= p90 {
        return 40_000;
    }
    if executable >= p50 {
        let span = (p90 - p50).max(1);
        let above = executable - p50;
        let drop = 610_000u64.saturating_mul(above) / span;
        650_000u32.saturating_sub(drop.min(650_000) as u32)
    } else {
        let gap = 350_000u64.saturating_mul(p50 - executable) / p50;
        650_000 + gap as u32
    }
}

pub fn estimate(inputs: &RunwayInputs) -> RunwayEstimate {
    let executable = inputs.remaining.saturating_sub(inputs.reserve_hold);
    let completion = completion_distribution(inputs);
    let mut best = 0u32;
    let mut binding = None;
    for dim in ResourceDim::ALL {
        let p50 = completion.p50.get(dim);
        let probability = dimension_exhaustion(executable.get(dim), p50, completion.p90.get(dim));
        if p50 > 0 && probability >= best {
            best = probability;
            binding = Some(dim);
        }
    }
    let health_gap = 1_000_000 - inputs.provider_health.ppm();
    let headroom_gap = 1_000_000 - inputs.rate_limit_headroom.ppm();
    let penalty = health_gap / 5 + headroom_gap / 5;
    let exhaustion =
        Probability::from_ppm(best.saturating_add(penalty).min(1_000_000)).expect("clamped ppm");
    RunwayEstimate {
        schema_version: SCHEMA_VERSION,
        executable_remaining: executable,
        continuation_required: inputs.reserve_hold,
        completion_required: completion,
        exhaustion_probability: exhaustion,
        binding_dimension: binding,
    }
}

fn completion_distribution(inputs: &RunwayInputs) -> Distribution<ResourceVector> {
    let gates = u64::from(inputs.gates_remaining);
    let nominal = u64::from(inputs.nominal_ytg_milli.max(1));
    let reference = u64::from(YTG_REFERENCE_MILLI);
    let mut p50 = inputs.observed_per_gate.scale(gates);
    p50 = scale_vector(p50, nominal, reference);
    p50 = p50.saturating_add(inputs.tool_call_cost.scale(gates));
    p50.time_ms = p50
        .time_ms
        .saturating_add(inputs.expected_tool_latency_ms.saturating_mul(gates));
    if inputs.recent_token_velocity_per_sec > 0 && p50.time_ms > 0 {
        let burn = inputs
            .recent_token_velocity_per_sec
            .saturating_mul(p50.time_ms)
            / 1_000;
        if burn > p50.tokens {
            p50.tokens = burn;
        }
    }
    let mut p90 = p50.saturating_add(half(p50));
    if inputs.historical_ytg_error_milli > 0 {
        let error = inputs.historical_ytg_error_milli as u64;
        let inflation = scale_vector(p50, error, nominal.max(1));
        p90 = p90.saturating_add(inflation);
    }
    if inputs.recent_token_velocity_per_sec > 0 {
        let burn = inputs
            .recent_token_velocity_per_sec
            .saturating_mul(p50.time_ms)
            / 1_000;
        let lifted = burn.saturating_add(burn / 2);
        if lifted > p90.tokens {
            p90.tokens = lifted;
        }
    }
    Distribution { p50, p90 }
}

fn scale_vector(vector: ResourceVector, numerator: u64, denominator: u64) -> ResourceVector {
    let denominator = denominator.max(1);
    ResourceVector {
        tokens: vector.tokens.saturating_mul(numerator) / denominator,
        context: vector.context.saturating_mul(numerator) / denominator,
        provider_quota: vector.provider_quota.saturating_mul(numerator) / denominator,
        account_budget: vector.account_budget.saturating_mul(numerator) / denominator,
        time_ms: vector.time_ms.saturating_mul(numerator) / denominator,
        tool_calls: vector.tool_calls.saturating_mul(numerator) / denominator,
    }
}

fn half(vector: ResourceVector) -> ResourceVector {
    ResourceVector {
        tokens: vector.tokens / 2,
        context: vector.context / 2,
        provider_quota: vector.provider_quota / 2,
        account_budget: vector.account_budget / 2,
        time_ms: vector.time_ms / 2,
        tool_calls: vector.tool_calls / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_curve_is_exact() {
        assert_eq!(dimension_exhaustion(0, 10, 15), 1_000_000);
        assert_eq!(dimension_exhaustion(100, 10, 15), 40_000);
        assert_eq!(dimension_exhaustion(10, 10, 15), 650_000);
        assert_eq!(dimension_exhaustion(5, 10, 15), 825_000);
        assert_eq!(dimension_exhaustion(10, 0, 0), 0);
    }

    #[test]
    fn positive_ytg_error_inflates_p90() {
        let base = sample(0);
        let harder = sample(5_000);
        assert!(harder.completion_required.p90.tokens > base.completion_required.p90.tokens);
        assert_eq!(
            base.completion_required.p50.tokens,
            harder.completion_required.p50.tokens
        );
    }

    fn sample(error: i32) -> RunwayEstimate {
        estimate(&RunwayInputs {
            remaining: ResourceVector::splat(10_000),
            reserve_hold: ResourceVector::zero(),
            observed_per_gate: ResourceVector {
                tokens: 100,
                ..ResourceVector::zero()
            },
            gates_remaining: 2,
            nominal_ytg_milli: 10_000,
            historical_ytg_error_milli: error,
            recent_token_velocity_per_sec: 0,
            tool_call_cost: ResourceVector::zero(),
            expected_tool_latency_ms: 0,
            provider_health: Probability::ONE,
            rate_limit_headroom: Probability::ONE,
            next_cost: ResourceVector::zero(),
            fresh_grant: ResourceVector::splat(10_000),
        })
    }
}
