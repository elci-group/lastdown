use std::path::PathBuf;

use lastdown::coordinator::{
    headroom_ppm, select_route, CalibrationSample, CalibrationTable, RouteContext, RoutePolicy,
    RoutedScout,
};
use lastdown::core::drive::{loop_signal, AcceptanceGate, LoopSignal, Objective, ProgressMark};
use lastdown::core::field::{
    Artifact, ClaimKind, Contradiction, Corroboration, DisclosureClass, Fact, FieldSnapshot,
    SideEffect, Supersession,
};
use lastdown::core::possession::{CapabilityProfile, ModelRoute, TrustDomain};
use lastdown::core::state_machine::{transition, DriveStatus, MachineEvent};
use lastdown::core::DriveBudget;
use lastdown::error::{FailureClass, PhaseError, ReserveError};
use lastdown::ids::{ArtifactId, ClaimId, DriveId, EffectId, FieldId, GateId, RouteId};
use lastdown::learning::{
    reference_outcome, reference_table, AssessmentRecord, ErrorKind, BOAST_KEY, BOAST_NOMINAL,
    BOAST_OBSERVED, PLAIN_KEY, PLAIN_NOMINAL, PLAIN_OBSERVED, TASK_CLASS, TOOLSET,
};
use lastdown::provider::BoundarySignal;
use lastdown::punt::continuation::forbidden_competitive_keys;
use lastdown::resource::{ContinuationReserve, ResourceVector};
use lastdown::scouting::{
    complete_scouts, measure_amnesia, parse_scout_response, request_scouts, ScoutBinding,
    ScoutRequest, ScoutResponse,
};
use lastdown::sentinel::{authorise, SpendClass};
use lastdown::sim::{ablate_dir, execute, load_fixture, Kill, Mode};
use lastdown::strategy::{
    apply_dynamic_reserve, boundary_response, possession_plan, BoundaryResponse, PossessionPlan,
};
use lastdown::telemetry::kinds;
use lastdown::vectoral::benchmark;
use lastdown::{Drive, DriveStatus as Status, FieldLedger};

fn fixture(name: &str) -> lastdown::sim::Fixture {
    load_fixture(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name)).unwrap()
}

fn route(id: &str, key: &str, cost: u64, trust: TrustDomain) -> ModelRoute {
    let mut capability = CapabilityProfile::coding_default().unwrap();
    capability.cost_per_1k_tokens = cost;
    capability.latency_ms_p50 = 1_000;
    ModelRoute {
        route_id: RouteId::new(id).unwrap(),
        model_key: key.to_string(),
        capability,
        trust_domain: trust,
    }
}

fn scout(
    candidate_id: &str,
    family_id: &str,
    priority: u32,
    route: ModelRoute,
    nominal: u64,
    success_ppm: u32,
    restricted: bool,
) -> RoutedScout {
    RoutedScout {
        candidate_id: candidate_id.to_string(),
        family_id: family_id.to_string(),
        priority,
        route,
        nominal_ytg_milli: nominal,
        success_ppm,
        reported_confidence_ppm: 500_000,
        resource_tokens: 1_000,
        recovery_cost_milli: 0,
        reliability_ppm: 1_000_000,
        domain_suitability_ppm: 1_000_000,
        capability_headroom_ppm: 1_000_000,
        restricted_field: restricted,
    }
}

fn policy() -> RoutePolicy {
    RoutePolicy {
        lambda_latency: 0,
        mu_risk: 0,
        progress_value: 1_000_000,
        task_class: TASK_CLASS.to_string(),
        toolset: TOOLSET.to_string(),
    }
}

fn decide(
    table: &CalibrationTable,
    candidates: &[RoutedScout],
) -> lastdown::coordinator::RouteDecision {
    let drive_id = DriveId::new("drv-route").unwrap();
    let mut log = lastdown::telemetry::TelemetryLog::default();
    let mut context = RouteContext {
        drive_id: &drive_id,
        field_id: None,
        now_ms: 1,
        log: &mut log,
    };
    let decision = select_route(table, &policy(), candidates, &mut context).unwrap();
    assert!(log.contains(kinds::ROUTE_CALCULATED));
    decision
}

