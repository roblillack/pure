use super::*;

fn pointer_to_root_span(root_index: usize) -> CursorPointer {
    CursorPointer {
        paragraph_path: ParagraphPath::new_root(root_index),
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn pointer_to_checklist_item_span(root_index: usize, item_index: usize) -> CursorPointer {
    let mut path = ParagraphPath::new_root(root_index);
    path.push_checklist_item(vec![item_index]);
    CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn text_paragraph(text: &str) -> Paragraph {
    Paragraph::new_text().with_content(vec![Span::new_text(text)])
}

fn checklist(items: &[&str]) -> Paragraph {
    let checklist_items = items
        .iter()
        .map(|text| ChecklistItem::new(false).with_content(vec![Span::new_text(*text)]))
        .collect::<Vec<_>>();
    Paragraph::new_checklist().with_checklist_items(checklist_items)
}

#[test]
fn apply_inline_style_splits_span() {
    let document = Document::new().with_paragraphs(vec![text_paragraph("hello world")]);
    let mut editor = DocumentEditor::new(document);

    let mut start = pointer_to_root_span(0);
    start.offset = 0;
    let mut end = pointer_to_root_span(0);
    end.offset = 5;

    assert!(
        editor.apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Bold)
    );

    let doc = editor.document();
    let paragraph = &doc.paragraphs[0];
    assert_eq!(paragraph.content().len(), 2);
    assert_eq!(paragraph.content()[0].text, "hello");
    assert_eq!(paragraph.content()[0].style, InlineStyle::Bold);
    assert_eq!(paragraph.content()[1].text, " world");
    assert_eq!(paragraph.content()[1].style, InlineStyle::None);
}

#[test]
fn apply_inline_style_across_segments() {
    let paragraph =
        Paragraph::new_text().with_content(vec![Span::new_text("hello "), Span::new_text("world")]);
    let document = Document::new().with_paragraphs(vec![paragraph]);
    let mut editor = DocumentEditor::new(document);

    let mut start = pointer_to_root_span(0);
    start.span_path = SpanPath::new(vec![0]);
    start.offset = 3;

    let mut end = pointer_to_root_span(0);
    end.span_path = SpanPath::new(vec![1]);
    end.offset = 2;

    assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::Underline));

    let doc = editor.document();
    let spans = doc.paragraphs[0].content();
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].text, "hel");
    assert_eq!(spans[0].style, InlineStyle::None);
    assert_eq!(spans[1].text, "lo wo");
    assert_eq!(spans[1].style, InlineStyle::Underline);
    assert_eq!(spans[2].text, "rld");
    assert_eq!(spans[2].style, InlineStyle::None);
}

#[test]
fn clear_inline_style_resets_to_plain() {
    let document = Document::new().with_paragraphs(vec![text_paragraph("styled text")]);
    let mut editor = DocumentEditor::new(document);

    let mut start = pointer_to_root_span(0);
    start.offset = 0;
    let mut end = pointer_to_root_span(0);
    end.offset = 6;

    assert!(
        editor.apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Code)
    );
    assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::None));

    let doc = editor.document();
    let spans = doc.paragraphs[0].content();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "styled text");
    assert_eq!(spans[0].style, InlineStyle::None);
}

#[test]
fn apply_inline_style_in_checklist_item() {
    let document = Document::new().with_paragraphs(vec![checklist(&["make tea"])]);
    let mut editor = DocumentEditor::new(document);

    let start = pointer_to_checklist_item_span(0, 0);
    let mut end = start.clone();
    end.offset = 4;

    assert!(
        editor.apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Italic)
    );

    let doc = editor.document();
    let Paragraph::Checklist { items } = &doc.paragraphs[0] else {
        panic!("expected checklist paragraph");
    };
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.content.len(), 2);
    assert_eq!(item.content[0].text, "make");
    assert_eq!(item.content[0].style, InlineStyle::Italic);
    assert_eq!(item.content[1].text, " tea");
    assert_eq!(item.content[1].style, InlineStyle::None);
}

#[test]
fn apply_inline_style_stacks_on_styled_span() {
    let document = Document::new().with_paragraphs(vec![text_paragraph(
        "Sometimes, stacking multiple styles gets messy.",
    )]);
    let mut editor = DocumentEditor::new(document);

    // Embolden "multiple styles gets messy" …
    let mut start = pointer_to_root_span(0);
    start.offset = 20;
    let mut end = pointer_to_root_span(0);
    end.offset = 46;
    assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::Bold));

    // … then highlight "gets messy" inside the bold range.
    let mut start = pointer_to_root_span(0);
    start.span_path = SpanPath::new(vec![1]);
    start.offset = 16;
    let mut end = pointer_to_root_span(0);
    end.span_path = SpanPath::new(vec![1]);
    end.offset = 26;
    assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::Highlight));

    let doc = editor.document();
    let spans = doc.paragraphs[0].content();
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].text, "Sometimes, stacking ");
    assert_eq!(spans[0].style, InlineStyle::None);
    assert_eq!(spans[1].text, "multiple styles ");
    assert_eq!(spans[1].style, InlineStyle::Bold);
    assert_eq!(spans[1].children.len(), 1);
    assert_eq!(spans[1].children[0].text, "gets messy");
    assert_eq!(spans[1].children[0].style, InlineStyle::Highlight);
    assert_eq!(spans[2].text, ".");
    assert_eq!(spans[2].style, InlineStyle::None);
}

