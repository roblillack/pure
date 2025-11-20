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

#[test]
fn vertical_movement_across_paragraph_types() {
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_header1().with_content(vec![Span::new_text("Header 1")]),
        text_paragraph("A regular text paragraph."),
        checklist(&["Item 1", "Item 2"]),
    ]);
    let mut editor = DocumentEditor::new(document);

    // Start at H1
    let h1_pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&h1_pointer));

    // Move down to text paragraph
    assert!(editor.move_down(), "Could not move down from H1 to text");
    let cursor = editor.cursor_pointer();
    assert_eq!(
        cursor.paragraph_path,
        pointer_to_root_span(1).paragraph_path,
        "Cursor should be on text paragraph"
    );

    // Move down to checklist
    assert!(
        editor.move_down(),
        "Could not move down from text to checklist"
    );
    let cursor = editor.cursor_pointer();
    assert_eq!(
        cursor.paragraph_path,
        pointer_to_checklist_item_span(2, &[0]).paragraph_path,
        "Cursor should be on checklist item 1"
    );

    // Move up to text paragraph
    assert!(editor.move_up(), "Could not move up from checklist to text");
    let cursor = editor.cursor_pointer();
    assert_eq!(
        cursor.paragraph_path,
        pointer_to_root_span(1).paragraph_path,
        "Cursor should be on text paragraph"
    );

    // Move up to H1
    assert!(editor.move_up(), "Could not move up from text to H1");
    let cursor = editor.cursor_pointer();
    assert_eq!(
        cursor.paragraph_path,
        pointer_to_root_span(0).paragraph_path,
        "Cursor should be on H1"
    );
}

#[test]
fn move_down_from_heading_to_checklist() {
    // This reproduces the issue from test.ftml where a heading is followed
    // directly by a checklist (no text paragraph in between)
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_header2().with_content(vec![Span::new_text("Todos")]),
        checklist(&["Item 1", "Item 2", "Item 3"]),
    ]);
    let mut editor = DocumentEditor::new(document);

    // Start at the beginning of the H2
    let h2_pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&h2_pointer));
    assert_eq!(editor.cursor_pointer().offset, 0);

    // Try to move down to the first checklist item
    assert!(
        editor.move_down(),
        "Could not move down from H2 to checklist item"
    );
    let cursor = editor.cursor_pointer();
    assert_eq!(
        cursor.paragraph_path,
        pointer_to_checklist_item_span(1, &[0]).paragraph_path,
        "Cursor should be on first checklist item"
    );
}

#[test]
fn move_down_from_heading_to_checklist_with_empty_paragraph() {
    // Test with an empty paragraph between heading and checklist (like in test.ftml)
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_header2().with_content(vec![Span::new_text("Todos")]),
        text_paragraph(""), // Empty paragraph from blank line in FTML
        checklist(&["Item 1", "Item 2"]),
    ]);
    let mut editor = DocumentEditor::new(document);

    // Debug: print segments
    eprintln!("\n=== All Segments ===");
    for (idx, segment) in editor.segments.iter().enumerate() {
        eprintln!(
            "Segment {}: path={:?}, kind={:?}, len={}",
            idx, segment.paragraph_path, segment.kind, segment.len
        );
    }

    // Start at the beginning of the H2
    let h2_pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&h2_pointer));
    eprintln!("\n=== After moving to H2 ===");
    eprintln!("cursor_segment: {}", editor.cursor_segment);
    eprintln!("cursor_path: {:?}", editor.cursor_pointer().paragraph_path);

    // Try to move down
    let result = editor.move_down();
    eprintln!("\n=== After move_down ===");
    eprintln!("move_down result: {}", result);
    eprintln!("cursor_segment: {}", editor.cursor_segment);
    eprintln!("cursor_path: {:?}", editor.cursor_pointer().paragraph_path);

    assert!(result, "Could not move down from H2");

    // Now try to move down again from the empty paragraph to the checklist
    let result2 = editor.move_down();
    eprintln!("\n=== After second move_down ===");
    eprintln!("move_down result: {}", result2);
    eprintln!("cursor_segment: {}", editor.cursor_segment);
    eprintln!("cursor_path: {:?}", editor.cursor_pointer().paragraph_path);

    assert!(
        result2,
        "Could not move down from empty paragraph to checklist"
    );

    // Verify we're actually on a checklist item
    let cursor = editor.cursor_pointer();
    assert_eq!(
        cursor.paragraph_path,
        pointer_to_checklist_item_span(2, &[0]).paragraph_path,
        "Cursor should be on first checklist item"
    );
}

#[test]
fn move_down_at_different_offsets() {
    // Test moving down from different cursor offsets
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_header2().with_content(vec![Span::new_text("Todos")]),
        checklist(&["Item 1"]),
    ]);

    // Test 1: Move down from offset 0
    {
        let mut editor = DocumentEditor::new(document.clone());
        let h2_pointer = pointer_to_root_span(0);
        assert!(editor.move_to_pointer(&h2_pointer));
        eprintln!("\n=== Test 1: offset 0 ===");
        eprintln!(
            "Before: segment={}, offset={}",
            editor.cursor_segment, editor.cursor.offset
        );
        let result = editor.move_down();
        eprintln!(
            "After: segment={}, offset={}, result={}",
            editor.cursor_segment, editor.cursor.offset, result
        );
        assert!(result, "Failed to move down from offset 0");
    }

    // Test 2: Move down from offset 3 (middle of "Todos")
    {
        let mut editor = DocumentEditor::new(document.clone());
        let mut h2_pointer = pointer_to_root_span(0);
        h2_pointer.offset = 3;
        assert!(editor.move_to_pointer(&h2_pointer));
        eprintln!("\n=== Test 2: offset 3 ===");
        eprintln!(
            "Before: segment={}, offset={}",
            editor.cursor_segment, editor.cursor.offset
        );
        let result = editor.move_down();
        eprintln!(
            "After: segment={}, offset={}, result={}",
            editor.cursor_segment, editor.cursor.offset, result
        );
        assert!(result, "Failed to move down from offset 3");
    }
}
