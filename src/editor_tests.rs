use tdoc::ftml;

use super::structure;
use super::*;

fn pointer_to_root_span(root_index: usize) -> CursorPointer {
    CursorPointer {
        paragraph_path: ParagraphPath::new_root(root_index),
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn pointer_to_child_span(root_index: usize, child_index: usize) -> CursorPointer {
    let mut path = ParagraphPath::new_root(root_index);
    path.push_child(child_index);
    CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn pointer_to_child_entry_span(
    root_index: usize,
    child_index: usize,
    entry_index: usize,
    paragraph_index: usize,
) -> CursorPointer {
    let mut path = ParagraphPath::new_root(root_index);
    path.push_child(child_index);
    path.push_entry(entry_index, paragraph_index);
    CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn pointer_to_entry_span(
    root_index: usize,
    entry_index: usize,
    paragraph_index: usize,
) -> CursorPointer {
    let mut path = ParagraphPath::new_root(root_index);
    path.push_entry(entry_index, paragraph_index);
    CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    }
}

fn pointer_to_checklist_item_span(root_index: usize, item_index: usize) -> CursorPointer {
    pointer_to_nested_checklist_item_span(root_index, vec![item_index])
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

fn text_paragraph(text: &str) -> Paragraph {
    Paragraph::new_text().with_content(vec![Span::new_text(text)])
}

fn unordered_list(items: &[&str]) -> Paragraph {
    let entries = items
        .iter()
        .map(|text| vec![text_paragraph(text)])
        .collect::<Vec<_>>();
    Paragraph::new_unordered_list().with_entries(entries)
}

fn ordered_list(items: &[&str]) -> Paragraph {
    let entries = items
        .iter()
        .map(|text| vec![text_paragraph(text)])
        .collect::<Vec<_>>();
    Paragraph::new_ordered_list().with_entries(entries)
}

fn checklist(items: &[&str]) -> Paragraph {
    let checklist_items = items
        .iter()
        .map(|text| ChecklistItem::new(false).with_content(vec![Span::new_text(*text)]))
        .collect::<Vec<_>>();
    Paragraph::new_checklist().with_checklist_items(checklist_items)
}

#[test]
fn ctrl_p_in_unordered_list_creates_sibling_paragraph() {
    let list = unordered_list(&["Alpha Beta"]);
    let document = Document::new().with_paragraphs(vec![list]);
    let mut editor = DocumentEditor::new(document);

    let mut pointer = pointer_to_entry_span(0, 0, 0);
    pointer.offset = 5;
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.insert_paragraph_break_as_sibling());

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let list = &doc.paragraphs[0];
    assert_eq!(list.entries().len(), 1);
    let entry = &list.entries()[0];
    assert_eq!(entry.len(), 2);
    assert_eq!(entry[0].content()[0].text, "Alpha");
    assert_eq!(entry[1].content()[0].text, " Beta");
}

#[test]
fn ctrl_p_in_checklist_behaves_like_enter() {
    let checklist = checklist(&["Task"]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    let mut pointer = pointer_to_checklist_item_span(0, 0);
    pointer.offset = 4;
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.insert_paragraph_break_as_sibling());

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let checklist = &doc.paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 2);
    assert_eq!(checklist.checklist_items()[0].content[0].text, "Task");
    assert!(!checklist.checklist_items()[1].checked);
}

#[test]
fn enter_split_checked_checklist_preserves_state() {
    let item = ChecklistItem::new(true).with_content(vec![Span::new_text("Done")]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![item]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    let mut pointer = pointer_to_checklist_item_span(0, 0);
    pointer.offset = 2;
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.insert_paragraph_break());

    let checklist = &editor.document().paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 2);
    assert!(checklist.checklist_items()[0].checked);
    assert!(checklist.checklist_items()[1].checked);
}

#[test]
fn enter_at_start_of_checked_checklist_preserves_state() {
    let item = ChecklistItem::new(true).with_content(vec![Span::new_text("Complete task")]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![item]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    let pointer = pointer_to_checklist_item_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.insert_paragraph_break());

    let checklist = &editor.document().paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 2);
    assert!(checklist.checklist_items()[0].checked);
    assert!(checklist.checklist_items()[1].checked);
}

#[test]
fn ctrl_p_split_checked_checklist_preserves_state() {
    let item = ChecklistItem::new(true).with_content(vec![Span::new_text("Task item")]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![item]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    let mut pointer = pointer_to_checklist_item_span(0, 0);
    pointer.offset = 4;
    assert!(editor.move_to_pointer(&pointer));

    assert!(editor.insert_paragraph_break_as_sibling());

    let checklist = &editor.document().paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 2);
    assert!(checklist.checklist_items()[0].checked);
    assert!(checklist.checklist_items()[1].checked);
}

#[test]
fn indent_text_paragraph_following_list() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" }}
            li { p {"Item 2" } }
        }
        p { "Following paragraph" }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.move_down());
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p { "Item 1"  } }
                li {
                    p { "Item 2"  }
                    p { "Following paragraph" }
                }
            }
        }
    );

    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());
    assert_eq!(editor.document().clone(), initial_doc);
}

#[test]
fn unindent_text_paragraph_from_beginning_of_list() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" } }
            li { p {"Item 2" } }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            p { "Item 1"  }
            ul {
                li { p { "Item 2" } }
                li { p { "Item 3" } }
            }
        }
    );
}

#[test]
fn unindent_text_paragraph_from_middle_of_list() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" } }
            li { p {"Item 2" } }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p { "Item 1" } }
            }
            p { "Item 2"  }
            ul {
                li { p { "Item 3" } }
            }
        }
    );
}

#[test]
fn unindent_text_paragraph_from_end_of_list() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" } }
            li { p {"Item 2" } }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.move_down());
    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p {"Item 1" } }
                li { p { "Item 2" } }
            }
            p { "Item 3"  }
        }
    );
}

#[test]
fn indent_list_item() {
    let initial_doc = ftml! {
        quote {
            p { "Paragraph in quote" }
            ul {
                li { p {"Item 1" } }
                li { p {"Item 2" } }
                li {
                    ol {
                        li { p { "Subitem 1" }  }
                        li {
                            p { "Subitem 2" }
                            p { "Subitem 2, paragraph 2" }
                        }

                    }
                }
            }
        }
        p { "Following paragraph" }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    for _ in 0..6 {
        assert!(editor.move_down());
    }

    // Indent the following paragraph into the quote
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            quote {
                p { "Paragraph in quote" }
                ul {
                    li { p {"Item 1" } }
                    li { p {"Item 2" } }
                    li {
                        ol {
                            li { p { "Subitem 1" }  }
                            li {
                                p { "Subitem 2" }
                                p { "Subitem 2, paragraph 2" }
                            }

                        }
                    }
                }
                p { "Following paragraph" }
            }
        }
    );

    // Indent further, into the unordered list this time
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            quote {
                p { "Paragraph in quote" }
                ul {
                    li { p {"Item 1" } }
                    li { p {"Item 2" } }
                    li {
                        ol {
                            li { p { "Subitem 1" }  }
                            li {
                                p { "Subitem 2" }
                                p { "Subitem 2, paragraph 2" }
                            }

                        }
                    }
                    li { p { "Following paragraph" } }
                }
            }
        }
    );

    // Indent further, into the ordered list this time
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            quote {
                p { "Paragraph in quote" }
                ul {
                    li { p {"Item 1" } }
                    li { p {"Item 2" } }
                    li {
                        ol {
                            li { p { "Subitem 1" }  }
                            li {
                                p { "Subitem 2" }
                                p { "Subitem 2, paragraph 2" }
                                ul {
                                    li { p { "Following paragraph" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    );
}

#[test]
fn indent_more_from_middle_of_list() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" } }
            li { p {"Item 2" } }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li {
                    p { "Item 1" }
                    ul {
                        li { p { "Item 2"  } }
                    }
                }
                li { p { "Item 3" } }
            }
        }
    );
}

#[test]
fn indent_checklist_item_into_previous_item() {
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![
        ChecklistItem::new(false).with_content(vec![Span::new_text("First")]),
        ChecklistItem::new(false).with_content(vec![Span::new_text("Second")]),
        ChecklistItem::new(false).with_content(vec![Span::new_text("Third")]),
    ]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    let pointer = pointer_to_checklist_item_span(0, 2);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());

    let checklist = &editor.document().paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 2);
    let second = &checklist.checklist_items()[1];
    assert_eq!(second.content[0].text, "Second");
    assert_eq!(second.children.len(), 1);
    assert_eq!(second.children[0].content[0].text, "Third");

    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());

    let checklist = &editor.document().paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 3);
    assert_eq!(checklist.checklist_items()[2].content[0].text, "Third");
}

