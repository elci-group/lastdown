use std::path::PathBuf;

use form3::term::ColorSupport;
use lastdown::jumbotron::{banner, Jumbotron};
use lastdown::sim::{bench, load_fixture};

fn fixture(name: &str) -> lastdown::sim::Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("elci/fixtures")
        .join(name);
    load_fixture(path).expect("fixture")
}

fn board(name: &str) -> Jumbotron {
    let fixture = fixture(name);
    let report = bench(&fixture).expect("bench");
    Jumbotron::from_bench(&report, &fixture)
}

#[test]
fn the_machine_frame_reports_the_drive_without_escape_bytes() {
    let board = board("token_budget.json");
    let frame = board.frame(0, ColorSupport::NoColor, false);
    assert!(!frame.contains('\x1b'));
    assert!(frame.contains("fixture=token_budget"));
    assert_eq!(board.word(), "TOUCHDOWN");
    assert!(frame.contains("█"));
    assert!(frame.contains("8000 / 8000 tokens"));
    assert!(frame.contains("lost=8000"));
    assert!(frame.contains("continuation_bytes=2940"));
    assert!(frame.lines().count() == board.height());
    assert_eq!(board.height(), 15);
}

#[test]
fn the_quota_board_keeps_the_account_window() {
    let board = board("quota_limit.json");
    assert_eq!(board.word(), "PUNT");
    let frame = board.frame(0, ColorSupport::NoColor, false);
    assert!(frame.contains("remaining=2500 limit=10000"));
    assert!(frame.contains("provider=groq"));
}

#[test]
fn animated_frames_move_the_wire_and_grow_the_bars() {
    let board = board("token_budget.json");
    let first = board.frame(0, ColorSupport::NoColor, true);
    let later = board.frame(12, ColorSupport::NoColor, true);
    assert_ne!(first, later);
    assert!(later.lines().count() == board.height());
}

#[test]
fn a_color_support_paints_truecolor_and_sgr() {
    let board = board("token_budget.json");
    let truecolor = board.frame(4, ColorSupport::Truecolor, true);
    assert!(truecolor.contains("\x1b[38;2;"));
    assert!(truecolor.contains("\x1b[92m"));
    let standard = board.frame(0, ColorSupport::Standard16, false);
    assert!(standard.contains("\x1b["));
}

#[test]
fn banner_letters_span_five_block_rows() {
    let rows = banner("TOUCHDOWN");
    assert_eq!(rows.len(), 5);
    let width = rows[0].chars().count();
    assert!(rows
        .iter()
        .all(|row| row.chars().count() == width && row.chars().all(|c| c == '█' || c == ' ')));
}
