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
fn save_then_load(extension: &str, document: Document) -> (Document, DocumentFormat) {
    let path = std::env::temp_dir().join(format!("pure_roundtrip_test.{extension}"));
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