#[test]
fn indent_nested_checklist_child() {
    let child_a = ChecklistItem::new(false).with_content(vec![Span::new_text("Child A")]);
    let child_b = ChecklistItem::new(false).with_content(vec![Span::new_text("Child B")]);
    let parent = ChecklistItem::new(false)
        .with_content(vec![Span::new_text("Parent")])
        .with_children(vec![child_a.clone(), child_b.clone()]);
    let sibling = ChecklistItem::new(false).with_content(vec![Span::new_text("Sibling")]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![parent, sibling]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    let pointer = pointer_to_nested_checklist_item_span(0, vec![0, 1]);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());

    let checklist = &editor.document().paragraphs[0];
    let parent = &checklist.checklist_items()[0];
    assert_eq!(parent.children.len(), 1);
    let first_child = &parent.children[0];
    assert_eq!(first_child.children.len(), 1);
    assert_eq!(first_child.children[0].content[0].text, "Child B");

    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());

    let parent = &editor.document().paragraphs[0].checklist_items()[0];
    assert_eq!(parent.children.len(), 2);
    assert_eq!(parent.children[1].content[0].text, "Child B");
}

#[test]
fn indent_text_paragraph_into_checklist_item() {
    let document =
        Document::new().with_paragraphs(vec![checklist(&["Parent"]), text_paragraph("Child")]);
    let mut editor = DocumentEditor::new(document);

    let pointer = pointer_to_root_span(1);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());

    let checklist = &editor.document().paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 1);
    let parent = &checklist.checklist_items()[0];
    assert_eq!(parent.children.len(), 1);
    assert_eq!(parent.children[0].content[0].text, "Child");

    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());

    let checklist = &editor.document().paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 2);
    assert_eq!(checklist.checklist_items()[1].content[0].text, "Child");
}

#[test]
fn unindent_nested_checklist_item_moves_trailing_siblings_as_children() {
    // Create a structure:
    // - Parent
    //   - Child A
    //   - Child B  <- this will be unnested
    //   - Child C
    //   - Child D
    let child_a = ChecklistItem::new(false).with_content(vec![Span::new_text("Child A")]);
    let child_b = ChecklistItem::new(false).with_content(vec![Span::new_text("Child B")]);
    let child_c = ChecklistItem::new(false).with_content(vec![Span::new_text("Child C")]);
    let child_d = ChecklistItem::new(false).with_content(vec![Span::new_text("Child D")]);
    let parent = ChecklistItem::new(false)
        .with_content(vec![Span::new_text("Parent")])
        .with_children(vec![child_a, child_b, child_c, child_d]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![parent]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    // Position cursor on "Child B" (index 1 of parent's children)
    let pointer = pointer_to_nested_checklist_item_span(0, vec![0, 1]);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());

    // After unnesting, the structure should be:
    // - Parent
    //   - Child A
    // - Child B  <- now at top level
    //   - Child C  <- moved as child of Child B
    //   - Child D  <- moved as child of Child B
    let checklist = &editor.document().paragraphs[0];
    let items = checklist.checklist_items();
    assert_eq!(items.len(), 2, "Should have 2 top-level items now");

    // Verify Parent still has Child A
    let parent = &items[0];
    assert_eq!(parent.content[0].text, "Parent");
    assert_eq!(
        parent.children.len(),
        1,
        "Parent should have only Child A left"
    );
    assert_eq!(parent.children[0].content[0].text, "Child A");

    // Verify Child B is now at top level with C and D as its children
    let child_b = &items[1];
    assert_eq!(child_b.content[0].text, "Child B");
    assert_eq!(
        child_b.children.len(),
        2,
        "Child B should have Child C and D as children"
    );
    assert_eq!(child_b.children[0].content[0].text, "Child C");
    assert_eq!(child_b.children[1].content[0].text, "Child D");
}

#[test]
fn backspace_merges_checklist_item_into_previous_paragraph() {
    let inner_ordered = Paragraph::new_ordered_list().with_entries(vec![
        vec![text_paragraph("Inner first paragraph")],
        vec![
            text_paragraph("Inner second paragraph"),
            text_paragraph("Target paragraph"),
        ],
    ]);

    let outer_list = Paragraph::new_unordered_list().with_entries(vec![
        vec![text_paragraph("Outer first paragraph")],
        vec![text_paragraph("Outer second paragraph"), inner_ordered],
    ]);

    let blockquote = Paragraph::new_quote().with_children(vec![outer_list]);

    let child_item =
        ChecklistItem::new(false).with_content(vec![Span::new_text("Child checklist")]);
    let parent_item = ChecklistItem::new(false)
        .with_content(vec![Span::new_text("Parent checklist")])
        .with_children(vec![child_item]);

    let checklist = Paragraph::new_checklist().with_checklist_items(vec![parent_item]);

    let document = Document::new().with_paragraphs(vec![blockquote, checklist]);
    let mut editor = DocumentEditor::new(document);

    let pointer = pointer_to_checklist_item_span(1, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.backspace());

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let Paragraph::Quote { children } = &doc.paragraphs[0] else {
        panic!("expected blockquote as first paragraph");
    };
    assert_eq!(children.len(), 1);
    let Paragraph::UnorderedList { entries } = &children[0] else {
        panic!("expected unordered list inside blockquote");
    };
    assert_eq!(entries.len(), 2);

    let second_entry = &entries[1];
    assert_eq!(second_entry.len(), 2);
    let Paragraph::OrderedList {
        entries: inner_entries,
    } = &second_entry[1]
    else {
        panic!("expected nested ordered list");
    };
    assert_eq!(inner_entries.len(), 2);
    let second_inner_entry = &inner_entries[1];
    assert_eq!(second_inner_entry.len(), 3);

    let merged_paragraph = &second_inner_entry[1];
    assert_eq!(
        merged_paragraph.content()[0].text,
        "Target paragraphParent checklist"
    );

    let Paragraph::Checklist { items } = &second_inner_entry[2] else {
        panic!("expected checklist paragraph inserted");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].content[0].text, "Child checklist");
}

#[test]
fn backspace_merges_checklist_item_into_previous_checklist_item() {
    let existing_child =
        ChecklistItem::new(false).with_content(vec![Span::new_text("Existing child")]);
    let first_item = ChecklistItem::new(false)
        .with_content(vec![Span::new_text("First item")])
        .with_children(vec![existing_child]);
    let second_child = ChecklistItem::new(false).with_content(vec![Span::new_text("Second child")]);
    let second_item = ChecklistItem::new(false)
        .with_content(vec![Span::new_text("Second item")])
        .with_children(vec![second_child]);

    let checklist = Paragraph::new_checklist().with_checklist_items(vec![first_item, second_item]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document);

    let pointer = pointer_to_checklist_item_span(0, 1);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.backspace());

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let checklist = &doc.paragraphs[0];
    assert_eq!(checklist.checklist_items().len(), 1);
    let merged = &checklist.checklist_items()[0];
    let merged_text: String = merged
        .content
        .iter()
        .map(|span| span.text.as_str())
        .collect();
    assert_eq!(merged_text, "First itemSecond item");
    assert_eq!(merged.children.len(), 2);
    assert_eq!(merged.children[0].content[0].text, "Existing child");
    assert_eq!(merged.children[1].content[0].text, "Second child");
}

