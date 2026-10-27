//! Possession plans that follow a punt loop, and the dynamic reserve.
//!
//! Provider exhaustion stays a preserve. It does not become a turnover.

use crate::core::drive::{DriveBudget, LoopSignal};
use crate::core::field::FieldSnapshot;
use crate::provider::BoundarySignal;
use crate::runway::reserve::estimate_reserve;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PossessionPlan {
    Continue,
    Reassess,
    Rollback { parent_field: Option<String> },
    Diagnose,
}

impl PossessionPlan {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::Reassess => "reassess",
            Self::Rollback { .. } => "rollback",
            Self::Diagnose => "diagnose",
        }
    }
}

pub fn possession_plan(signal: LoopSignal, parent_field: Option<&str>) -> PossessionPlan {
    match signal {
        LoopSignal::None => PossessionPlan::Continue,
        LoopSignal::Reassess => PossessionPlan::Reassess,
        LoopSignal::Rollback => PossessionPlan::Rollback {
            parent_field: parent_field.map(str::to_string),
        },
        LoopSignal::Diagnose => PossessionPlan::Diagnose,
    }
}

/// A provider boundary preserves the field. The signal names the boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryResponse {
    Preserve,
}

pub fn boundary_response(signal: BoundarySignal) -> BoundaryResponse {
    match signal {
        BoundarySignal::RateLimited
        | BoundarySignal::ContextOverflow
        | BoundarySignal::Timeout
        | BoundarySignal::QuotaRejected
        | BoundarySignal::ProviderTerminated
        | BoundarySignal::MalformedOutput
        | BoundarySignal::PartialToolExecution => BoundaryResponse::Preserve,
    }
}

/// When the reserve is unlocked, the hold tracks the field that must be saved.
///
/// Fixtures lock the reserve so their punt arithmetic stays exact. An unlocked
/// budget grows when uncommitted work appears.
pub fn apply_dynamic_reserve(budget: &mut DriveBudget, field: &FieldSnapshot) {
    if budget.lock_reserve {
        return;
    }
    let reserve = estimate_reserve(field);
    budget.reserve_hold = reserve.to_hold();
    budget.reserve = reserve;
}
