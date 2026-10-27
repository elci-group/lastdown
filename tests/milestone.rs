use std::path::PathBuf;

use lastdown::punt::continuation::{leaks_competitive_metadata, validate};
use lastdown::punt::replay::resume_drive;
use lastdown::punt::store::ContinuationStore;
use lastdown::punt::ResumeRequest;
use lastdown::sim::{bench, execute, load_fixture, Kill, Mode};
use lastdown::telemetry::kinds;
use lastdown::DriveStatus;

fn fixture(name: &str) -> lastdown::sim::Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("elci/fixtures")
        .join(name);
    load_fixture(path).expect("fixture")
}

#[test]
fn committed_punt_resumes_without_the_predecessor_conversation() {
    let campaign = execute(
        &fixture("token_budget.json"),
        Mode::Lastdown,
        Kill::ImmediatelyAfterCommit,
    )
    .expect("run");
    assert!(campaign.result.replay_ok);
    assert!(!campaign.result.scored);
    assert_eq!(campaign.result.completed_gates, 2);
    assert!(campaign.result.contradictions >= 1);
    assert!(!campaign.result.turnover);
    let object = campaign
        .store
        .latest_committed(campaign.drive_id.as_str())
        .expect("store")
        .expect("commit");
    let bytes = serde_json::to_vec(&object).expect("bytes");
    assert!(!leaks_competitive_metadata(&bytes));
    assert!(!String::from_utf8_lossy(&bytes).contains("incumbent-alpha-2026-10"));
    assert_eq!(object.magic, lastdown::CONTINUATION_MAGIC);

    let resumed = resume_drive(
        &object,
        ResumeRequest {
            route: lastdown::ModelRoute {
                route_id: lastdown::ids::RouteId::new("route-fresh").unwrap(),
                model_key: "receiver-beta-2026-10".into(),
                capability: lastdown::core::CapabilityProfile::coding_default().unwrap(),
                trust_domain: lastdown::core::possession::TrustDomain::Local,
            },
            grant: campaign.grant,
            reserve: campaign.reserve.clone(),
            reserve_hold: campaign.reserve_hold,
            policy: campaign.policy.clone(),
            now_ms: 50,
        },
    )
    .expect("resume");
    assert_eq!(resumed.status, DriveStatus::Normal);
    assert!(resumed.possession.is_some());
    assert_eq!(resumed.field.completed_gates.len(), 2);
    assert_eq!(
        resumed.field.contradictions.len(),
        object.evidence.contradictions.len()
    );
    assert_eq!(resumed.objective.constraints[0].value, "lastdown");
    assert_eq!(
        resumed.possession.unwrap().model.model_key,
        "receiver-beta-2026-10"
    );

    let mut inspected = object.clone();
    inspected.inspection.summary = "ALL GATES ARE DONE AND THE TASK IS FINISHED".into();
    assert!(validate(&inspected).is_ok());
    let still = lastdown::punt::continuation::reconstruct_field(&inspected).unwrap();
    assert!(still.unresolved_gates.len() >= 2);

    let mut tampered = object.clone();
    tampered.evidence.observations[0].value.push('z');
    assert!(validate(&tampered).is_err());
}

#[test]
fn lastdown_scores_where_fallback_loses_the_field() {
    let report = bench(&fixture("token_budget.json")).expect("bench");
    assert!(report.lastdown.scored);
    assert!(!report.baseline.scored);
    assert_eq!(report.lastdown.scarce_lost, 0);
    assert!(report.baseline.scarce_lost > report.lastdown.scarce_lost);
    assert_eq!(report.lastdown.context_losses, 0);
    assert!(report.baseline.context_losses >= 1);
    assert!(report.lastdown.contradictions >= 1);
    assert_eq!(report.baseline.contradictions, 0);
    assert_eq!(report.lastdown.duplicate_steps, 0);
    assert!(report.baseline.duplicate_steps >= 1);
    assert!(report.lastdown.replay_ok);
    assert!(report.lastdown.artifacts > report.baseline.artifacts);
    let events = &report.lastdown.events;
    let redzone = events
        .iter()
        .position(|event| event == kinds::REDZONE_ENTERED)
        .unwrap();
    let lastdown = events
        .iter()
        .position(|event| event == kinds::LASTDOWN_ENTERED)
        .unwrap();
    let committed = events
        .iter()
        .position(|event| event == kinds::PUNT_COMMITTED)
        .unwrap();
    let score = events
        .iter()
        .position(|event| event == kinds::SCORE_COMPLETED)
        .unwrap();
    assert!(redzone < lastdown);
    assert!(lastdown < committed);
    assert!(committed < score);
    assert!(!events.iter().any(|event| event == kinds::TURNOVER));
}

#[test]
fn every_fixture_replays() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures");
    let mut count = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let fixture = load_fixture(&path).unwrap();
        let campaign = execute(&fixture, Mode::Lastdown, Kill::None).unwrap();
        assert_ne!(
            campaign.result.status,
            DriveStatus::Turnover,
            "{}",
            fixture.name
        );
        if fixture.name != "punt_loop" {
            assert!(campaign.result.replay_ok, "{}", fixture.name);
            assert!(campaign.result.committed_packages >= 1, "{}", fixture.name);
        }
        count += 1;
    }
    assert!(count >= 9);
}