#[test]
fn unindent_nested_list_item_becomes_sibling() {
    let initial_doc = ftml! {
        ul {
            li {
                p { "Parent" }
                ul {
                    li { p { "Child" } }
                }
            }
            li { p { "After" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc);

    let mut path = ParagraphPath::new_root(0);
    path.push_entry(0, 1); // nested list paragraph within first entry
    path.push_entry(0, 0); // first entry inside nested list
    let pointer = CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    };
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.can_indent_less());
    assert!(editor.unindent_current_paragraph());

    let expected = ftml! {
        ul {
            li { p { "Parent" } }
            li { p { "Child" } }
            li { p { "After" } }
        }
    };
    assert_eq!(editor.document().clone(), expected);
}

#[test]
fn indent_numbered_item_under_bullet_item() {
    let document = Document::new().with_paragraphs(vec![
        unordered_list(&["Bullet"]),
        ordered_list(&["First", "Second"]),
    ]);
    let mut editor = DocumentEditor::new(document);

    let pointer = pointer_to_entry_span(1, 0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());

    let expected = ftml! {
        ul {
            li {
                p { "Bullet" }
                ol {
                    li { p { "First" } }
                }
            }
        }
        ol {
            li { p { "Second" } }
        }
    };
    assert_eq!(editor.document().clone(), expected);
}

#[test]
fn indent_bullet_item_under_numbered_item() {
    let document = Document::new().with_paragraphs(vec![
        ordered_list(&["One"]),
        unordered_list(&["Alpha", "Beta"]),
    ]);
    let mut editor = DocumentEditor::new(document);

    let pointer = pointer_to_entry_span(1, 0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.can_indent_more());
    assert!(editor.indent_current_paragraph());

    let expected = ftml! {
        ol {
            li {
                p { "One" }
                ul {
                    li { p { "Alpha" } }
                }
            }
        }
        ul {
            li { p { "Beta" } }
        }
    };
    assert_eq!(editor.document().clone(), expected);
}

#[test]
fn convert_paragraph_from_middle_of_list() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" } }
            li { p {"Item 2" } }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.set_paragraph_type(ParagraphType::Quote));
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p { "Item 1" } }
            }
            quote {
                p { "Item 2"  }
            }
            ul {
                li { p { "Item 3" } }
            }
        }
    );
}

#[test]
fn convert_nested_list_item_to_text_keeps_parent_list() {
    let initial_doc = ftml! {
        ul {
            li {
                p { "Parent" }
                ul {
                    li { p { "Child" } }
                }
            }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc);
    let mut path = ParagraphPath::new_root(0);
    path.push_entry(0, 1);
    path.push_entry(0, 0);
    let pointer = CursorPointer {
        paragraph_path: path,
        span_path: SpanPath::new(vec![0]),
        offset: 0,
        segment_kind: SegmentKind::Text,
    };
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Text));

    let expected = ftml! {
        ul {
            li {
                p { "Parent" }
                p { "Child" }
            }
        }
    };
    assert_eq!(editor.document().clone(), expected);
}

#[test]
fn convert_nested_checklist_item_to_text_is_forbidden() {
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![
        ChecklistItem::new(false)
            .with_content(vec![Span::new_text("Parent")])
            .with_children(vec![
                ChecklistItem::new(false).with_content(vec![Span::new_text("Child")]),
            ]),
    ]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let mut editor = DocumentEditor::new(document.clone());

    let pointer = pointer_to_nested_checklist_item_span(0, vec![0, 0]);
    assert!(editor.move_to_pointer(&pointer));
    assert!(!editor.set_paragraph_type(ParagraphType::Text));
    assert_eq!(editor.document().clone(), document);
}

#[test]
fn convert_checklist_item_with_children_to_text() {
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![
        ChecklistItem::new(false)
            .with_content(vec![Span::new_text("Parent")])
            .with_children(vec![
                ChecklistItem::new(false).with_content(vec![Span::new_text("Child")]),
            ]),
    ]);
    let mut editor = DocumentEditor::new(Document::new().with_paragraphs(vec![checklist]));
    let pointer = pointer_to_checklist_item_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Text));

    let document = editor.document();
    assert_eq!(document.paragraphs.len(), 2);
    assert_eq!(document.paragraphs[0].paragraph_type(), ParagraphType::Text);
    assert_eq!(document.paragraphs[0].content()[0].text, "Parent");
    assert_eq!(document.paragraphs[1].paragraph_type(), ParagraphType::Text);
    assert_eq!(document.paragraphs[1].content()[0].text, "Child");
}

#[test]
fn convert_checklist_item_with_children_to_quote() {
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![
        ChecklistItem::new(false)
            .with_content(vec![Span::new_text("Parent")])
            .with_children(vec![
                ChecklistItem::new(false).with_content(vec![Span::new_text("Child")]),
            ]),
    ]);
    let mut editor = DocumentEditor::new(Document::new().with_paragraphs(vec![checklist]));
    let pointer = pointer_to_checklist_item_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Quote));

    let document = editor.document();
    assert_eq!(document.paragraphs.len(), 1);
    let quote = &document.paragraphs[0];
    assert_eq!(quote.paragraph_type(), ParagraphType::Quote);
    let children = quote.children();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].paragraph_type(), ParagraphType::Text);
    assert_eq!(children[0].content()[0].text, "Parent");
    assert_eq!(children[1].paragraph_type(), ParagraphType::Checklist);
    assert_eq!(children[1].checklist_items()[0].content[0].text, "Child");
}

#[test]
fn convert_checklist_item_with_children_to_unordered_list() {
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![
        ChecklistItem::new(false)
            .with_content(vec![Span::new_text("Parent")])
            .with_children(vec![
                ChecklistItem::new(false).with_content(vec![Span::new_text("Child")]),
            ]),
    ]);
    let mut editor = DocumentEditor::new(Document::new().with_paragraphs(vec![checklist]));
    let pointer = pointer_to_checklist_item_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::UnorderedList));

    let document = editor.document();
    assert_eq!(document.paragraphs.len(), 1);
    let list = &document.paragraphs[0];
    assert_eq!(list.paragraph_type(), ParagraphType::UnorderedList);
    let entries = list.entries();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].len() >= 2);
    assert_eq!(entries[0][0].paragraph_type(), ParagraphType::Text);
    assert_eq!(entries[0][0].content()[0].text, "Parent");
    assert_eq!(entries[0][1].paragraph_type(), ParagraphType::Checklist);
    assert_eq!(entries[0][1].checklist_items()[0].content[0].text, "Child");
}

#[test]
fn convert_paragraph_from_list_to_text_extracts_item() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" } }
            li { p {"Item 2" } }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.set_paragraph_type(ParagraphType::Text));
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p {"Item 1" } }
            }
            p {"Item 2" }
            ul {
                li { p {"Item 3" } }
            }
        }
    );
}

#[test]
fn convert_paragraph_from_list_in_middle_of_list() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" } }
            li {
                p {"Item 2, paragraph 1" }
                p {"Item 2, paragraph 2" }
            }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.set_paragraph_type(ParagraphType::Quote));
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p {"Item 1" } }
                li {
                    quote { p {"Item 2, paragraph 1" } }
                    p {"Item 2, paragraph 2" }
                }
                li { p {"Item 3" } }
            }
        }
    );

    assert!(editor.set_paragraph_type(ParagraphType::Text));
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p {"Item 1" } }
                li {
                    p {"Item 2, paragraph 1" }
                    p {"Item 2, paragraph 2" }
                }
                li { p {"Item 3" } }
            }
        }
    );
}

#[test]
fn convert_paragraph_to_nested_list_in_middle_of_list() {
    let initial_doc = ftml! {
        ul {
            li { p {"Item 1" } }
            li {
                p {"Item 2, paragraph 1" }
                p {"Item 2, paragraph 2" }
            }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.indent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p {"Item 1" } }
                li {
                    ul {
                        li { p {"Item 2, paragraph 1" } }
                    }
                    p {"Item 2, paragraph 2" }
                }
                li { p {"Item 3" } }
            }
        }
    );

    assert!(editor.unindent_current_paragraph());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ul {
                li { p {"Item 1" } }
                li { p {"Item 2, paragraph 2" } }
                li { p {"Item 2, paragraph 1" } }
                li { p {"Item 3" } }
            }
        }
    );
}

