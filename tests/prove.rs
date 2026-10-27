use std::path::PathBuf;

use lastdown::sim::{ablation_report, load_fixture_dir, proof_lines, prove_dir, Verdict};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures")
}

#[test]
fn the_ledger_supports_reserve_conservation_only() {
    let directory = fixtures_dir();
    let proof = prove_dir(&directory).expect("prove");
    let ablation =
        ablation_report(&load_fixture_dir(&directory).expect("fixtures")).expect("ablate");

    assert_eq!(proof.reserve_verdict, Verdict::Supported);
    assert_eq!(proof.punt_verdict, Verdict::Unmeasured);
    assert_eq!(proof.amnesia_verdict, Verdict::Unmeasured);
    assert_eq!(proof.lastdown_lost, ablation.reserve.with_mechanism);
    assert_eq!(proof.baseline_lost, ablation.reserve.without_mechanism);
    assert!(proof.lastdown_lost < proof.baseline_lost);
    assert!(proof.lastdown_context_losses <= proof.baseline_context_losses);
    assert_eq!(proof.lastdown_turnovers, 0);
    assert_eq!(proof.isolation_clean, ablation.amnesia.with_mechanism);
    assert_eq!(proof.isolation_raw, ablation.amnesia.without_mechanism);
    assert_eq!(proof.isolation_clean, 0);
    assert!(proof.isolation_raw > 0);

    let token = proof
        .rows
        .iter()
        .find(|row| row.name == "token_budget")
        .expect("token_budget");
    assert!(token.lastdown_scored);
    assert!(!token.baseline_scored);
    assert_eq!(token.lastdown_consumed, 8000);
    assert_eq!(token.lastdown_lost, 0);
    assert_eq!(token.baseline_consumed, 8000);
    assert_eq!(token.baseline_lost, 8000);
    assert_eq!(token.lastdown_continuation_bytes, 2940);
    assert_eq!(token.baseline_continuation_bytes, 0);
    assert!(token.lastdown_quota_remaining.is_none());

    let quota = proof
        .rows
        .iter()
        .find(|row| row.name == "quota_limit")
        .expect("quota_limit");
    assert!(!quota.lastdown_scored);
    assert!(quota.baseline_scored);
    assert_eq!(quota.lastdown_consumed, 2000);
    assert_eq!(quota.lastdown_lost, 0);
    assert_eq!(quota.lastdown_gates, 1);
    assert_eq!(quota.lastdown_quota_remaining, Some(2500));
    assert_eq!(quota.baseline_consumed, 8000);
    assert_eq!(quota.baseline_lost, 0);
    assert!(quota.baseline_quota_remaining.is_none());

    let transfer = proof.transfer.as_ref().expect("token_budget transfer");
    assert_eq!(transfer.fixture, "token_budget");
    assert_eq!(transfer.stopped_gates, 2);
    assert_eq!(transfer.stopped_consumed, 4000);
    assert_eq!(transfer.stopped_lost, 0);
    assert_eq!(transfer.package_bytes, 2940);
    assert!(transfer.resume_scored);
    assert_eq!(transfer.resume_consumed, 4000);
    assert_eq!(transfer.resume_lost, 0);
    assert_eq!(transfer.resume_gates, 4);
    assert!(!transfer.resume_turnover);

    let lines = proof_lines(&proof);
    assert!(lines.iter().any(|line| {
        line.contains("claim=reserve_conservation verdict=supported")
            && line.contains("wall_clock_ms=-")
            && line.contains("provider_switches=-")
    }));
    assert!(lines.iter().any(|line| {
        line.contains("claim=punt_advantage verdict=unmeasured")
            && line.contains("reason=same_script_receiver")
            && line.contains("transfer_process=in_memory")
            && line.contains("package_bytes=2940")
    }));
    assert!(lines.iter().any(|line| {
        line.contains("claim=amnesia_advantage verdict=unmeasured")
            && line.contains("reason=no_sighted_control")
            && line.contains("isolation_clean=0")
    }));
}
