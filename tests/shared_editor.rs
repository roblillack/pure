//! Integration test: drive the *shared* `rutle` core from Pure and render
//! it into a real Ratatui buffer via `RatatuiDrawContext`. This proves the shared
//! crate is usable from the TUI side end-to-end (edit -> layout -> terminal cells).

use pure_tui::ratatui_draw_context::{terminal_theme, RatatuiDrawContext};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use rutle::Renderer as StructuredRichDisplay;
use std::io::Cursor;
use tdoc::{markdown, Document};

// rutle works on `tdoc::Document` directly and leaves (de)serialization to tdoc,
// so these thin markdown round-trip helpers (formerly `tdoc_editor`'s
// `markdown_converter`) live in the test.
fn markdown_to_document(md: &str) -> Document {
    markdown::parse(Cursor::new(md.as_bytes())).unwrap_or_else(|_| Document::new())
}

fn document_to_markdown(doc: &Document) -> String {
    let mut buffer: Vec<u8> = Vec::new();
    markdown::write(&mut buffer, doc).expect("serialize document to markdown");
    String::from_utf8(buffer).unwrap_or_default()
}

/// Read a row of the buffer back as a trimmed string.
fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
    let mut s = String::new();
    for col in 0..width {
        if let Some(cell) = buf.cell((col, row)) {
            s.push_str(cell.symbol());
        }
    }
    s.trim_end().to_string()
}

fn buffer_text(buf: &Buffer, area: Rect) -> String {
    (0..area.height)
        .map(|r| row_text(buf, r, area.width))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render(display: &mut StructuredRichDisplay, area: Rect) -> Buffer {
    let mut buf = Buffer::empty(area);
    let mut ctx = RatatuiDrawContext::new(&mut buf, area);
    display.draw(&mut ctx);
    buf
}

#[test]
fn shared_editor_lays_out_into_terminal_buffer() {
    let area = Rect::new(0, 0, 40, 12);
    let mut display = StructuredRichDisplay::new(0, 0, area.width as i32, area.height as i32);
    display.set_theme(terminal_theme());
    display
        .editor_mut()
        .set_document(markdown_to_document("# Title\n\nHello world\n"));

    let buf = render(&mut display, area);
    let text = buffer_text(&buf, area);

    assert!(text.contains("Title"), "heading missing:\n{text}");
    assert!(text.contains("Hello world"), "paragraph missing:\n{text}");
}

#[test]
fn shared_editor_reflects_edits() {
    let area = Rect::new(0, 0, 40, 12);
    let mut display = StructuredRichDisplay::new(0, 0, area.width as i32, area.height as i32);
    display.set_theme(terminal_theme());
    display
        .editor_mut()
        .set_document(markdown_to_document("Hello world\n"));

    // Place the caret at the end of the line and type via the shared editor.
    {
        let editor = display.editor_mut();
        editor.move_cursor_to_line_end();
        let _ = editor.insert_text("!!!");
    }

    let buf = render(&mut display, area);
    let text = buffer_text(&buf, area);

    assert!(
        text.contains("Hello world!!!"),
        "edit not reflected in layout:\n{text}"
    );
}

#[test]
fn shared_editor_renders_list_structure() {
    let area = Rect::new(0, 0, 40, 12);
    let mut display = StructuredRichDisplay::new(0, 0, area.width as i32, area.height as i32);
    display.set_theme(terminal_theme());
    display
        .editor_mut()
        .set_document(markdown_to_document("- one\n- two\n"));

    let buf = render(&mut display, area);
    let text = buffer_text(&buf, area);

    assert!(text.contains("one"), "list item 1 missing:\n{text}");
    assert!(text.contains("two"), "list item 2 missing:\n{text}");
}

/// Copying a styled selection and pasting it must keep the inline styling — the
/// structure-preserving clipboard path (`get_selection_document` +
/// `insert_document`) that Pure's app uses for Ctrl+C / Ctrl+V.
#[test]
fn shared_editor_paste_preserves_bold() {
    use rutle::Editor;
    let mut e = Editor::default();
    e.set_document(markdown_to_document("Pack the **essentials** here\n"));
    e.select_all();
    let frag = e.get_selection_document().expect("fragment");
    e.clear_selection();
    e.move_cursor_to_line_end();
    let _ = e.insert_document(&frag);
    let out = document_to_markdown(e.document());
    let bold = out.matches("**essentials**").count();
    assert_eq!(bold, 2, "expected 2 bold copies, got {bold}: {out}");
}

/// Pasting a styled single-paragraph fragment into a *nested* leaf (here a
/// block quote) must still preserve inline styling, not degrade to markdown
/// text — exercises `insert_document`'s non-top-level path.
#[test]
fn shared_editor_paste_into_quote_preserves_bold() {
    use rutle::{DocumentPosition, Editor};
    let mut e = Editor::default();
    e.set_document(markdown_to_document(
        "Pack the **essentials** here\n\n> Travel light.\n",
    ));
    e.set_cursor(DocumentPosition::new(0, 0));
    e.move_cursor_to_line_end_extend();
    let frag = e.get_selection_document().expect("fragment");
    e.clear_selection();
    e.move_cursor_down();
    e.move_cursor_to_line_end();
    let _ = e.insert_document(&frag);
    let out = document_to_markdown(e.document());
    assert!(
        out.contains("**essentials**"),
        "bold lost when pasting into a quote: {out}"
    );
}
