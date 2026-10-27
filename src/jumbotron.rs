//! Stadium jumbotron for a fixture run, built on `form3` animation
//! primitives.
//!
//! Big block letters name the outcome, the objective rolls on the wire,
//! and the budget and quota bars grow to the final drive numbers.
//! A terminal gets a short frame loop; a pipe gets one deterministic
//! frame with color decided by the environment.

use std::io::{self, Write};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use form3::anim::{interval_ms, Aura, ProgressBar, ProgressStyle, Spinner, BRAILLE};
use form3::ansi;
use form3::compat::Colorize;
use form3::term::{ColorSupport, TermInfo};

use crate::sim::{bench, load_fixture, BenchReport, Fixture, SimError, SimResult};
use crate::DriveStatus;

const FRAMES: u64 = 24;
const LABEL_WIDTH: usize = 9;
const MARQUEE_WIDTH: usize = 44;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accent {
    Score,
    Goal,
    Punt,
    Turnover,
    Neutral,
}

impl Accent {
    fn aura(self) -> Aura {
        match self {
            Self::Score => Aura::new((0, 80, 40), (34, 197, 94), (214, 255, 231)),
            Self::Goal => Aura::new((90, 60, 0), (250, 204, 21), (255, 245, 200)),
            Self::Punt => Aura::new((0, 60, 90), (56, 189, 248), (214, 245, 255)),
            Self::Turnover => Aura::new((90, 10, 10), (239, 68, 68), (255, 220, 220)),
            Self::Neutral => Aura::new((60, 60, 60), (180, 180, 180), (245, 245, 245)),
        }
    }

    fn tint(self, text: &str, support: ColorSupport) -> String {
        let styled = match self {
            Self::Score => text.bright_green(),
            Self::Goal => text.bright_yellow(),
            Self::Punt => text.bright_cyan(),
            Self::Turnover => text.bright_red(),
            Self::Neutral => text.bright_white(),
        };
        styled.with_color_support(support).to_string()
    }
}

#[derive(Clone, Debug)]
struct Arm {
    status: &'static str,
    scored: bool,
    consumed: u64,
    lost: u64,
    gates: usize,
    packages: usize,
    replay_ok: bool,
    artifacts: usize,
    contradictions: usize,
    context_losses: u32,
    duplicate_steps: u32,
    continuation_bytes: usize,
    quota_remaining: Option<u64>,
}

fn arm(result: &SimResult) -> Arm {
    Arm {
        status: status_label(&result.status),
        scored: result.scored,
        consumed: result.scarce_consumed,
        lost: result.scarce_lost,
        gates: result.completed_gates,
        packages: result.committed_packages,
        replay_ok: result.replay_ok,
        artifacts: result.artifacts,
        contradictions: result.contradictions,
        context_losses: result.context_losses,
        duplicate_steps: result.duplicate_steps,
        continuation_bytes: result.continuation_bytes,
        quota_remaining: result.quota_remaining,
    }
}

fn status_label(status: &DriveStatus) -> &'static str {
    match status {
        DriveStatus::Kickoff => "kickoff",
        DriveStatus::Normal => "normal",
        DriveStatus::Redzone => "redzone",
        DriveStatus::Lastdown => "lastdown",
        DriveStatus::Attempt => "attempt",
        DriveStatus::FieldGoal => "field-goal",
        DriveStatus::Reset => "reset",
        DriveStatus::Punt => "punt",
        DriveStatus::BlindScout => "blind-scout",
        DriveStatus::Route => "route",
        DriveStatus::Score => "score",
        DriveStatus::Turnover => "turnover",
    }
}

