use std::path::PathBuf;

use lastdown::punt::continuation::leaks_competitive_metadata;
use lastdown::sim::{
    committed_package, execute, export_handoff, load_fixture, resume_handoff, Kill, Mode,
};
use lastdown::telemetry::kinds;
use lastdown::DriveStatus;

fn fixture(name: &str) -> lastdown::sim::Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("elci/fixtures")
        .join(name);
    load_fixture(path).expect("fixture")
}

#[test]
fn handoff_file_resumes_the_token_budget_and_scores() {
    let fixture = fixture("token_budget.json");
    let stopped = execute(&fixture, Mode::Lastdown, Kill::ImmediatelyAfterCommit).expect("stop");
    assert!(!stopped.result.scored);
    assert_eq!(stopped.result.completed_gates, 2);
    assert!(stopped.result.replay_ok);
    let bytes = committed_package(&stopped).expect("package");
    assert_eq!(bytes.len(), stopped.result.continuation_bytes);
    assert!(!leaks_competitive_metadata(&bytes));
    assert!(!String::from_utf8_lossy(&bytes).contains("incumbent-alpha-2026-10"));

    let path = std::env::temp_dir().join(format!("lastdown-handoff-{}.json", std::process::id()));
    export_handoff(&stopped, &path).expect("write");
    let resumed = resume_handoff(&fixture, &path).expect("resume");
    let _ = std::fs::remove_file(&path);

    assert!(resumed.result.scored);
    assert_eq!(resumed.result.status, DriveStatus::Score);
    assert_eq!(resumed.result.scarce_lost, 0);
    assert_eq!(resumed.result.scarce_consumed, 4000);
    assert_eq!(resumed.result.completed_gates, 4);
    assert_eq!(resumed.result.artifacts, 4);
    assert!(resumed.result.contradictions >= 1);
    assert!(!resumed.result.turnover);
    assert_eq!(resumed.result.context_losses, 0);
    assert_eq!(resumed.result.duplicate_steps, 0);
    assert!(resumed
        .result
        .events
        .iter()
        .any(|event| event == kinds::SCORE_COMPLETED));
    assert!(!resumed
        .result
        .events
        .iter()
        .any(|event| event == kinds::PUNT_COMMITTED || event == kinds::TURNOVER));
    let resumed_bytes = committed_package(&resumed).expect("kept package");
    assert!(!String::from_utf8_lossy(&resumed_bytes).contains("receiver-beta-2026-10"));
    assert!(!String::from_utf8_lossy(&resumed_bytes).contains("incumbent-alpha-2026-10"));
}

#[test]
fn tampered_handoff_is_refused() {
    let fixture = fixture("token_budget.json");
    let stopped = execute(&fixture, Mode::Lastdown, Kill::ImmediatelyAfterCommit).expect("stop");
    let mut bytes = committed_package(&stopped).expect("package");
    let path = std::env::temp_dir().join(format!(
        "lastdown-handoff-tamper-{}.json",
        std::process::id()
    ));
    bytes.push(b' ');
    std::fs::write(&path, &bytes).expect("write");
    let refused = match resume_handoff(&fixture, &path) {
        Ok(_) => panic!("tampered handoff was accepted"),
        Err(error) => error,
    };
    let _ = std::fs::remove_file(&path);
    let message = refused.to_string();
    assert!(message.contains("canonical"));
}