#[test]
fn split_paragraph_list_in_middle_of_list_item() {
    let initial_doc = ftml! {
        ol {
            li { p {"Item 1" } }
            li {
                p {"Item 2, paragraph 1" }
                p {"Item 2, paragraph 2" }
            }
            li { p {"Item 3" } }
        }
    };
    let mut editor = DocumentEditor::new(initial_doc.clone());
    assert!(editor.move_down());
    assert!(editor.move_down());
    assert!(editor.insert_paragraph_break());
    assert_eq!(
        editor.document().clone(),
        ftml! {
            ol {
                li { p {"Item 1" } }
                li { p {"Item 2, paragraph 1" } }
                li { p {"Item 2, paragraph 2" } }
                li { p {"Item 3" } }
            }
        }
    );
}

#[test]
fn move_down_advances_to_next_paragraph() {
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_text().with_content(vec![Span::new_text("One")]),
        Paragraph::new_text().with_content(vec![Span::new_text("Two")]),
    ]);
    let mut editor = DocumentEditor::new(document);
    assert!(editor.move_down());
    let pointer = editor.cursor_pointer();
    assert_eq!(
        pointer.paragraph_path.steps(),
        pointer_to_root_span(1).paragraph_path.steps()
    );
    assert!(!editor.move_down());
}

#[test]
fn move_up_moves_to_previous_paragraph() {
    let document = Document::new().with_paragraphs(vec![
        Paragraph::new_text().with_content(vec![Span::new_text("Alpha")]),
        Paragraph::new_text().with_content(vec![Span::new_text("Beta")]),
    ]);
    let mut editor = DocumentEditor::new(document);
    assert!(editor.move_down());
    assert!(editor.move_up());
    let pointer = editor.cursor_pointer();
    assert_eq!(
        pointer.paragraph_path.steps(),
        pointer_to_root_span(0).paragraph_path.steps()
    );
    assert_eq!(pointer.offset, 0);
    assert!(!editor.move_up());
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
    let spans = &doc.paragraphs[0].content();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "styled text");
    assert_eq!(spans[0].style, InlineStyle::None);
}

#[test]
fn top_level_paragraph_type_change_updates_current_paragraph() {
    let document =
        Document::new().with_paragraphs(vec![text_paragraph("Alpha"), text_paragraph("Beta")]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Header1));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 2);
    assert_eq!(doc.paragraphs[0].paragraph_type(), ParagraphType::Header1);
    assert_eq!(doc.paragraphs[1].paragraph_type(), ParagraphType::Text);
}

#[test]
fn changing_sole_child_promotes_parent_container() {
    let quote = Paragraph::new_quote().with_children(vec![text_paragraph("Nested")]);
    let document = Document::new().with_paragraphs(vec![quote]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_child_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Header2));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let paragraph = &doc.paragraphs[0];
    assert_eq!(paragraph.paragraph_type(), ParagraphType::Header2);
    assert!(paragraph.children().is_empty());
    assert!(paragraph.entries().is_empty());
    assert_eq!(paragraph.content().len(), 1);
    assert_eq!(paragraph.content()[0].text, "Nested");
}

#[test]
fn changing_child_with_siblings_only_updates_that_child() {
    let quote = Paragraph::new_quote()
        .with_children(vec![text_paragraph("First"), text_paragraph("Second")]);
    let document = Document::new().with_paragraphs(vec![quote]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_child_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Header3));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let quote = &doc.paragraphs[0];
    assert_eq!(quote.paragraph_type(), ParagraphType::Quote);
    assert_eq!(quote.children().len(), 2);
    assert_eq!(quote.children()[0].paragraph_type(), ParagraphType::Header3);
    assert_eq!(quote.children()[1].paragraph_type(), ParagraphType::Text);
}

#[test]
fn checklist_item_to_text_promotes_parent_list_when_single_item() {
    let item = ChecklistItem::new(false).with_content(vec![Span::new_text("Task")]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![item]);
    let document = Document::new().with_paragraphs(vec![checklist]);

    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_checklist_item_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Text));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let paragraph = &doc.paragraphs[0];
    assert_eq!(paragraph.paragraph_type(), ParagraphType::Text);
    assert!(paragraph.checklist_items().is_empty());
    assert_eq!(paragraph.content().len(), 1);
    assert_eq!(paragraph.content()[0].text, "Task");
}

#[test]
fn checklist_item_with_siblings_only_changes_item() {
    let first = ChecklistItem::new(false).with_content(vec![Span::new_text("First")]);
    let second = ChecklistItem::new(false).with_content(vec![Span::new_text("Second")]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![first, second.clone()]);
    let document = Document::new().with_paragraphs(vec![checklist]);

    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_checklist_item_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Header1));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 2);
    assert_eq!(doc.paragraphs[0].paragraph_type(), ParagraphType::Header1);
    assert_eq!(doc.paragraphs[0].content()[0].text, "First");

    let checklist = &doc.paragraphs[1];
    assert_eq!(checklist.paragraph_type(), ParagraphType::Checklist);
    assert_eq!(checklist.checklist_items().len(), 1);
    assert_eq!(checklist.checklist_items()[0].content[0].text, "Second");
}

#[test]
fn checklist_item_state_updates_through_editor() {
    let item = ChecklistItem::new(false).with_content(vec![Span::new_text("Task")]);
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![item]);
    let document = Document::new().with_paragraphs(vec![checklist]);

    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_checklist_item_span(0, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert_eq!(editor.current_checklist_item_state(), Some(false));

    assert!(editor.set_current_checklist_item_checked(true));
    assert_eq!(editor.current_checklist_item_state(), Some(true));

    assert!(!editor.set_current_checklist_item_checked(true));
    assert_eq!(editor.current_checklist_item_state(), Some(true));
}

#[test]
fn unordered_list_item_conversion_splits_list() {
    let first = text_paragraph("First");
    let second = text_paragraph("Second");
    let third = text_paragraph("Third");
    let list =
        Paragraph::new_unordered_list().with_entries(vec![vec![first], vec![second], vec![third]]);
    let document = Document::new().with_paragraphs(vec![list]);

    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_entry_span(0, 1, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Header2));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 3);
    assert_eq!(
        doc.paragraphs[0].paragraph_type(),
        ParagraphType::UnorderedList
    );
    assert_eq!(doc.paragraphs[0].entries().len(), 1);
    assert_eq!(doc.paragraphs[0].entries()[0][0].content()[0].text, "First");

    assert_eq!(doc.paragraphs[1].paragraph_type(), ParagraphType::Header2);
    assert_eq!(doc.paragraphs[1].content()[0].text, "Second");

    assert_eq!(
        doc.paragraphs[2].paragraph_type(),
        ParagraphType::UnorderedList
    );
    assert_eq!(doc.paragraphs[2].entries().len(), 1);
    assert_eq!(doc.paragraphs[2].entries()[0][0].content()[0].text, "Third");
}

#[test]
fn nested_list_item_conversion_inside_quote() {
    let list = Paragraph::new_unordered_list().with_entries(vec![
        vec![text_paragraph("Alpha")],
        vec![text_paragraph("Beta")],
    ]);
    let quote = Paragraph::new_quote().with_children(vec![list]);
    let document = Document::new().with_paragraphs(vec![quote]);

    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_child_entry_span(0, 0, 1, 0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Text));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let quote = &doc.paragraphs[0];
    assert_eq!(quote.children().len(), 2);
    assert_eq!(
        quote.children()[0].paragraph_type(),
        ParagraphType::UnorderedList
    );
    assert_eq!(quote.children()[0].entries().len(), 1);
    assert_eq!(
        quote.children()[0].entries()[0][0].content()[0].text,
        "Alpha"
    );

    assert_eq!(quote.children()[1].paragraph_type(), ParagraphType::Text);
    assert_eq!(quote.children()[1].content()[0].text, "Beta");
}

