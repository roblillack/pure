use super::*;

fn pointer_to_root_span(root_index: usize) -> CursorPointer {
    CursorPointer {
        paragraph_path: ParagraphPath::new_root(root_index),
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn text_paragraph(text: &str) -> Paragraph {
    Paragraph::new_text().with_content(vec![Span::new_text(text)])
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
        editor
            .apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Bold)
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
    let paragraph = Paragraph::new_text()
        .with_content(vec![Span::new_text("hello "), Span::new_text("world")]);
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
        editor
            .apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Code)
    );
    assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::None));

    let doc = editor.document();
    let spans = doc.paragraphs[0].content();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "styled text");
    assert_eq!(spans[0].style, InlineStyle::None);
}
