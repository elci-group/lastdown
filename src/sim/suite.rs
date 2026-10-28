//! The benchmark suite: a seeded, paired, three-arm comparison.
//!
//! Every variant of every fixture runs under `lastdown`, a byte-capped
//! summary restart, and a naive cold restart. The harness computes one
//! waste metric identically for every arm, pairs the observations per
//! variant, and tests the two value claims with an exact sign test and a
//! deterministic bootstrap interval under pre-declared decision rules.

use std::path::Path;

use serde_json::Value;

use crate::punt::continuation::{seal, ContinuationObject};
use crate::sim::ablation::load_fixture_dir;
use crate::sim::runner::{
    committed_package, execute, resume_committed, Fixture, Kill, Mode, SimError, SimResult,
};

/// One screen of prose. The summary keeps gates until this byte budget is
/// spent at the package's own canonical bytes per committed gate.
pub const SUMMARY_BUDGET_BYTES: u64 = 1024;
const BOOTSTRAP_RESAMPLES: usize = 10_000;
const BOOTSTRAP_SEED: u64 = 0x5EED_1A57;
const MIN_PAIRS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Supported,
    Unsupported,
    Unmeasured,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
            Self::Unmeasured => "unmeasured",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArmRow {
    pub waste: u64,
    pub scored: bool,
    pub turnover: bool,
    pub replay_ok: bool,
    pub context_losses: u32,
    pub continuation_bytes: usize,
    pub summary_duplicates: u32,
}

#[derive(Clone, Debug)]
pub struct VariantRow {
    pub fixture: String,
    pub variant: String,
    pub lastdown: ArmRow,
    pub summary: Option<ArmRow>,
    pub naive: ArmRow,
}

#[derive(Clone, Debug)]
pub struct Claim {
    pub verdict: Verdict,
    pub n: usize,
    pub wins: usize,
    pub losses: usize,
    pub ties: usize,
    pub median_delta: i64,
    pub ci_low: i64,
    pub ci_high: i64,
    pub p: f64,
    pub holm_p: f64,
}

#[derive(Clone, Debug)]
pub struct SuiteReport {
    pub rows: Vec<VariantRow>,
    pub transfer: Claim,
    pub amnesia: Claim,
    pub lastdown_turnovers: u32,
    pub lastdown_replay_ok: bool,
    pub lastdown_scored_rate: f64,
    pub summary_scored_rate: f64,
    pub naive_scored_rate: f64,
}

pub fn benchmark_dir(directory: &Path) -> Result<SuiteReport, SimError> {
    let mut fixtures = load_fixture_dir(directory)?;
    fixtures.sort_by(|left, right| left.name.cmp(&right.name));
    let mut rows = Vec::new();
    for fixture in &fixtures {
        for (label, variant) in variants(fixture) {
            rows.push(run_variant(&label, &variant)?);
        }
    }
    Ok(assemble(rows))
}

fn run_variant(label: &str, fixture: &Fixture) -> Result<VariantRow, SimError> {
    let lastdown = arm_row(
        fixture,
        vec![execute(fixture, Mode::Lastdown, Kill::None)?.result],
    );
    let naive = arm_row(
        fixture,
        vec![execute(fixture, Mode::Baseline, Kill::None)?.result],
    );
    let summary = summary_arm(fixture)?;
    Ok(VariantRow {
        fixture: fixture.name.clone(),
        variant: label.to_string(),
        lastdown,
        summary,
        naive,
    })
}

fn arm_row(fixture: &Fixture, results: Vec<SimResult>) -> ArmRow {
    let consumed: u64 = results.iter().map(|result| result.scarce_consumed).sum();
    let gates = results
        .last()
        .map(|result| result.completed_gates)
        .unwrap_or(0);
    let useful: u64 = fixture.steps.iter().take(gates).map(|step| step.cost).sum();
    ArmRow {
        waste: consumed.saturating_sub(useful),
        scored: results.last().is_some_and(|result| result.scored),
        turnover: results.iter().any(|result| result.turnover),
        replay_ok: results.iter().all(|result| result.replay_ok),
        context_losses: results.iter().map(|result| result.context_losses).sum(),
        continuation_bytes: results.iter().map(|result| result.continuation_bytes).sum(),
        summary_duplicates: 0,
    }
}

