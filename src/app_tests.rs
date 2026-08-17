//! Format routing tests for load/save: extension detection plus full
//! save → reload round-trips through each supported writer/parser.

use std::io::Cursor;

use super::*;

/// All span text in a document, concatenated, for content assertions that
/// don't care about exact structure or styling. Styled spans carry their text
/// in child spans, so this recurses.
fn doc_text(document: &Document) -> String {
    fn push_span(out: &mut String, span: &tdoc::Span) {
        out.push_str(&span.text);
        for child in &span.children {
            push_span(out, child);
        }
    }

    let mut out = String::new();
    for paragraph in &document.paragraphs {
        for span in paragraph.content() {
            push_span(&mut out, span);
        }
    }
    out
}

/// Save `document` under a temp file with `extension`, reload it, and return
/// the reloaded document together with the format `load_document` detected.
///
/// The file name carries a per-call counter: tests run in parallel, and two of
/// them round-tripping the same extension would otherwise race on one path.
fn save_then_load(extension: &str, document: Document) -> (Document, DocumentFormat) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("pure_roundtrip_test_{id}.{extension}"));
    let _ = fs::remove_file(&path);

    let format = DocumentFormat::from_path(&path);
    let mut app = App::new(document, Some(path.clone()), format, None);
    app.set_interactive(false);
    app.save().expect("save document");

    let (reloaded, detected, _) = load_document(&path).expect("reload document");
    let _ = fs::remove_file(&path);
    (reloaded, detected)
}

#[test]
fn from_path_detects_every_format() {
    let cases = [
        ("notes.md", DocumentFormat::Markdown),
        ("notes.markdown", DocumentFormat::Markdown),
        ("page.html", DocumentFormat::Html),
        ("page.htm", DocumentFormat::Html),
        ("PAGE.HTML", DocumentFormat::Html),
        ("capsule.gmi", DocumentFormat::Gemini),
        ("capsule.gemini", DocumentFormat::Gemini),
        ("doc.ftml", DocumentFormat::Ftml),
        ("README", DocumentFormat::Ftml),
    ];
    for (name, expected) in cases {
        assert_eq!(
            DocumentFormat::from_path(Path::new(name)),
            expected,
            "extension mapping for {name}"
        );
    }
}

#[test]
fn html_round_trips_through_save_and_load() {
    let source =
        parse(Cursor::new("<h1>Title</h1><p>Hello <b>bold</b> world.</p>")).expect("parse source");

    let (reloaded, format) = save_then_load("html", source);

    assert_eq!(format, DocumentFormat::Html);
    let text = doc_text(&reloaded);
    assert!(text.contains("Title"), "heading survived: {text:?}");
    assert!(text.contains("Hello"), "body survived: {text:?}");
    assert!(text.contains("bold"), "inline run survived: {text:?}");

    // HTML can express inline styles, so bold should survive as a style.
    let has_bold = reloaded
        .paragraphs
        .iter()
        .flat_map(|paragraph| paragraph.content().iter())
        .any(|span| span.style == InlineStyle::Bold);
    assert!(has_bold, "bold styling survived the round-trip");
}

/// The document used for the rule/definition-list round-trips: a rule between
/// two paragraphs, and a two-item definition list.
fn rules_and_definitions_document() -> Document {
    tdoc::markdown::parse(Cursor::new(
        "Intro\n\n---\n\nCoffee\n: A black hot drink.\n\nTea\n: A leaf infusion.\n\nOutro\n",
    ))
    .expect("parse markdown")
}

/// All term and definition text of every definition list in the document.
fn definition_text(document: &Document) -> Vec<(Vec<String>, Vec<String>)> {
    document
        .paragraphs
        .iter()
        .flat_map(|p| p.definition_items())
        .map(|item| {
            let terms = item
                .terms
                .iter()
                .map(|spans| spans.iter().map(|s| s.text.clone()).collect())
                .collect();
            let definitions = item
                .definition
                .iter()
                .map(|p| p.content().iter().map(|s| s.text.clone()).collect())
                .collect();
            (terms, definitions)
        })
        .collect()
}

#[test]
fn rules_and_definition_lists_round_trip_through_markdown_and_html() {
    // Gemtext has neither an `<hr>` nor a `<dl>` equivalent, and FTML is covered
    // separately below — these are the two formats that express both in full.
    for extension in ["md", "html"] {
        let (reloaded, _) = save_then_load(extension, rules_and_definitions_document());

        let rules = reloaded
            .paragraphs
            .iter()
            .filter(|p| p.paragraph_type() == ParagraphType::HorizontalRule)
            .count();
        assert_eq!(rules, 1, "the rule survived .{extension}");

        assert_eq!(
            definition_text(&reloaded),
            vec![
                (
                    vec!["Coffee".to_string()],
                    vec!["A black hot drink.".to_string()]
                ),
                (
                    vec!["Tea".to_string()],
                    vec!["A leaf infusion.".to_string()]
                ),
            ],
            "terms and definitions survived .{extension}"
        );

        let text = doc_text(&reloaded);
        assert!(
            text.contains("Intro") && text.contains("Outro"),
            "the surrounding paragraphs survived .{extension}: {text:?}"
        );
    }
}

