use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::core::drive::{
    loop_signal, AcceptanceGate, CapabilityNeed, Constraint, Drive, DriveBudget, LoopSignal,
    Objective,
};
use crate::core::field::{
    satisfy_gate, Artifact, ClaimKind, Contradiction, DisclosureClass, Fact, FieldSnapshot,
    SideEffect, TestRecord,
};
use crate::core::possession::{
    CapabilityProfile, LastdownPolicy, ModelRoute, Possession, PossessionBudget, TrustDomain,
};
use crate::core::state_machine::DriveStatus;
use crate::error::{
    FieldError, FixtureError, IdError, PhaseError, PuntError, RangeError, ReserveError,
};
use crate::ids::{ArtifactId, ClaimId, DriveId, EffectId, FieldId, GateId, RouteId};
use crate::padagonia::FieldLedger;
use crate::provider::BoundarySignal;
use crate::punt::continuation::{canonical_bytes, validate};
use crate::punt::replay::ResumeRequest;
use crate::punt::store::{ContinuationStore, MemoryStore};
use crate::resource::{ContinuationReserve, Probability, ResourceDim, ResourceVector};
use crate::runway::RunwayInputs;
use crate::schema::SCHEMA_VERSION;
use crate::sentinel::{authorise, Directive, Sentinel, SpendClass};

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("fixture: {0}")]
    Fixture(&'static str),
    #[error(transparent)]
    FixtureFile(#[from] FixtureError),
    #[error(transparent)]
    Phase(#[from] PhaseError),
    #[error(transparent)]
    Punt(#[from] PuntError),
    #[error(transparent)]
    Reserve(#[from] ReserveError),
    #[error(transparent)]
    Field(#[from] FieldError),
    #[error(transparent)]
    Id(#[from] IdError),
    #[error(transparent)]
    Range(#[from] RangeError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kill {
    None,
    ImmediatelyAfterCommit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Lastdown,
    Baseline,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub schema_version: u32,
    pub name: String,
    pub scarce: ResourceDim,
    pub grant: u64,
    pub reserve: u64,
    pub other_grant: u64,
    pub velocity_tokens_per_sec: u64,
    pub provider_health_ppm: u32,
    pub rate_limit_headroom_ppm: u32,
    pub nominal_ytg_milli: u32,
    pub historical_ytg_error_milli: i32,
    pub objective: String,
    pub constraint_key: String,
    pub constraint_value: String,
    pub gates: Vec<GateSpec>,
    pub steps: Vec<StepSpec>,
    #[serde(default)]
    pub fault_at_step: Option<usize>,
    #[serde(default)]
    pub fault: Option<BoundarySignal>,
    pub kill: Kill,
    pub destination: TrustDomain,
    pub min_attempt_ppm: u32,
    pub reset_bonus_ppm: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSpec {
    pub id: String,
    pub name: String,
    pub utility: u64,
    pub partial_utility: u64,
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepSpec {
    pub gate: String,
    pub cost: u64,
    pub body: String,
    #[serde(default)]
    pub observation: Option<String>,
    #[serde(default)]
    pub assertion: Option<String>,
    #[serde(default)]
    pub uncommitted: bool,
    #[serde(default)]
    pub passing_test: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimResult {
    pub fixture: String,
    pub mode: &'static str,
    pub status: DriveStatus,
    pub scored: bool,
    pub field_goal: bool,
    pub turnover: bool,
    pub committed_packages: usize,
    pub committed_generation: Option<u64>,
    pub replay_ok: bool,
    pub scarce_consumed: u64,
    pub scarce_lost: u64,
    pub artifacts: usize,
    pub contradictions: usize,
    pub observations: usize,
    pub context_losses: u32,
    pub duplicate_steps: u32,
    pub continuation_bytes: usize,
    pub summary_bytes: usize,
    pub dirty: bool,
    pub frozen_effects: usize,
    pub reassessment: bool,
    pub completed_gates: usize,
    pub events: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchReport {
    pub lastdown: SimResult,
    pub baseline: SimResult,
}

pub struct Campaign {
    pub result: SimResult,
    pub store: MemoryStore,
    pub grant: ResourceVector,
    pub reserve: ContinuationReserve,
    pub reserve_hold: ResourceVector,
    pub policy: LastdownPolicy,
    pub drive_id: DriveId,
    pub ledger: FieldLedger,
}

struct Material {
    grant: ResourceVector,
    hold: ResourceVector,
    reserve: ContinuationReserve,
    policy: LastdownPolicy,
    steps: Vec<PlannedStep>,
    observed_per_gate: ResourceVector,
}

struct PlannedStep {
    gate: GateId,
    cost: ResourceVector,
    scarce_cost: u64,
    body: String,
    observation: Option<String>,
    assertion: Option<String>,
    uncommitted: bool,
    passing_test: Option<bool>,
    fault: Option<BoundarySignal>,
}

struct Spend {
    gate: GateId,
    units: u64,
}

pub fn load_fixture(path: impl AsRef<Path>) -> Result<Fixture, FixtureError> {
    let text =
        fs::read_to_string(path.as_ref()).map_err(|error| FixtureError::Io(error.to_string()))?;
    let fixture: Fixture =
        serde_json::from_str(&text).map_err(|error| FixtureError::Json(error.to_string()))?;
    if fixture.schema_version != SCHEMA_VERSION {
        return Err(FixtureError::Schema(fixture.schema_version));
    }
    if fixture.gates.is_empty() || fixture.steps.is_empty() {
        return Err(FixtureError::Invariant("empty drive"));
    }
    if fixture.fault.is_some() != fixture.fault_at_step.is_some() {
        return Err(FixtureError::Invariant("fault"));
    }
    for ppm in [
        fixture.provider_health_ppm,
        fixture.rate_limit_headroom_ppm,
        fixture.min_attempt_ppm,
    ] {
        if ppm > 1_000_000 {
            return Err(FixtureError::Invariant("probability"));
        }
    }
    Ok(fixture)
}

pub fn bench(fixture: &Fixture) -> Result<BenchReport, SimError> {
    Ok(BenchReport {
        lastdown: execute(fixture, Mode::Lastdown, Kill::None)?.result,
        baseline: execute(fixture, Mode::Baseline, Kill::None)?.result,
    })
}

pub fn execute(fixture: &Fixture, mode: Mode, kill: Kill) -> Result<Campaign, SimError> {
    let kill = if kill == Kill::None {
        fixture.kill
    } else {
        kill
    };
    match mode {
        Mode::Lastdown => run_lastdown(fixture, kill),
        Mode::Baseline => run_baseline(fixture),
    }
}

pub fn run_lastdown(fixture: &Fixture, kill: Kill) -> Result<Campaign, SimError> {
    let (mut drive, material) = materialise(fixture)?;
    let drive_id = drive.drive_id.clone();
    let mut sentinel = Sentinel::new(MemoryStore::default(), fixture.destination, 0);
    sentinel.start(&mut drive)?;
    let mut cursor = 0usize;
    let mut seq = 0u32;
    let mut cleared = HashSet::new();
    let mut marks = Vec::new();
    let mut spent = Vec::new();
    let mut cycles = 0u32;
    let mut reassessment = false;
    while cursor < material.steps.len() && cycles < 32 {
        cycles += 1;
        if drive.required_gates_complete() {
            break;
        }
        let inputs = runway_inputs(fixture, &drive, &material, cursor);
        let observation = sentinel.observe(&mut drive, &inputs)?;
        match observation.directive {
            Directive::Continue | Directive::EnteredRedzone | Directive::Attempt => {
                if material.steps[cursor].fault.is_some() && !cleared.contains(&cursor) {
                    cleared.insert(cursor);
                    let signal = material.steps[cursor]
                        .fault
                        .unwrap_or(BoundarySignal::Timeout);
                    if signal == BoundarySignal::PartialToolExecution {
                        charge(&mut drive, &material.steps[cursor], &mut seq, &mut spent)?;
                        cursor += 1;
                    }
                    sentinel.emergency(&mut drive, signal)?;
                    if stop_after_commit(&mut marks, &drive, kill, &mut reassessment, &mut sentinel)
                    {
                        break;
                    }
                    drive = fresh_possession(&mut sentinel, &drive, &material)?;
                    continue;
                }
                charge(&mut drive, &material.steps[cursor], &mut seq, &mut spent)?;
                sentinel.note_advance(&drive, "positive_advance");
                cursor += 1;
            }
            Directive::Punt | Directive::Reset | Directive::FieldGoal => {
                let field_goal = observation.directive == Directive::FieldGoal;
                sentinel.preserve(&mut drive)?;
                if stop_after_commit(&mut marks, &drive, kill, &mut reassessment, &mut sentinel)
                    || field_goal
                {
                    break;
                }
                drive = fresh_possession(&mut sentinel, &drive, &material)?;
            }
            Directive::Score | Directive::Turnover => break,
        }
    }
    if drive.required_gates_complete() && drive.status != DriveStatus::Score {
        let inputs = runway_inputs(fixture, &drive, &material, cursor);
        let _ = sentinel.observe(&mut drive, &inputs)?;
    }
    let ledger = sentinel.ledger().clone();
    let events = sentinel
        .log()
        .kinds()
        .into_iter()
        .map(str::to_string)
        .collect();
    let store = sentinel.into_store();
    pack(PackArgs {
        fixture,
        mode: "lastdown",
        events,
        store,
        drive: &drive,
        spent: &spent,
        duplicate_steps: 0,
        reassessment,
        drive_id,
        material: &material,
        ledger,
    })
}

fn stop_after_commit(
    marks: &mut Vec<crate::core::drive::ProgressMark>,
    drive: &Drive,
    kill: Kill,
    reassessment: &mut bool,
    sentinel: &mut Sentinel<MemoryStore>,
) -> bool {
    if let Some(mark) = drive.possession_marks.last().copied() {
        marks.push(mark);
    }
    let signal = loop_signal(marks);
    if signal != LoopSignal::None {
        *reassessment = true;
        sentinel.note_loop(drive, signal);
        return true;
    }
    kill == Kill::ImmediatelyAfterCommit
}

fn charge(
    drive: &mut Drive,
    step: &PlannedStep,
    seq: &mut u32,
    spent: &mut Vec<Spend>,
) -> Result<(), SimError> {
    authorise(drive, SpendClass::Ordinary, &step.cost)?;
    apply_step(drive, step, seq)?;
    spent.push(Spend {
        gate: step.gate.clone(),
        units: step.scarce_cost,
    });
    Ok(())
}

fn fresh_possession(
    sentinel: &mut Sentinel<MemoryStore>,
    drive: &Drive,
    material: &Material,
) -> Result<Drive, SimError> {
    let generation = drive.generation.saturating_sub(1);
    let now_ms = sentinel
        .log()
        .events()
        .last()
        .map(|event| event.at_ms)
        .unwrap_or(0);
    let request = ResumeRequest {
        route: ModelRoute {
            route_id: RouteId::new("route-receiver")?,
            model_key: "receiver-beta-2026-10".to_string(),
            capability: CapabilityProfile::coding_default()?,
            trust_domain: TrustDomain::Local,
        },
        grant: material.grant,
        reserve: material.reserve.clone(),
        reserve_hold: material.hold,
        policy: material.policy.clone(),
        now_ms,
    };
    Ok(sentinel.resume(&drive.drive_id, generation, request)?)
}

fn run_baseline(fixture: &Fixture) -> Result<Campaign, SimError> {
    let (drive, material) = materialise(fixture)?;
    let drive_id = drive.drive_id.clone();
    let mut seen_before_loss: HashSet<String> = HashSet::new();
    let mut spent = Vec::new();
    let mut duplicate_steps = 0u32;
    let mut context_losses = 0u32;
    let mut lost_units = 0u64;
    let mut seq = 0u32;
    let mut final_drive = drive.clone();
    let mut scored = false;
    for attempt in 0..2 {
        let mut attempt_drive = drive.clone();
        attempt_drive.status = DriveStatus::Normal;
        let mut failed = false;
        let spent_at_start = spent.len();
        for step in &material.steps {
            if attempt == 0 && step.fault.is_some() {
                if step.fault == Some(BoundarySignal::PartialToolExecution) {
                    apply_step(&mut attempt_drive, step, &mut seq)?;
                    spent.push(Spend {
                        gate: step.gate.clone(),
                        units: step.scarce_cost,
                    });
                }
                failed = true;
                break;
            }
            let used = attempt_drive.budget.consumed.get(fixture.scarce);
            if used.saturating_add(step.scarce_cost) > fixture.grant {
                failed = true;
                break;
            }
            if attempt > 0 && seen_before_loss.contains(step.gate.as_str()) {
                duplicate_steps += 1;
            }
            apply_step(&mut attempt_drive, step, &mut seq)?;
            spent.push(Spend {
                gate: step.gate.clone(),
                units: step.scarce_cost,
            });
        }
        if failed {
            context_losses += 1;
            lost_units += spent[spent_at_start..]
                .iter()
                .map(|item| item.units)
                .sum::<u64>();
            for item in &spent {
                seen_before_loss.insert(item.gate.as_str().to_string());
            }
            // The dead context takes its field with it. Spend from this
            // attempt stays in `spent` so the lost units are still counted,
            // but the gates are not recovered.
            continue;
        }
        final_drive = attempt_drive;
        final_drive.status = if final_drive.required_gates_complete() {
            scored = true;
            DriveStatus::Score
        } else {
            DriveStatus::Normal
        };
        break;
    }
    if !scored {
        final_drive.status = DriveStatus::Normal;
        final_drive.field = FieldSnapshot::genesis(
            FieldId::new("fld-lost")?,
            &final_drive
                .acceptance_gates
                .iter()
                .map(|gate| gate.gate_id.clone())
                .collect::<Vec<_>>(),
        );
        // Genesis puts every gate back to unresolved. That is the summary.
    }
    let summary = "Partial progress was made before the provider boundary.";
    let mut campaign = pack(PackArgs {
        fixture,
        mode: "baseline",
        events: Vec::new(),
        store: MemoryStore::default(),
        drive: &final_drive,
        spent: &spent,
        duplicate_steps,
        reassessment: false,
        drive_id,
        material: &material,
        ledger: FieldLedger::default(),
    })?;
    campaign.result.context_losses = context_losses;
    campaign.result.scarce_lost = lost_units;
    campaign.result.summary_bytes = summary.len();
    campaign.result.scored = scored;
    campaign.result.status = final_drive.status;
    if !scored {
        campaign.result.artifacts = 0;
        campaign.result.contradictions = 0;
        campaign.result.observations = 0;
        campaign.result.completed_gates = 0;
        campaign.result.dirty = false;
        campaign.result.frozen_effects = 0;
        campaign.result.scarce_lost = campaign.result.scarce_consumed;
    }
    Ok(campaign)
}

struct PackArgs<'a> {
    fixture: &'a Fixture,
    mode: &'static str,
    events: Vec<String>,
    store: MemoryStore,
    drive: &'a Drive,
    spent: &'a [Spend],
    duplicate_steps: u32,
    reassessment: bool,
    drive_id: DriveId,
    material: &'a Material,
    ledger: FieldLedger,
}

fn pack(args: PackArgs<'_>) -> Result<Campaign, SimError> {
    let PackArgs {
        fixture,
        mode,
        events,
        store,
        drive,
        spent,
        duplicate_steps,
        reassessment,
        drive_id,
        material,
        ledger,
    } = args;
    let latest = store.latest_committed(drive_id.as_str())?;
    let (replay_ok, continuation_bytes, generation) = match &latest {
        Some(object) => {
            let bytes = canonical_bytes(object)?;
            (
                validate(object).is_ok(),
                bytes.len(),
                Some(object.identity.generation),
            )
        }
        None => (false, 0, None),
    };
    let recovered_gates: HashSet<String> = drive
        .field
        .completed_gates
        .iter()
        .map(|gate| gate.as_str().to_string())
        .collect();
    let scarce_consumed = spent.iter().map(|item| item.units).sum();
    let scarce_lost = spent
        .iter()
        .filter(|item| !recovered_gates.contains(item.gate.as_str()))
        .map(|item| item.units)
        .sum();
    let result = SimResult {
        fixture: fixture.name.clone(),
        mode,
        status: drive.status,
        scored: drive.status == DriveStatus::Score,
        field_goal: drive.status == DriveStatus::FieldGoal,
        turnover: drive.status == DriveStatus::Turnover,
        committed_packages: store.committed_count_for(drive_id.as_str()),
        committed_generation: generation,
        replay_ok,
        scarce_consumed,
        scarce_lost,
        artifacts: drive.field.artifacts.len(),
        contradictions: drive.field.contradictions.len(),
        observations: drive.field.verified_facts.len(),
        context_losses: 0,
        duplicate_steps,
        continuation_bytes,
        summary_bytes: 0,
        dirty: drive.field.repository.dirty,
        frozen_effects: drive
            .field
            .side_effects
            .iter()
            .filter(|effect| effect.frozen)
            .count(),
        reassessment,
        completed_gates: drive.field.completed_gates.len(),
        events,
    };
    Ok(Campaign {
        result,
        store,
        grant: material.grant,
        reserve: material.reserve.clone(),
        reserve_hold: material.hold,
        policy: material.policy.clone(),
        drive_id,
        ledger,
    })
}

fn runway_inputs(
    fixture: &Fixture,
    drive: &Drive,
    material: &Material,
    cursor: usize,
) -> RunwayInputs {
    let next_cost = material
        .steps
        .get(cursor)
        .map(|step| step.cost)
        .unwrap_or_else(ResourceVector::zero);
    RunwayInputs {
        remaining: drive.budget.remaining(),
        reserve_hold: drive.budget.reserve_hold,
        observed_per_gate: material.observed_per_gate,
        gates_remaining: drive.gates_remaining(),
        nominal_ytg_milli: fixture.nominal_ytg_milli,
        historical_ytg_error_milli: fixture.historical_ytg_error_milli,
        recent_token_velocity_per_sec: fixture.velocity_tokens_per_sec,
        tool_call_cost: ResourceVector::zero(),
        expected_tool_latency_ms: 0,
        provider_health: Probability::from_ppm(fixture.provider_health_ppm)
            .unwrap_or(Probability::ONE),
        rate_limit_headroom: Probability::from_ppm(fixture.rate_limit_headroom_ppm)
            .unwrap_or(Probability::ONE),
        next_cost,
        fresh_grant: material.grant,
    }
}

fn materialise(fixture: &Fixture) -> Result<(Drive, Material), SimError> {
    let mut grant = ResourceVector::splat(fixture.other_grant);
    grant.set(fixture.scarce, fixture.grant);
    let mut hold = ResourceVector::zero();
    hold.tokens = fixture.reserve;
    hold.set(fixture.scarce, fixture.reserve);
    let reserve = ContinuationReserve {
        schema_version: SCHEMA_VERSION,
        serialization_tokens: fixture.reserve,
        verification_budget: 0,
        coordinator_budget: 0,
        persistence_budget: 0,
        safety_margin: 0,
    };
    let policy = LastdownPolicy {
        schema_version: SCHEMA_VERSION,
        redzone_ppm: 250_000,
        lastdown_ppm: 550_000,
        min_attempt_ppm: fixture.min_attempt_ppm,
        fresh_context_success_ppm: 980_000,
        reset_bonus_ppm: fixture.reset_bonus_ppm,
    };
    policy.validated()?;
    let gates = fixture
        .gates
        .iter()
        .map(|gate| {
            Ok(AcceptanceGate {
                gate_id: GateId::new(gate.id.clone())?,
                name: gate.name.clone(),
                utility: gate.utility,
                partial_utility: gate.partial_utility,
                required: gate.required,
            })
        })
        .collect::<Result<Vec<_>, SimError>>()?;
    let mut steps = Vec::new();
    for (index, step) in fixture.steps.iter().enumerate() {
        let mut cost = ResourceVector::zero();
        cost.set(fixture.scarce, step.cost);
        let fault = if fixture.fault_at_step == Some(index) {
            fixture.fault
        } else {
            None
        };
        steps.push(PlannedStep {
            gate: GateId::new(step.gate.clone())?,
            cost,
            scarce_cost: step.cost,
            body: step.body.clone(),
            observation: step.observation.clone(),
            assertion: step.assertion.clone(),
            uncommitted: step.uncommitted,
            passing_test: step.passing_test,
            fault,
        });
    }
    let observed_per_gate = steps
        .first()
        .map(|step| step.cost)
        .unwrap_or_else(ResourceVector::zero);
    let gate_ids: Vec<GateId> = gates.iter().map(|gate| gate.gate_id.clone()).collect();
    let drive = Drive {
        schema_version: SCHEMA_VERSION,
        drive_id: DriveId::new(format!("drv-{}", fixture.name))?,
        generation: 1,
        objective: Objective {
            schema_version: SCHEMA_VERSION,
            statement: fixture.objective.clone(),
            constraints: vec![Constraint {
                key: fixture.constraint_key.clone(),
                value: fixture.constraint_value.clone(),
            }],
            required_capabilities: vec![CapabilityNeed {
                name: "coding".to_string(),
                minimum: 500,
            }],
        },
        acceptance_gates: gates,
        field: FieldSnapshot::genesis(FieldId::new("fld-0")?, &gate_ids),
        possession: Some(Possession {
            schema_version: SCHEMA_VERSION,
            model: ModelRoute {
                route_id: RouteId::new("route-incumbent")?,
                model_key: "incumbent-alpha-2026-10".to_string(),
                capability: CapabilityProfile::coding_default()?,
                trust_domain: fixture.destination,
            },
            started_at_ms: 0,
            starting_field: FieldId::new("fld-0")?,
            started_completed: 0,
            budget: PossessionBudget {
                schema_version: SCHEMA_VERSION,
                granted_tokens: grant.tokens,
            },
            policy: policy.clone(),
        }),
        budget: DriveBudget {
            granted: grant,
            consumed: ResourceVector::zero(),
            reserve: reserve.clone(),
            reserve_hold: hold,
            lock_reserve: true,
        },
        status: DriveStatus::Kickoff,
        attempt_used: false,
        preserving: false,
        failure: None,
        possession_marks: Vec::new(),
    };
    Ok((
        drive,
        Material {
            grant,
            hold,
            reserve,
            policy,
            steps,
            observed_per_gate,
        },
    ))
}

fn apply_step(drive: &mut Drive, step: &PlannedStep, seq: &mut u32) -> Result<(), SimError> {
    *seq = seq.saturating_add(1);
    let id = *seq;
    let mut field = drive.field.child(FieldId::new(format!("fld-{id}"))?);
    satisfy_gate(&mut field, &step.gate)?;
    field.artifacts.push(Artifact::new(
        ArtifactId::new(format!("art-{id}"))?,
        "work",
        DisclosureClass::Internal,
        step.body.clone(),
    ));
    let observation_id = ClaimId::new(format!("obs-{id}"))?;
    let assertion_id = ClaimId::new(format!("assert-{id}"))?;
    if let Some(value) = &step.observation {
        field.verified_facts.push(Fact {
            claim_id: observation_id.clone(),
            kind: ClaimKind::Observation,
            subject: step.gate.as_str().to_string(),
            predicate: "observed".to_string(),
            value: value.clone(),
            authority: "evidence".to_string(),
            authority_rank: 50,
        });
    }
    if let Some(value) = &step.assertion {
        field.assertions.push(Fact {
            claim_id: assertion_id.clone(),
            kind: ClaimKind::Assertion,
            subject: step.gate.as_str().to_string(),
            predicate: "claimed".to_string(),
            value: value.clone(),
            authority: "incumbent".to_string(),
            authority_rank: 10,
        });
    }
    if step.observation.is_some() && step.assertion.is_some() {
        field.contradictions.push(Contradiction {
            id: format!("con-{id}"),
            left: observation_id,
            right: assertion_id,
        });
    }
    if let Some(passed) = step.passing_test {
        field.tests.push(TestRecord::new(
            format!("test-{id}"),
            step.gate.as_str(),
            passed,
            step.body.clone(),
        ));
    }
    if step.uncommitted {
        field.side_effects.push(SideEffect::new(
            EffectId::new(format!("fx-{id}"))?,
            "filesystem",
            false,
            step.body.clone(),
        ));
        field.refresh_repository_dirty();
    }
    field.check_invariants()?;
    drive.budget.consumed = drive.budget.consumed.saturating_add(step.cost);
    drive.field = field;
    Ok(())
}
