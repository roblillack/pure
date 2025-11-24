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

fn insert_text(editor: &mut DocumentEditor, text: &str) {
    for ch in text.chars() {
        assert!(editor.insert_char(ch), "failed to insert char {ch}");
    }
}

fn document_with_bold_span() -> Document {
    let mut bold = Span::new_text("World");
    bold.style = InlineStyle::Bold;
    let paragraph = Paragraph::new_text().with_content(vec![
        Span::new_text("Hello "),
        bold,
        Span::new_text("!"),
    ]);
    Document::new().with_paragraphs(vec![paragraph])
}

fn document_with_checklist_bold_span() -> Document {
    let mut bold = Span::new_text("World");
    bold.style = InlineStyle::Bold;
    let item = ChecklistItem::new(false).with_content(vec![
        Span::new_text("Hello "),
        bold,
        Span::new_text("!"),
    ]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![item]);
    Document::new().with_paragraphs(vec![checklist])
}

fn document_with_checklist_nested_bold_span() -> Document {
    let mut bold = Span::new_text("");
    bold.style = InlineStyle::Bold;
    bold.children = vec![Span::new_text("World")];
    let item = ChecklistItem::new(false).with_content(vec![
        Span::new_text("Hello "),
        bold,
        Span::new_text("!"),
    ]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![item]);
    Document::new().with_paragraphs(vec![checklist])
}

#[test]
fn delete_word_backward_removes_previous_word() {
    let document = Document::new().with_paragraphs(vec![text_paragraph("foo bar baz")]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.move_word_right());
    assert!(editor.move_word_right());

    assert!(editor.delete_word_backward());

    let doc = editor.document();
    assert_eq!(doc.paragraphs[0].content()[0].text, "foo baz");
    assert_eq!(editor.cursor_pointer().offset, 4);
}

#[test]
fn delete_word_forward_removes_next_word() {
    let document = Document::new().with_paragraphs(vec![text_paragraph("foo bar baz")]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.delete_word_forward());

    let doc = editor.document();
    assert_eq!(doc.paragraphs[0].content()[0].text, "bar baz");
    assert_eq!(editor.cursor_pointer().offset, 0);
}

#[test]
fn insert_char_before_reveal_end_marker_appends_to_span() {
    let mut editor = DocumentEditor::new(document_with_bold_span());
    editor.set_reveal_codes(true);

    for _ in 0..12 {
        assert!(editor.move_right());
    }
    insert_text(&mut editor, " class people");

    let doc = editor.document();
    assert_eq!(doc.paragraphs[0].content()[0].text, "Hello ");
    assert_eq!(doc.paragraphs[0].content()[1].text, "World class people");
}

#[test]
fn insert_char_before_reveal_end_marker_in_checklist_appends_to_span() {
    let mut editor = DocumentEditor::new(document_with_checklist_bold_span());
    editor.set_reveal_codes(true);

    for _ in 0..12 {
        assert!(editor.move_right());
    }
    insert_text(&mut editor, " class people");

    let doc = editor.document();
    let checklist = &doc.paragraphs[0];
    assert_eq!(checklist.checklist_items()[0].content[0].text, "Hello ");
    assert_eq!(
        checklist.checklist_items()[0].content[1].text,
        "World class people"
    );
}

#[test]
fn insert_char_on_reveal_end_marker_in_checklist_appends_to_span() {
    let mut editor = DocumentEditor::new(document_with_checklist_bold_span());
    editor.set_reveal_codes(true);

    while editor.move_right() {}
    while !matches!(
        editor.cursor_pointer().segment_kind,
        SegmentKind::RevealEnd(_)
    ) {
        assert!(editor.move_left());
    }
    let pointer = editor.cursor_pointer();
    assert_eq!(pointer.span_path.indices(), &[1]);

    insert_text(&mut editor, " dear");

    let doc = editor.document();
    let checklist = &doc.paragraphs[0];
    let item = &checklist.checklist_items()[0];
    assert_eq!(item.content[0].text, "Hello ");
    assert_eq!(item.content[1].text, "World dear");
}

#[test]
fn insert_char_on_reveal_end_marker_in_checklist_with_nested_bold_span_appends_to_span() {
    let mut editor = DocumentEditor::new(document_with_checklist_nested_bold_span());
    editor.set_reveal_codes(true);

    while editor.move_right() {}
    while !matches!(
        editor.cursor_pointer().segment_kind,
        SegmentKind::RevealEnd(_)
    ) {
        assert!(editor.move_left());
    }

    insert_text(&mut editor, " dear");

    let doc = editor.document();
    let checklist = &doc.paragraphs[0];
    let item = &checklist.checklist_items()[0];
    assert_eq!(item.content[0].text, "Hello ");
    assert_eq!(item.content[1].children[0].text, "World dear");
}

fn pointer_to_nested_checklist_item_span(root_index: usize, indices: Vec<usize>) -> CursorPointer {
    let mut path = ParagraphPath::new_root(root_index);
    path.push_checklist_item(indices);
    CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

#[test]
fn insert_paragraph_break_in_nested_checklist_creates_sibling() {
    // Create a checklist with a nested item structure:
    // - Parent item
    //   - Nested item 1
    //   - Nested item 2
    let mut parent_item = ChecklistItem::new(false).with_content(vec![Span::new_text("Parent")]);
    parent_item.children = vec![
        ChecklistItem::new(false).with_content(vec![Span::new_text("Nested 1")]),
        ChecklistItem::new(false).with_content(vec![Span::new_text("Nested 2")]),
    ];
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![parent_item]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    // Position cursor at the end of "Nested 1" (indices [0, 0] = first child of first parent)
    let mut pointer = pointer_to_nested_checklist_item_span(0, vec![0, 0]);
    pointer.offset = 8; // After "Nested 1"
    assert!(editor.move_to_pointer(&pointer));

    // Insert a paragraph break
    assert!(editor.insert_paragraph_break());

    // Verify the structure: should have a new sibling at the same nesting level
    let doc = editor.document();
    let checklist = &doc.paragraphs[0];
    let items = checklist.checklist_items();
    assert_eq!(items.len(), 1, "Should still have one parent item");

    let parent = &items[0];
    assert_eq!(parent.content[0].text, "Parent");
    assert_eq!(
        parent.children.len(),
        3,
        "Should now have 3 nested items (was 2, added 1)"
    );

    // Verify the first nested item was split correctly
    assert_eq!(parent.children[0].content[0].text, "Nested 1");

    // The new item should be at index 1 (between old items 0 and 1)
    assert_eq!(
        parent.children[1].content[0].text, "",
        "New item should be empty"
    );

    // The old second item should now be at index 2
    assert_eq!(parent.children[2].content[0].text, "Nested 2");
}
