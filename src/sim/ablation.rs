//! Ablation of the reserve, athletic amnesia, and calibration.
//!
//! Each row compares the system with the mechanism to the same measurement
//! with that mechanism removed. Boundary fixtures compare Lastdown with the
//! reserve to the baseline that spends until the provider stops.

use std::fs;
use std::path::Path;

use super::runner::{execute, load_fixture, Fixture, Kill, Mode, SimError};
use crate::core::drive::Objective;
use crate::core::field::{Artifact, ClaimKind, DisclosureClass, Fact, FieldSnapshot};
use crate::error::FixtureError;
use crate::ids::{ArtifactId, ClaimId, FieldId, GateId};
use crate::learning::reference_outcome;
use crate::schema::SCHEMA_VERSION;
use crate::scouting::measure_amnesia;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MechanismGap {
    pub name: &'static str,
    pub with_mechanism: u64,
    pub without_mechanism: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureComparison {
    pub name: String,
    pub lastdown_lost: u64,
    pub baseline_lost: u64,
    pub lastdown_consumed: u64,
    pub baseline_consumed: u64,
    pub lastdown_scored: bool,
    pub baseline_scored: bool,
    pub lastdown_duplicate: u32,
    pub baseline_duplicate: u32,
    pub lastdown_context_losses: u32,
    pub baseline_context_losses: u32,
    pub lastdown_gates: usize,
    pub baseline_gates: usize,
    pub lastdown_continuation_bytes: usize,
    pub baseline_continuation_bytes: usize,
    pub lastdown_turnover: bool,
    pub baseline_turnover: bool,
    pub lastdown_quota_remaining: Option<u64>,
    pub baseline_quota_remaining: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AblationReport {
    pub fixtures: Vec<FixtureComparison>,
    pub reserve: MechanismGap,
    pub amnesia: MechanismGap,
    pub calibration: MechanismGap,
}

pub fn load_fixture_dir(directory: &Path) -> Result<Vec<Fixture>, SimError> {
    let mut paths = Vec::new();
    let entries = fs::read_dir(directory)
        .map_err(|error| SimError::FixtureFile(FixtureError::Io(error.to_string())))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| SimError::FixtureFile(FixtureError::Io(error.to_string())))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    let mut fixtures = Vec::with_capacity(paths.len());
    for path in paths {
        fixtures.push(load_fixture(path)?);
    }
    Ok(fixtures)
}

pub fn ablation_report(fixtures: &[Fixture]) -> Result<AblationReport, SimError> {
    let mut rows = Vec::with_capacity(fixtures.len());
    for fixture in fixtures {
        let lastdown = execute(fixture, Mode::Lastdown, Kill::None)?;
        let baseline = execute(fixture, Mode::Baseline, Kill::None)?;
        rows.push(FixtureComparison {
            name: fixture.name.clone(),
            lastdown_lost: lastdown.result.scarce_lost,
            baseline_lost: baseline.result.scarce_lost,
            lastdown_consumed: lastdown.result.scarce_consumed,
            baseline_consumed: baseline.result.scarce_consumed,
            lastdown_scored: lastdown.result.scored,
            baseline_scored: baseline.result.scored,
            lastdown_duplicate: lastdown.result.duplicate_steps,
            baseline_duplicate: baseline.result.duplicate_steps,
            lastdown_context_losses: lastdown.result.context_losses,
            baseline_context_losses: baseline.result.context_losses,
            lastdown_gates: lastdown.result.completed_gates,
            baseline_gates: baseline.result.completed_gates,
            lastdown_continuation_bytes: lastdown.result.continuation_bytes,
            baseline_continuation_bytes: baseline.result.continuation_bytes,
            lastdown_turnover: lastdown.result.turnover,
            baseline_turnover: baseline.result.turnover,
            lastdown_quota_remaining: lastdown.result.quota_remaining,
            baseline_quota_remaining: baseline.result.quota_remaining,
        });
    }
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    let with_reserve: u64 = rows.iter().map(|row| row.lastdown_lost).sum();
    let without_reserve: u64 = rows.iter().map(|row| row.baseline_lost).sum();
    let (clean, raw) = measure_amnesia(&poisoned_field(), &poisoned_objective(), &[predecessor()])
        .map_err(|_| SimError::Fixture("amnesia"))?;
    let reference = reference_outcome();
    Ok(AblationReport {
        fixtures: rows,
        reserve: MechanismGap {
            name: "reserve",
            with_mechanism: with_reserve,
            without_mechanism: without_reserve,
        },
        amnesia: MechanismGap {
            name: "amnesia",
            with_mechanism: u64::try_from(clean).unwrap_or(u64::MAX),
            without_mechanism: u64::try_from(raw).unwrap_or(u64::MAX),
        },
        calibration: MechanismGap {
            name: "calibration",
            with_mechanism: reference.effective_mae,
            without_mechanism: reference.nominal_mae,
        },
    })
}

pub fn ablate_dir(directory: &Path) -> Result<AblationReport, SimError> {
    let fixtures = load_fixture_dir(directory)?;
    ablation_report(&fixtures)
}

fn predecessor() -> String {
    "incumbent-alpha-2026-10".to_string()
}

fn poisoned_objective() -> Objective {
    Objective {
        schema_version: SCHEMA_VERSION,
        statement: "Land the patch with the suite green.".to_string(),
        constraints: Vec::new(),
        required_capabilities: Vec::new(),
    }
}

fn poisoned_field() -> FieldSnapshot {
    let gate = GateId::new("g1").expect("gate");
    let mut field = FieldSnapshot::genesis(FieldId::new("fld-poison").expect("field"), &[gate]);
    field.verified_facts.push(Fact {
        claim_id: ClaimId::new("obs-green").expect("claim"),
        kind: ClaimKind::Observation,
        subject: "suite".to_string(),
        predicate: "status".to_string(),
        value: "green".to_string(),
        authority: predecessor(),
        authority_rank: 1,
    });
    field.assertions.push(Fact {
        claim_id: ClaimId::new("assert-opinion").expect("claim"),
        kind: ClaimKind::Assertion,
        subject: "drive".to_string(),
        predicate: "previous_ytg".to_string(),
        value: "3".to_string(),
        authority: predecessor(),
        authority_rank: 1,
    });
    field.artifacts.push(Artifact::new(
        ArtifactId::new("art-note").expect("artifact"),
        "note",
        DisclosureClass::Internal,
        format!("notes: {} nearly finished", predecessor()),
    ));
    field
}
