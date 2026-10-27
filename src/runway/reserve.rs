use crate::core::field::FieldSnapshot;
use crate::resource::ContinuationReserve;
use crate::schema::SCHEMA_VERSION;

/// Base reserve for a field that is only reasoning, with no artifacts.
pub const BASE_SERIALIZATION: u64 = 400;
pub const TOKENS_PER_FACT: u64 = 40;
pub const TOKENS_PER_UNCOMMITTED: u64 = 180;
pub const BYTES_PER_TOKEN: u64 = 4;
pub const BASE_VERIFICATION: u64 = 120;
pub const BASE_COORDINATOR: u64 = 80;
pub const BASE_PERSISTENCE: u64 = 80;
pub const BASE_SAFETY: u64 = 120;

/// Grow the reserve with artifacts, facts, and uncommitted side effects.
///
/// A coding field with a dirty worktree holds a larger reserve than a pure
/// reasoning field of the same gate count.
pub fn estimate_reserve(field: &FieldSnapshot) -> ContinuationReserve {
    let artifact_bytes: u64 = field
        .artifacts
        .iter()
        .map(|artifact| artifact.byte_len)
        .sum();
    let uncommitted = field
        .side_effects
        .iter()
        .filter(|effect| !effect.committed)
        .count() as u64;
    let facts = (field.verified_facts.len() + field.assertions.len()) as u64;
    let serialization_tokens = BASE_SERIALIZATION
        .saturating_add(artifact_bytes / BYTES_PER_TOKEN)
        .saturating_add(facts.saturating_mul(TOKENS_PER_FACT))
        .saturating_add(uncommitted.saturating_mul(TOKENS_PER_UNCOMMITTED));
    ContinuationReserve {
        schema_version: SCHEMA_VERSION,
        serialization_tokens,
        verification_budget: BASE_VERIFICATION.saturating_add(uncommitted.saturating_mul(40)),
        coordinator_budget: BASE_COORDINATOR,
        persistence_budget: BASE_PERSISTENCE.saturating_add(artifact_bytes / 64),
        safety_margin: BASE_SAFETY.saturating_add(uncommitted.saturating_mul(20)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::field::{Artifact, DisclosureClass, FieldSnapshot, SideEffect};
    use crate::ids::{ArtifactId, EffectId, FieldId, GateId};

    fn empty() -> FieldSnapshot {
        FieldSnapshot::genesis(FieldId::new("fld-0").unwrap(), &[GateId::new("g").unwrap()])
    }

    #[test]
    fn uncommitted_work_grows_the_reserve() {
        let reasoning = estimate_reserve(&empty());
        let mut dirty = empty();
        dirty.artifacts.push(Artifact::new(
            ArtifactId::new("a").unwrap(),
            "patch",
            DisclosureClass::Internal,
            "x".repeat(400),
        ));
        dirty.side_effects.push(SideEffect::new(
            EffectId::new("e").unwrap(),
            "filesystem",
            false,
            "diff",
        ));
        let coding = estimate_reserve(&dirty);
        assert!(coding.token_total() > reasoning.token_total());
    }
}
