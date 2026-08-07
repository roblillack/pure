//! Cursor-movement benchmark.
//!
//! Opens a (large) document in a headless `App` backed by ratatui's
//! `TestBackend` — a mock terminal — and measures how long it takes to move
//! the cursor from the top of the document to the bottom, pressing `Down` as
//! fast as possible. Each keypress goes through the real event handler and is
//! followed by a real redraw, so the measurement captures the full
//! per-keystroke cost (cursor re-layout + viewport render), exactly what the
//! interactive editor does while you hold the Down arrow.
//!
//! Every file is measured twice: once with reveal-codes (reveal tags) off and
//! once with it on (toggled via F9), since revealing the markup changes the
//! layout work per line.
//!
//! This example uses only the public `App` API and so compiles unchanged on
//! both the shared-`tdoc-editor` branch and the homegrown `main` branch, which
//! lets the two implementations be compared directly:
//!
//! ```sh
//! cargo run --release --example bench_cursor
//! cargo run --release --example bench_cursor -- USER-GUIDE.md
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use pure_tui::app::{App, load_document};

/// Mock-terminal geometry. A typical editor window; wide enough that the
/// responsive page margin kicks in, matching real usage.
const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;

/// Consecutive unchanged cursor positions that mean "we've hit the bottom".
const STABLE: usize = 8;
/// Defensive cap so calibration can never spin forever.
const MAX_STEPS: usize = 200_000;

const WARMUP_RUNS: usize = 2;
const MEASURED_RUNS: usize = 7;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Build a fresh app showing `path`, draw the first frame, and optionally
/// enable reveal codes. The initial parse + draw is intentionally *outside*
/// any timed region.
fn build_app(path: &PathBuf, reveal: bool) -> (App, Terminal<TestBackend>) {
    let (document, format, _) =
        load_document(path).unwrap_or_else(|e| panic!("load {}: {e}", path.display()));
    let mut app = App::new(document, Some(path.clone()), format, None);
    app.set_interactive(false);

    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("test terminal");
    terminal.draw(|f| app.draw(f)).expect("initial draw");

    if reveal {
        app.handle_event(key(KeyCode::F(9))).expect("toggle reveal");
        terminal.draw(|f| app.draw(f)).expect("draw after reveal");
    }

    (app, terminal)
}

/// One Down keystroke through the real event path, followed by a real redraw.
fn step_down(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    app.handle_event(key(KeyCode::Down)).expect("handle Down");
    terminal.draw(|f| app.draw(f)).expect("redraw");
}

/// The full rendered frame as text (every cell symbol). The status bar carries
/// the cursor's content line number, so this string changes on every press
/// that actually advances the cursor — and stops changing once we can no
/// longer move down — regardless of where the on-screen caret settles while
/// the viewport scrolls underneath it.
fn frame_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    let mut out = String::with_capacity((area.width as usize + 1) * area.height as usize);
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// How many Down presses it takes to reach the bottom of the document — the
/// point after which the rendered frame stops changing.
fn calibrate(path: &PathBuf, reveal: bool) -> usize {
    let (mut app, mut terminal) = build_app(path, reveal);
    let mut last = frame_text(&terminal);
    let mut unchanged = 0usize;
    let mut last_progress = 0usize;

    for step in 1..=MAX_STEPS {
        step_down(&mut app, &mut terminal);
        let now = frame_text(&terminal);
        if now == last {
            unchanged += 1;
            if unchanged >= STABLE {
                break;
            }
        } else {
            unchanged = 0;
            last_progress = step;
            last = now;
        }
    }
    last_progress.max(1)
}

/// Time `steps` Down presses (each with a redraw) on a freshly built app.
fn time_traversal(path: &PathBuf, reveal: bool, steps: usize) -> Duration {
    let (mut app, mut terminal) = build_app(path, reveal);
    let start = Instant::now();
    for _ in 0..steps {
        step_down(&mut app, &mut terminal);
    }
    start.elapsed()
}

fn median(mut xs: Vec<Duration>) -> Duration {
    xs.sort();
    xs[xs.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let files: Vec<PathBuf> = if args.is_empty() {
        vec![
            PathBuf::from("USER-GUIDE.md"),
            PathBuf::from("ARCHITECTURE.md"),
            PathBuf::from("README.md"),
        ]
    } else {
        args.iter().map(PathBuf::from).collect()
    };

    println!(
        "Cursor-down benchmark — mock terminal {WIDTH}x{HEIGHT}, \
         {MEASURED_RUNS} measured runs (after {WARMUP_RUNS} warmup)\n"
    );
    println!(
        "{:<22} {:>7} {:>8} {:>11} {:>11} {:>11}",
        "file / reveal", "steps", "runs", "min", "median", "per-key"
    );
    println!("{}", "-".repeat(74));

    for file in &files {
        if !file.exists() {
            eprintln!("skip {} (not found)", file.display());
            continue;
        }
        for reveal in [false, true] {
            let steps = calibrate(file, reveal);

            for _ in 0..WARMUP_RUNS {
                let _ = time_traversal(file, reveal, steps);
            }
            let mut samples = Vec::with_capacity(MEASURED_RUNS);
            for _ in 0..MEASURED_RUNS {
                samples.push(time_traversal(file, reveal, steps));
            }

            let min = *samples.iter().min().unwrap();
            let med = median(samples);
            let per_key = med / steps.max(1) as u32;

            let label = format!(
                "{} [{}]",
                file.file_name().unwrap().to_string_lossy(),
                if reveal { "reveal" } else { "plain " }
            );
            println!(
                "{label:<22} {steps:>7} {:>8} {:>9.2?} {:>9.2?} {:>9.2?}",
                MEASURED_RUNS, min, med, per_key,
            );
        }
    }
}