#[test]
fn apply_inline_style_already_styled_is_noop() {
    let document = Document::new().with_paragraphs(vec![text_paragraph("hello world")]);
    let mut editor = DocumentEditor::new(document);

    let mut start = pointer_to_root_span(0);
    start.offset = 0;
    let mut end = pointer_to_root_span(0);
    end.offset = 5;
    assert!(
        editor.apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Bold)
    );

    // Re-selecting part of the bold range and bolding again changes nothing.
    let mut start = pointer_to_root_span(0);
    start.offset = 1;
    let mut end = pointer_to_root_span(0);
    end.offset = 4;
    assert!(!editor.apply_inline_style_to_selection(&(start, end), InlineStyle::Bold));

    let doc = editor.document();
    let spans = doc.paragraphs[0].content();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].text, "hello");
    assert_eq!(spans[0].style, InlineStyle::Bold);
    assert!(spans[0].children.is_empty());
}

#[test]
fn apply_inline_style_across_styled_and_plain_spans_wraps_both() {
    let paragraph = Paragraph::new_text().with_content(vec![
        Span::new_text("plain "),
        Span::new_styled(InlineStyle::Bold).with_text("bold"),
    ]);
    let document = Document::new().with_paragraphs(vec![paragraph]);
    let mut editor = DocumentEditor::new(document);

    let mut start = pointer_to_root_span(0);
    start.span_path = SpanPath::new(vec![0]);
    start.offset = 0;
    let mut end = pointer_to_root_span(0);
    end.span_path = SpanPath::new(vec![1]);
    end.offset = 4;
    assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::Italic));

    let doc = editor.document();
    let spans = doc.paragraphs[0].content();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].style, InlineStyle::Italic);
    assert_eq!(spans[0].text, "plain ");
    assert_eq!(spans[0].children.len(), 1);
    assert_eq!(spans[0].children[0].style, InlineStyle::Bold);
    assert_eq!(spans[0].children[0].text, "bold");
}

#[test]
fn clear_inline_style_removes_stacked_styles() {
    let paragraph = Paragraph::new_text().with_content(vec![
        Span::new_text("a "),
        Span::new_styled(InlineStyle::Bold)
            .with_text("b ")
            .with_children(vec![
                Span::new_styled(InlineStyle::Highlight).with_text("c d"),
            ]),
    ]);
    let document = Document::new().with_paragraphs(vec![paragraph]);
    let mut editor = DocumentEditor::new(document);

    // Clear formatting from "b c" — part of the bold text plus part of the
    // nested highlight. Both styles must go, including the bold carried by
    // the ancestor span.
    let mut start = pointer_to_root_span(0);
    start.span_path = SpanPath::new(vec![1]);
    start.offset = 0;
    let mut end = pointer_to_root_span(0);
    end.span_path = SpanPath::new(vec![1, 0]);
    end.offset = 1;
    assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::None));

    let doc = editor.document();
    let spans = doc.paragraphs[0].content();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].text, "a b c");
    assert_eq!(spans[0].style, InlineStyle::None);
    assert!(spans[0].children.is_empty());
    assert_eq!(spans[1].style, InlineStyle::Bold);
    assert_eq!(spans[1].text, "");
    assert_eq!(spans[1].children.len(), 1);
    assert_eq!(spans[1].children[0].style, InlineStyle::Highlight);
    assert_eq!(spans[1].children[0].text, " d");
}

#[test]
fn apply_inline_style_keeps_cursor_position() {
    let document = Document::new().with_paragraphs(vec![text_paragraph(
        "Pure is a modern, terminal-based word processor for your terminal",
    )]);
    let mut editor = DocumentEditor::new(document);

    // Select "terminal-based word processor" (chars 18..47) with the cursor
    // sitting at the selection end, like after shift-selecting forward.
    let mut start = pointer_to_root_span(0);
    start.offset = 18;
    let mut end = pointer_to_root_span(0);
    end.offset = 47;
    assert!(editor.move_to_pointer(&end));

    assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::Italic));

    let cursor = editor.cursor_pointer();
    assert_eq!(
        cursor.span_path.indices(),
        &[1],
        "cursor should stay on the newly styled span"
    );
    assert_eq!(
        cursor.offset, 29,
        "cursor should stay at the end of the styled text"
    );
}
