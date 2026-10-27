//! Three claims about the fixture directory.
//!
//! Reserve conservation is computed from Lastdown against the exhaustion
//! baseline. Punt advantage and athletic-amnesia advantage stay unmeasured:
//! the resume path runs the same steps, and the isolation count is a leak
//! scan rather than a sighted-scout comparison. Absent simulator measurements
//! print as `-`.

use std::path::Path;

use crate::punt::continuation::ContinuationObject;

use super::ablation::{ablation_report, load_fixture_dir, AblationReport, FixtureComparison};
use super::runner::{committed_package, execute, resume_committed, Fixture, Kill, Mode, SimError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Supported,
    Unmeasured,
    Unsupported,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Unmeasured => "unmeasured",
            Self::Unsupported => "unsupported",
        }
    }
}

/// In-memory resume of `token_budget`. This is the transfer observation.
/// It is not a comparison against a different receiver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferNote {
    pub fixture: String,
    pub stopped_gates: usize,
    pub stopped_consumed: u64,
    pub stopped_lost: u64,
    pub package_bytes: usize,
    pub resume_gates: usize,
    pub resume_consumed: u64,
    pub resume_lost: u64,
    pub resume_scored: bool,
    pub resume_turnover: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofReport {
    pub rows: Vec<FixtureComparison>,
    pub reserve_verdict: Verdict,
    pub lastdown_lost: u64,
    pub baseline_lost: u64,
    pub lastdown_context_losses: u64,
    pub baseline_context_losses: u64,
    pub lastdown_turnovers: u64,
    pub baseline_turnovers: u64,
    pub punt_verdict: Verdict,
    pub transfer: Option<TransferNote>,
    pub amnesia_verdict: Verdict,
    pub isolation_clean: u64,
    pub isolation_raw: u64,
}

pub fn prove_dir(directory: &Path) -> Result<ProofReport, SimError> {
    let fixtures = load_fixture_dir(directory)?;
    let ablation = ablation_report(&fixtures)?;
    let transfer = transfer_note(&fixtures)?;
    Ok(assemble(ablation, transfer))
}

pub fn proof_lines(report: &ProofReport) -> Vec<String> {
    let mut lines = Vec::with_capacity(report.rows.len() + 3);
    for row in &report.rows {
        lines.push(format!(
            "prove fixture={} lastdown_scored={} baseline_scored={} lastdown_consumed={} baseline_consumed={} lastdown_lost={} baseline_lost={} lastdown_gates={} baseline_gates={} lastdown_context_losses={} baseline_context_losses={} lastdown_duplicate_steps={} baseline_duplicate_steps={} lastdown_continuation_bytes={} baseline_continuation_bytes={} lastdown_turnover={} baseline_turnover={} lastdown_quota_remaining={} baseline_quota_remaining={}",
            row.name,
            row.lastdown_scored,
            row.baseline_scored,
            row.lastdown_consumed,
            row.baseline_consumed,
            row.lastdown_lost,
            row.baseline_lost,
            row.lastdown_gates,
            row.baseline_gates,
            row.lastdown_context_losses,
            row.baseline_context_losses,
            row.lastdown_duplicate,
            row.baseline_duplicate,
            row.lastdown_continuation_bytes,
            row.baseline_continuation_bytes,
            row.lastdown_turnover,
            row.baseline_turnover,
            dash_u64(row.lastdown_quota_remaining),
            dash_u64(row.baseline_quota_remaining),
        ));
    }
    lines.push(format!(
        "prove claim=reserve_conservation verdict={} fixtures={} lastdown_lost={} baseline_lost={} lastdown_context_losses={} baseline_context_losses={} lastdown_turnovers={} baseline_turnovers={} wall_clock_ms=- provider_switches=-",
        report.reserve_verdict.label(),
        report.rows.len(),
        report.lastdown_lost,
        report.baseline_lost,
        report.lastdown_context_losses,
        report.baseline_context_losses,
        report.lastdown_turnovers,
        report.baseline_turnovers,
    ));
    lines.push(match &report.transfer {
        Some(note) => format!(
            "prove claim=punt_advantage verdict={} reason=same_script_receiver summarise_restart=absent orchestrator_switch=absent transfer_process=in_memory transfer_fixture={} stopped_gates={} stopped_consumed={} stopped_lost={} package_bytes={} resume_scored={} resume_consumed={} resume_lost={} resume_gates={} resume_turnover={}",
            report.punt_verdict.label(),
            note.fixture,
            note.stopped_gates,
            note.stopped_consumed,
            note.stopped_lost,
            note.package_bytes,
            note.resume_scored,
            note.resume_consumed,
            note.resume_lost,
            note.resume_gates,
            note.resume_turnover,
        ),
        None => format!(
            "prove claim=punt_advantage verdict={} reason=same_script_receiver summarise_restart=absent orchestrator_switch=absent transfer_process=- transfer_fixture=- stopped_gates=- stopped_consumed=- stopped_lost=- package_bytes=- resume_scored=- resume_consumed=- resume_lost=- resume_gates=- resume_turnover=-",
            report.punt_verdict.label(),
        ),
    });
    lines.push(format!(
        "prove claim=amnesia_advantage verdict={} reason=no_sighted_control isolation_clean={} isolation_raw={}",
        report.amnesia_verdict.label(),
        report.isolation_clean,
        report.isolation_raw,
    ));
    lines
}

