use std::path::PathBuf;

use lastdown::core::drive::{loop_signal, AcceptanceGate, LoopSignal, Objective, ProgressMark};
use lastdown::core::field::{Artifact, DisclosureClass, FieldSnapshot};
use lastdown::core::state_machine::{transition, DriveStatus, MachineEvent};
use lastdown::core::DriveBudget;
use lastdown::error::{FailureClass, PhaseError, ReserveError};
use lastdown::ids::{ArtifactId, DriveId, FieldId, GateId};
use lastdown::provider::BoundarySignal;
use lastdown::punt::protocol::PuntProtocol;
use lastdown::punt::store::{ContinuationStore, FsStore, MutatingReadStore};
use lastdown::resource::{Probability, ResourceVector};
use lastdown::runway::decision::{decide, Choice, DecisionInput};
use lastdown::runway::reserve::estimate_reserve;
use lastdown::sentinel::{authorise, SpendClass};
use lastdown::sim::{execute, load_fixture, Kill, Mode};
use lastdown::{Drive, DriveStatus as Status};

fn token_drive(remaining_tokens: u64, hold_tokens: u64, status: Status) -> Drive {
    let mut granted = ResourceVector::splat(1_000_000);
    granted.tokens = remaining_tokens;
    let mut hold = ResourceVector::zero();
    hold.tokens = hold_tokens;
    let gates = vec![GateId::new("g").unwrap()];
    Drive {
        schema_version: lastdown::SCHEMA_VERSION,
        drive_id: DriveId::new("drv-reserve").unwrap(),
        generation: 1,
        objective: Objective {
            schema_version: lastdown::SCHEMA_VERSION,
            statement: "hold the reserve".into(),
            constraints: Vec::new(),
            required_capabilities: Vec::new(),
        },
        acceptance_gates: vec![AcceptanceGate {
            gate_id: gates[0].clone(),
            name: "g".into(),
            utility: 10,
            partial_utility: 4,
            required: true,
        }],
        field: FieldSnapshot::genesis(FieldId::new("fld-0").unwrap(), &gates),
        possession: None,
        budget: DriveBudget {
            granted,
            consumed: ResourceVector::zero(),
            reserve: lastdown::ContinuationReserve::zero(),
            reserve_hold: hold,
            lock_reserve: true,
        },
        status,
        attempt_used: false,
        preserving: false,
        failure: None,
        possession_marks: Vec::new(),
    }
}

#[test]
fn ordinary_execution_cannot_spend_the_reserve() {
    let mut drive = token_drive(1_000, 400, Status::Normal);
    let mut cost = ResourceVector::zero();
    cost.tokens = 600;
    assert!(authorise(&drive, SpendClass::Ordinary, &cost).is_ok());
    cost.tokens = 601;
    assert_eq!(
        authorise(&drive, SpendClass::Ordinary, &cost),
        Err(ReserveError::ReserveBoundary)
    );
    cost.tokens = 1_000;
    assert_eq!(
        authorise(&drive, SpendClass::Ordinary, &cost),
        Err(ReserveError::ReserveBoundary)
    );
    drive.budget.consumed.tokens = 600;
    cost.tokens = 1;
    assert_eq!(
        authorise(&drive, SpendClass::Ordinary, &cost),
        Err(ReserveError::ReserveBoundary)
    );
    drive.preserving = true;
    cost.tokens = 400;
    assert!(authorise(&drive, SpendClass::Continuation, &cost).is_ok());
    assert_eq!(
        authorise(
            &token_drive(1_000, 400, Status::Normal),
            SpendClass::Continuation,
            &cost
        ),
        Err(ReserveError::WrongPhase)
    );
}

#[test]
fn provider_exhaustion_is_rejected_as_turnover() {
    let error = transition(
        DriveStatus::Redzone,
        MachineEvent::Unrecoverable(FailureClass::ProviderExhausted),
    );
    assert_eq!(error, Err(PhaseError::ExhaustionIsNotTurnover));
    let signal = BoundarySignal::QuotaRejected;
    assert_eq!(
        FailureClass::from_signal(signal),
        FailureClass::ProviderExhausted
    );
}

#[test]
fn hail_mary_and_field_goal_are_distinct() {
    let ppm = |value| Probability::from_ppm(value).unwrap();
    let banned = decide(&DecisionInput {
        full_utility: 1_000,
        banked_utility: 0,
        p_score: ppm(400_000),
        p_preserve: ppm(980_000),
        p_finish_after_handoff: ppm(200_000),
        min_attempt: ppm(550_000),
        reset_bonus_ppm: 0,
    });
    assert_eq!(banned.chosen, Choice::Punt);
    assert!(banned.score_disqualified);
    let goal = decide(&DecisionInput {
        full_utility: 100,
        banked_utility: 40,
        p_score: Probability::ZERO,
        p_preserve: ppm(980_000),
        p_finish_after_handoff: Probability::ZERO,
        min_attempt: ppm(550_000),
        reset_bonus_ppm: 0,
    });
    assert_eq!(goal.chosen, Choice::FieldGoal);
}

#[test]
fn prepared_package_is_not_resumable_until_commit() {
    let fixture = load_fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures/token_budget.json"),
    )
    .unwrap();
    let campaign = execute(&fixture, Mode::Lastdown, Kill::ImmediatelyAfterCommit).unwrap();
    let generation = campaign.result.committed_generation.unwrap();
    assert!(campaign
        .store
        .load_committed(campaign.drive_id.as_str(), generation)
        .is_ok());
    assert!(campaign
        .store
        .load_committed(campaign.drive_id.as_str(), generation + 9)
        .is_err());
}

