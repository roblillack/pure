//! Tests for dictionary loading, tokenization, and document scanning.
//!
//! They run against the bundled en_US dictionary (the default
//! `bundled-dictionary` feature), so results are the same on every machine.
//! Parsing it takes a moment, so the checker is built once and shared.

use std::sync::OnceLock;

use tdoc::ftml;

use super::*;

fn checker() -> &'static SpellChecker {
    static CHECKER: OnceLock<SpellChecker> = OnceLock::new();
    CHECKER.get_or_init(|| {
        SpellChecker::from_str(
            include_str!("../dictionaries/en_US/en_US.aff"),
            include_str!("../dictionaries/en_US/en_US.dic"),
            "en_US",
            "test en_US",
        )
        .expect("the bundled dictionary parses")
    })
}

/// The words a scan reports, in document order.
fn misspelled(document: &Document) -> Vec<String> {
    checker()
        .check_document(document)
        .into_iter()
        .map(|m| m.word)
        .collect()
}

#[test]
fn known_words_pass_and_typos_dont() {
    let checker = checker();
    assert!(!checker.is_misspelled("essentials"));
    assert!(!checker.is_misspelled("Essentials"), "capitalized is fine");
    assert!(
        !checker.is_misspelled("don't"),
        "apostrophes are word chars"
    );
    assert!(!checker.is_misspelled("don’t"), "typographic ones too");
    assert!(checker.is_misspelled("essentails"));
    assert!(checker.is_misspelled("packk"));
}

#[test]
fn acronyms_short_words_and_identifiers_are_left_alone() {
    let checker = checker();
    // Acronyms: no dictionary can hold every one a document mentions.
    assert!(!checker.is_misspelled("FTML"));
    assert!(!checker.is_misspelled("TUI"));
    // Single letters are never worth reporting.
    assert!(!checker.is_misspelled("q"));
    // An interior capital reads as an identifier, not a misspelling…
    assert!(!checker.is_misspelled("wrapWidth"));
    // …but a known name with one still passes on its own merits.
    assert!(!checker.is_misspelled("McDonald"));
}

#[test]
fn suggestions_offer_the_intended_word_first() {
    let suggestions = checker().suggest("essentails");
    assert_eq!(
        suggestions.first().map(String::as_str),
        Some("essentials"),
        "got {suggestions:?}"
    );
    assert!(
        suggestions.len() <= MAX_SUGGESTIONS,
        "the list is capped: {suggestions:?}"
    );
}

#[test]
fn added_and_ignored_words_stop_being_reported() {
    let mut checker = SpellChecker::from_str(
        include_str!("../dictionaries/en_US/en_US.aff"),
        include_str!("../dictionaries/en_US/en_US.dic"),
        "en_US",
        "test en_US",
    )
    .expect("the bundled dictionary parses");
    assert!(checker.is_misspelled("rutle"));

    checker.ignore_word("rutle");
    assert!(!checker.is_misspelled("rutle"));
    assert!(
        !checker.is_misspelled("Rutle"),
        "ignoring a word ignores its capitalized form too"
    );

    // With no personal word list configured, adding is session-only and can't fail.
    assert!(checker.is_misspelled("ftml"));
    checker.add_word("ftml").expect("session-only add");
    assert!(!checker.is_misspelled("ftml"));
}

