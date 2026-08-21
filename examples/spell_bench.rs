//! Spell-checking benchmark and noise check.
//!
//! Loads the dictionary the way `F7` does, scans a document with it, and reports
//! how long each step took plus every word the scan flagged (most frequent
//! first). The timings show what a spell-check pass costs interactively; the
//! word list shows how noisy the scan is on real prose — a technical document's
//! long tail of legitimately unknown terms is expected, false positives such as
//! URLs, code, or possessives are not.
//!
//! ```sh
//! cargo run --release --example spell_bench            # scans USER-GUIDE.md
//! cargo run --release --example spell_bench notes.md
//! ```

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use pure_tui::app::load_document;
use pure_tui::config::Config;
use pure_tui::spell::SpellChecker;

fn main() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("USER-GUIDE.md"));

    let started = Instant::now();
    let checker = match SpellChecker::load(&Config::default()) {
        Ok(checker) => checker,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    println!(
        "dictionary: {} ({}) loaded in {:?}",
        checker.language(),
        checker.source(),
        started.elapsed()
    );

    let (document, _, _) = match load_document(&path) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    let started = Instant::now();
    let found = checker.check_document(&document);
    println!(
        "{}: {} paragraphs scanned in {:?}, {} words flagged",
        path.display(),
        document.paragraphs.len(),
        started.elapsed(),
        found.len()
    );

    if let Some(first) = found.first() {
        let started = Instant::now();
        let suggestions = checker.suggest(&first.word);
        println!(
            "suggestions for {:?} in {:?}: {suggestions:?}",
            first.word,
            started.elapsed()
        );
    }

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for word in &found {
        *counts.entry(word.word.as_str()).or_default() += 1;
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by_key(|(word, count)| (Reverse(*count), *word));
    for (word, count) in sorted {
        println!("  {count:4}  {word}");
    }
}
