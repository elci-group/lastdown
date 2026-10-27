use crate::core::drive::{Drive, DriveBudget};
use crate::core::field::FieldSnapshot;
use crate::core::possession::{Possession, PossessionBudget};
use crate::core::state_machine::DriveStatus;
use crate::error::PuntError;
use crate::ids::DriveId;
use crate::punt::continuation::{objective_from, reconstruct_field, validate, ContinuationObject};
use crate::resource::{ContinuationReserve, ResourceVector};
use crate::schema::SCHEMA_VERSION;

/// Two fields support the same next possession when their evidence, gates,
/// tools, and side effects agree. Inspection prose is not consulted.
pub fn operationally_equivalent(left: &FieldSnapshot, right: &FieldSnapshot) -> bool {
    left.field_id == right.field_id
        && left.parent == right.parent
        && left.artifacts == right.artifacts
        && left.verified_facts == right.verified_facts
        && left.assertions == right.assertions
        && left.completed_gates == right.completed_gates
        && left.partial_gates == right.partial_gates
        && left.unresolved_gates == right.unresolved_gates
        && left.tests == right.tests
        && left.tool_state == right.tool_state
        && left.contradictions == right.contradictions
        && left.corroborations == right.corroborations
        && left.supersessions == right.supersessions
        && left.blockers == right.blockers
        && left.repository == right.repository
        && same_effects(&left.side_effects, &right.side_effects)
}

fn same_effects(
    left: &[crate::core::field::SideEffect],
    right: &[crate::core::field::SideEffect],
) -> bool {
    let mut left: Vec<_> = left.to_vec();
    let mut right: Vec<_> = right.to_vec();
    left.sort_by(|a, b| a.effect_id.as_str().cmp(b.effect_id.as_str()));
    right.sort_by(|a, b| a.effect_id.as_str().cmp(b.effect_id.as_str()));
    left == right
}

pub fn validated_field(object: &ContinuationObject) -> Result<FieldSnapshot, PuntError> {
    validate(object)?;
    reconstruct_field(object)
}

#[derive(Clone, Debug)]
pub struct ResumeRequest {
    pub route: crate::core::possession::ModelRoute,
    pub grant: ResourceVector,
    pub reserve: ContinuationReserve,
    pub reserve_hold: ResourceVector,
    pub policy: crate::core::possession::LastdownPolicy,
    pub now_ms: u64,
}

/// Build a drive from a committed continuation and nothing else.
///
/// The predecessor conversation is not an input. The new possession starts at
/// `Normal` on the reconstructed field.
pub fn resume_drive(
    object: &ContinuationObject,
    request: ResumeRequest,
) -> Result<Drive, PuntError> {
    let field = validated_field(object)?;
    let drive_id = DriveId::new(object.identity.drive_id.clone())
        .map_err(|_| PuntError::Replay("drive id"))?;
    request
        .policy
        .validated()
        .map_err(|_| PuntError::Replay("policy"))?;
    let started_completed = field.completed_gates.len();
    Ok(Drive {
        schema_version: SCHEMA_VERSION,
        drive_id,
        generation: object.identity.generation.saturating_add(1),
        objective: objective_from(&object.objective),
        acceptance_gates: object.objective.acceptance_gates.clone(),
        field: field.clone(),
        possession: Some(Possession {
            schema_version: SCHEMA_VERSION,
            model: request.route,
            started_at_ms: request.now_ms,
            starting_field: field.field_id.clone(),
            started_completed,
            budget: PossessionBudget {
                schema_version: SCHEMA_VERSION,
                granted_tokens: request.grant.tokens,
            },
            policy: request.policy,
        }),
        budget: DriveBudget {
            granted: request.grant,
            consumed: ResourceVector::zero(),
            reserve: request.reserve,
            reserve_hold: request.reserve_hold,
            lock_reserve: true,
        },
        status: DriveStatus::Normal,
        attempt_used: false,
        preserving: false,
        failure: None,
        possession_marks: Vec::new(),
    })
}