fn assemble(ablation: AblationReport, transfer: Option<TransferNote>) -> ProofReport {
    let lastdown_lost = ablation.reserve.with_mechanism;
    let baseline_lost = ablation.reserve.without_mechanism;
    let lastdown_context_losses = ablation
        .fixtures
        .iter()
        .map(|row| u64::from(row.lastdown_context_losses))
        .sum();
    let baseline_context_losses = ablation
        .fixtures
        .iter()
        .map(|row| u64::from(row.baseline_context_losses))
        .sum();
    let lastdown_turnovers = ablation
        .fixtures
        .iter()
        .filter(|row| row.lastdown_turnover)
        .count() as u64;
    let baseline_turnovers = ablation
        .fixtures
        .iter()
        .filter(|row| row.baseline_turnover)
        .count() as u64;
    let reserve_verdict = reserve_verdict(
        ablation.fixtures.len(),
        lastdown_lost,
        baseline_lost,
        lastdown_context_losses,
        baseline_context_losses,
        lastdown_turnovers,
    );
    ProofReport {
        rows: ablation.fixtures,
        reserve_verdict,
        lastdown_lost,
        baseline_lost,
        lastdown_context_losses,
        baseline_context_losses,
        lastdown_turnovers,
        baseline_turnovers,
        punt_verdict: Verdict::Unmeasured,
        transfer,
        amnesia_verdict: Verdict::Unmeasured,
        isolation_clean: ablation.amnesia.with_mechanism,
        isolation_raw: ablation.amnesia.without_mechanism,
    }
}

fn reserve_verdict(
    fixtures: usize,
    lastdown_lost: u64,
    baseline_lost: u64,
    lastdown_context_losses: u64,
    baseline_context_losses: u64,
    lastdown_turnovers: u64,
) -> Verdict {
    if fixtures == 0 {
        return Verdict::Unmeasured;
    }
    if lastdown_lost < baseline_lost
        && lastdown_context_losses <= baseline_context_losses
        && lastdown_turnovers == 0
    {
        Verdict::Supported
    } else {
        Verdict::Unsupported
    }
}