#[test]
fn converting_between_lists_merges_all_entries() {
    let document = Document::new().with_paragraphs(vec![
        unordered_list(&["One"]),
        text_paragraph("Two"),
        unordered_list(&["Three"]),
    ]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_root_span(1);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::UnorderedList));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let list = &doc.paragraphs[0];
    assert_eq!(list.paragraph_type(), ParagraphType::UnorderedList);
    assert_eq!(list.entries().len(), 3);
    assert_eq!(list.entries()[0][0].content()[0].text, "One");
    assert_eq!(list.entries()[1][0].content()[0].text, "Two");
    assert_eq!(list.entries()[2][0].content()[0].text, "Three");
}

#[test]
fn converting_before_list_merges_forward_only() {
    let document =
        Document::new().with_paragraphs(vec![text_paragraph("Start"), ordered_list(&["Next"])]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_root_span(0);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::OrderedList));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let list = &doc.paragraphs[0];
    assert_eq!(list.paragraph_type(), ParagraphType::OrderedList);
    assert_eq!(list.entries().len(), 2);
    assert_eq!(list.entries()[0][0].content()[0].text, "Start");
    assert_eq!(list.entries()[1][0].content()[0].text, "Next");
}

#[test]
fn converting_to_checklist_merges_with_previous_only() {
    let document =
        Document::new().with_paragraphs(vec![checklist(&["Item 1"]), text_paragraph("Item 2")]);
    let mut editor = DocumentEditor::new(document);
    let pointer = pointer_to_root_span(1);
    assert!(editor.move_to_pointer(&pointer));
    assert!(editor.set_paragraph_type(ParagraphType::Checklist));

    let doc = editor.document();
    assert_eq!(doc.paragraphs.len(), 1);
    let list = &doc.paragraphs[0];
    assert_eq!(list.paragraph_type(), ParagraphType::Checklist);
    assert_eq!(list.checklist_items().len(), 2);
    assert_eq!(list.checklist_items()[0].content[0].text, "Item 1");
    assert_eq!(list.checklist_items()[1].content[0].text, "Item 2");
}

#[test]
fn converting_list_with_children_to_checklist_is_recursive() {
    let nested = unordered_list(&["Child 1", "Child 2"]);
    let quoted = Paragraph::new_quote().with_children(vec![text_paragraph("Nested quote")]);
    let list = Paragraph::new_unordered_list().with_entries(vec![
        vec![text_paragraph("Parent"), nested],
        vec![text_paragraph("Sibling"), quoted],
    ]);
    let mut document = Document::new().with_paragraphs(vec![list]);
    let path = ParagraphPath::new_root(0);
    assert!(structure::update_existing_list_type(
        &mut document,
        &path,
        ParagraphType::Checklist
    ));

    let checklist = &document.paragraphs[0];
    assert_eq!(checklist.paragraph_type(), ParagraphType::Checklist);
    assert_eq!(checklist.checklist_items().len(), 2);

    let parent = &checklist.checklist_items()[0];
    assert_eq!(parent.content[0].text, "Parent");
    assert_eq!(parent.children.len(), 2);
    assert_eq!(parent.children[0].content[0].text, "Child 1");
    assert_eq!(parent.children[1].content[0].text, "Child 2");

    let sibling = &checklist.checklist_items()[1];
    assert_eq!(sibling.content[0].text, "Sibling");
    assert_eq!(sibling.children.len(), 1);
    assert_eq!(sibling.children[0].content[0].text, "Nested quote");
}

#[test]
fn converting_quote_children_to_checklist_is_recursive() {
    let mut quote = Paragraph::new_quote()
        .with_children(vec![text_paragraph("First"), unordered_list(&["Second"])]);
    structure::apply_paragraph_type_in_place(&mut quote, ParagraphType::Checklist);

    let checklist = quote;
    assert_eq!(checklist.paragraph_type(), ParagraphType::Checklist);
    assert_eq!(checklist.checklist_items().len(), 2);
    assert_eq!(checklist.checklist_items()[0].content[0].text, "First");
    assert_eq!(checklist.checklist_items()[1].content[0].text, "Second");
}

#[test]
fn cursor_valid_after_nesting_checklist_item() {
    use crate::editor_display::EditorDisplay;

    // Reproduce issue where cursor position becomes [?,?] after nesting
    let checklist = Paragraph::new_checklist().with_checklist_items(vec![
        ChecklistItem::new(false).with_content(vec![Span::new_text("First")]),
        ChecklistItem::new(false).with_content(vec![Span::new_text("Second")]),
    ]);
    let document = Document::new().with_paragraphs(vec![checklist]);
    let editor = DocumentEditor::new(document);
    let mut display = EditorDisplay::new(editor);

    // Move to second item
    let pointer = pointer_to_checklist_item_span(0, 1);
    assert!(display.move_to_pointer(&pointer));

    // Indent to nest under first item
    assert!(display.indent_current_paragraph());

    // The cursor should still be valid and point to a segment
    let cursor = display.cursor_pointer();
    let segments = &display.segments;

    // Find if cursor points to a valid segment
    let segment_found = segments.iter().any(|segment| segment.matches(&cursor));
    assert!(
        segment_found,
        "Cursor should point to a valid segment after nesting. Cursor: {:?}",
        cursor
    );

    // Additionally verify the cursor path is correct for nested item
    assert_eq!(cursor.paragraph_path.steps().len(), 2);
    if let Some(PathStep::ChecklistItem { indices }) = cursor.paragraph_path.steps().last() {
        assert_eq!(
            indices,
            &vec![0usize, 0],
            "Cursor should point to nested item [0, 0]"
        );
    } else {
        panic!("Expected ChecklistItem path step");
    }

    // Now test rendering to ensure visual position is tracked
    display.render_document_with_positions(80, 0, None);

    // Check that the cursor visual position (from layout) is set
    let last_visual = display.cursor_visual();
    assert!(
        last_visual.is_some(),
        "cursor_visual() should return Some after rendering (cursor pointer: {:?}), not None (which shows as [?,?])",
        cursor
    );
}

#[test]
fn cursor_can_move_into_quote_blocks() {
    use crate::editor_display::EditorDisplay;

    let doc = ftml! {
        p { "Before quote" }
        quote {
            p { "First line in quote" }
            p { "Second line in quote" }
        }
        p { "After quote" }
    };
    let mut display = EditorDisplay::new(DocumentEditor::new(doc));

    // Render to populate visual_positions
    let _ = display.render_document_with_positions(80, 0, None);

    // Start at first paragraph, should be at [0]
    let pos0 = display.cursor_pointer();
    assert_eq!(
        pos0.paragraph_path.numeric_steps(),
        vec![0],
        "Should start at first paragraph"
    );

    // Move down - should enter the quote's first child at [1, 0]
    display.move_cursor_vertical(1);
    let pos1 = display.cursor_pointer();
    assert_eq!(
        pos1.paragraph_path.numeric_steps(),
        vec![1, 0],
        "Should be at first child of quote block"
    );

    // Move down again - should go to quote's second child at [1, 1]
    display.move_cursor_vertical(1);
    let pos2 = display.cursor_pointer();
    assert_eq!(
        pos2.paragraph_path.numeric_steps(),
        vec![1, 1],
        "Should be at second child of quote block"
    );

    // Move down again - should exit quote and go to "After quote" at [2]
    display.move_cursor_vertical(1);
    let pos3 = display.cursor_pointer();
    assert_eq!(
        pos3.paragraph_path.numeric_steps(),
        vec![2],
        "Should be at 'After quote' paragraph"
    );

    // Now test moving up from the bottom
    // Move up - should enter quote's last child at [1, 1]
    display.move_cursor_vertical(-1);
    let pos4 = display.cursor_pointer();
    assert_eq!(
        pos4.paragraph_path.numeric_steps(),
        vec![1, 1],
        "Should be at last child of quote block"
    );

    // Move up again - should go to quote's first child at [1, 0]
    display.move_cursor_vertical(-1);
    let pos5 = display.cursor_pointer();
    assert_eq!(
        pos5.paragraph_path.numeric_steps(),
        vec![1, 0],
        "Should be at first child of quote block"
    );

    // Move up again - should exit quote and go to "Before quote" at [0]
    display.move_cursor_vertical(-1);
    let pos6 = display.cursor_pointer();
    assert_eq!(
        pos6.paragraph_path.numeric_steps(),
        vec![0],
        "Should be at 'Before quote' paragraph"
    );
}

