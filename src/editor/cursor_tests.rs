use super::*;

fn text_paragraph(text: &str) -> Paragraph {
    Paragraph::new_text().with_content(vec![Span::new_text(text)])
}

fn pointer_to_root_span(root_index: usize) -> CursorPointer {
    CursorPointer {
        paragraph_path: ParagraphPath::new_root(root_index),
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn pointer_to_checklist_item_span(root_index: usize, indices: &[usize]) -> CursorPointer {
    let mut path = ParagraphPath::new_root(root_index);
    path.push_checklist_item(indices.to_vec());
    CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn checklist(items: &[&str]) -> Paragraph {
    let checklist_items = items
        .iter()
        .map(|text| ChecklistItem::new(false).with_content(vec![Span::new_text(*text)]))
        .collect::<Vec<_>>();
    Paragraph::new_checklist().with_checklist_items(checklist_items)
}

#[test]
fn move_word_left_within_span() {
    let document = Document::new().with_paragraphs(vec![text_paragraph("hello world")]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&pointer));
    editor.move_to_segment_end();

    assert!(editor.move_word_left());
    assert_eq!(editor.cursor_pointer().offset, 6);

    assert!(editor.move_word_left());
    assert_eq!(editor.cursor_pointer().offset, 0);
}

#[test]
fn move_word_right_advances_to_next_word() {
    let document = Document::new().with_paragraphs(vec![text_paragraph("foo bar baz")]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.move_word_right());
    assert_eq!(editor.cursor_pointer().offset, 4);

    assert!(editor.move_word_right());
    assert_eq!(editor.cursor_pointer().offset, 8);
}

#[test]
fn move_word_navigation_crosses_segments() {
    let document =
        Document::new().with_paragraphs(vec![text_paragraph("alpha"), text_paragraph("beta")]);
    let mut editor = DocumentEditor::new(document);

    let first = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&first));
    editor.move_to_segment_end();

    assert!(editor.move_word_right());
    let pointer = editor.cursor_pointer();
    let expected_second = pointer_to_root_span(1);
    assert_eq!(pointer.paragraph_path, expected_second.paragraph_path);
    assert_eq!(pointer.span_path, expected_second.span_path);
    assert_eq!(pointer.offset, 0);

    assert!(editor.move_word_left());
    let pointer = editor.cursor_pointer();
    let expected_first = pointer_to_root_span(0);
    assert_eq!(pointer.paragraph_path, expected_first.paragraph_path);
    assert_eq!(pointer.span_path, expected_first.span_path);
    assert_eq!(pointer.offset, 0);
}

#[test]
fn move_word_right_within_checklist_item() {
    let document = Document::new().with_paragraphs(vec![checklist(&["Task"])]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_checklist_item_span(0, &[0]);
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.move_word_right());
    assert_eq!(editor.cursor_pointer().offset, 4);
}