fn transfer_note(fixtures: &[Fixture]) -> Result<Option<TransferNote>, SimError> {
    let Some(fixture) = fixtures.iter().find(|item| item.name == "token_budget") else {
        return Ok(None);
    };
    let stopped = execute(fixture, Mode::Lastdown, Kill::ImmediatelyAfterCommit)?;
    let bytes = committed_package(&stopped)?;
    let object: ContinuationObject =
        serde_json::from_slice(&bytes).map_err(|_| SimError::Fixture("transfer"))?;
    let resumed = resume_committed(fixture, &object)?;
    Ok(Some(TransferNote {
        fixture: fixture.name.clone(),
        stopped_gates: stopped.result.completed_gates,
        stopped_consumed: stopped.result.scarce_consumed,
        stopped_lost: stopped.result.scarce_lost,
        package_bytes: bytes.len(),
        resume_gates: resumed.result.completed_gates,
        resume_consumed: resumed.result.scarce_consumed,
        resume_lost: resumed.result.scarce_lost,
        resume_scored: resumed.result.scored,
        resume_turnover: resumed.result.turnover,
    }))
}

fn dash_u64(value: Option<u64>) -> String {
    match value {
        Some(amount) => amount.to_string(),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::ablation::MechanismGap;

    fn gap(name: &'static str, with_mechanism: u64, without_mechanism: u64) -> MechanismGap {
        MechanismGap {
            name,
            with_mechanism,
            without_mechanism,
        }
    }

    fn row(
        name: &str,
        lastdown_lost: u64,
        baseline_lost: u64,
        turnover: bool,
    ) -> FixtureComparison {
        FixtureComparison {
            name: name.to_string(),
            lastdown_lost,
            baseline_lost,
            lastdown_consumed: 0,
            baseline_consumed: 0,
            lastdown_scored: false,
            baseline_scored: false,
            lastdown_duplicate: 0,
            baseline_duplicate: 0,
            lastdown_context_losses: 0,
            baseline_context_losses: 0,
            lastdown_gates: 0,
            baseline_gates: 0,
            lastdown_continuation_bytes: 0,
            baseline_continuation_bytes: 0,
            lastdown_turnover: turnover,
            baseline_turnover: false,
            lastdown_quota_remaining: None,
            baseline_quota_remaining: None,
        }
    }

    fn report(
        fixtures: Vec<FixtureComparison>,
        lost_with: u64,
        lost_without: u64,
    ) -> AblationReport {
        AblationReport {
            fixtures,
            reserve: gap("reserve", lost_with, lost_without),
            amnesia: gap("amnesia", 0, 1),
            calibration: gap("calibration", 1, 2),
        }
    }

    #[test]
    fn empty_directory_leaves_reserve_unmeasured() {
        let proof = assemble(report(Vec::new(), 0, 0), None);
        assert_eq!(proof.reserve_verdict, Verdict::Unmeasured);
        assert_eq!(proof.punt_verdict, Verdict::Unmeasured);
        assert_eq!(proof.amnesia_verdict, Verdict::Unmeasured);
    }

    #[test]
    fn equal_loss_is_unsupported() {
        let proof = assemble(report(vec![row("flat", 5, 5, false)], 5, 5), None);
        assert_eq!(proof.reserve_verdict, Verdict::Unsupported);
    }

    #[test]
    fn a_turnover_is_unsupported() {
        let proof = assemble(report(vec![row("turn", 0, 8, true)], 0, 8), None);
        assert_eq!(proof.reserve_verdict, Verdict::Unsupported);
        assert_eq!(proof.lastdown_turnovers, 1);
    }

    #[test]
    fn a_strict_loss_gap_is_supported() {
        let proof = assemble(report(vec![row("gap", 0, 8, false)], 0, 8), None);
        assert_eq!(proof.reserve_verdict, Verdict::Supported);
        let lines = proof_lines(&proof);
        assert!(lines
            .iter()
            .any(|line| line.contains("claim=reserve_conservation verdict=supported")));
        assert!(lines
            .iter()
            .any(|line| line.contains("claim=punt_advantage verdict=unmeasured")));
        assert!(lines.iter().any(|line| line.contains("wall_clock_ms=-")));
    }
}