fn summary_arm(fixture: &Fixture) -> Result<Option<ArmRow>, SimError> {
    let stopped = execute(fixture, Mode::Lastdown, Kill::ImmediatelyAfterCommit)?;
    let bytes = match committed_package(&stopped) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    let object: ContinuationObject =
        serde_json::from_slice(&bytes).map_err(|_| SimError::Fixture("suite"))?;
    let stopped_gates = stopped.result.completed_gates;
    let per_gate = (bytes.len() as u64 / stopped_gates.max(1) as u64).max(1);
    let keep = ((SUMMARY_BUDGET_BYTES / per_gate) as usize).min(stopped_gates);

    let mut summary = object;
    let dropped = summary.position.completed.split_off(keep);
    summary.position.unresolved.extend(dropped);
    summary.evidence.artifacts.truncate(keep);
    let summary = seal(summary).map_err(|_| SimError::Fixture("seal"))?;
    let resumed = resume_committed(fixture, &summary)?;

    let duplicates = stopped_gates.saturating_sub(keep) as u32;
    let mut row = arm_row(fixture, vec![stopped.result, resumed.result]);
    row.summary_duplicates = duplicates;
    Ok(Some(row))
}

fn variants(fixture: &Fixture) -> Vec<(String, Fixture)> {
    let mut list = vec![("base".to_string(), fixture.clone())];
    if let Some(k) = fixture.fault_at_step {
        if fixture.fault.is_some() {
            for step in [k.saturating_sub(1), k + 1] {
                if step < fixture.steps.len() {
                    let mut moved = fixture.clone();
                    moved.fault_at_step = Some(step);
                    list.push((format!("fault{step}"), moved));
                }
            }
        }
        return list;
    }
    if fixture.ingauge.is_some() {
        for delta in [-500i64, 500] {
            let mut window = fixture.clone();
            if let Some(ingauge) = window.ingauge.as_mut() {
                jitter_remaining(ingauge, delta);
            }
            list.push((format!("quota{delta:+}"), window));
        }
        return list;
    }
    for scale in [900u64, 1100] {
        let mut attempt = fixture.clone();
        attempt.min_attempt_ppm =
            ((fixture.min_attempt_ppm as u64 * scale) / 1000).clamp(1, 1_000_000) as u32;
        list.push((format!("attempt{scale}"), attempt));
    }
    list
}

fn jitter_remaining(value: &mut Value, delta: i64) {
    match value {
        Value::Object(map) => {
            let requests = map.get("metric").and_then(Value::as_str) == Some("requests");
            if !requests {
                if let Some(remaining) = map.get_mut("remaining") {
                    if let Some(amount) = remaining.as_u64() {
                        let next = if delta >= 0 {
                            amount.saturating_add(delta as u64)
                        } else {
                            amount.saturating_sub((-delta) as u64)
                        };
                        *remaining = Value::from(next);
                    }
                }
            }
            for (_, child) in map.iter_mut() {
                jitter_remaining(child, delta);
            }
        }
        Value::Array(items) => {
            for item in items {
                jitter_remaining(item, delta);
            }
        }
        _ => {}
    }
}

fn assemble(rows: Vec<VariantRow>) -> SuiteReport {
    let transfers = deltas(&rows, |row| {
        pair(&row.lastdown, row.summary.as_ref(), |arm| arm.waste)
    });
    let amnesias = deltas(&rows, |row| {
        row.summary
            .as_ref()
            .map(|arm| arm.summary_duplicates as i64)
    });
    let naive = deltas(&rows, |row| {
        pair(&row.lastdown, Some(&row.naive), |arm| arm.waste)
    });
    // Guardrails bind the mechanism arm only. The restart arms exist to
    // fail; their replay and scored numbers are the measured contrast.
    let guardrails = rows
        .iter()
        .all(|row| !row.lastdown.turnover && row.lastdown.replay_ok)
        && rows.iter().all(|row| {
            row.summary
                .as_ref()
                .is_none_or(|arm| arm.replay_ok && !arm.turnover)
        });
    let scored = |pick: fn(&VariantRow) -> &ArmRow| -> f64 {
        let total = rows.len().max(1) as f64;
        rows.iter().filter(|row| pick(row).scored).count() as f64 / total
    };
    let mut transfer = claim(&transfers);
    let mut amnesia = claim(&amnesias);
    let naive_claim = claim(&naive);
    holm_two(&mut transfer, &mut amnesia);
    if !guardrails {
        transfer.verdict = Verdict::Unsupported;
        amnesia.verdict = Verdict::Unsupported;
    }
    if naive_claim.median_delta <= 0 {
        transfer.verdict = match transfer.verdict {
            Verdict::Supported => Verdict::Unmeasured,
            other => other,
        };
    }
    let lastdown_turnovers = rows.iter().filter(|row| row.lastdown.turnover).count() as u32;
    let lastdown_replay_ok = rows.iter().all(|row| row.lastdown.replay_ok);
    let lastdown_scored_rate = scored(|row| &row.lastdown);
    let summary_scored_rate = rows
        .iter()
        .filter(|row| row.summary.as_ref().is_some_and(|arm| arm.scored))
        .count() as f64
        / rows.len().max(1) as f64;
    let naive_scored_rate = scored(|row| &row.naive);
    SuiteReport {
        rows,
        transfer,
        amnesia,
        lastdown_turnovers,
        lastdown_replay_ok,
        lastdown_scored_rate,
        summary_scored_rate,
        naive_scored_rate,
    }
}