fn punt_drive() -> Drive {
    let gates = vec![GateId::new("g1").unwrap()];
    let mut field = FieldSnapshot::genesis(FieldId::new("fld-scout").unwrap(), &gates);
    field.verified_facts.push(Fact {
        claim_id: ClaimId::new("obs-green").unwrap(),
        kind: ClaimKind::Observation,
        subject: "suite".into(),
        predicate: "status".into(),
        value: "green".into(),
        authority: "incumbent-alpha-2026-10".into(),
        authority_rank: 1,
    });
    field.assertions.push(Fact {
        claim_id: ClaimId::new("assert-opinion").unwrap(),
        kind: ClaimKind::Assertion,
        subject: "drive".into(),
        predicate: "previous_ytg".into(),
        value: "3".into(),
        authority: "incumbent-alpha-2026-10".into(),
        authority_rank: 1,
    });
    field.artifacts.push(Artifact::new(
        ArtifactId::new("art-note").unwrap(),
        "note",
        DisclosureClass::Internal,
        "notes: incumbent-alpha-2026-10 nearly finished",
    ));
    Drive {
        schema_version: lastdown::SCHEMA_VERSION,
        drive_id: DriveId::new("drv-scout").unwrap(),
        generation: 2,
        objective: Objective {
            schema_version: lastdown::SCHEMA_VERSION,
            statement: "Land the patch with the suite green.".into(),
            constraints: Vec::new(),
            required_capabilities: Vec::new(),
        },
        acceptance_gates: vec![AcceptanceGate {
            gate_id: gates[0].clone(),
            name: "g1".into(),
            utility: 10,
            partial_utility: 4,
            required: true,
        }],
        field,
        possession: None,
        budget: DriveBudget {
            granted: ResourceVector::splat(1_000),
            consumed: ResourceVector::zero(),
            reserve: ContinuationReserve::zero(),
            reserve_hold: ResourceVector::zero(),
            lock_reserve: true,
        },
        status: Status::Punt,
        attempt_used: false,
        preserving: false,
        failure: None,
        possession_marks: Vec::new(),
    }
}

#[test]
fn gate1_ordinary_spend_stops_at_the_reserve() {
    let drive = punt_drive();
    let mut drive = drive;
    drive.status = Status::Normal;
    drive.budget.reserve_hold.tokens = 400;
    drive.budget.granted.tokens = 1_000;
    let mut cost = ResourceVector::zero();
    cost.tokens = 601;
    assert_eq!(
        authorise(&drive, SpendClass::Ordinary, &cost),
        Err(ReserveError::ReserveBoundary)
    );
}

#[test]
fn gate2_committed_punt_replays_and_lands_in_the_ledger() {
    let campaign = execute(
        &fixture("elci/fixtures/token_budget.json"),
        Mode::Lastdown,
        Kill::ImmediatelyAfterCommit,
    )
    .unwrap();
    assert!(campaign.result.replay_ok);
    assert!(campaign.result.continuation_bytes > 0);
    assert!(!campaign.ledger.is_empty());
    let entry = &campaign.ledger.entries()[0];
    assert!(!entry.contradictions.is_empty());
    assert!(entry.claims.iter().any(|claim| claim.kind == "observation"));
    assert!(entry.claims.iter().any(|claim| claim.kind == "assertion"));
    assert!(entry.contradictions.iter().all(|edge| {
        entry.claims.iter().any(|claim| claim.id == edge.left)
            && entry.claims.iter().any(|claim| claim.id == edge.right)
    }));
}

