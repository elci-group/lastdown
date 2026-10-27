use std::path::PathBuf;

use lastdown::sim::{execute, load_fixture, Kill, Mode};
use lastdown::DriveStatus;

fn fixture(name: &str) -> lastdown::sim::Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("elci/fixtures")
        .join(name);
    load_fixture(path).expect("fixture")
}

#[test]
fn token_budget_bench_stays_on_its_numbers() {
    let report = lastdown::sim::bench(&fixture("token_budget.json")).expect("bench");
    assert!(report.lastdown.quota_remaining.is_none());
    assert!(report.lastdown.scored);
    assert_eq!(report.lastdown.scarce_consumed, 8000);
    assert_eq!(report.lastdown.scarce_lost, 0);
    assert_eq!(report.lastdown.artifacts, 4);
    assert_eq!(report.lastdown.contradictions, 1);
    assert_eq!(report.lastdown.continuation_bytes, 2940);
    assert!(!report.baseline.scored);
    assert_eq!(report.baseline.scarce_consumed, 8000);
    assert_eq!(report.baseline.scarce_lost, 8000);
    assert_eq!(report.baseline.continuation_bytes, 0);
}

#[test]
fn engaged_quota_punts_before_the_account_limit() {
    let fixture = fixture("quota_limit.json");
    let campaign = execute(&fixture, Mode::Lastdown, Kill::None).expect("run");
    assert!(!campaign.result.turnover);
    assert!(!campaign.result.scored);
    assert_eq!(campaign.result.status, DriveStatus::Punt);
    assert!(campaign.result.replay_ok);
    assert!(campaign.result.committed_packages >= 1);
    assert_eq!(campaign.result.completed_gates, 1);
    assert_eq!(campaign.result.scarce_consumed, 2000);
    assert_eq!(campaign.result.scarce_lost, 0);
    assert_eq!(campaign.result.quota_remaining, Some(2500));
    assert_eq!(campaign.result.context_losses, 0);
    assert_eq!(campaign.result.duplicate_steps, 0);

    let baseline = execute(&fixture, Mode::Baseline, Kill::None).expect("baseline");
    assert!(baseline.result.scored);
    assert_eq!(baseline.result.scarce_consumed, 8000);
    assert_eq!(baseline.result.scarce_lost, 0);
    assert!(baseline.result.quota_remaining.is_none());
}
