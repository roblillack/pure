use super::*;
use tdoc::ChecklistItem;

fn root_pointer(span_path: Vec<usize>, offset: usize) -> CursorPointer {
    CursorPointer {
        paragraph_path: ParagraphPath::new_root(0),
        span_path: SpanPath::new(span_path),
        offset,
        segment_kind: SegmentKind::Text,
    }
}

fn link_paragraph() -> Paragraph {
    Paragraph::new_text().with_content(vec![
        Span::new_text("see "),
        Span::new_styled(InlineStyle::Link)
            .with_children(vec![Span::new_text("the book")])
            .with_link_target("https://old.test"),
    ])
}

#[test]
fn set_link_creates_link_over_selection() {
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_text().with_content(vec![Span::new_text("hello world")]),
    ]);
    let mut editor = DocumentEditor::new(document);

    let start = root_pointer(vec![0], 6);
    let end = root_pointer(vec![0], 11);
    assert!(editor.set_link(&(start, end), "world", Some("https://example.test")));

    let paragraph = &editor.document().paragraphs[0];
    assert_eq!(paragraph.content().len(), 2);
    assert_eq!(paragraph.content()[0].text, "hello ");
    assert_eq!(paragraph.content()[0].style, InlineStyle::None);
    let link = &paragraph.content()[1];
    assert_eq!(link.style, InlineStyle::Link);
    assert_eq!(link.text, "world");
    assert_eq!(link.link_target.as_deref(), Some("https://example.test"));
}

#[test]
fn set_link_inserts_new_link_at_cursor_without_selection() {
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_text().with_content(vec![Span::new_text("ab")]),
    ]);
    let mut editor = DocumentEditor::new(document);

    let at = root_pointer(vec![0], 1);
    assert!(editor.set_link(&(at.clone(), at), "site", Some("https://example.test")));

    let paragraph = &editor.document().paragraphs[0];
    let texts: Vec<&str> = paragraph
        .content()
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(texts, vec!["a", "site", "b"]);
    assert_eq!(paragraph.content()[1].style, InlineStyle::Link);
    assert_eq!(
        paragraph.content()[1].link_target.as_deref(),
        Some("https://example.test")
    );
}

#[test]
fn link_at_cursor_reports_enclosing_link() {
    let document = Document::new().with_paragraphs(vec![link_paragraph()]);
    let mut editor = DocumentEditor::new(document);

    // Place the cursor inside the link's text (span [1, 0]).
    assert!(editor.move_to_pointer(&root_pointer(vec![1, 0], 2)));

    let link = editor.link_at_cursor().expect("cursor sits inside a link");
    assert_eq!(link.text, "the book");
    assert_eq!(link.target.as_deref(), Some("https://old.test"));
}

#[test]
fn link_at_cursor_is_none_outside_links() {
    let document = Document::new().with_paragraphs(vec![link_paragraph()]);
    let mut editor = DocumentEditor::new(document);

    // The leading "see " text is span [0]; the cursor there is not a link.
    assert!(editor.move_to_pointer(&root_pointer(vec![0], 1)));
    assert!(editor.link_at_cursor().is_none());
}

#[test]
fn set_link_retargets_and_relabels_existing_link() {
    let document = Document::new().with_paragraphs(vec![link_paragraph()]);
    let mut editor = DocumentEditor::new(document);

    assert!(editor.move_to_pointer(&root_pointer(vec![1, 0], 2)));
    let link = editor.link_at_cursor().expect("cursor sits inside a link");
    assert!(editor.set_link(&link.range, "The Manual", Some("https://new.test")));

    let paragraph = &editor.document().paragraphs[0];
    // "see " text is preserved, the link is replaced in place.
    assert_eq!(paragraph.content()[0].text, "see ");
    let link = &paragraph.content()[1];
    assert_eq!(link.style, InlineStyle::Link);
    assert_eq!(link.link_target.as_deref(), Some("https://new.test"));
    let mut visible = String::new();
    fn collect(span: &Span, out: &mut String) {
        out.push_str(&span.text);
        for child in &span.children {
            collect(child, out);
        }
    }
    collect(link, &mut visible);
    assert_eq!(visible, "The Manual");
}

#[test]
fn set_link_with_no_target_unlinks_to_plain_text() {
    let document = Document::new().with_paragraphs(vec![link_paragraph()]);
    let mut editor = DocumentEditor::new(document);

    assert!(editor.move_to_pointer(&root_pointer(vec![1, 0], 2)));
    let link = editor.link_at_cursor().expect("cursor sits inside a link");
    assert!(editor.set_link(&link.range, "the book", None));

    let paragraph = &editor.document().paragraphs[0];
    // The link collapses back into the surrounding plain text.
    assert!(
        paragraph
            .content()
            .iter()
            .all(|span| span.style == InlineStyle::None && span.link_target.is_none())
    );
    let mut text = String::new();
    for span in paragraph.content() {
        text.push_str(&span.text);
    }
    assert_eq!(text, "see the book");
}

#[test]
fn set_link_targets_checklist_items() {
    let item = ChecklistItem::new(false).with_content(vec![Span::new_text("buy milk")]);
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_checklist().with_checklist_items(vec![item]),
    ]);
    let mut editor = DocumentEditor::new(document);

    let mut path = ParagraphPath::new_root(0);
    path.push_checklist_item(vec![0]);
    let start = CursorPointer {
        paragraph_path: path.clone(),
        span_path: SpanPath::new(vec![0]),
        offset: 4,
        segment_kind: SegmentKind::Text,
    };
    let end = CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 8,
        segment_kind: SegmentKind::Text,
    };
    assert!(editor.set_link(&(start, end), "milk", Some("https://milk.test")));

    let paragraph = &editor.document().paragraphs[0];
    let item = &paragraph.checklist_items()[0];
    let link = item
        .content
        .iter()
        .find(|span| span.style == InlineStyle::Link)
        .expect("checklist item gained a link");
    assert_eq!(link.text, "milk");
    assert_eq!(link.link_target.as_deref(), Some("https://milk.test"));
}