#[test]
fn gate3_candidates_receive_evidence_without_the_scoreboard() {
    let mut drive = punt_drive();
    let hidden = vec!["incumbent-alpha-2026-10".to_string()];
    let (clean, raw) = measure_amnesia(&drive.field, &drive.objective, &hidden).unwrap();
    assert_eq!(clean, 0);
    assert!(raw > 0);
    let binding = ScoutBinding {
        candidate_id: "cand-a".to_string(),
        family_id: "coding".to_string(),
        priority: 1,
        route: route("route-a", "receiver-beta-2026-10", 1, TrustDomain::Local),
    };
    let mut log = lastdown::telemetry::TelemetryLog::default();
    let invitations = request_scouts(
        &mut drive,
        &mut log,
        ScoutRequest {
            task_class: TASK_CLASS,
            answer_min_milli: 1,
            answer_max_milli: 20_000,
            hidden_model_keys: &hidden,
            bindings: &[binding],
            now_ms: 5,
        },
    )
    .unwrap();
    let bytes = serde_json::to_vec(&invitations).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    for key in forbidden_competitive_keys() {
        assert!(!text.contains(key), "{key}");
    }
    assert!(!text.contains("incumbent-alpha-2026-10"));
    assert!(!text.contains("receiver-beta-2026-10"));
    assert!(!text.to_ascii_lowercase().contains("nearly finished"));
    assert!(text.contains("green"));
    assert!(!text.contains("effective_ytg"));
    assert_eq!(drive.status, DriveStatus::BlindScout);
    assert!(log.contains(kinds::SCOUT_REQUESTED));

    let mut rejected = ScoutResponse {
        schema_version: lastdown::SCHEMA_VERSION,
        field_id: "fld-scout".to_string(),
        candidate_id: "cand-a".to_string(),
        distance_milli: 4_000,
        success_ppm: 700_000,
        confidence_ppm: 1_000_000,
        resource_tokens: 2_000,
        blocking_unknowns: vec!["api shape".to_string()],
        tool_requirements: Vec::new(),
        continuation_tokens: 400,
    };
    assert!(parse_scout_response(&serde_json::to_vec(&rejected).unwrap()).is_ok());
    assert!(complete_scouts(&mut drive, &mut log, 6, &invitations, &[rejected.clone()]).is_err());
    assert_eq!(drive.status, DriveStatus::BlindScout);

    rejected.blocking_unknowns.clear();
    rejected.confidence_ppm = 600_000;
    let accepted = complete_scouts(&mut drive, &mut log, 7, &invitations, &[rejected]).unwrap();
    assert_eq!(accepted[0].nominal_ytg_milli, 4_000);
    assert_eq!(drive.status, DriveStatus::Route);
    assert!(log.contains(kinds::SCOUT_RETURNED));
    let returned = serde_json::to_vec(&accepted).unwrap();
    assert!(!invitations_contain_nominal(&invitations));
    assert!(!String::from_utf8(returned).unwrap().contains("route_rank"));
}

fn invitations_contain_nominal(invitations: &[lastdown::scouting::ScoutInvitation]) -> bool {
    let bytes = serde_json::to_vec(invitations).unwrap();
    String::from_utf8(bytes).unwrap().contains("nominal_ytg")
}