#[test]
fn cursor_moves_into_last_wrapped_line_when_moving_up() {
    use crate::editor_display::EditorDisplay;

    // Create a document with a long first paragraph that will wrap,
    // followed by a second paragraph
    let doc = ftml! {
        p { "This is a text paragraph which is rendered with a pretty tight wrap width to force creating multiple visual lines." }
        p { "This is the 2nd paragraph." }
    };
    let mut display = EditorDisplay::new(DocumentEditor::new(doc));

    // Render with narrow width to force wrapping of the first paragraph
    let _ = display.render_document_with_positions(30, 0, None);

    println!("\n=== Initial Visual Positions ===");
    println!(
        "Total visual positions: {}",
        display.visual_positions().len()
    );
    for (idx, vp) in display.visual_positions().into_iter().enumerate().take(20) {
        println!(
            "{}: line={}, col={}, path={:?}, offset={}",
            idx,
            vp.position.line,
            vp.position.column,
            vp.pointer.paragraph_path.numeric_steps(),
            vp.pointer.offset
        );
    }

    // Find the first position in the second paragraph
    let second_para_first = display
        .visual_positions()
        .iter()
        .find(|vp| vp.pointer.paragraph_path.numeric_steps() == vec![1])
        .expect("Should have positions for second paragraph")
        .clone();

    println!("\n=== Moving to 2nd paragraph (first position) ===");
    println!(
        "Target: line={}, col={}, offset={}",
        second_para_first.position.line,
        second_para_first.position.column,
        second_para_first.pointer.offset
    );

    // Move to the first position in the second paragraph
    assert!(display.move_to_pointer(&second_para_first.pointer));

    // Re-render to update visual positions
    let _ = display.render_document_with_positions(30, 0, None);

    let current_before_move = display.cursor_pointer();
    let current_visual_before = display
        .visual_positions()
        .into_iter()
        .find(|vp| vp.pointer == current_before_move);

    if let Some(cv) = current_visual_before {
        println!(
            "Current position before move: line={}, col={}",
            cv.position.line, cv.position.column
        );
    }

    // Find the last wrapped line of the first paragraph
    let first_para_last_line = display
        .visual_positions()
        .iter()
        .filter(|vp| vp.pointer.paragraph_path.numeric_steps() == vec![0])
        .map(|vp| vp.position.line)
        .max()
        .expect("Should have positions for first paragraph");

    println!(
        "\nLast wrapped line of first paragraph: line={}",
        first_para_last_line
    );

    // Move up - should land on the last wrapped line of the first paragraph
    println!("\n=== Moving up ===");
    display.move_cursor_vertical(-1);

    let after_move = display.cursor_pointer();
    println!(
        "After move cursor: path={:?}, offset={}",
        after_move.paragraph_path.numeric_steps(),
        after_move.offset
    );

    // Re-render to get updated visual position
    let _ = display.render_document_with_positions(30, 0, None);

    let after_visual = display
        .visual_positions()
        .into_iter()
        .find(|vp| vp.pointer == after_move);

    if let Some(av) = after_visual {
        println!(
            "After move visual position: line={}, col={}",
            av.position.line, av.position.column
        );

        assert_eq!(
            av.position.line, first_para_last_line,
            "Cursor should be on the last wrapped line of the first paragraph (line {}), but is on line {}",
            first_para_last_line, av.position.line
        );
    } else {
        panic!("Could not find visual position after move");
    }

    // Verify we're still in the first paragraph
    assert_eq!(
        after_move.paragraph_path.numeric_steps(),
        vec![0],
        "Cursor should be in the first paragraph"
    );
}

#[test]
fn cursor_moves_into_last_wrapped_line_when_moving_up_into_quote() {
    use crate::editor_display::EditorDisplay;

    // Create a document with a quote block containing wrapped text,
    // followed by a second paragraph
    let doc = ftml! {
        quote {
            p { "This is a text paragraph which is rendered with a pretty tight wrap width to force creating multiple visual lines." }
        }
        p { "This is the 2nd paragraph." }
    };
    let mut display = EditorDisplay::new(DocumentEditor::new(doc));

    // Render with narrow width to force wrapping of the first paragraph in the quote
    let _ = display.render_document_with_positions(30, 0, None);

    println!("\n=== Initial Visual Positions ===");
    for (idx, vp) in display.visual_positions().into_iter().enumerate() {
        println!(
            "{}: line={}, col={}, path={:?}, offset={}",
            idx,
            vp.position.line,
            vp.position.column,
            vp.pointer.paragraph_path.numeric_steps(),
            vp.pointer.offset
        );
    }

    // Find the first position in the second paragraph (root paragraph at index 1)
    let second_para_first = display
        .visual_positions()
        .iter()
        .find(|vp| vp.pointer.paragraph_path.numeric_steps() == vec![1])
        .expect("Should have positions for second paragraph")
        .clone();

    println!("\n=== Moving to 2nd paragraph (first position) ===");
    println!(
        "Target: line={}, col={}, offset={}",
        second_para_first.position.line,
        second_para_first.position.column,
        second_para_first.pointer.offset
    );

    // Move to the first position in the second paragraph
    assert!(display.move_to_pointer(&second_para_first.pointer));

    // Re-render to update visual positions
    let _ = display.render_document_with_positions(30, 0, None);

    let current_before_move = display.cursor_pointer();
    let current_visual_before = display
        .visual_positions()
        .into_iter()
        .find(|vp| vp.pointer == current_before_move);

    if let Some(cv) = current_visual_before {
        println!(
            "Current position before move: line={}, col={}",
            cv.position.line, cv.position.column
        );
    }

    // Find the last wrapped line of the paragraph in the quote (path [0, 0])
    let first_para_last_line = display
        .visual_positions()
        .iter()
        .filter(|vp| vp.pointer.paragraph_path.numeric_steps() == vec![0, 0])
        .map(|vp| vp.position.line)
        .max()
        .expect("Should have positions for first paragraph in quote");

    println!(
        "\nLast wrapped line of first paragraph in quote: line={}",
        first_para_last_line
    );

    // Move up - should land on the last wrapped line of the first paragraph in the quote
    println!("\n=== Moving up ===");
    display.move_cursor_vertical(-1);

    let after_move = display.cursor_pointer();
    println!(
        "After move cursor: path={:?}, offset={}",
        after_move.paragraph_path.numeric_steps(),
        after_move.offset
    );

    // Re-render to get updated visual position
    let _ = display.render_document_with_positions(30, 0, None);

    let after_visual = display
        .visual_positions()
        .into_iter()
        .find(|vp| vp.pointer == after_move);

    if let Some(av) = after_visual {
        println!(
            "After move visual position: line={}, col={}",
            av.position.line, av.position.column
        );

        assert_eq!(
            av.position.line, first_para_last_line,
            "Cursor should be on the last wrapped line of the first paragraph in quote (line {}), but is on line {}",
            first_para_last_line, av.position.line
        );
    } else {
        panic!("Could not find visual position after move");
    }

    // Verify we're in the quote's first paragraph
    assert_eq!(
        after_move.paragraph_path.numeric_steps(),
        vec![0, 0],
        "Cursor should be in the first paragraph of the quote"
    );
}