/// Strict FTML has no thematic-break element and no `<dl>`, so `tdoc`'s FTML
/// writer deliberately drops a rule and flattens a definition list into plain
/// paragraphs (see `tdoc`'s `ftml::writer::should_skip` /
/// `write_flattened_definition_list`). That is upstream behavior, not something
/// Pure can round-trip — this test pins down exactly how much is lost, so the
/// day FTML grows the elements the loss shows up here as a change.
#[test]
fn ftml_loses_rules_and_flattens_definition_lists() {
    let (reloaded, format) = save_then_load("ftml", rules_and_definitions_document());
    assert_eq!(format, DocumentFormat::Ftml);

    assert!(
        !reloaded
            .paragraphs
            .iter()
            .any(|p| p.paragraph_type() == ParagraphType::HorizontalRule),
        "FTML has no thematic break, so the rule is dropped"
    );
    assert!(
        definition_text(&reloaded).is_empty(),
        "FTML has no <dl>, so the list is flattened away"
    );

    // The *text* still survives: every term and definition comes back as an
    // ordinary paragraph, so saving as FTML costs structure, never content.
    let text = doc_text(&reloaded);
    for expected in [
        "Intro",
        "Coffee",
        "A black hot drink.",
        "Tea",
        "A leaf infusion.",
        "Outro",
    ] {
        assert!(text.contains(expected), "{expected:?} survived: {text:?}");
    }
}

#[test]
fn gemini_round_trips_through_save_and_load() {
    let source =
        parse(Cursor::new("<h1>Title</h1><p>A plain paragraph.</p>")).expect("parse source");

    let (reloaded, format) = save_then_load("gmi", source);

    assert_eq!(format, DocumentFormat::Gemini);
    let text = doc_text(&reloaded);
    assert!(text.contains("Title"), "heading survived: {text:?}");
    assert!(text.contains("plain paragraph"), "body survived: {text:?}");
}

/// A one-item definition list: "Coffee" / "A hot drink." — four words, wherever
/// it is placed.
fn sample_definition_list() -> tdoc::Paragraph {
    tdoc::paragraph::Paragraph::DefinitionList {
        items: vec![tdoc::paragraph::DefinitionItem {
            terms: vec![vec![tdoc::Span::new_text("Coffee")]],
            definition: vec![
                tdoc::paragraph::Paragraph::new_text()
                    .with_content(vec![tdoc::Span::new_text("A hot drink.")]),
            ],
        }],
    }
}

fn document_of(paragraphs: Vec<tdoc::Paragraph>) -> Document {
    Document {
        paragraphs,
        ..Document::new()
    }
}

/// The status bar's word count has to reach into a definition list explicitly —
/// its text is in neither `content()` nor `children()` — and has to keep doing so
/// wherever the list is nested.
#[test]
fn definition_list_words_are_counted_wherever_the_list_sits() {
    use tdoc::paragraph::{DefinitionItem, Paragraph};

    let cases: Vec<(&str, Document)> = vec![
        ("top level", document_of(vec![sample_definition_list()])),
        (
            "inside a quote",
            document_of(vec![
                Paragraph::new_quote().with_children(vec![sample_definition_list()]),
            ]),
        ),
        (
            "inside a list item",
            document_of(vec![
                Paragraph::new_unordered_list().with_entries(vec![vec![sample_definition_list()]]),
            ]),
        ),
    ];
    for (where_, doc) in cases {
        assert_eq!(
            count_words(&doc),
            4,
            "a term and its definition count {where_}"
        );
    }

    // A definition holds whole paragraphs, so a list can nest inside one.
    let nested = document_of(vec![Paragraph::DefinitionList {
        items: vec![DefinitionItem {
            terms: vec![vec![tdoc::Span::new_text("Outer")]],
            definition: vec![sample_definition_list()],
        }],
    }]);
    assert_eq!(count_words(&nested), 5, "the outer term counts too");
}

/// A definition holds full paragraphs, so anything can sit inside one — and the
/// word count has to follow it there. Tables used to count zero words anywhere,
/// which a definition list made reachable in the middle of ordinary prose.
#[test]
fn words_inside_tables_and_nested_checklists_are_counted() {
    let table = tdoc::markdown::parse(Cursor::new(
        "| A | B |\n| --- | --- |\n| one two | three |\n",
    ))
    .expect("parse table");
    assert_eq!(count_words(&table), 5, "header and body cells both count");

    // Checklist items nest items rather than paragraphs, so the walk recurses on
    // the item type; before, only the first level of nesting was counted.
    let checklist = tdoc::markdown::parse(Cursor::new(
        "- [ ] one\n    - [ ] two\n        - [ ] three four\n",
    ))
    .expect("parse checklist");
    assert_eq!(count_words(&checklist), 4, "nesting counts at every depth");
}
