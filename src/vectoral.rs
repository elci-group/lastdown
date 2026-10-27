//! Integer field projection used only as a size comparison.
//!
//! The toungsten tree does not currently publish a library API, so this module
//! keeps a fixed integer projection of gate, evidence, and disclosure counts.
//! [`scalar_field_distance`] remains the distance replay and routing treat as
//! authoritative. Resume does not read the vector.

use serde::{Deserialize, Serialize};

use crate::core::field::{DisclosureClass, FieldSnapshot};
use crate::schema::SCHEMA_VERSION;

/// Resume reconstructs the field from the continuation object.
pub const VECTOR_REQUIRED_FOR_RESUME: bool = false;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldVector {
    pub schema_version: u32,
    pub completed_gates: u32,
    pub unresolved_gates: u32,
    pub partial_gates: u32,
    pub contradictions: u32,
    pub artifacts: u32,
    pub facts: u32,
    pub restricted: u32,
    pub dirty: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VectorBenchmark {
    pub scalar_distance: u32,
    pub vector_l1: u64,
    pub json_bytes: usize,
    pub vector_bytes: usize,
    pub vector_required_for_resume: bool,
}

pub fn scalar_field_distance(field: &FieldSnapshot) -> u32 {
    field.unresolved_gates.len() as u32 + field.partial_gates.len() as u32
}

pub fn encode(field: &FieldSnapshot) -> FieldVector {
    FieldVector {
        schema_version: SCHEMA_VERSION,
        completed_gates: field.completed_gates.len() as u32,
        unresolved_gates: field.unresolved_gates.len() as u32,
        partial_gates: field.partial_gates.len() as u32,
        contradictions: field.contradictions.len() as u32,
        artifacts: field.artifacts.len() as u32,
        facts: (field.verified_facts.len() + field.assertions.len()) as u32,
        restricted: field
            .artifacts
            .iter()
            .filter(|artifact| artifact.disclosure == DisclosureClass::Restricted)
            .count() as u32,
        dirty: u32::from(field.repository.dirty),
    }
}

pub fn benchmark(field: &FieldSnapshot) -> VectorBenchmark {
    let vector = encode(field);
    let json_bytes = serde_json::to_vec(field)
        .map(|bytes| bytes.len())
        .unwrap_or(0);
    let vector_bytes = serde_json::to_vec(&vector)
        .map(|bytes| bytes.len())
        .unwrap_or(0);
    let vector_l1 = u64::from(vector.completed_gates)
        + u64::from(vector.unresolved_gates)
        + u64::from(vector.partial_gates)
        + u64::from(vector.contradictions)
        + u64::from(vector.artifacts)
        + u64::from(vector.facts)
        + u64::from(vector.restricted)
        + u64::from(vector.dirty);
    VectorBenchmark {
        scalar_distance: scalar_field_distance(field),
        vector_l1,
        json_bytes,
        vector_bytes,
        vector_required_for_resume: VECTOR_REQUIRED_FOR_RESUME,
    }
}