fn status_word(result: &SimResult) -> (&'static str, Accent) {
    if result.turnover {
        ("TURNOVER", Accent::Turnover)
    } else if result.scored {
        ("TOUCHDOWN", Accent::Score)
    } else if result.field_goal {
        ("FIELD GOAL", Accent::Goal)
    } else {
        match result.status {
            DriveStatus::Score => ("TOUCHDOWN", Accent::Score),
            DriveStatus::FieldGoal => ("FIELD GOAL", Accent::Goal),
            DriveStatus::Turnover => ("TURNOVER", Accent::Turnover),
            DriveStatus::Punt => ("PUNT", Accent::Punt),
            _ => ("IN DRIVE", Accent::Neutral),
        }
    }
}

fn glyph(letter: char) -> [&'static str; 5] {
    match letter {
        'A' => [".##.", "#..#", "####", "#..#", "#..#"],
        'B' => ["###.", "#..#", "###.", "#..#", "###."],
        'C' => [".###", "#...", "#...", "#...", ".###"],
        'D' => ["###.", "#..#", "#..#", "#..#", "###."],
        'E' => ["####", "#...", "###.", "#...", "####"],
        'F' => ["####", "#...", "###.", "#...", "#..."],
        'G' => [".###", "#...", "#.##", "#..#", ".###"],
        'H' => ["#..#", "#..#", "####", "#..#", "#..#"],
        'I' => ["###", ".#.", ".#.", ".#.", "###"],
        'J' => ["..##", "...#", "...#", "#..#", ".##."],
        'K' => ["#..#", "#..#", "###.", "#..#", "#..#"],
        'L' => ["#...", "#...", "#...", "#...", "####"],
        'M' => ["#...#", "##.##", "#.#.#", "#...#", "#...#"],
        'N' => ["#...#", "##..#", "#.#.#", "#..##", "#...#"],
        'O' => [".###.", "#...#", "#...#", "#...#", ".###."],
        'P' => ["###.", "#..#", "###.", "#...", "#..."],
        'Q' => [".###.", "#...#", "#.#.#", "#..#.", ".##.#"],
        'R' => ["###.", "#..#", "###.", "#..#", "#..#"],
        'S' => [".###", "#...", ".##.", "...#", "###."],
        'T' => ["#####", "..#..", "..#..", "..#..", "..#.."],
        'U' => ["#...#", "#...#", "#...#", "#...#", ".###."],
        'V' => ["#...#", "#...#", "#...#", ".#.#.", "..#.."],
        'W' => ["#...#", "#...#", "#.#.#", "##.##", "#...#"],
        'X' => ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"],
        'Y' => ["#...#", ".#.#.", "..#..", "..#..", "..#.."],
        'Z' => ["####", "...#", "..#.", ".#..", "####"],
        '-' => ["....", "....", "####", "....", "...."],
        _ => ["..", "..", "..", "..", ".."],
    }
}

/// Five rows of block letters. Unknown characters render as a gap.
pub fn banner(word: &str) -> Vec<String> {
    let mut rows = vec![String::new(); 5];
    for letter in word.chars() {
        let cells = glyph(letter.to_ascii_uppercase());
        for (index, row) in rows.iter_mut().enumerate() {
            if !row.is_empty() {
                row.push_str("  ");
            }
            row.push_str(&cells[index].replace('#', "█").replace('.', " "));
        }
    }
    rows
}

fn marquee(text: &str, width: usize, tick: u64) -> String {
    let unit: Vec<char> = format!("{}  ♦  ", text.trim()).chars().collect();
    if unit.is_empty() || width == 0 {
        return String::new();
    }
    let start = tick as usize % unit.len();
    (0..width)
        .map(|step| unit[(start + step) % unit.len()])
        .collect()
}

fn progress(tick: u64, animate: bool) -> f32 {
    if animate {
        ((tick % FRAMES) + 1) as f32 / FRAMES as f32
    } else {
        1.0
    }
}

fn grow(value: u64, step: f32) -> u64 {
    (value as f32 * step) as u64
}

fn bar(length: u64, position: u64, style: ProgressStyle) -> String {
    let mut bar = ProgressBar::new(length);
    style.apply(&mut bar);
    bar.set_position(position);
    bar.to_string()
}

