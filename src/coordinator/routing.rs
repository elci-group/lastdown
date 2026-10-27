//! Family routing from effective distance, cost, latency, and disclosure.
//!
//! Ties break on the explicit priority, then on the route id. The same inputs
//! produce the same explanation. Model keys are opaque. Nothing in this module
//! branches on a vendor name.

use std::collections::BTreeMap;

use crate::coordinator::calibration::CalibrationTable;
use crate::coordinator::effective_ytg::{effective_ytg, EffectiveYtg, YtgFactors};
use crate::core::possession::{ModelRoute, TrustDomain};
use crate::error::RouteError;
use crate::ids::{DriveId, FieldId};
use crate::resource::{scale_by_probability, Probability};
use crate::telemetry::{kinds, TelemetryEvent, TelemetryLog};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutePolicy {
    pub lambda_latency: u64,
    pub mu_risk: u64,
    pub progress_value: u64,
    pub task_class: String,
    pub toolset: String,
}

/// One scout joined to the route the coordinator already bound in private.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedScout {
    pub candidate_id: String,
    pub family_id: String,
    pub priority: u32,
    pub route: ModelRoute,
    pub nominal_ytg_milli: u64,
    /// The candidate's own success estimate. It enters utility. It does not
    /// rewrite the historical calibration error.
    pub success_ppm: u32,
    pub reported_confidence_ppm: u32,
    pub resource_tokens: u64,
    pub recovery_cost_milli: u64,
    pub reliability_ppm: u32,
    pub domain_suitability_ppm: u32,
    pub capability_headroom_ppm: u32,
    pub restricted_field: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteExplanation {
    pub candidate_id: String,
    pub family_id: String,
    pub route_id: String,
    pub model_key: String,
    pub priority: u32,
    pub reported_confidence_ppm: u32,
    pub effective: EffectiveYtg,
    pub expected_cost: u64,
    pub latency_penalty: u64,
    pub risk_penalty: u64,
    pub utility: u64,
    pub eligible: bool,
    pub skip_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteDecision {
    pub selected_candidate: String,
    pub selected_family: String,
    pub selected_route_id: String,
    pub selected_model_key: String,
    pub explanations: Vec<RouteExplanation>,
}

pub struct RouteContext<'a> {
    pub drive_id: &'a DriveId,
    pub field_id: Option<&'a FieldId>,
    pub now_ms: u64,
    pub log: &'a mut TelemetryLog,
}

pub fn headroom_ppm(have: u16, need: u16) -> u32 {
    if need == 0 || have >= need {
        1_000_000
    } else {
        u32::from(have).saturating_mul(1_000_000) / u32::from(need)
    }
}

pub fn select_route(
    table: &CalibrationTable,
    policy: &RoutePolicy,
    candidates: &[RoutedScout],
    context: &mut RouteContext<'_>,
) -> Result<RouteDecision, RouteError> {
    let mut explanations = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        explanations.push(explain(table, policy, candidate));
    }
    let winner = explanations
        .iter()
        .filter(|item| item.eligible)
        .reduce(|best, next| if beats(next, best) { next } else { best })
        .ok_or(RouteError::NoneEligible)?;
    let selected_candidate = winner.candidate_id.clone();
    let selected_family = winner.family_id.clone();
    let selected_route_id = winner.route_id.clone();
    let selected_model_key = winner.model_key.clone();
    let utility = winner.utility;
    let effective_ytg_milli = winner.effective.effective_ytg_milli;
    explanations.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    let decision = RouteDecision {
        selected_candidate,
        selected_family,
        selected_route_id,
        selected_model_key,
        explanations,
    };
    let mut detail = BTreeMap::new();
    detail.insert("family".to_string(), decision.selected_family.clone());
    detail.insert("route_id".to_string(), decision.selected_route_id.clone());
    detail.insert("utility".to_string(), utility.to_string());
    detail.insert(
        "effective_ytg_milli".to_string(),
        effective_ytg_milli.to_string(),
    );
    context.log.push(TelemetryEvent::new(
        kinds::ROUTE_CALCULATED,
        context.now_ms,
        context.drive_id.clone(),
        context.field_id.cloned(),
        detail,
    ));
    Ok(decision)
}

fn explain(
    table: &CalibrationTable,
    policy: &RoutePolicy,
    candidate: &RoutedScout,
) -> RouteExplanation {
    let error = table
        .error_milli(
            &candidate.route.model_key,
            &policy.task_class,
            &policy.toolset,
        )
        .unwrap_or(0);
    let variance = table
        .spread_milli(
            &candidate.route.model_key,
            &policy.task_class,
            &policy.toolset,
        )
        .unwrap_or(0);
    let confidence = table.confidence_ppm(
        &candidate.route.model_key,
        &policy.task_class,
        &policy.toolset,
    );
    let effective = effective_ytg(&YtgFactors {
        nominal_ytg_milli: candidate.nominal_ytg_milli,
        historical_error_milli: error,
        variance_milli: variance,
        calibration_confidence_ppm: confidence,
        recovery_cost_milli: candidate.recovery_cost_milli,
        provider_reliability_ppm: candidate.reliability_ppm,
        domain_suitability_ppm: candidate.domain_suitability_ppm,
        capability_headroom_ppm: candidate.capability_headroom_ppm,
    });
    let disclosure_blocked =
        candidate.restricted_field && candidate.route.trust_domain == TrustDomain::ForeignProvider;
    let (utility, expected_cost, latency_penalty, risk_penalty) = if disclosure_blocked {
        (0, 0, 0, 0)
    } else {
        score(candidate, effective.effective_ytg_milli, policy)
    };
    RouteExplanation {
        candidate_id: candidate.candidate_id.clone(),
        family_id: candidate.family_id.clone(),
        route_id: candidate.route.route_id.as_str().to_string(),
        model_key: candidate.route.model_key.clone(),
        priority: candidate.priority,
        reported_confidence_ppm: candidate.reported_confidence_ppm,
        effective,
        expected_cost,
        latency_penalty,
        risk_penalty,
        utility,
        eligible: !disclosure_blocked,
        skip_reason: if disclosure_blocked {
            Some("disclosure".to_string())
        } else {
            None
        },
    }
}

fn score(candidate: &RoutedScout, eytg: u64, policy: &RoutePolicy) -> (u64, u64, u64, u64) {
    let expected_cost = candidate
        .resource_tokens
        .saturating_mul(candidate.route.capability.cost_per_1k_tokens)
        .saturating_mul(eytg)
        / 10_000;
    let latency_penalty = candidate
        .route
        .capability
        .latency_ms_p50
        .saturating_mul(policy.lambda_latency);
    let failure = 1_000_000u64.saturating_sub(u64::from(candidate.success_ppm.min(1_000_000)));
    let risk_penalty = failure.saturating_mul(policy.mu_risk) / 1_000_000;
    let denom = expected_cost
        .saturating_add(latency_penalty)
        .saturating_add(risk_penalty)
        .max(1);
    let probability =
        Probability::from_ppm(candidate.success_ppm.min(1_000_000)).unwrap_or(Probability::ZERO);
    let numer = scale_by_probability(probability, policy.progress_value);
    let utility = numer.saturating_mul(1_000_000) / denom;
    (utility, expected_cost, latency_penalty, risk_penalty)
}

fn beats(challenger: &RouteExplanation, incumbent: &RouteExplanation) -> bool {
    if challenger.utility != incumbent.utility {
        return challenger.utility > incumbent.utility;
    }
    if challenger.priority != incumbent.priority {
        return challenger.priority < incumbent.priority;
    }
    challenger.route_id < incumbent.route_id
}
