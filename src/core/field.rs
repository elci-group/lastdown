use serde::{Deserialize, Serialize};
use sha2::{Digest as ShaDigest, Sha256};

use crate::error::FieldError;
use crate::ids::{ArtifactId, ClaimId, EffectId, FieldId, GateId};
use crate::schema::SCHEMA_VERSION;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisclosureClass {
    Public,
    Internal,
    Restricted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Observation,
    Assertion,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digest(String);

impl Digest {
    pub fn of(bytes: &[u8]) -> Self {
        let hash = Sha256::digest(bytes);
        Self(hex_encode(&hash))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches(&self, bytes: &[u8]) -> bool {
        self == &Self::of(bytes)
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub artifact_id: ArtifactId,
    pub kind: String,
    pub disclosure: DisclosureClass,
    pub body: String,
    pub byte_len: u64,
    pub digest: Digest,
}

impl Artifact {
    pub fn new(
        artifact_id: ArtifactId,
        kind: impl Into<String>,
        disclosure: DisclosureClass,
        body: impl Into<String>,
    ) -> Self {
        let body = body.into();
        let digest = Digest::of(body.as_bytes());
        let byte_len = body.len() as u64;
        Self {
            artifact_id,
            kind: kind.into(),
            disclosure,
            body,
            byte_len,
            digest,
        }
    }

    pub fn rehash(&mut self) {
        self.byte_len = self.body.len() as u64;
        self.digest = Digest::of(self.body.as_bytes());
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub claim_id: ClaimId,
    pub kind: ClaimKind,
    pub subject: String,
    pub predicate: String,
    pub value: String,
    pub authority: String,
    pub authority_rank: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRecord {
    pub test_id: String,
    pub name: String,
    pub passed: bool,
    pub detail: String,
    pub digest: Digest,
}

impl TestRecord {
    pub fn new(
        test_id: impl Into<String>,
        name: impl Into<String>,
        passed: bool,
        detail: impl Into<String>,
    ) -> Self {
        let detail = detail.into();
        let digest = Digest::of(detail.as_bytes());
        Self {
            test_id: test_id.into(),
            name: name.into(),
            passed,
            detail,
            digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideEffect {
    pub effect_id: EffectId,
    pub kind: String,
    pub committed: bool,
    pub frozen: bool,
    pub payload: String,
    pub payload_digest: Digest,
}

impl SideEffect {
    pub fn new(
        effect_id: EffectId,
        kind: impl Into<String>,
        committed: bool,
        payload: impl Into<String>,
    ) -> Self {
        let payload = payload.into();
        let payload_digest = Digest::of(payload.as_bytes());
        Self {
            effect_id,
            kind: kind.into(),
            committed,
            frozen: false,
            payload,
            payload_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contradiction {
    pub id: String,
    pub left: ClaimId,
    pub right: ClaimId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corroboration {
    pub id: String,
    pub source: ClaimId,
    pub target: ClaimId,
    pub independent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Supersession {
    pub id: String,
    pub winner: ClaimId,
    pub loser: ClaimId,
    pub operator: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocker {
    pub blocker_id: String,
    pub gate_id: Option<GateId>,
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRecord {
    pub name: String,
    pub available: bool,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolState {
    pub tools: Vec<ToolRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoState {
    pub head: Option<String>,
    pub dirty: bool,
    pub paths: Vec<String>,
}

/// Full snapshot of the field. The parent id is lineage. The body is not a diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSnapshot {
    pub schema_version: u32,
    pub field_id: FieldId,
    pub parent: Option<FieldId>,
    pub artifacts: Vec<Artifact>,
    pub verified_facts: Vec<Fact>,
    pub assertions: Vec<Fact>,
    pub completed_gates: Vec<GateId>,
    pub partial_gates: Vec<GateId>,
    pub unresolved_gates: Vec<GateId>,
    pub tests: Vec<TestRecord>,
    pub side_effects: Vec<SideEffect>,
    pub tool_state: ToolState,
    pub contradictions: Vec<Contradiction>,
    pub corroborations: Vec<Corroboration>,
    pub supersessions: Vec<Supersession>,
    pub blockers: Vec<Blocker>,
    pub repository: RepoState,
}

impl FieldSnapshot {
    pub fn genesis(field_id: FieldId, gates: &[GateId]) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            field_id,
            parent: None,
            artifacts: Vec::new(),
            verified_facts: Vec::new(),
            assertions: Vec::new(),
            completed_gates: Vec::new(),
            partial_gates: Vec::new(),
            unresolved_gates: gates.to_vec(),
            tests: Vec::new(),
            side_effects: Vec::new(),
            tool_state: ToolState::default(),
            contradictions: Vec::new(),
            corroborations: Vec::new(),
            supersessions: Vec::new(),
            blockers: Vec::new(),
            repository: RepoState::default(),
        }
    }

    pub fn child(&self, field_id: FieldId) -> Self {
        let mut child = self.clone();
        child.field_id = field_id;
        child.parent = Some(self.field_id.clone());
        child
    }

    pub fn claim_ids(&self) -> Vec<&ClaimId> {
        self.verified_facts
            .iter()
            .chain(self.assertions.iter())
            .map(|fact| &fact.claim_id)
            .collect()
    }

    pub fn freeze_side_effects(&mut self) {
        for effect in &mut self.side_effects {
            effect.frozen = true;
        }
    }

    pub fn unfreeze_side_effects(&mut self) {
        for effect in &mut self.side_effects {
            if !effect.committed {
                effect.frozen = false;
            }
        }
    }

    pub fn refresh_repository_dirty(&mut self) {
        self.repository.dirty = self.side_effects.iter().any(|effect| !effect.committed);
        self.repository.paths = self
            .side_effects
            .iter()
            .filter(|effect| effect.kind == "filesystem")
            .map(|effect| effect.effect_id.as_str().to_string())
            .collect();
    }

    pub fn check_invariants(&self) -> Result<(), FieldError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(FieldError::Invariant("schema"));
        }
        for artifact in &self.artifacts {
            if !artifact.digest.matches(artifact.body.as_bytes())
                || artifact.byte_len != artifact.body.len() as u64
            {
                return Err(FieldError::DigestMismatch);
            }
        }
        for effect in &self.side_effects {
            if !effect.payload_digest.matches(effect.payload.as_bytes()) {
                return Err(FieldError::DigestMismatch);
            }
        }
        for test in &self.tests {
            if !test.digest.matches(test.detail.as_bytes()) {
                return Err(FieldError::DigestMismatch);
            }
        }
        let ids = self.claim_ids();
        for contradiction in &self.contradictions {
            if !ids.contains(&&contradiction.left) || !ids.contains(&&contradiction.right) {
                return Err(FieldError::Invariant("contradiction endpoint"));
            }
            if contradiction.left == contradiction.right {
                return Err(FieldError::Invariant("contradiction self link"));
            }
        }
        Ok(())
    }
}

/// Move a gate into `completed`. Unknown gates fail. The three lists stay disjoint.
pub fn satisfy_gate(field: &mut FieldSnapshot, gate: &GateId) -> Result<(), FieldError> {
    if !take_gate(field, gate) {
        return Err(FieldError::UnknownGate(gate.as_str().to_string()));
    }
    field.completed_gates.push(gate.clone());
    Ok(())
}

pub fn mark_partial(field: &mut FieldSnapshot, gate: &GateId) -> Result<(), FieldError> {
    if field
        .completed_gates
        .iter()
        .any(|existing| existing == gate)
    {
        return Ok(());
    }
    if field.partial_gates.iter().any(|existing| existing == gate) {
        return Ok(());
    }
    let Some(index) = field
        .unresolved_gates
        .iter()
        .position(|existing| existing == gate)
    else {
        return Err(FieldError::UnknownGate(gate.as_str().to_string()));
    };
    field.unresolved_gates.remove(index);
    field.partial_gates.push(gate.clone());
    Ok(())
}

fn take_gate(field: &mut FieldSnapshot, gate: &GateId) -> bool {
    if let Some(index) = field
        .unresolved_gates
        .iter()
        .position(|existing| existing == gate)
    {
        field.unresolved_gates.remove(index);
        return true;
    }
    if let Some(index) = field
        .partial_gates
        .iter()
        .position(|existing| existing == gate)
    {
        field.partial_gates.remove(index);
        return true;
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvanceMark {
    PositiveAdvance,
    Neutral,
    Regression,
}

pub fn advance_mark(before: &FieldSnapshot, after: &FieldSnapshot) -> AdvanceMark {
    let before_score = before.completed_gates.len() + before.partial_gates.len();
    let after_score = after.completed_gates.len() + after.partial_gates.len();
    if after_score > before_score {
        AdvanceMark::PositiveAdvance
    } else if after.completed_gates.len() < before.completed_gates.len()
        || after.artifacts.len() + after.verified_facts.len()
            < before.artifacts.len() + before.verified_facts.len()
    {
        AdvanceMark::Regression
    } else {
        AdvanceMark::Neutral
    }
}