/// A percent-free twenty-cell gauge. The quota row must stay under eighty
/// columns so a redraw never wraps and smears the loop.
fn gauge(length: u64, position: u64) -> String {
    let mut bar = ProgressBar::new(length);
    bar.set_template("{bar}");
    bar.set_bar_style('─', '·', 20);
    bar.set_position(position);
    bar.to_string()
}

fn label(text: &str, support: ColorSupport) -> String {
    format!("{text:<LABEL_WIDTH$}")
        .bright_black()
        .with_color_support(support)
        .to_string()
}

fn row(label_text: &str, value: String, support: ColorSupport) -> String {
    format!("{} {value}", label(label_text, support))
}

/// A rendered drive board for one fixture bench.
pub struct Jumbotron {
    fixture: String,
    objective: String,
    word: &'static str,
    accent: Accent,
    lastdown: Arm,
    baseline: Arm,
    gates_total: usize,
    script: u64,
    quota_limit: Option<u64>,
    quota_provider: Option<String>,
    spinner: Spinner,
}

impl Jumbotron {
    pub fn from_bench(report: &BenchReport, fixture: &Fixture) -> Self {
        let (word, accent) = status_word(&report.lastdown);
        let reading = fixture
            .ingauge
            .as_ref()
            .map(crate::quota::read_ingauge)
            .filter(|reading| reading.engaged);
        Self {
            fixture: fixture.name.clone(),
            objective: fixture.objective.clone(),
            word,
            accent,
            lastdown: arm(&report.lastdown),
            baseline: arm(&report.baseline),
            gates_total: fixture.gates.len(),
            script: fixture.steps.iter().map(|step| step.cost).sum(),
            quota_limit: reading.as_ref().and_then(|reading| reading.limit),
            quota_provider: reading.and_then(|reading| reading.provider),
            spinner: Spinner::default(),
        }
    }

    pub fn word(&self) -> &'static str {
        self.word
    }

    /// Lines in one frame. The frame shape never changes, so the loop can
    /// walk the cursor up this many rows between frames.
    pub fn height(&self) -> usize {
        self.frame(0, ColorSupport::NoColor, false).lines().count()
    }

    pub fn frame(&self, tick: u64, support: ColorSupport, animate: bool) -> String {
        let step = progress(tick, animate);
        let spin = if animate {
            self.spinner.frame(tick)
        } else {
            "▪"
        };

        let mut lines = Vec::with_capacity(14);
        lines.push(format!(
            "{} {}",
            "LASTDOWN JUMBOTRON"
                .bright_cyan()
                .bold()
                .with_color_support(support),
            format!("fixture={} [{spin}]", self.fixture)
                .bright_black()
                .with_color_support(support),
        ));

        let aura = self.accent.aura();
        for (index, banner_row) in banner(self.word).iter().enumerate() {
            let glow = (((index + 1) as f32 / 5.0) * step).clamp(0.35, 1.0);
            if support == ColorSupport::NoColor {
                lines.push(banner_row.clone());
            } else {
                lines.push(aura.paint(banner_row, glow, support));
            }
        }

        let drive = &self.lastdown;
        lines.push(row(
            "drive",
            format!(
                "status={} scored={} packages={} replay={}",
                drive.status,
                drive.scored,
                drive.packages,
                if drive.replay_ok { "ok" } else { "bad" },
            ),
            support,
        ));
        lines.push(row(
            "consumed",
            format!("{} / {} tokens", drive.consumed, self.script),
            support,
        ));
        lines.push(row(
            "budget",
            self.accent.tint(
                &bar(
                    self.script,
                    grow(drive.consumed, step),
                    ProgressStyle::Default,
                ),
                support,
            ),
            support,
        ));
        lines.push(row(
            "lost",
            format!(
                "{}  context_losses={}  duplicate_steps={}",
                drive.lost, drive.context_losses, drive.duplicate_steps,
            ),
            support,
        ));
        lines.push(row(
            "gates",
            format!(
                "{}/{}  artifacts={}  contradictions={}",
                drive.gates, self.gates_total, drive.artifacts, drive.contradictions,
            ),
            support,
        ));
        lines.push(row(
            "package",
            format!("continuation_bytes={}", drive.continuation_bytes),
            support,
        ));
        lines.push(row(
            "baseline",
            format!(
                "scored={} lost={}",
                self.baseline.scored, self.baseline.lost
            ),
            support,
        ));
        lines.push(row("quota", self.quota_text(step), support));
        lines.push(row(
            "wire",
            self.accent.tint(
                &marquee(&self.objective, MARQUEE_WIDTH, tick_animate(tick, animate)),
                support,
            ),
            support,
        ));

        lines.join("\n")
    }

    fn quota_text(&self, step: f32) -> String {
        match (self.lastdown.quota_remaining, self.quota_limit) {
            (Some(remaining), Some(limit)) => {
                let provider = self.quota_provider.as_deref().unwrap_or("-");
                let fill = gauge(limit, grow(limit.saturating_sub(remaining), step));
                format!("provider={provider} remaining={remaining} limit={limit} {fill}")
            }
            (Some(remaining), None) => format!("remaining={remaining} limit=-"),
            (None, _) => "-".to_string(),
        }
    }
}