#[test]
fn gate4_effective_distance_beats_raw_nominal_distance() {
    let outcome = reference_outcome();
    assert_eq!(outcome.effective_mae, 0);
    assert_eq!(outcome.nominal_mae, 3_100);
    assert!(outcome.overconfident_nominal < outcome.calibrated_nominal);
    assert!(outcome.overconfident_effective > outcome.calibrated_effective);
    assert_eq!(outcome.overconfident_effective, BOAST_OBSERVED);
    assert_eq!(outcome.calibrated_effective, PLAIN_OBSERVED);

    let table = reference_table();
    let candidates = vec![
        scout(
            "cand-boast",
            "coding",
            1,
            route("route-boast", BOAST_KEY, 10, TrustDomain::Local),
            BOAST_NOMINAL,
            900_000,
            false,
        ),
        scout(
            "cand-plain",
            "coding",
            5,
            route("route-plain", PLAIN_KEY, 10, TrustDomain::Local),
            PLAIN_NOMINAL,
            900_000,
            false,
        ),
    ];
    let decision = decide(&table, &candidates);
    assert_eq!(decision.selected_model_key, PLAIN_KEY);
    let reversed = decide(&table, &[candidates[1].clone(), candidates[0].clone()]);
    assert_eq!(reversed, decision);
    let plain = decision
        .explanations
        .iter()
        .find(|item| item.model_key == PLAIN_KEY)
        .unwrap();
    assert_eq!(plain.effective.effective_ytg_milli, PLAIN_OBSERVED);
    assert_eq!(plain.effective.historical_error_milli, 200);
}

