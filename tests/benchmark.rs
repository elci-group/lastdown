use lastdown::sim::{benchmark_dir, suite_lines, SuiteVerdict};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("elci/fixtures")
}

#[test]
fn the_suite_is_bit_identical_across_runs() {
    let first = suite_lines(&benchmark_dir(&fixtures_dir()).expect("suite"));
    let second = suite_lines(&benchmark_dir(&fixtures_dir()).expect("suite"));
    assert_eq!(first, second);
    assert!(first.len() > 30);
}

#[test]
fn pinned_rows_keep_the_contract_numbers() {
    let report = benchmark_dir(&fixtures_dir()).expect("suite");
    let token_base = report
        .rows
        .iter()
        .find(|row| row.fixture == "token_budget" && row.variant == "base")
        .expect("token_budget base");
    assert_eq!(token_base.lastdown.waste, 0);
    assert!(token_base.lastdown.scored);
    assert_eq!(token_base.naive.waste, 8000);
    assert!(!token_base.naive.scored);
    assert!(token_base.summary.is_some());
    assert!(token_base.summary.as_ref().is_some_and(|arm| arm.scored));
    assert_eq!(
        token_base
            .summary
            .as_ref()
            .map(|arm| arm.summary_duplicates),
        Some(2)
    );
}

#[test]
fn guardrails_hold_and_claims_earn_support() {
    let report = benchmark_dir(&fixtures_dir()).expect("suite");
    assert_eq!(report.lastdown_turnovers, 0);
    assert!(report.lastdown_replay_ok);
    assert!(report.lastdown_scored_rate >= report.summary_scored_rate);
    assert!(report.lastdown_scored_rate > report.naive_scored_rate);
    assert_eq!(report.transfer.verdict, SuiteVerdict::Supported);
    assert_eq!(report.amnesia.verdict, SuiteVerdict::Supported);
    assert!(report.transfer.median_delta > 0);
    assert!(report.transfer.wins > report.transfer.losses);
    assert!(report.transfer.p <= 0.05);
    assert!(report.transfer.holm_p >= report.transfer.p);
    assert!(report.transfer.ci_low > 0);
    assert!(report.amnesia.median_delta > 0);
    assert!(report.naive_scored_rate < report.lastdown_scored_rate);
}

#[test]
fn the_ledger_lines_carry_the_claim_fields() {
    let report = benchmark_dir(&fixtures_dir()).expect("suite");
    let lines = suite_lines(&report);
    let transfer = lines
        .iter()
        .find(|line| line.contains("claim=transfer_advantage"))
        .expect("transfer line");
    assert!(transfer.contains("verdict=supported"));
    assert!(transfer.contains("holm_p="));
    assert!(transfer.contains("ci95=["));
    assert!(lines
        .iter()
        .any(|line| line.contains("guardrails lastdown_turnovers=0")));
    assert!(lines.iter().all(|line| !line
        .contains("lastdown_waste=0 summary_waste=0 naive_waste=0")
        || line.contains("punt_loop")
        || line.contains("variant=fault0")));
}