fn pair(lastdown: &ArmRow, other: Option<&ArmRow>, pick: fn(&ArmRow) -> u64) -> Option<i64> {
    other.map(|arm| pick(arm) as i64 - pick(lastdown) as i64)
}

fn deltas(rows: &[VariantRow], pair_of: impl Fn(&VariantRow) -> Option<i64>) -> Vec<i64> {
    rows.iter().filter_map(pair_of).collect()
}

fn claim(deltas: &[i64]) -> Claim {
    let wins = deltas.iter().filter(|delta| **delta > 0).count();
    let losses = deltas.iter().filter(|delta| **delta < 0).count();
    let ties = deltas.len() - wins - losses;
    let n = deltas.len();
    let median_delta = median(deltas);
    let (ci_low, ci_high) = bootstrap(deltas);
    let p = sign_test(wins, losses);
    let verdict = if n < MIN_PAIRS {
        Verdict::Unmeasured
    } else if p <= 0.05 {
        if median_delta > 0 {
            Verdict::Supported
        } else {
            Verdict::Unsupported
        }
    } else {
        Verdict::Unmeasured
    };
    Claim {
        verdict,
        n,
        wins,
        losses,
        ties,
        median_delta,
        ci_low,
        ci_high,
        p,
        holm_p: p.min(1.0),
    }
}

fn holm_two(first: &mut Claim, second: &mut Claim) {
    let order = if first.p <= second.p { [0, 1] } else { [1, 0] };
    let claims = [first, second];
    let mut previous = 0.0;
    for (rank, index) in order.into_iter().enumerate() {
        let adjusted = ((claims[index].p * (2 - rank) as f64).min(1.0)).max(previous);
        claims[index].holm_p = adjusted;
        previous = adjusted;
    }
}

