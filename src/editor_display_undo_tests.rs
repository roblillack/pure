use tdoc::{InlineStyle, ftml};

use super::EditorDisplay;
use crate::editor::{CursorPointer, DocumentEditor, ParagraphPath, SegmentKind, SpanPath};

fn display_from(doc: tdoc::Document) -> EditorDisplay {
    EditorDisplay::new(DocumentEditor::new(doc))
}

fn paragraph_text(display: &EditorDisplay, index: usize) -> String {
    fn collect(spans: &[tdoc::Span], out: &mut String) {
        for span in spans {
            out.push_str(&span.text);
            collect(&span.children, out);
        }
    }
    let mut out = String::new();
    collect(display.document().paragraphs[index].content(), &mut out);
    out
}

fn pointer(root: usize, offset: usize) -> CursorPointer {
    CursorPointer {
        paragraph_path: ParagraphPath::new_root(root),
        span_path: SpanPath::new(vec![0]),
        offset,
        segment_kind: SegmentKind::Text,
    }
}

#[test]
fn undo_restores_state_before_insert() {
    let mut display = display_from(ftml! { p { "Hello" } });
    assert!(!display.can_undo());
    assert!(!display.undo());

    assert!(display.insert_char('X'));
    assert_eq!(paragraph_text(&display, 0), "XHello");
    assert!(display.can_undo());

    assert!(display.undo());
    assert_eq!(paragraph_text(&display, 0), "Hello");
    assert_eq!(display.cursor_pointer().offset, 0);
    assert!(!display.can_undo());
    assert!(display.can_redo());

    assert!(display.redo());
    assert_eq!(paragraph_text(&display, 0), "XHello");
    assert_eq!(display.cursor_pointer().offset, 1);
    assert!(display.can_undo());
    assert!(!display.can_redo());
}

#[test]
fn typing_run_coalesces_into_one_undo_step() {
    let mut display = display_from(ftml! { p { "world" } });
    for ch in ['h', 'e', 'y', ' '] {
        assert!(display.insert_char(ch));
    }
    assert_eq!(paragraph_text(&display, 0), "hey world");

    assert!(display.undo());
    assert_eq!(paragraph_text(&display, 0), "world");
    assert!(!display.can_undo());
}

#[test]
fn cursor_move_breaks_coalescing() {
    let mut display = display_from(ftml! { p { "base" } });
    assert!(display.insert_char('a'));
    assert!(display.insert_char('b'));
    assert_eq!(paragraph_text(&display, 0), "abbase");

    assert!(display.move_left());
    assert!(display.insert_char('c'));
    assert_eq!(paragraph_text(&display, 0), "acbbase");

    assert!(display.undo());
    assert_eq!(paragraph_text(&display, 0), "abbase");
    assert!(display.undo());
    assert_eq!(paragraph_text(&display, 0), "base");
    assert!(!display.can_undo());
}

#[test]
fn backspace_run_coalesces() {
    let mut display = display_from(ftml! { p { "Hello" } });
    display.move_to_segment_end();
    assert!(display.backspace());
    assert!(display.backspace());
    assert_eq!(paragraph_text(&display, 0), "Hel");

    assert!(display.undo());
    assert_eq!(paragraph_text(&display, 0), "Hello");
    assert!(!display.can_undo());
}

#[test]
fn delete_does_not_coalesce_with_typing() {
    let mut display = display_from(ftml! { p { "Hello" } });
    assert!(display.insert_char('X'));
    assert!(display.delete());
    assert_eq!(paragraph_text(&display, 0), "Xello");

    assert!(display.undo());
    assert_eq!(paragraph_text(&display, 0), "XHello");
    assert!(display.undo());
    assert_eq!(paragraph_text(&display, 0), "Hello");
}

#[test]
fn new_edit_clears_redo_history() {
    let mut display = display_from(ftml! { p { "Hello" } });
    assert!(display.insert_char('a'));
    assert!(display.undo());
    assert!(display.can_redo());

    assert!(display.insert_char('b'));
    assert!(!display.can_redo());
    assert!(!display.redo());
    assert_eq!(paragraph_text(&display, 0), "bHello");
}

#[test]
fn undo_restores_paragraph_break() {
    let mut display = display_from(ftml! { p { "HelloWorld" } });
    for _ in 0..5 {
        assert!(display.move_right());
    }
    assert!(display.insert_paragraph_break());
    assert_eq!(display.document().paragraphs.len(), 2);

    assert!(display.undo());
    assert_eq!(display.document().paragraphs.len(), 1);
    assert_eq!(paragraph_text(&display, 0), "HelloWorld");
    assert_eq!(display.cursor_pointer().offset, 5);

    assert!(display.redo());
    assert_eq!(display.document().paragraphs.len(), 2);
}

#[test]
fn undo_restores_inline_style() {
    let mut display = display_from(ftml! { p { "Hello" } });
    let selection = (pointer(0, 0), pointer(0, 5));
    assert!(display.apply_inline_style_to_selection(&selection, InlineStyle::Bold));

    assert!(display.undo());
    let content = display.document().paragraphs[0].content();
    assert_eq!(content.len(), 1);
    assert_eq!(content[0].text, "Hello");
    assert!(content[0].children.is_empty());
}

#[test]
fn undo_restores_removed_selection() {
    let mut display = display_from(ftml! { p { "Hello world" } });
    let selection = (pointer(0, 0), pointer(0, 6));
    assert!(display.remove_selection(&selection));
    assert_eq!(paragraph_text(&display, 0), "world");

    assert!(display.undo());
    assert_eq!(paragraph_text(&display, 0), "Hello world");
}

#[test]
fn undo_depth_is_capped() {
    let mut display = display_from(ftml! { p { "x" } });
    for _ in 0..(super::MAX_UNDO_DEPTH + 10) {
        assert!(display.insert_char('a'));
        // Break coalescing so every insert becomes its own undo step
        display.last_edit_kind = None;
    }

    let mut undo_steps = 0;
    while display.undo() {
        undo_steps += 1;
    }
    assert_eq!(undo_steps, super::MAX_UNDO_DEPTH);
}