#[test]
fn test_paragraph_break_updates_subsequent_paragraph_lines() {
    use crate::editor_display::EditorDisplay;
    use ratatui::layout::Rect;

    // Create a document with two paragraphs that will wrap
    // First paragraph: "This is a very long line that will definitely wrap when we render it at a narrow width"
    // Second paragraph: "Second paragraph here"
    let doc = Document::new().with_paragraphs(vec![
        text_paragraph("This is a very long line that will definitely wrap when we render it at a narrow width"),
        text_paragraph("Second paragraph here"),
    ]);

    let mut display = EditorDisplay::new(DocumentEditor::new(doc));

    // Render with narrow width to force wrapping
    let text_area = Rect {
        x: 0,
        y: 0,
        width: 30,
        height: 20,
    };
    display.render_document(28, 2, None); // wrap_width=28, left_padding=2
    display.update_after_render(text_area);

    // Print initial layout
    eprintln!("\n=== INITIAL LAYOUT ===");
    let layout = display.get_layout();
    eprintln!("Total lines: {}", layout.total_lines);
    for info in &layout.paragraph_lines {
        eprintln!(
            "Paragraph {}: lines {}-{} ({} lines)",
            info.paragraph_index,
            info.start_line,
            info.end_line,
            info.end_line - info.start_line + 1
        );
    }

    // Move cursor to middle of first paragraph (around character 30)
    for _ in 0..30 {
        display.move_right();
    }

    eprintln!("\n=== BEFORE INSERT PARAGRAPH BREAK ===");
    eprintln!("Cursor pointer: {:?}", display.cursor_pointer());
    if let Some(cursor_vis) = display.cursor_visual() {
        eprintln!(
            "Cursor visual: line={}, col={}",
            cursor_vis.line, cursor_vis.column
        );
    }

    // Insert a paragraph break (Ctrl-J)
    display.insert_paragraph_break();

    // Render again
    display.render_document(28, 2, None);
    display.update_after_render(text_area);

    eprintln!("\n=== AFTER INSERT PARAGRAPH BREAK ===");
    let layout = display.get_layout();
    eprintln!("Total lines: {}", layout.total_lines);
    for info in &layout.paragraph_lines {
        eprintln!(
            "Paragraph {}: lines {}-{} ({} lines)",
            info.paragraph_index,
            info.start_line,
            info.end_line,
            info.end_line - info.start_line + 1
        );
    }
    eprintln!("Cursor pointer: {:?}", display.cursor_pointer());
    if let Some(cursor_vis) = display.cursor_visual() {
        eprintln!(
            "Cursor visual: line={}, col={}",
            cursor_vis.line, cursor_vis.column
        );
    }

    // Now try to move down to the original second paragraph (now third paragraph)
    // We should be able to reach it
    let initial_para = display
        .cursor_pointer()
        .paragraph_path
        .root_index()
        .unwrap();
    eprintln!("\n=== MOVING DOWN ===");

    // Move down several times to reach the third paragraph
    for i in 0..10 {
        display.move_cursor_vertical(1);
        let current_para = display
            .cursor_pointer()
            .paragraph_path
            .root_index()
            .unwrap();
        if let Some(cursor_vis) = display.cursor_visual() {
            eprintln!(
                "After move {}: para={}, line={}, col={}",
                i + 1,
                current_para,
                cursor_vis.line,
                cursor_vis.column
            );
        }

        // If we've reached paragraph 2 (the original second paragraph), success!
        if current_para == 2 {
            eprintln!("\n✓ Successfully reached paragraph 2");
            return;
        }
    }

    // If we get here, we couldn't reach paragraph 2
    panic!("Failed to reach paragraph 2 (original second paragraph) after moving down 10 times");
}

#[test]
fn test_incremental_update_adjusts_subsequent_paragraphs() {
    use crate::editor_display::EditorDisplay;
    use ratatui::layout::Rect;

    // Create document with two wrapping paragraphs
    let doc = Document::new().with_paragraphs(vec![
        text_paragraph("First paragraph with enough text to wrap when rendered at narrow width"),
        text_paragraph("Second paragraph also wraps"),
    ]);

    let mut display = EditorDisplay::new(DocumentEditor::new(doc));

    // Initial render
    let text_area = Rect {
        x: 0,
        y: 0,
        width: 30,
        height: 20,
    };
    display.render_document(28, 2, None);
    display.update_after_render(text_area);

    eprintln!("\n=== INITIAL STATE ===");
    let layout = display.get_layout();
    for info in &layout.paragraph_lines {
        eprintln!(
            "Para {}: lines {}-{}",
            info.paragraph_index, info.start_line, info.end_line
        );
    }

    // Insert a character in the first paragraph (should trigger incremental update)
    display.move_right();
    display.move_right();
    display.insert_char('X');

    // Mark dirty to trigger incremental update
    let incremental_ok = display.clear_render_cache();
    eprintln!("\n=== AFTER INSERT 'X' ===");
    eprintln!("Incremental update succeeded: {}", incremental_ok);

    // Re-render (should use incremental update if it worked)
    display.render_document(28, 2, None);
    display.update_after_render(text_area);

    let layout = display.get_layout();
    for info in &layout.paragraph_lines {
        eprintln!(
            "Para {}: lines {}-{}",
            info.paragraph_index, info.start_line, info.end_line
        );
    }

    // Try to move cursor to second paragraph
    eprintln!("\n=== MOVING TO SECOND PARAGRAPH ===");
    for i in 0..20 {
        let para = display
            .cursor_pointer()
            .paragraph_path
            .root_index()
            .unwrap();
        if para == 1 {
            eprintln!("✓ Reached paragraph 1 after {} moves", i);
            return;
        }
        display.move_cursor_vertical(1);
        if let Some(cursor_vis) = display.cursor_visual() {
            eprintln!(
                "Move {}: para={}, line={}",
                i + 1,
                display
                    .cursor_pointer()
                    .paragraph_path
                    .root_index()
                    .unwrap(),
                cursor_vis.line
            );
        }
    }

    panic!("Failed to reach paragraph 1 after moving down 20 times");
}

