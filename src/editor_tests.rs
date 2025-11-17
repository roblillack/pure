use tdoc::ftml;

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

fn pointer_to_nested_checklist_item_span(
    root_index: usize,
    indices: Vec<usize>,
) -> CursorPointer {
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
        .map(|text| {
            ChecklistItem::new(false).with_content(vec![Span::new_text(*text)])
        })
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
    assert_eq!(checklist.checklist_items()[1].checked, false);
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
                            }
                            li { p { "Following paragraph" } }
                        }
                    }
                }
            }
        }
    );

    // Indent further, into the second list item this time
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
                                p { "Following paragraph" }
                            }
                        }
                    }
                }
            }
        }
    );

    assert!(!editor.can_indent_more());
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
    let document = Document::new().with_paragraphs(vec![
        checklist(&["Parent"]),
        text_paragraph("Child"),
    ]);
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
        editor
            .apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Code)
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
    let list = Paragraph::new_unordered_list().with_entries(vec![
        vec![first],
        vec![second],
        vec![third],
    ]);
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
    assert_eq!(quote.children()[0].entries()[0][0].content()[0].text, "Alpha");

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