#[test]
fn gate5_lastdown_loses_less_than_the_baseline() {
    let report =
        ablate_dir(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures")).unwrap();
    assert!(report.fixtures.len() >= 9);
    let mut lastdown_lost = 0u64;
    let mut baseline_lost = 0u64;
    for row in &report.fixtures {
        assert!(
            row.lastdown_lost <= row.baseline_lost,
            "{} lost {} vs {}",
            row.name,
            row.lastdown_lost,
            row.baseline_lost
        );
        lastdown_lost += row.lastdown_lost;
        baseline_lost += row.baseline_lost;
    }
    assert!(lastdown_lost < baseline_lost);
    let token = report
        .fixtures
        .iter()
        .find(|row| row.name == "token_budget")
        .unwrap();
    assert_eq!(token.lastdown_lost, 0);
    assert_eq!(token.baseline_lost, 8_000);
    assert!(token.lastdown_scored);
    assert!(!token.baseline_scored);
}

#[test]
fn gate6_provider_loss_keeps_field_position() {
    let campaign = execute(
        &fixture("elci/fixtures/provider_loss.json"),
        Mode::Lastdown,
        Kill::None,
    )
    .unwrap();
    let baseline = execute(
        &fixture("elci/fixtures/provider_loss.json"),
        Mode::Baseline,
        Kill::None,
    )
    .unwrap();
    assert_eq!(campaign.result.context_losses, 0);
    assert!(baseline.result.context_losses >= 1);
    assert!(campaign.result.scarce_lost <= baseline.result.scarce_lost);
    assert!(campaign.result.artifacts >= 1);
    assert!(!campaign.result.turnover);
    assert_eq!(
        boundary_response(BoundarySignal::ProviderTerminated),
        BoundaryResponse::Preserve
    );
    assert_eq!(
        FailureClass::from_signal(BoundarySignal::ProviderTerminated),
        FailureClass::ProviderExhausted
    );
    assert_eq!(
        transition(
            DriveStatus::Lastdown,
            MachineEvent::Unrecoverable(FailureClass::ProviderExhausted)
        ),
        Err(PhaseError::ExhaustionIsNotTurnover)
    );
}

#[test]
fn gate7_foreign_route_cannot_take_a_restricted_field() {
    let table = CalibrationTable::default();
    let candidates = vec![
        scout(
            "cand-remote",
            "coding",
            1,
            route(
                "route-remote",
                "remote-2026-10",
                1,
                TrustDomain::ForeignProvider,
            ),
            1_000,
            900_000,
            true,
        ),
        scout(
            "cand-local",
            "coding",
            8,
            route("route-local", "local-2026-10", 5, TrustDomain::Local),
            9_000,
            800_000,
            true,
        ),
    ];
    let decision = decide(&table, &candidates);
    assert_eq!(decision.selected_model_key, "local-2026-10");
    let remote = decision
        .explanations
        .iter()
        .find(|item| item.candidate_id == "cand-remote")
        .unwrap();
    assert!(!remote.eligible);
    assert_eq!(remote.skip_reason.as_deref(), Some("disclosure"));
}

#[test]
fn gate8_punt_loops_escalate_and_then_stop() {
    let campaign = execute(
        &fixture("elci/fixtures/punt_loop.json"),
        Mode::Lastdown,
        Kill::None,
    )
    .unwrap();
    assert!(campaign.result.reassessment);
    assert!(!campaign.result.turnover);
    assert_eq!(campaign.result.committed_packages, 3);
    assert!(campaign
        .result
        .events
        .iter()
        .any(|event| event == kinds::LOOP_REASSESS));
    assert_eq!(
        loop_signal(&[
            ProgressMark::Neutral,
            ProgressMark::Neutral,
            ProgressMark::Neutral
        ]),
        LoopSignal::Reassess
    );
    assert_eq!(
        loop_signal(&[ProgressMark::Regression, ProgressMark::Regression]),
        LoopSignal::Rollback
    );
    let long = vec![ProgressMark::Neutral; 6];
    assert_eq!(loop_signal(&long), LoopSignal::Diagnose);
    assert_eq!(
        possession_plan(LoopSignal::Rollback, Some("fld-parent")),
        PossessionPlan::Rollback {
            parent_field: Some("fld-parent".to_string()),
        }
    );
    assert_eq!(
        possession_plan(LoopSignal::Diagnose, None),
        PossessionPlan::Diagnose
    );
    for signal in [
        BoundarySignal::RateLimited,
        BoundarySignal::ContextOverflow,
        BoundarySignal::Timeout,
        BoundarySignal::QuotaRejected,
        BoundarySignal::ProviderTerminated,
        BoundarySignal::MalformedOutput,
        BoundarySignal::PartialToolExecution,
    ] {
        assert_eq!(boundary_response(signal), BoundaryResponse::Preserve);
    }
}

#[test]
fn gate9_routing_has_no_vendor_branch() {
    for source in [
        include_str!("../src/coordinator/routing.rs"),
        include_str!("../src/coordinator/effective_ytg.rs"),
        include_str!("../src/coordinator/calibration.rs"),
        include_str!("../src/scouting/scout.rs"),
        include_str!("../src/scouting/amnesia.rs"),
    ] {
        assert!(!source.contains("provider =="));
        assert!(!source.contains("== \"grok\""));
        assert!(!source.contains("== \"openai\""));
    }
    let table = CalibrationTable::default();
    let candidates = vec![
        scout(
            "cand-grok",
            "coding",
            1,
            route("route-grok", "grok-2026-10", 1, TrustDomain::Local),
            9_000,
            400_000,
            false,
        ),
        scout(
            "cand-compact",
            "coding",
            4,
            route("route-compact", "compact-2026-10", 1, TrustDomain::Local),
            3_000,
            900_000,
            false,
        ),
    ];
    assert_eq!(
        decide(&table, &candidates).selected_model_key,
        "compact-2026-10"
    );

    let frontier = vec![
        scout(
            "cand-frontier",
            "coding",
            1,
            route("route-frontier", "frontier-2026-10", 50, TrustDomain::Local),
            5_000,
            1_000_000,
            false,
        ),
        scout(
            "cand-small",
            "coding",
            2,
            route("route-small", "compact-2026-10", 1, TrustDomain::Local),
            5_000,
            1_000_000,
            false,
        ),
    ];
    assert_eq!(
        decide(&table, &frontier).selected_model_key,
        "compact-2026-10"
    );

    let cheap = vec![
        scout(
            "cand-cheap",
            "coding",
            1,
            route("route-cheap", "cheap-2026-10", 1, TrustDomain::Local),
            5_000,
            10_000,
            false,
        ),
        scout(
            "cand-fit",
            "coding",
            2,
            route("route-fit", "fit-2026-10", 20, TrustDomain::Local),
            8_000,
            900_000,
            false,
        ),
    ];
    assert_eq!(decide(&table, &cheap).selected_model_key, "fit-2026-10");

    let tie = vec![
        scout(
            "cand-z",
            "coding",
            1,
            route("route-z", "z@2026-10", 10, TrustDomain::Local),
            4_000,
            800_000,
            false,
        ),
        scout(
            "cand-a",
            "coding",
            2,
            route("route-a", "a@2026-10", 10, TrustDomain::Local),
            4_000,
            800_000,
            false,
        ),
    ];
    assert_eq!(decide(&table, &tie).selected_route_id, "route-z");
    assert_eq!(headroom_ppm(400, 800), 500_000);
    assert_eq!(headroom_ppm(900, 800), 1_000_000);
}

#[test]
fn gate9_versions_do_not_share_calibration() {
    let mut table = CalibrationTable::default();
    table.record(CalibrationSample {
        model_key: "model-x@2027-01".to_string(),
        task_class: TASK_CLASS.to_string(),
        toolset: TOOLSET.to_string(),
        nominal_ytg_milli: 1_000,
        observed_ytg_milli: 9_000,
    });
    assert_eq!(
        table.error_milli("model-x@2026-10", TASK_CLASS, TOOLSET),
        None
    );
    assert_eq!(
        table.error_milli("model-x@2027-01", TASK_CLASS, TOOLSET),
        Some(8_000)
    );
    assert_eq!(table.error_milli("model-x@2027-01", "prose", TOOLSET), None);
}

#[test]
fn gate10_ablation_shows_why_each_mechanism_exists() {
    let report =
        ablate_dir(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures")).unwrap();
    assert!(report.reserve.with_mechanism < report.reserve.without_mechanism);
    assert_eq!(report.amnesia.with_mechanism, 0);
    assert!(report.amnesia.without_mechanism > 0);
    assert!(report.calibration.with_mechanism < report.calibration.without_mechanism);
    assert_eq!(report.reserve.name, "reserve");
    assert_eq!(report.amnesia.name, "amnesia");
    assert_eq!(report.calibration.name, "calibration");
}

#[test]
fn ledger_keeps_both_sides_and_seals_at_rest() {
    let left = ClaimId::new("obs-left").unwrap();
    let right = ClaimId::new("assert-right").unwrap();
    let mut field = FieldSnapshot::genesis(FieldId::new("fld-ledger").unwrap(), &[]);
    field.verified_facts.push(Fact {
        claim_id: left.clone(),
        kind: ClaimKind::Observation,
        subject: "suite".into(),
        predicate: "status".into(),
        value: "green".into(),
        authority: "probe".into(),
        authority_rank: 1,
    });
    field.assertions.push(Fact {
        claim_id: right.clone(),
        kind: ClaimKind::Assertion,
        subject: "suite".into(),
        predicate: "status".into(),
        value: "red".into(),
        authority: "probe".into(),
        authority_rank: 2,
    });
    field.contradictions.push(Contradiction {
        id: "con-1".into(),
        left: left.clone(),
        right: right.clone(),
    });
    field.corroborations.push(Corroboration {
        id: "cor-1".into(),
        source: left.clone(),
        target: right.clone(),
        independent: true,
    });
    field.supersessions.push(Supersession {
        id: "sup-1".into(),
        winner: left.clone(),
        loser: right.clone(),
        operator: "hierarchical".into(),
    });
    let mut ledger = FieldLedger::default();
    ledger.record(&field);
    ledger.record(&field);
    ledger.record(&field);
    assert_eq!(
        ledger.revise(1),
        Err(lastdown::error::LedgerError::Immutable)
    );
    assert_eq!(
        ledger.revise(99),
        Err(lastdown::error::LedgerError::Missing)
    );
    let entry = &ledger.entries()[0];
    assert!(entry.claims.iter().any(|claim| claim.id == left.as_str()));
    assert!(entry.claims.iter().any(|claim| claim.id == right.as_str()));
    assert_eq!(entry.supersessions[0].operator, "hierarchical");
    assert_eq!(entry.contradictions.len(), 1);
    ledger.apply_retention(1);
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger.entries()[0].sequence, 3);

    let root = std::env::temp_dir().join(format!("lastdown-ledger-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    ledger.persist_sealed(&root, b"ledger-key").unwrap();
    let loaded = FieldLedger::load_sealed(&root, b"ledger-key").unwrap();
    assert_eq!(loaded, ledger);
    assert!(FieldLedger::load_sealed(&root, b"other-key").is_err());
    std::fs::remove_file(root.join("COMMIT")).unwrap();
    assert!(FieldLedger::load_sealed(&root, b"ledger-key").is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unlocked_reserve_grows_with_uncommitted_work() {
    let gate = GateId::new("g").unwrap();
    let field = FieldSnapshot::genesis(FieldId::new("fld-reserve").unwrap(), &[gate]);
    let mut budget = DriveBudget {
        granted: ResourceVector::splat(1_000_000),
        consumed: ResourceVector::zero(),
        reserve: ContinuationReserve::zero(),
        reserve_hold: ResourceVector::zero(),
        lock_reserve: false,
    };
    apply_dynamic_reserve(&mut budget, &field);
    let before = budget.reserve_hold.tokens;
    assert!(before > 0);
    let mut dirty = field.clone();
    dirty.side_effects.push(SideEffect::new(
        EffectId::new("fx-1").unwrap(),
        "filesystem",
        false,
        "diff",
    ));
    apply_dynamic_reserve(&mut budget, &dirty);
    assert!(budget.reserve_hold.tokens > before);
    budget.lock_reserve = true;
    let locked = budget.reserve_hold.tokens;
    dirty.side_effects.push(SideEffect::new(
        EffectId::new("fx-2").unwrap(),
        "filesystem",
        false,
        "more",
    ));
    apply_dynamic_reserve(&mut budget, &dirty);
    assert_eq!(budget.reserve_hold.tokens, locked);
}

#[test]
fn vector_projection_stays_off_the_resume_path() {
    let gate = GateId::new("g").unwrap();
    let mut field = FieldSnapshot::genesis(
        FieldId::new("fld-vector").unwrap(),
        std::slice::from_ref(&gate),
    );
    field.unresolved_gates.push(GateId::new("g2").unwrap());
    field.artifacts.push(Artifact::new(
        ArtifactId::new("art-long").unwrap(),
        "patch",
        DisclosureClass::Internal,
        "x".repeat(2_000),
    ));
    let report = benchmark(&field);
    assert_eq!(report.scalar_distance, 2);
    assert!(report.json_bytes > report.vector_bytes);
    assert!(!report.vector_required_for_resume);
    let encoded = serde_json::to_vec(&lastdown::vectoral::encode(&field)).unwrap();
    assert!(!String::from_utf8(encoded)
        .unwrap()
        .contains(&"x".repeat(40)));
}

#[test]
fn assessments_keep_field_distance_apart_from_model_distance() {
    let record = AssessmentRecord {
        field_id: "fld-ref".into(),
        route_id: "route-plain".into(),
        model_key: PLAIN_KEY.into(),
        task_class: TASK_CLASS.into(),
        field_distance_gates: 4,
        nominal_ytg_milli: PLAIN_NOMINAL,
        effective_ytg_milli: PLAIN_OBSERVED,
        observed_ytg_milli: Some(PLAIN_OBSERVED),
    };
    let text = String::from_utf8(serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(!text.contains("task_ytg"));
    assert_eq!(record.field_distance_gates, 4);
    assert_ne!(
        u64::from(record.field_distance_gates),
        record.nominal_ytg_milli
    );
    let mut log = lastdown::learning::AssessmentLog::default();
    log.push(record);
    assert_eq!(log.mean_absolute_error(ErrorKind::Nominal), Some(200));
}