#[test]
fn test_cursor_down_after_paragraph_break_lands_on_correct_line() {
    use crate::editor_display::EditorDisplay;
    use ratatui::layout::Rect;

    // Simulate: paragraph with wrapping text, add word, Ctrl-J, Down
    let doc = Document::new().with_paragraphs(vec![
        text_paragraph("First paragraph with some text that will wrap"),
        text_paragraph("Second paragraph here"),
    ]);

    let mut display = EditorDisplay::new(DocumentEditor::new(doc));

    let text_area = Rect {
        x: 0,
        y: 0,
        width: 30,
        height: 20,
    };
    display.render_document(28, 2, None);
    display.update_after_render(text_area);

    eprintln!("\n=== INITIAL STATE ===");
    let layout = display.get_layout();
    for info in &layout.paragraph_lines {
        eprintln!(
            "Para {}: lines {}-{}",
            info.paragraph_index, info.start_line, info.end_line
        );
    }
    if let Some(cursor) = display.cursor_visual() {
        eprintln!("Cursor at line {}", cursor.line);
    }

    // Add a word at the beginning
    display.insert_char('W');
    display.insert_char('o');
    display.insert_char('r');
    display.insert_char('d');
    display.insert_char(' ');

    eprintln!("\n=== AFTER ADDING 'Word ' ===");
    display.render_document(28, 2, None);
    display.update_after_render(text_area);

    let layout = display.get_layout();
    for info in &layout.paragraph_lines {
        eprintln!(
            "Para {}: lines {}-{}",
            info.paragraph_index, info.start_line, info.end_line
        );
    }
    if let Some(cursor) = display.cursor_visual() {
        eprintln!(
            "Cursor at line {} (para {})",
            cursor.line,
            display
                .cursor_pointer()
                .paragraph_path
                .root_index()
                .unwrap()
        );
    }

    // Press Ctrl-J (insert paragraph break)
    display.insert_paragraph_break();

    eprintln!("\n=== AFTER Ctrl-J (paragraph break) ===");
    display.render_document(28, 2, None);
    display.update_after_render(text_area);

    let layout = display.get_layout();
    for info in &layout.paragraph_lines {
        eprintln!(
            "Para {}: lines {}-{}",
            info.paragraph_index, info.start_line, info.end_line
        );
    }
    let cursor_before_down = display.cursor_visual().unwrap();
    let para_before_down = display
        .cursor_pointer()
        .paragraph_path
        .root_index()
        .unwrap();
    eprintln!(
        "Cursor at line {} (para {})",
        cursor_before_down.line, para_before_down
    );

    // Find the first line of paragraph 1 (the continuation of original para 0) BEFORE moving
    let para1_info = layout
        .paragraph_lines
        .iter()
        .find(|info| info.paragraph_index == 1)
        .expect("Paragraph 1 should exist");
    let para1_start = para1_info.start_line;
    let para1_end = para1_info.end_line;

    // Press Down once
    display.move_cursor_vertical(1);

    eprintln!("\n=== AFTER pressing Down ===");
    let cursor_after_down = display.cursor_visual().unwrap();
    let para_after_down = display
        .cursor_pointer()
        .paragraph_path
        .root_index()
        .unwrap();
    eprintln!(
        "Cursor at line {} (para {})",
        cursor_after_down.line, para_after_down
    );

    // The cursor should move down by exactly 1 line (or 2 if there's a blank line)
    // It should land on the first line of the next paragraph
    let expected_line = cursor_before_down.line + 1;

    eprintln!(
        "Expected cursor at line {} (para 1 starts at {})",
        expected_line, para1_start
    );

    // The user expects: pressing Down from line 2 should move to line 3 (next line in same para)
    // OR if para 1 only has 1 line, should skip blank line and go to para 2
    // The bug: cursor might skip too far

    // Let's check what actually happened vs what we expect
    let expected_next_line = cursor_before_down.line + 1;

    eprintln!("\nDIAGNOSTICS:");
    eprintln!("- Cursor was at line {}", cursor_before_down.line);
    eprintln!("- Expected to move to line {}", expected_next_line);
    eprintln!("- Actually moved to line {}", cursor_after_down.line);
    eprintln!("- Para 1 spans lines {}-{}", para1_start, para1_end);

    // Now continue pressing Down to reach para 2 (the original second paragraph)
    // Track all line numbers we visit
    let mut visited_lines = vec![cursor_after_down.line];

    eprintln!("\n=== CONTINUING TO PARA 2 ===");
    for i in 0..10 {
        let para = display
            .cursor_pointer()
            .paragraph_path
            .root_index()
            .unwrap();
        if para == 2 {
            eprintln!("Reached para 2 after {} more moves", i);
            break;
        }
        display.move_cursor_vertical(1);
        if let Some(cursor) = display.cursor_visual() {
            visited_lines.push(cursor.line);
            eprintln!(
                "Move {}: line {}, para {}",
                i + 1,
                cursor.line,
                display
                    .cursor_pointer()
                    .paragraph_path
                    .root_index()
                    .unwrap()
            );
        }
    }

    // Check for gaps in visited lines
    eprintln!("\nVisited lines: {:?}", visited_lines);

    for i in 1..visited_lines.len() {
        let jump = visited_lines[i].saturating_sub(visited_lines[i - 1]);
        if jump > 2 {
            panic!(
                "BUG FOUND! Cursor jumped {} lines from {} to {} (expected max 2 to account for blank lines)",
                jump,
                visited_lines[i - 1],
                visited_lines[i]
            );
        }
    }

    eprintln!("\n✓ No unexpected line skipping detected");
}

#[test]
fn test_cursor_down_after_incremental_wrap_no_line_skip() {
    use crate::editor_display::EditorDisplay;

    // Regression test for bug where cursor would jump extra lines after
    // incremental update caused text wrapping.
    // Bug: After wrapping paragraph 0 from 1 to 2 lines, subsequent paragraphs
    // were being adjusted TWICE (once by paragraph_index, once by start_line),
    // causing cursor to land on wrong line when moving down.

    eprintln!("\n=== Test: Cursor Down After Incremental Wrap (No Line Skip) ===");

    // Create document with 3 short paragraphs
    let doc = Document::new().with_paragraphs(vec![
        text_paragraph("Hi"),
        text_paragraph("Second"),
        text_paragraph("Third"),
    ]);

    let mut display = EditorDisplay::new(DocumentEditor::new(doc));

    // Initial render with narrow width to test wrapping
    let wrap_width = 20; // Narrow enough to cause wrapping
    let left_padding = 2;
    display.render_document(wrap_width, left_padding, None);

    eprintln!("\nInitial state:");
    eprintln!("  Paragraph 0: 'Hi'");
    eprintln!("  Paragraph 1: 'Second'");
    eprintln!("  Paragraph 2: 'Third'");

    // Cursor starts at beginning of document - move to end of first paragraph
    display.move_right();
    display.move_right();

    let initial_cursor = display.cursor_visual().unwrap();
    eprintln!("\nCursor at end of 'Hi': line {}", initial_cursor.line);
    assert_eq!(initial_cursor.line, 0, "Cursor should start on line 0");

    // Add text character by character to trigger incremental updates
    // Add enough to cause wrapping (using characters that will wrap)
    // With wrap_width=20, left_padding=2, content_width=18
    // "Hi" (2) + 20 more chars = 22 total, which should wrap
    eprintln!("\nAdding text to cause wrapping...");
    for ch in "XXXXXXXXXXXXXXXXXXXX".chars() {
        // 20 X's
        display.insert_char(ch);
        // Each insert marks paragraph as modified
    }

    // Trigger incremental update and render
    display.clear_render_cache();
    display.render_document(wrap_width, left_padding, None);

    // Get cursor position after wrapping
    let after_wrap_cursor = display.cursor_visual().unwrap();
    eprintln!("\nAfter wrapping:");
    eprintln!("  Paragraph 0 text: 'Hi{}'", "X".repeat(20));
    eprintln!("  Cursor now at line: {}", after_wrap_cursor.line);

    // Check paragraph layout to see if wrapping occurred
    let layout = display.get_layout();
    let para0_info = layout
        .paragraph_lines
        .iter()
        .find(|info| info.paragraph_index == 0)
        .unwrap();
    eprintln!(
        "  Paragraph 0 spans lines {}-{}",
        para0_info.start_line, para0_info.end_line
    );

    // Paragraph 0 should have wrapped to 2 lines
    // Expected layout:
    // Line 0: "HiXXXXXXXXXXXXXXX" (up to wrap width)
    // Line 1: (remaining text of paragraph 0)
    // Line 2: (blank line between paragraphs)
    // Line 3: "Second" (paragraph 1 should be at line 3, NOT line 4)

    assert!(
        para0_info.end_line > para0_info.start_line,
        "Paragraph 0 should have wrapped to multiple lines, but is at lines {}-{}",
        para0_info.start_line,
        para0_info.end_line
    );

    // Now move cursor down - this is where the bug occurred
    eprintln!("\nMoving cursor down...");
    display.move_cursor_vertical(1);

    let after_down_cursor = display.cursor_visual().unwrap();
    eprintln!("Cursor after Down: line {}", after_down_cursor.line);

    // Cursor should move to next content line (paragraph 1)
    // With the bug, it would skip from line 1 -> 4
    // Without bug, it should go line 1 -> 3 (line 2 is blank)

    // The key assertion: cursor should NOT jump to line 4 or beyond
    // Paragraph 1 starts at line 3 (after para 0's 2 lines + 1 blank)
    assert!(
        after_down_cursor.line <= 3,
        "Cursor should not skip lines - expected at most line 3, got line {}",
        after_down_cursor.line
    );

    // More specific: cursor should land on paragraph 1's line (line 3)
    // Search nearest line should find line 3, not line 4
    assert_eq!(
        after_down_cursor.line, 3,
        "Cursor should land on paragraph 1 at line 3, not skip to line 4+"
    );

    eprintln!("\n✓ Cursor correctly landed on line 3 (no line skip bug)");
}
