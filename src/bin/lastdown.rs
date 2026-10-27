use std::env;
use std::process::ExitCode;

use std::path::Path;

use lastdown::sim::{
    ablate_dir, bench, execute, export_handoff, load_fixture, resume_handoff, Kill, Mode,
};

const USAGE: &str = "usage: lastdown bench [fixture] | lastdown sim <fixture> | lastdown ablate [dir] | lastdown handoff <fixture> <file> | lastdown resume <fixture> <file>";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let outcome = match command.as_str() {
        "bench" => {
            let path = args
                .next()
                .unwrap_or_else(|| "elci/fixtures/token_budget.json".to_string());
            match load_fixture(&path).and_then(|fixture| {
                bench(&fixture)
                    .map_err(|error| lastdown::error::FixtureError::Io(error.to_string()))
            }) {
                Ok(report) => {
                    print_result("lastdown", &report.lastdown);
                    print_result("baseline", &report.baseline);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("bench failed: {error}");
                    ExitCode::from(1)
                }
            }
        }
        "sim" => {
            let Some(path) = args.next() else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            match load_fixture(&path).and_then(|fixture| {
                execute(&fixture, Mode::Lastdown, Kill::None)
                    .map_err(|error| lastdown::error::FixtureError::Io(error.to_string()))
            }) {
                Ok(campaign) => {
                    print_result("lastdown", &campaign.result);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("sim failed: {error}");
                    ExitCode::from(1)
                }
            }
        }
        "ablate" => {
            let path = args.next().unwrap_or_else(|| "elci/fixtures".to_string());
            match ablate_dir(Path::new(&path)) {
                Ok(report) => {
                    for row in &report.fixtures {
                        println!(
                            "fixture {} lastdown_lost={} baseline_lost={} lastdown_scored={} baseline_scored={}",
                            row.name,
                            row.lastdown_lost,
                            row.baseline_lost,
                            row.lastdown_scored,
                            row.baseline_scored
                        );
                    }
                    for gap in [&report.reserve, &report.amnesia, &report.calibration] {
                        println!(
                            "mechanism {} with={} without={}",
                            gap.name, gap.with_mechanism, gap.without_mechanism
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("ablate failed: {error}");
                    ExitCode::from(1)
                }
            }
        }
        "handoff" => {
            let Some(fixture_path) = args.next() else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            let Some(out) = args.next() else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            match load_and_handoff(&fixture_path, &out) {
                Ok(campaign) => {
                    print_result("lastdown", &campaign.result);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("handoff failed: {error}");
                    ExitCode::from(1)
                }
            }
        }
        "resume" => {
            let Some(fixture_path) = args.next() else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            let Some(file) = args.next() else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            match load_fixture(&fixture_path).and_then(|fixture| {
                resume_handoff(&fixture, Path::new(&file))
                    .map_err(|error| lastdown::error::FixtureError::Io(error.to_string()))
            }) {
                Ok(campaign) => {
                    print_result("lastdown", &campaign.result);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("resume failed: {error}");
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    };
    outcome
}

fn load_and_handoff(
    fixture_path: &str,
    out: &str,
) -> Result<lastdown::sim::Campaign, lastdown::error::FixtureError> {
    let fixture = load_fixture(fixture_path)?;
    let campaign = execute(&fixture, Mode::Lastdown, Kill::ImmediatelyAfterCommit)
        .map_err(|error| lastdown::error::FixtureError::Io(error.to_string()))?;
    export_handoff(&campaign, Path::new(out))
        .map_err(|error| lastdown::error::FixtureError::Io(error.to_string()))?;
    Ok(campaign)
}

fn print_result(label: &str, result: &lastdown::sim::SimResult) {
    println!(
        "{label} fixture={} status={} scored={} scarce_consumed={} scarce_lost={} artifacts={} contradictions={} context_losses={} duplicate_steps={} replay_ok={} continuation_bytes={}",
        result.fixture,
        result.status,
        result.scored,
        result.scarce_consumed,
        result.scarce_lost,
        result.artifacts,
        result.contradictions,
        result.context_losses,
        result.duplicate_steps,
        result.replay_ok,
        result.continuation_bytes
    );
}