fn tick_animate(tick: u64, animate: bool) -> u64 {
    if animate {
        tick
    } else {
        0
    }
}

/// Run the fixture bench and put it on the board.
pub fn run(path: &Path) -> Result<(), SimError> {
    let fixture = load_fixture(path)?;
    let report = bench(&fixture)?;
    let board = Jumbotron::from_bench(&report, &fixture);
    let info = TermInfo::detect();
    if !info.is_tty {
        println!("{}", board.frame(0, info.color_support, false));
        return Ok(());
    }
    let height = board.height() as u16;
    let delay = Duration::from_millis(interval_ms(&BRAILLE));
    let mut out = io::stdout();
    let _ = write!(out, "{}", ansi::hide_cursor());
    for tick in 0..=FRAMES {
        if tick > 0 {
            let _ = write!(
                out,
                "{}{}",
                ansi::move_up(height),
                ansi::clear_to_end_of_screen()
            );
        }
        let _ = writeln!(out, "{}", board.frame(tick, info.color_support, true));
        let _ = out.flush();
        sleep(delay);
    }
    let _ = write!(
        out,
        "{}{}",
        ansi::move_up(height),
        ansi::clear_to_end_of_screen()
    );
    let _ = writeln!(out, "{}", board.frame(FRAMES, info.color_support, false));
    let _ = write!(out, "{}", ansi::show_cursor());
    let _ = out.flush();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_characters_render_as_gaps() {
        assert_eq!(banner("a?b")[0], " ██       ███ ");
    }

    #[test]
    fn banner_rows_share_one_width() {
        let rows = banner("Punt");
        assert_eq!(rows.len(), 5);
        let width = rows[0].chars().count();
        assert!(width > 0);
        assert!(rows
            .iter()
            .all(|row| row.chars().count() == width && row.chars().all(|c| c == '█' || c == ' ')));
    }

    #[test]
    fn marquee_windows_wrap_the_objective() {
        let first = marquee("abc", 4, 0);
        let second = marquee("abc", 4, 1);
        assert_eq!(first.chars().count(), 4);
        assert_eq!(second.chars().count(), 4);
        assert_ne!(first, second);
        let unit_len = "abc  ♦  ".chars().count() as u64;
        assert_eq!(marquee("abc", 4, unit_len), first);
    }

    #[test]
    fn static_progress_is_complete() {
        assert_eq!(progress(0, false), 1.0);
        assert_eq!(progress(0, true), 1.0 / FRAMES as f32);
    }
}