#[test]
fn add_word_appends_to_the_personal_word_list() {
    let dir = std::env::temp_dir().join("pure_spell_personal_test");
    let path = dir.join("dictionary.txt");
    let _ = fs::remove_dir_all(&dir);

    let mut checker = SpellChecker::from_str(
        include_str!("../dictionaries/en_US/en_US.aff"),
        include_str!("../dictionaries/en_US/en_US.dic"),
        "en_US",
        "test en_US",
    )
    .expect("the bundled dictionary parses");
    checker.personal_path = Some(path.clone());
    checker.add_word("Vizzlo").expect("write the word list");
    assert_eq!(
        fs::read_to_string(&path).expect("word list written"),
        "Vizzlo\n"
    );

    // A fresh checker picks the word up again from the file.
    let mut reloaded = SpellChecker::from_str(
        include_str!("../dictionaries/en_US/en_US.aff"),
        include_str!("../dictionaries/en_US/en_US.dic"),
        "en_US",
        "test en_US",
    )
    .expect("the bundled dictionary parses");
    assert!(reloaded.is_misspelled("Vizzlo"));
    reloaded.personal_path = Some(path);
    reloaded.load_personal_words();
    assert!(!reloaded.is_misspelled("Vizzlo"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scanning_reports_words_in_document_order_with_their_offsets() {
    let document = ftml! {
        h1 { "Packing Lst" }
        p { "Pack the " b { "essentails" } " before the trip." }
    };
    let found = checker().check_document(&document);
    assert_eq!(
        found.iter().map(|m| m.word.as_str()).collect::<Vec<_>>(),
        ["Lst", "essentails"]
    );
    // Offsets index the leaf's flattened plain text, styling included: the
    // heading's typo starts at "Packing ", the paragraph's inside the bold span.
    assert_eq!((found[0].start, found[0].end), (8, 11));
    assert_eq!((found[1].start, found[1].end), (9, 19));
    assert_ne!(found[0].path, found[1].path, "different paragraphs");
}

#[test]
fn code_is_not_prose() {
    let document = ftml! {
        p { "Run the " code { "wrapp_width" } " helper on teh document." }
        code { "let mispeled = 1;" }
    };
    // Only the prose typo is reported: the inline code span and the code block
    // are skipped whole.
    assert_eq!(misspelled(&document), ["teh"]);
}

#[test]
fn urls_paths_and_versions_are_skipped() {
    let document = ftml! {
        p { "See https://exampl.test/aa and rob@exampl.test or src/app.rs, v1.2 — teh end." }
    };
    assert_eq!(misspelled(&document), ["teh"]);
}

#[test]
fn nested_structure_is_scanned_too() {
    let document = ftml! {
        ul {
            li { p { "Passport annd tickets" } }
        }
        quote {
            p { "Travle light." }
        }
    };
    assert_eq!(misspelled(&document), ["annd", "Travle"]);
}

#[test]
fn tables_are_skipped_because_they_are_read_only() {
    let document = tdoc::markdown::parse(std::io::Cursor::new(
        "Intro with a typoo.\n\n| Name | Note |\n| --- | --- |\n| Alice | tabel typoo |\n",
    ))
    .expect("parse markdown");
    assert_eq!(misspelled(&document), ["typoo"]);
}

#[test]
fn words_keeps_offsets_and_trims_punctuation() {
    // Offsets are byte offsets, so the multi-byte dash and quotes shift them.
    let text = "Hello, world — don't “quote” dogs' F7 3D";
    assert_eq!(
        words(text),
        [
            (0, "Hello"),
            (7, "world"),
            // Apostrophes stay inside a word, but a trailing one is dropped.
            (17, "don't"),
            (26, "quote"),
            (35, "dogs"),
            // Digits end a word; what's left is too short to ever be reported.
            (41, "F"),
            (45, "D"),
        ]
    );
}

#[test]
fn possessives_of_known_words_pass() {
    let checker = checker();
    // Only nouns carry the dictionary's `'s` flag, so these need the stem check.
    assert!(!checker.is_misspelled("Pure's"));
    assert!(
        !checker.is_misspelled("Pure’s"),
        "typographic apostrophe too"
    );
    // A typo stays a typo, possessive or not.
    assert!(checker.is_misspelled("essentails's"));
}

#[test]
fn prose_chunks_exclude_code_shaped_text() {
    assert!(is_prose_chunk("word"));
    assert!(is_prose_chunk("sentence."));
    assert!(is_prose_chunk("and/or"));
    assert!(!is_prose_chunk("https://example.test"));
    assert!(!is_prose_chunk("rob@example.test"));
    assert!(!is_prose_chunk("www.example.test"));
    assert!(!is_prose_chunk("src/app.rs"));
    assert!(!is_prose_chunk("wrap_width"));
    assert!(!is_prose_chunk("1.2.3"));
    // File extensions written on their own, as in "(.ftml, .md)".
    assert!(!is_prose_chunk(".ftml,"));
    assert!(!is_prose_chunk(".md"));
    // But an ellipsis or a leading dash is still prose.
    assert!(is_prose_chunk("word…"));
    // Chunks starting with multi-byte characters must not be sliced blindly
    // while looking for a `www.` prefix.
    assert!(is_prose_chunk("…é"));
    assert!(is_prose_chunk("„Wörter“"));
    assert!(!is_prose_chunk("WWW.example.test"));
}

#[test]
fn context_windows_center_the_word_and_mark_trimmed_ends() {
    let text = "Pack the essentails before the long trip across the continent.";
    let window = context_window(text, 9, 19, 30);
    let (before, word, after) = window.parts();
    assert_eq!(word, "essentails");
    assert!(
        window.text.starts_with("Pack") && before.starts_with("Pack"),
        "the start fits, so no leading ellipsis: {:?}",
        window.text
    );
    assert!(
        window.text.ends_with('…') && !after.is_empty(),
        "the end is trimmed: {:?}",
        window.text
    );
    assert!(
        window.text.chars().count() <= 30,
        "window stays within its width: {:?}",
        window.text
    );

    // A short paragraph is shown whole, hard breaks flattened to spaces.
    let window = context_window("a\nb typoo", 4, 9, 40);
    assert_eq!(window.text, "a b typoo");
    assert_eq!(window.parts().1, "typoo");
}

#[test]
fn language_tags_normalize_to_the_dictionary_file_shape() {
    assert_eq!(normalize_language("en_US"), "en_US");
    assert_eq!(normalize_language("en-us"), "en_US");
    assert_eq!(normalize_language(" DE_de "), "de_DE");
    assert_eq!(normalize_language("eo"), "eo");
}

#[test]
fn a_missing_dictionary_explains_where_to_put_one() {
    let error = SpellError::NoDictionary {
        language: "de_DE".to_string(),
        dictionary_dir: Some(PathBuf::from("/home/rob/.config/pure/dictionaries")),
    };
    let message = error.to_string();
    assert!(message.contains("de_DE.aff"), "{message}");
    assert!(
        message.contains("/home/rob/.config/pure/dictionaries"),
        "{message}"
    );
}

#[cfg(feature = "bundled-dictionary")]
#[test]
fn the_bundled_dictionary_is_found_without_any_config() {
    // Only meaningful where the user hasn't dropped their own en_US dictionary
    // into `~/.config/pure/dictionaries`; that copy legitimately wins.
    let checker = SpellChecker::discover("en_US").expect("en_US resolves");
    assert_eq!(checker.language(), "en_US");
    assert!(!checker.is_misspelled("essentials"));
}
