//! Sanitised field projection for a candidate who has not seen the scoreboard.

use serde::{Deserialize, Serialize};

use crate::core::drive::Objective;
use crate::core::field::{DisclosureClass, Fact, FieldSnapshot};
use crate::error::ScoutError;
use crate::punt::continuation::forbidden_competitive_keys;
use crate::schema::SCHEMA_VERSION;
use crate::secrets::redact_secrets;

const PHRASES: &[&str] = &["nearly finished", "almost done", "favourite", "favorite"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidatePacket {
    pub schema_version: u32,
    pub field_id: String,
    pub objective: String,
    pub constraints: Vec<PacketConstraint>,
    pub completed_gates: Vec<String>,
    pub partial_gates: Vec<String>,
    pub unresolved_gates: Vec<String>,
    pub facts: Vec<PacketFact>,
    pub artifacts: Vec<PacketArtifact>,
    pub contradictions: Vec<PacketEdge>,
    pub tests: Vec<PacketTest>,
    pub blockers: Vec<PacketBlocker>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketConstraint {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketFact {
    pub kind: String,
    pub subject: String,
    pub predicate: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketArtifact {
    pub artifact_id: String,
    pub disclosure: String,
    pub body: Option<String>,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketEdge {
    pub left: String,
    pub right: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketTest {
    pub name: String,
    pub passed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketBlocker {
    pub code: String,
    pub detail: String,
}

pub fn candidate_packet(
    field: &FieldSnapshot,
    objective: &Objective,
    hidden_keys: &[String],
) -> Result<CandidatePacket, ScoutError> {
    let packet = CandidatePacket {
        schema_version: SCHEMA_VERSION,
        field_id: field.field_id.as_str().to_string(),
        objective: scrub_text(&objective.statement, hidden_keys),
        constraints: objective
            .constraints
            .iter()
            .map(|constraint| PacketConstraint {
                key: scrub_text(&constraint.key, hidden_keys),
                value: scrub_text(&constraint.value, hidden_keys),
            })
            .collect(),
        completed_gates: field
            .completed_gates
            .iter()
            .map(|gate| gate.as_str().to_string())
            .collect(),
        partial_gates: field
            .partial_gates
            .iter()
            .map(|gate| gate.as_str().to_string())
            .collect(),
        unresolved_gates: field
            .unresolved_gates
            .iter()
            .map(|gate| gate.as_str().to_string())
            .collect(),
        facts: field
            .verified_facts
            .iter()
            .chain(field.assertions.iter())
            .filter(|fact| keep_fact(fact, hidden_keys))
            .map(|fact| PacketFact {
                kind: match fact.kind {
                    crate::core::field::ClaimKind::Observation => "observation",
                    crate::core::field::ClaimKind::Assertion => "assertion",
                }
                .to_string(),
                subject: fact.subject.clone(),
                predicate: fact.predicate.clone(),
                value: fact.value.clone(),
            })
            .collect(),
        artifacts: field
            .artifacts
            .iter()
            .map(|artifact| {
                let disclosure = match artifact.disclosure {
                    DisclosureClass::Public => "public",
                    DisclosureClass::Internal => "internal",
                    DisclosureClass::Restricted => "restricted",
                };
                if artifact.disclosure == DisclosureClass::Restricted {
                    PacketArtifact {
                        artifact_id: artifact.artifact_id.as_str().to_string(),
                        disclosure: disclosure.to_string(),
                        body: None,
                        digest: artifact.digest.as_str().to_string(),
                    }
                } else {
                    let body = scrub_text(&artifact.body, hidden_keys);
                    let digest = crate::core::field::Digest::of(body.as_bytes());
                    PacketArtifact {
                        artifact_id: artifact.artifact_id.as_str().to_string(),
                        disclosure: disclosure.to_string(),
                        body: Some(body),
                        digest: digest.as_str().to_string(),
                    }
                }
            })
            .collect(),
        contradictions: field
            .contradictions
            .iter()
            .map(|edge| PacketEdge {
                left: edge.left.as_str().to_string(),
                right: edge.right.as_str().to_string(),
            })
            .collect(),
        tests: field
            .tests
            .iter()
            .map(|test| PacketTest {
                name: scrub_text(&test.name, hidden_keys),
                passed: test.passed,
            })
            .collect(),
        blockers: field
            .blockers
            .iter()
            .map(|blocker| PacketBlocker {
                code: scrub_text(&blocker.code, hidden_keys),
                detail: scrub_text(&blocker.detail, hidden_keys),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&packet).map_err(|_| ScoutError::Malformed)?;
    if forbidden_hit_count(&bytes, hidden_keys) > 0 {
        return Err(ScoutError::Leak);
    }
    Ok(packet)
}

/// `(hits in the candidate packet, hits in the field before the filter)`.
pub fn measure_amnesia(
    field: &FieldSnapshot,
    objective: &Objective,
    hidden_keys: &[String],
) -> Result<(usize, usize), ScoutError> {
    let raw = competitive_hits(field, objective, hidden_keys);
    let packet = candidate_packet(field, objective, hidden_keys)?;
    let bytes = serde_json::to_vec(&packet).map_err(|_| ScoutError::Malformed)?;
    Ok((forbidden_hit_count(&bytes, hidden_keys), raw))
}

pub fn forbidden_hit_count(bytes: &[u8], model_keys: &[String]) -> usize {
    let text = String::from_utf8_lossy(bytes);
    let mut hits = 0usize;
    for key in forbidden_competitive_keys() {
        if text.contains(key) {
            hits += 1;
        }
    }
    for key in model_keys {
        if !key.is_empty() && text.contains(key) {
            hits += 1;
        }
    }
    hits
}

fn competitive_hits(field: &FieldSnapshot, objective: &Objective, model_keys: &[String]) -> usize {
    let mut raw = String::new();
    raw.push_str(&objective.statement);
    raw.push('\n');
    for fact in field.verified_facts.iter().chain(field.assertions.iter()) {
        raw.push_str(&fact.subject);
        raw.push('\n');
        raw.push_str(&fact.predicate);
        raw.push('\n');
        raw.push_str(&fact.value);
        raw.push('\n');
        raw.push_str(&fact.authority);
        raw.push('\n');
    }
    for artifact in &field.artifacts {
        raw.push_str(&artifact.body);
        raw.push('\n');
    }
    forbidden_hit_count(raw.as_bytes(), model_keys)
}

fn keep_fact(fact: &Fact, hidden_keys: &[String]) -> bool {
    !tainted(&fact.subject, hidden_keys)
        && !tainted(&fact.predicate, hidden_keys)
        && !tainted(&fact.value, hidden_keys)
}

fn tainted(text: &str, hidden_keys: &[String]) -> bool {
    scrub_text(text, hidden_keys) != text
}

fn scrub_text(text: &str, hidden_keys: &[String]) -> String {
    let (mut out, _) = redact_secrets(text);
    for key in forbidden_competitive_keys() {
        if !key.is_empty() {
            out = out.replace(key, "");
        }
    }
    for key in hidden_keys {
        if !key.is_empty() {
            out = out.replace(key, "");
        }
    }
    remove_phrases(&out)
}

fn remove_phrases(text: &str) -> String {
    let mut out = text.to_string();
    for phrase in PHRASES {
        out = remove_ascii_case(&out, phrase);
    }
    out
}

fn remove_ascii_case(text: &str, phrase: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let phrase = phrase.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if lower[index..].starts_with(phrase.as_str()) {
            index += phrase.len();
            continue;
        }
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}
