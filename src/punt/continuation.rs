use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::drive::{AcceptanceGate, CapabilityNeed, Constraint, Drive, Objective};
use crate::core::field::Digest;
use crate::core::field::{
    Artifact, Blocker, Contradiction, Corroboration, Fact, FieldSnapshot, RepoState, SideEffect,
    Supersession, TestRecord, ToolState,
};
use crate::core::possession::TrustDomain;
use crate::error::PuntError;
use crate::ids::FieldId;
use crate::resource::ResourceVector;
use crate::schema::{CONTINUATION_MAGIC, SCHEMA_VERSION};
use crate::secrets::redact_secrets;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentitySection {
    pub drive_id: String,
    pub field_id: String,
    pub parent_field_id: Option<String>,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveSection {
    pub statement: String,
    pub constraints: Vec<Constraint>,
    pub acceptance_gates: Vec<AcceptanceGate>,
    pub required_capabilities: Vec<CapabilityNeed>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionSection {
    pub completed: Vec<String>,
    pub partial: Vec<String>,
    pub unresolved: Vec<String>,
    pub blockers: Vec<Blocker>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSection {
    pub observations: Vec<Fact>,
    pub assertions: Vec<Fact>,
    pub artifacts: Vec<Artifact>,
    pub tests: Vec<TestRecord>,
    pub contradictions: Vec<Contradiction>,
    pub corroborations: Vec<Corroboration>,
    pub supersessions: Vec<Supersession>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionSection {
    pub filesystem_delta: Vec<SideEffect>,
    pub repository_state: RepoState,
    pub tool_state: ToolState,
    pub external_side_effects: Vec<SideEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationSection {
    pub available_actions: Vec<String>,
    pub required_capabilities: Vec<CapabilityNeed>,
    pub minimum_budget: ResourceVector,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionSection {
    /// Human text. Excluded from the integrity hash.
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub producer: String,
    pub produced_at_ms: u64,
    pub redactions: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegritySection {
    pub section_hashes: BTreeMap<String, String>,
    pub object_hash: String,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationObject {
    pub magic: String,
    pub schema_version: u32,
    pub identity: IdentitySection,
    pub objective: ObjectiveSection,
    pub position: PositionSection,
    pub evidence: EvidenceSection,
    pub execution: ExecutionSection,
    pub continuation: ContinuationSection,
    pub inspection: InspectionSection,
    pub integrity: IntegritySection,
}

#[derive(Serialize)]
struct Preimage<'a> {
    magic: &'a str,
    schema_version: u32,
    section_hashes: &'a BTreeMap<String, String>,
    provenance: &'a Provenance,
}

pub fn redact_field(field: &mut FieldSnapshot) -> u32 {
    let mut redactions = 0u32;
    for artifact in &mut field.artifacts {
        let (body, count) = redact_secrets(&artifact.body);
        artifact.body = body;
        artifact.rehash();
        redactions += count;
    }
    for fact in field
        .verified_facts
        .iter_mut()
        .chain(field.assertions.iter_mut())
    {
        redactions += redact_in(&mut fact.subject);
        redactions += redact_in(&mut fact.predicate);
        redactions += redact_in(&mut fact.value);
        redactions += redact_in(&mut fact.authority);
    }
    for test in &mut field.tests {
        redactions += redact_in(&mut test.detail);
        redactions += redact_in(&mut test.name);
        test.digest = Digest::of(test.detail.as_bytes());
    }
    for effect in &mut field.side_effects {
        redactions += redact_in(&mut effect.payload);
        effect.payload_digest = Digest::of(effect.payload.as_bytes());
    }
    for blocker in &mut field.blockers {
        redactions += redact_in(&mut blocker.detail);
        redactions += redact_in(&mut blocker.code);
    }
    for tool in &mut field.tool_state.tools {
        if let Some(cursor) = tool.cursor.as_mut() {
            redactions += redact_in(cursor);
        }
    }
    redactions
}

fn redact_in(slot: &mut String) -> u32 {
    let (next, count) = redact_secrets(slot);
    *slot = next;
    count
}

pub fn disclosure_allows(field: &FieldSnapshot, destination: TrustDomain) -> bool {
    if destination != TrustDomain::ForeignProvider {
        return true;
    }
    !field
        .artifacts
        .iter()
        .any(|artifact| artifact.disclosure == crate::core::field::DisclosureClass::Restricted)
}

pub fn assemble(
    drive: &Drive,
    field: &FieldSnapshot,
    redactions: u32,
    produced_at_ms: u64,
) -> Result<ContinuationObject, PuntError> {
    let (filesystem_delta, external_side_effects) = split_effects(&field.side_effects);
    let objective = ObjectiveSection {
        statement: {
            let (text, _) = redact_secrets(&drive.objective.statement);
            text
        },
        constraints: drive
            .objective
            .constraints
            .iter()
            .map(|constraint| Constraint {
                key: redact_secrets(&constraint.key).0,
                value: redact_secrets(&constraint.value).0,
            })
            .collect(),
        acceptance_gates: drive.acceptance_gates.clone(),
        required_capabilities: drive.objective.required_capabilities.clone(),
    };
    let position = PositionSection {
        completed: ids(&field.completed_gates),
        partial: ids(&field.partial_gates),
        unresolved: ids(&field.unresolved_gates),
        blockers: field.blockers.clone(),
    };
    let evidence = EvidenceSection {
        observations: field.verified_facts.clone(),
        assertions: field.assertions.clone(),
        artifacts: field.artifacts.clone(),
        tests: field.tests.clone(),
        contradictions: field.contradictions.clone(),
        corroborations: field.corroborations.clone(),
        supersessions: field.supersessions.clone(),
    };
    let execution = ExecutionSection {
        filesystem_delta,
        repository_state: field.repository.clone(),
        tool_state: field.tool_state.clone(),
        external_side_effects,
    };
    let continuation = ContinuationSection {
        available_actions: actions(field),
        required_capabilities: drive.objective.required_capabilities.clone(),
        minimum_budget: drive.budget.reserve_hold,
    };
    let bare = ContinuationObject {
        magic: CONTINUATION_MAGIC.to_string(),
        schema_version: SCHEMA_VERSION,
        identity: IdentitySection {
            drive_id: drive.drive_id.as_str().to_string(),
            field_id: field.field_id.as_str().to_string(),
            parent_field_id: field.parent.as_ref().map(|id| id.as_str().to_string()),
            generation: drive.generation,
        },
        objective,
        position,
        evidence,
        execution,
        continuation,
        inspection: InspectionSection {
            summary: "Human inspection summary. Authoritative state is the typed sections."
                .to_string(),
        },
        integrity: IntegritySection {
            section_hashes: BTreeMap::new(),
            object_hash: String::new(),
            provenance: Provenance {
                producer: "lastdown-sentinel".to_string(),
                produced_at_ms,
                redactions,
            },
        },
    };
    seal(bare)
}

fn split_effects(effects: &[SideEffect]) -> (Vec<SideEffect>, Vec<SideEffect>) {
    let mut filesystem = Vec::new();
    let mut external = Vec::new();
    for effect in effects {
        if effect.kind == "filesystem" {
            filesystem.push(effect.clone());
        } else {
            external.push(effect.clone());
        }
    }
    (filesystem, external)
}

fn ids(gates: &[crate::ids::GateId]) -> Vec<String> {
    gates.iter().map(|gate| gate.as_str().to_string()).collect()
}

fn actions(field: &FieldSnapshot) -> Vec<String> {
    let mut actions = Vec::new();
    for gate in &field.unresolved_gates {
        actions.push(format!("satisfy:{}", gate.as_str()));
    }
    for gate in &field.partial_gates {
        actions.push(format!("complete:{}", gate.as_str()));
    }
    for tool in &field.tool_state.tools {
        if tool.available {
            actions.push(format!("tool:{}", tool.name));
        }
    }
    actions
}

pub fn seal(mut object: ContinuationObject) -> Result<ContinuationObject, PuntError> {
    object.integrity.section_hashes = section_hashes(&object)?;
    object.integrity.object_hash = object_hash(&object)?;
    Ok(object)
}

pub fn validate(object: &ContinuationObject) -> Result<(), PuntError> {
    if object.magic != CONTINUATION_MAGIC {
        return Err(PuntError::Integrity);
    }
    if object.schema_version != SCHEMA_VERSION {
        return Err(PuntError::Schema(object.schema_version));
    }
    let hashes = section_hashes(object)?;
    if hashes != object.integrity.section_hashes {
        return Err(PuntError::Integrity);
    }
    if object_hash(object)? != object.integrity.object_hash {
        return Err(PuntError::Integrity);
    }
    let field = reconstruct_field(object)?;
    field.check_invariants().map_err(|_| PuntError::Integrity)?;
    check_partition(object)?;
    Ok(())
}

fn check_partition(object: &ContinuationObject) -> Result<(), PuntError> {
    let mut listed = Vec::new();
    listed.extend(object.position.completed.iter().cloned());
    listed.extend(object.position.partial.iter().cloned());
    listed.extend(object.position.unresolved.iter().cloned());
    listed.sort();
    let mut gates: Vec<String> = object
        .objective
        .acceptance_gates
        .iter()
        .map(|gate| gate.gate_id.as_str().to_string())
        .collect();
    gates.sort();
    if listed != gates {
        return Err(PuntError::Field("gate partition"));
    }
    Ok(())
}

fn section_hashes(object: &ContinuationObject) -> Result<BTreeMap<String, String>, PuntError> {
    let mut hashes = BTreeMap::new();
    hashes.insert("identity".to_string(), hash_of(&object.identity)?);
    hashes.insert("objective".to_string(), hash_of(&object.objective)?);
    hashes.insert("position".to_string(), hash_of(&object.position)?);
    hashes.insert("evidence".to_string(), hash_of(&object.evidence)?);
    hashes.insert("execution".to_string(), hash_of(&object.execution)?);
    hashes.insert("continuation".to_string(), hash_of(&object.continuation)?);
    Ok(hashes)
}

fn object_hash(object: &ContinuationObject) -> Result<String, PuntError> {
    let preimage = Preimage {
        magic: object.magic.as_str(),
        schema_version: object.schema_version,
        section_hashes: &object.integrity.section_hashes,
        provenance: &object.integrity.provenance,
    };
    hash_of(&preimage)
}

fn hash_of<T: Serialize>(value: &T) -> Result<String, PuntError> {
    let bytes = serde_json::to_vec(value).map_err(|error| PuntError::Store(error.to_string()))?;
    Ok(Digest::of(&bytes).as_str().to_string())
}

pub fn reconstruct_field(object: &ContinuationObject) -> Result<FieldSnapshot, PuntError> {
    let field_id = FieldId::new(object.identity.field_id.clone())
        .map_err(|_| PuntError::Replay("field id"))?;
    let parent = match &object.identity.parent_field_id {
        Some(id) => Some(FieldId::new(id.clone()).map_err(|_| PuntError::Replay("parent id"))?),
        None => None,
    };
    let mut side_effects = object.execution.filesystem_delta.clone();
    side_effects.extend(object.execution.external_side_effects.clone());
    Ok(FieldSnapshot {
        schema_version: SCHEMA_VERSION,
        field_id,
        parent,
        artifacts: object.evidence.artifacts.clone(),
        verified_facts: object.evidence.observations.clone(),
        assertions: object.evidence.assertions.clone(),
        completed_gates: parse_gates(&object.position.completed)?,
        partial_gates: parse_gates(&object.position.partial)?,
        unresolved_gates: parse_gates(&object.position.unresolved)?,
        tests: object.evidence.tests.clone(),
        side_effects,
        tool_state: object.execution.tool_state.clone(),
        contradictions: object.evidence.contradictions.clone(),
        corroborations: object.evidence.corroborations.clone(),
        supersessions: object.evidence.supersessions.clone(),
        blockers: object.position.blockers.clone(),
        repository: object.execution.repository_state.clone(),
    })
}

fn parse_gates(ids: &[String]) -> Result<Vec<crate::ids::GateId>, PuntError> {
    ids.iter()
        .map(|id| crate::ids::GateId::new(id.clone()).map_err(|_| PuntError::Replay("gate id")))
        .collect()
}

pub fn objective_from(section: &ObjectiveSection) -> Objective {
    Objective {
        schema_version: SCHEMA_VERSION,
        statement: section.statement.clone(),
        constraints: section.constraints.clone(),
        required_capabilities: section.required_capabilities.clone(),
    }
}

pub fn canonical_bytes(object: &ContinuationObject) -> Result<Vec<u8>, PuntError> {
    serde_json::to_vec(object).map_err(|error| PuntError::Store(error.to_string()))
}

/// Competitive and conversational keys that must not appear in a continuation.
pub fn forbidden_competitive_keys() -> &'static [&'static str] {
    &[
        "previous_model",
        "previous_ytg",
        "nominal_ytg",
        "competitor_assessments",
        "route_rank",
        "selection_history",
        "coordinator_notes",
        "model_reputation",
        "conversation",
        "\"messages\"",
    ]
}

pub fn leaks_competitive_metadata(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    forbidden_competitive_keys()
        .iter()
        .any(|key| text.contains(key))
}
