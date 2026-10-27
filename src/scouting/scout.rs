//! Open a blind round from a punt, then accept schema-valid answers.
//!
//! Bindings stay with the coordinator. The invitation carries the sanitised
//! packet and an opaque candidate id.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::drive::Drive;
use crate::core::possession::ModelRoute;
use crate::core::state_machine::{transition, MachineEvent};
use crate::error::ScoutError;
use crate::schema::SCHEMA_VERSION;
use crate::scouting::amnesia::{candidate_packet, CandidatePacket};
use crate::scouting::schema::{check_response, ScoutResponse};
use crate::telemetry::{kinds, TelemetryEvent, TelemetryLog};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoutBinding {
    pub candidate_id: String,
    pub family_id: String,
    pub priority: u32,
    pub route: ModelRoute,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoutInvitation {
    pub schema_version: u32,
    pub candidate_id: String,
    pub field_id: String,
    pub task_class: String,
    pub answer_min_milli: u64,
    pub answer_max_milli: u64,
    pub packet: CandidatePacket,
}

pub struct ScoutRequest<'a> {
    pub task_class: &'a str,
    pub answer_min_milli: u64,
    pub answer_max_milli: u64,
    pub hidden_model_keys: &'a [String],
    pub bindings: &'a [ScoutBinding],
    pub now_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedScout {
    pub candidate_id: String,
    pub nominal_ytg_milli: u64,
    pub success_ppm: u32,
    pub confidence_ppm: u32,
    pub resource_tokens: u64,
    pub blocking_unknowns: Vec<String>,
    pub tool_requirements: Vec<String>,
    pub continuation_tokens: u64,
}

pub fn request_scouts(
    drive: &mut Drive,
    log: &mut TelemetryLog,
    request: ScoutRequest<'_>,
) -> Result<Vec<ScoutInvitation>, ScoutError> {
    if request.bindings.is_empty() {
        return Err(ScoutError::Inconsistent("no candidates"));
    }
    if request.answer_min_milli > request.answer_max_milli {
        return Err(ScoutError::Inconsistent("range"));
    }
    let mut hidden = request.hidden_model_keys.to_vec();
    for binding in request.bindings {
        hidden.push(binding.route.model_key.clone());
    }
    let packet = candidate_packet(&drive.field, &drive.objective, &hidden)?;
    drive.status = transition(drive.status, MachineEvent::BeginBlindScout)
        .map_err(|_| ScoutError::WrongPhase)?;
    let mut invitations = Vec::with_capacity(request.bindings.len());
    for binding in request.bindings {
        invitations.push(ScoutInvitation {
            schema_version: SCHEMA_VERSION,
            candidate_id: binding.candidate_id.clone(),
            field_id: drive.field.field_id.as_str().to_string(),
            task_class: request.task_class.to_string(),
            answer_min_milli: request.answer_min_milli,
            answer_max_milli: request.answer_max_milli,
            packet: packet.clone(),
        });
    }
    let mut detail = BTreeMap::new();
    detail.insert("candidates".to_string(), invitations.len().to_string());
    log.push(TelemetryEvent::new(
        kinds::SCOUT_REQUESTED,
        request.now_ms,
        drive.drive_id.clone(),
        Some(drive.field.field_id.clone()),
        detail,
    ));
    Ok(invitations)
}

pub fn complete_scouts(
    drive: &mut Drive,
    log: &mut TelemetryLog,
    now_ms: u64,
    invitations: &[ScoutInvitation],
    responses: &[ScoutResponse],
) -> Result<Vec<AcceptedScout>, ScoutError> {
    if responses.is_empty() {
        return Err(ScoutError::NoneAccepted);
    }
    let mut seen = BTreeSet::new();
    let mut accepted = Vec::with_capacity(responses.len());
    for response in responses {
        let invitation = invitations
            .iter()
            .find(|item| item.candidate_id == response.candidate_id)
            .ok_or(ScoutError::UnknownCandidate)?;
        if !seen.insert(response.candidate_id.clone()) {
            return Err(ScoutError::Inconsistent("duplicate"));
        }
        check_response(invitation, response)?;
        accepted.push(AcceptedScout {
            candidate_id: response.candidate_id.clone(),
            nominal_ytg_milli: response.distance_milli,
            success_ppm: response.success_ppm,
            confidence_ppm: response.confidence_ppm,
            resource_tokens: response.resource_tokens,
            blocking_unknowns: response.blocking_unknowns.clone(),
            tool_requirements: response.tool_requirements.clone(),
            continuation_tokens: response.continuation_tokens,
        });
    }
    drive.status = transition(drive.status, MachineEvent::ScoutComplete)
        .map_err(|_| ScoutError::WrongPhase)?;
    let mut detail = BTreeMap::new();
    detail.insert("accepted".to_string(), accepted.len().to_string());
    log.push(TelemetryEvent::new(
        kinds::SCOUT_RETURNED,
        now_ms,
        drive.drive_id.clone(),
        Some(drive.field.field_id.clone()),
        detail,
    ));
    Ok(accepted)
}