#[test]
fn mutated_readback_does_not_release_possession() {
    let mut drive = token_drive(5_000, 800, Status::Punt);
    drive.possession = Some(lastdown::Possession {
        schema_version: lastdown::SCHEMA_VERSION,
        model: lastdown::ModelRoute {
            route_id: lastdown::ids::RouteId::new("route").unwrap(),
            model_key: "incumbent-alpha-2026-10".into(),
            capability: lastdown::core::CapabilityProfile::coding_default().unwrap(),
            trust_domain: lastdown::core::possession::TrustDomain::Local,
        },
        started_at_ms: 0,
        starting_field: drive.field.field_id.clone(),
        started_completed: 0,
        budget: lastdown::core::possession::PossessionBudget {
            schema_version: lastdown::SCHEMA_VERSION,
            granted_tokens: 5_000,
        },
        policy: lastdown::core::possession::LastdownPolicy::default(),
    });
    drive.field.artifacts.push(Artifact::new(
        ArtifactId::new("art-secret").unwrap(),
        "tool",
        DisclosureClass::Internal,
        "token=sk-live-abcdefghij",
    ));
    let mut protocol = PuntProtocol {
        store: MutatingReadStore::new(),
        log: lastdown::telemetry::TelemetryLog::default(),
        now_ms: 1,
        ledger: lastdown::padagonia::FieldLedger::default(),
    };
    let error = protocol.execute(&mut drive, lastdown::core::possession::TrustDomain::Local);
    assert!(error.is_err());
    assert!(drive.possession.is_some());
    assert_eq!(protocol.store.committed_len(), 0);
    assert_ne!(drive.status, Status::Turnover);
}

#[test]
fn foreign_destination_cannot_receive_a_restricted_artifact() {
    let mut drive = token_drive(5_000, 800, Status::Punt);
    drive.field.artifacts.push(Artifact::new(
        ArtifactId::new("art-restricted").unwrap(),
        "source",
        DisclosureClass::Restricted,
        "customer table",
    ));
    let mut protocol = PuntProtocol {
        store: lastdown::punt::MemoryStore::default(),
        log: lastdown::telemetry::TelemetryLog::default(),
        now_ms: 1,
        ledger: lastdown::padagonia::FieldLedger::default(),
    };
    let error = protocol.execute(
        &mut drive,
        lastdown::core::possession::TrustDomain::ForeignProvider,
    );
    assert!(error.is_err());
    assert!(drive.possession.is_none() || protocol.store.committed_count_for("drv-reserve") == 0);
    assert_eq!(protocol.store.committed_count_for("drv-reserve"), 0);
}

#[test]
fn filesystem_store_survives_a_new_handle() {
    let fixture = load_fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures/token_budget.json"),
    )
    .unwrap();
    let campaign = execute(&fixture, Mode::Lastdown, Kill::ImmediatelyAfterCommit).unwrap();
    let object = campaign
        .store
        .latest_committed(campaign.drive_id.as_str())
        .unwrap()
        .unwrap();
    let root = std::env::temp_dir().join(format!("lastdown-fs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut store = FsStore::new(&root);
    store.prepare(&object).unwrap();
    assert!(store
        .load_committed(
            object.identity.drive_id.as_str(),
            object.identity.generation
        )
        .is_err());
    store
        .commit(&object.identity.drive_id, object.identity.generation)
        .unwrap();
    let reopened = FsStore::new(&root);
    let loaded = reopened
        .latest_committed(&object.identity.drive_id)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.integrity.object_hash, object.integrity.object_hash);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn punt_loop_reassesses_instead_of_turning_over() {
    let fixture = load_fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures/punt_loop.json"),
    )
    .unwrap();
    let campaign = execute(&fixture, Mode::Lastdown, Kill::None).unwrap();
    assert!(campaign.result.reassessment);
    assert!(!campaign.result.turnover);
    assert!(campaign.result.committed_packages <= 3);
    assert_eq!(campaign.result.committed_packages, 3);
}

#[test]
fn neutral_marks_request_reassessment() {
    let marks = [
        ProgressMark::Neutral,
        ProgressMark::Neutral,
        ProgressMark::Neutral,
    ];
    assert_eq!(loop_signal(&marks), LoopSignal::Reassess);
}

#[test]
fn contradictions_survive_and_projection_keeps_both_sides() {
    let fixture = load_fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures/token_budget.json"),
    )
    .unwrap();
    let campaign = execute(&fixture, Mode::Lastdown, Kill::None).unwrap();
    assert!(campaign.result.contradictions >= 1);
    let object = campaign
        .store
        .latest_committed(campaign.drive_id.as_str())
        .unwrap()
        .unwrap();
    let field = lastdown::punt::continuation::reconstruct_field(&object).unwrap();
    let projection = lastdown::padagonia::project(&field);
    assert!(projection
        .claims
        .iter()
        .any(|claim| claim.kind == "observation"));
    assert!(projection
        .claims
        .iter()
        .any(|claim| claim.kind == "assertion"));
    assert_eq!(projection.contradictions.len(), field.contradictions.len());
    assert!(estimate_reserve(&field).token_total() > 0);
}

#[test]
fn boundary_signal_preserves_instead_of_turnover() {
    let fixture = load_fixture(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures/provider_loss.json"),
    )
    .unwrap();
    let report_down = execute(&fixture, Mode::Lastdown, Kill::None).unwrap();
    let report_base = execute(&fixture, Mode::Baseline, Kill::None).unwrap();
    assert!(!report_down.result.turnover);
    assert_eq!(report_down.result.context_losses, 0);
    assert!(report_base.result.context_losses >= 1);
    assert!(report_down.result.scarce_lost <= report_base.result.scarce_lost);
}