fn median(deltas: &[i64]) -> i64 {
    let mut sorted = deltas.to_vec();
    sorted.sort_unstable();
    match sorted.len() {
        0 => 0,
        1 => sorted[0],
        even => {
            let mid = even / 2;
            if even % 2 == 0 {
                (sorted[mid - 1] + sorted[mid]) / 2
            } else {
                sorted[mid]
            }
        }
    }
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn bootstrap(deltas: &[i64]) -> (i64, i64) {
    if deltas.is_empty() {
        return (0, 0);
    }
    let mut rng = XorShift(BOOTSTRAP_SEED);
    let mut medians = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
    for _ in 0..BOOTSTRAP_RESAMPLES {
        let mut sample = Vec::with_capacity(deltas.len());
        for _ in 0..deltas.len() {
            let index = (rng.next() % deltas.len() as u64) as usize;
            sample.push(deltas[index]);
        }
        medians.push(median(&sample));
    }
    medians.sort_unstable();
    let low = medians[(BOOTSTRAP_RESAMPLES as f64 * 0.025) as usize];
    let high = medians[((BOOTSTRAP_RESAMPLES as f64 * 0.975) as usize).min(medians.len() - 1)];
    (low, high)
}

fn sign_test(wins: usize, losses: usize) -> f64 {
    let trials = wins + losses;
    if trials == 0 {
        return 1.0;
    }
    let tail = wins.min(losses);
    let mut mass: u128 = 0;
    for count in 0..=tail {
        mass += choose(trials, count);
    }
    let total: u128 = 1u128 << trials.min(127);
    let p = 2.0 * mass as f64 / total as f64;
    p.min(1.0)
}

fn choose(n: usize, k: usize) -> u128 {
    let mut result: u128 = 1;
    for step in 0..k {
        result = result * (n - step) as u128 / (step + 1) as u128;
    }
    result
}

pub fn suite_lines(report: &SuiteReport) -> Vec<String> {
    let mut lines = Vec::with_capacity(report.rows.len() + 4);
    for row in &report.rows {
        let summary = row.summary.as_ref();
        lines.push(format!(
            "suite fixture={} variant={} lastdown_waste={} summary_waste={} naive_waste={} lastdown_scored={} summary_scored={} naive_scored={} summary_duplicates={} lastdown_bytes={} summary_bytes={} naive_bytes={}",
            row.fixture,
            row.variant,
            row.lastdown.waste,
            dash(summary.map(|arm| arm.waste)),
            row.naive.waste,
            row.lastdown.scored,
            summary.is_some_and(|arm| arm.scored),
            row.naive.scored,
            dash_u32(summary.map(|arm| arm.summary_duplicates)),
            row.lastdown.continuation_bytes,
            dash_bytes(summary.map(|arm| arm.continuation_bytes)),
            row.naive.continuation_bytes,
        ));
    }
    lines.push(format!(
        "suite guardrails lastdown_turnovers={} lastdown_replay_ok={} scored_rates lastdown={:.2} summary={:.2} naive={:.2}",
        report.lastdown_turnovers,
        report.lastdown_replay_ok,
        report.lastdown_scored_rate,
        report.summary_scored_rate,
        report.naive_scored_rate,
    ));
    lines.push(claim_line(
        "transfer_advantage",
        &report.transfer,
        "waste_vs_summary_and_naive",
    ));
    lines.push(claim_line(
        "amnesia_advantage",
        &report.amnesia,
        "duplicates_vs_summary",
    ));
    lines
}

fn claim_line(name: &str, claim: &Claim, reason: &str) -> String {
    format!(
        "suite claim={name} verdict={} reason={reason} n={} wins={} losses={} ties={} median_delta={} ci95=[{},{}] p={:.4} holm_p={:.4}",
        claim.verdict.label(),
        claim.n,
        claim.wins,
        claim.losses,
        claim.ties,
        claim.median_delta,
        claim.ci_low,
        claim.ci_high,
        claim.p,
        claim.holm_p,
    )
}

fn dash(value: Option<u64>) -> String {
    value
        .map(|amount| amount.to_string())
        .unwrap_or("-".to_string())
}

fn dash_bytes(value: Option<usize>) -> String {
    value
        .map(|amount| amount.to_string())
        .unwrap_or("-".to_string())
}

fn dash_u32(value: Option<u32>) -> String {
    value
        .map(|amount| amount.to_string())
        .unwrap_or("-".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_test_bounds_and_extremes() {
        assert_eq!(sign_test(0, 0), 1.0);
        assert!(sign_test(10, 0) < 0.01);
        assert_eq!(sign_test(5, 5), 1.0);
        assert!(sign_test(8, 2) <= 0.12);
    }

    #[test]
    fn median_handles_even_and_empty() {
        assert_eq!(median(&[]), 0);
        assert_eq!(median(&[3, 1]), 2);
        assert_eq!(median(&[4, 1, 2]), 2);
    }

    #[test]
    fn jitter_moves_only_token_windows() {
        let mut value = serde_json::json!({
            "data": {"snapshots": [{"remaining": [
                {"metric": "tokens", "remaining": 4500},
                {"metric": "requests", "remaining": 3}
            ]}]}
        });
        jitter_remaining(&mut value, 500);
        let tokens = value["data"]["snapshots"][0]["remaining"][0]["remaining"]
            .as_u64()
            .unwrap();
        let requests = value["data"]["snapshots"][0]["remaining"][1]["remaining"]
            .as_u64()
            .unwrap();
        assert_eq!(tokens, 5000);
        assert_eq!(requests, 3);
    }

    #[test]
    fn holm_adjusts_the_smaller_p_twice() {
        let mut a = claim(&[1i64; 10]);
        a.p = 0.01;
        let mut b = claim(&[1, 2]);
        b.p = 0.04;
        holm_two(&mut a, &mut b);
        assert_eq!(a.holm_p, 0.02);
        assert_eq!(b.holm_p, 0.04);
    }
}
