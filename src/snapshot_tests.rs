//! SVG snapshot tests for full-app interactions.
//!
//! Each test drives the real `App` through synthetic key/mouse events and
//! snapshots the rendered terminal as SVG (see [`crate::test_harness`]).
//! Review changed snapshots with `cargo insta review`; the `.snap.svg` files
//! under `src/snapshots/` open directly in a browser.

use crossterm::event::{Event, KeyCode, KeyModifiers};
use tdoc::{Document, ftml};

use super::TestApp;

const WIDTH: u16 = 72;
const HEIGHT: u16 = 18;

/// Emit one binary insta snapshot named `<name>.svg`.
fn assert_svg(name: &str, app: &mut TestApp) {
    let svg = app.svg();
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.bind(|| {
        insta::assert_binary_snapshot!(format!("{name}.svg").as_str(), svg.into_bytes());
    });
}

fn sample_document() -> Document {
    ftml! {
        h1 { "Packing List" }
        p { "Pack the " b { "essentials" } " before the " i { "long" } " trip." }
        ul {
            li { p { "Passport" } }
            li { p { "Tickets" } }
        }
        quote {
            p { "Travel light." }
        }
    }
}

fn sample_app() -> TestApp {
    TestApp::new(WIDTH, HEIGHT, sample_document())
}

#[test]
fn initial_document() {
    let mut app = sample_app();
    assert_svg("initial_document", &mut app);
}

#[test]
fn typing_inserts_text() {
    let mut app = sample_app();
    app.type_text("Summer ");
    assert_svg("typing_inserts_text", &mut app);
}

#[test]
fn undo_reverts_typing_and_redo_restores_it() {
    let mut app = sample_app();
    app.type_text("Summer ");
    app.ctrl('z');
    assert_svg("undo_reverts_typing", &mut app);
    app.ctrl('y');
    assert_svg("redo_restores_typing", &mut app);
}

#[test]
fn nothing_to_undo_shows_status_message() {
    let mut app = sample_app();
    app.ctrl('z');
    assert_svg("nothing_to_undo", &mut app);
}

#[test]
fn undo_restores_paragraph_break() {
    let mut app = sample_app();
    for _ in 0..7 {
        app.key(KeyCode::Right);
    }
    app.key(KeyCode::Enter);
    assert_svg("paragraph_break", &mut app);
    app.ctrl('z');
    assert_svg("paragraph_break_undone", &mut app);
}

#[test]
fn selection_highlights_text() {
    let mut app = sample_app();
    for _ in 0..7 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }
    assert_svg("selection_highlight", &mut app);
}

#[test]
fn context_menu_opens() {
    let mut app = sample_app();
    app.key(KeyCode::Esc);
    assert_svg("context_menu", &mut app);
}

#[test]
fn click_positions_cursor() {
    let mut app = sample_app();
    app.click(12, 6);
    assert_svg("click_positions_cursor", &mut app);
}

#[test]
fn f10_activates_menu_bar() {
    let mut app = sample_app();
    app.key(KeyCode::F(10));
    assert_svg("menu_bar_activated", &mut app);
}

#[test]
fn alt_f_opens_file_menu() {
    let mut app = sample_app();
    app.key_with(KeyCode::Char('f'), KeyModifiers::ALT);
    assert_svg("menu_bar_file_menu", &mut app);
}

#[test]
fn cursor_keys_walk_through_menus() {
    let mut app = sample_app();
    app.key_with(KeyCode::Char('f'), KeyModifiers::ALT);
    app.key(KeyCode::Right);
    app.key(KeyCode::Down);
    assert_svg("menu_bar_edit_menu_second_item", &mut app);
}

#[test]
fn esc_closes_menu_bar() {
    let mut app = sample_app();
    app.key(KeyCode::F(10));
    app.key(KeyCode::Esc);
    assert_svg("menu_bar_closed", &mut app);
}

#[test]
fn menu_undo_reverts_typing() {
    let mut app = sample_app();
    app.type_text("Summer ");
    app.key_with(KeyCode::Char('e'), KeyModifiers::ALT);
    app.key(KeyCode::Enter);
    assert_svg("menu_undo_reverts_typing", &mut app);
}

#[test]
fn menu_view_toggles_reveal_codes() {
    let mut app = sample_app();
    app.key_with(KeyCode::Char('v'), KeyModifiers::ALT);
    app.key(KeyCode::Enter);
    assert_svg("menu_reveal_codes_enabled", &mut app);
    app.key_with(KeyCode::Char('v'), KeyModifiers::ALT);
    assert_svg("menu_reveal_codes_checkmark", &mut app);
}

#[test]
fn inline_style_keeps_cursor_and_backspace_after_reveal_tag_removes_style() {
    let document = ftml! {
        p { "Intro paragraph." }
        p { "Pure is a modern, terminal-based word processor." }
    };
    let mut app = TestApp::new(WIDTH, HEIGHT, document);

    // Select "terminal-based word processor" in the second paragraph.
    app.key(KeyCode::Down);
    for _ in 0..18 {
        app.key(KeyCode::Right);
    }
    for _ in 0..29 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }

    // Apply italic via the context menu; the cursor must stay at the end of
    // the styled text instead of jumping to the start of the selection.
    app.key(KeyCode::Esc);
    app.key(KeyCode::Char('i'));
    assert_svg("italic_keeps_cursor_at_selection_end", &mut app);

    // Enable reveal codes, move the cursor right behind the `[Italic>` tag
    // and press backspace: the style is removed, the paragraphs stay intact,
    // and the cursor stays at the position of the removed tag.
    app.key_with(KeyCode::Char('v'), KeyModifiers::ALT);
    app.key(KeyCode::Enter);
    for _ in 0..29 {
        app.key(KeyCode::Left);
    }
    app.key(KeyCode::Backspace);
    assert_svg("backspace_after_reveal_tag_removes_style", &mut app);
}

#[test]
fn stacked_inline_styles_render_combined_and_unstack_via_reveal_tag() {
    let document = ftml! {
        p { "Stacking styles gets messy." }
    };
    let mut app = TestApp::new(WIDTH, HEIGHT, document);

    // Embolden "styles gets messy" …
    for _ in 0..9 {
        app.key(KeyCode::Right);
    }
    for _ in 0..17 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }
    app.key(KeyCode::Esc);
    app.key(KeyCode::Char('b'));

    // … then highlight "gets messy" on top: both styles must render.
    for _ in 0..2 {
        app.key_with(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    }
    app.key(KeyCode::Esc);
    app.key_with(KeyCode::Char('H'), KeyModifiers::SHIFT);
    assert_svg("stacked_styles_render_combined", &mut app);

    // Reveal codes shows the highlight nested inside the bold span.
    app.key_with(KeyCode::Char('v'), KeyModifiers::ALT);
    app.key(KeyCode::Enter);
    assert_svg("stacked_styles_nest_in_reveal_codes", &mut app);

    // Deleting the `<Bold]` end tag unstacks: the whole bold range loses its
    // bold while the nested highlight survives.
    app.key(KeyCode::End);
    app.key(KeyCode::Left);
    app.key(KeyCode::Backspace);
    assert_svg("stacked_styles_unstack_via_reveal_tag", &mut app);
}

#[test]
fn editing_keeps_reveal_codes_visible() {
    let document = ftml! {
        p { "Pure is a modern, " i { "terminal-based word processor" } "." }
    };
    let mut app = TestApp::new(WIDTH, HEIGHT, document);

    // Enable reveal codes via the View menu.
    app.key_with(KeyCode::Char('v'), KeyModifiers::ALT);
    app.key(KeyCode::Enter);
    assert!(
        app.svg().contains("[Italic&gt;"),
        "reveal tags should be visible after enabling reveal codes"
    );

    // Delete the space after "Pure" with backspace; the reveal tags later in
    // the line must stay visible.
    for _ in 0..5 {
        app.key(KeyCode::Right);
    }
    app.key(KeyCode::Backspace);
    assert!(
        app.svg().contains("[Italic&gt;"),
        "reveal tags must stay visible after deleting a character"
    );

    // Same when typing the space back in.
    app.key(KeyCode::Char(' '));
    assert!(
        app.svg().contains("[Italic&gt;"),
        "reveal tags must stay visible after inserting a character"
    );
    assert_svg("editing_keeps_reveal_codes_visible", &mut app);
}

#[test]
fn menu_format_opens_formatting_menu() {
    let mut app = sample_app();
    app.key(KeyCode::F(10));
    app.key_with(KeyCode::Char('o'), KeyModifiers::NONE);
    app.key(KeyCode::Enter);
    assert_svg("menu_format_opens_formatting_menu", &mut app);
}

#[test]
fn cut_removes_selection_and_paste_reinserts_it() {
    let mut app = sample_app();
    // Select "Packing" in the heading and cut it.
    for _ in 0..7 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }
    app.ctrl('x');
    assert_svg("cut_removes_selection", &mut app);

    // Paste it back at the end of the heading line.
    app.key(KeyCode::End);
    app.ctrl('v');
    assert_svg("paste_reinserts_cut_text", &mut app);
}

#[test]
fn ctrl_c_copies_selection_and_is_ignored_without_one() {
    let mut app = sample_app();
    for _ in 0..7 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }
    app.ctrl('c');
    assert!(
        !app.app.should_quit(),
        "Ctrl+C with a selection must copy, not quit"
    );
    assert_svg("ctrl_c_copies_selection", &mut app);

    // Collapse the selection; now Ctrl+C is simply ignored.
    app.key(KeyCode::Right);
    app.ctrl('c');
    assert!(
        !app.app.should_quit(),
        "Ctrl+C without a selection must do nothing"
    );
}

#[test]
fn bracketed_paste_inserts_text_and_paragraphs() {
    let mut app = sample_app();
    // Paste at the end of the "Pack the essentials..." paragraph.
    app.key(KeyCode::Down);
    app.key(KeyCode::End);
    app.event(Event::Paste(
        " Plus a pasted line break\nand a\n\npasted paragraph.".to_string(),
    ));
    assert_svg("bracketed_paste_inserts_text_and_paragraphs", &mut app);
}

#[test]
fn paste_replaces_selection_and_undoes_in_one_step() {
    let mut app = sample_app();
    // Select "Packing" and paste over it.
    for _ in 0..7 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }
    app.event(Event::Paste("Shopping".to_string()));
    assert_svg("paste_replaces_selection", &mut app);

    // The paste itself is one undo step; a second undo restores the
    // cut-away selection.
    app.ctrl('z');
    app.ctrl('z');
    assert_svg("paste_undo_restores_selection_text", &mut app);
}

#[test]
fn paste_with_empty_clipboard_shows_hint() {
    let mut app = sample_app();
    app.ctrl('v');
    assert_svg("paste_with_empty_clipboard_shows_hint", &mut app);
}

#[test]
fn paste_preserves_inline_styles() {
    let mut app = sample_app();
    // Select the whole "Pack the essentials before the long trip." line,
    // including its bold and italic spans.
    app.key(KeyCode::Down);
    app.key(KeyCode::Home);
    app.key_with(KeyCode::End, KeyModifiers::SHIFT);
    app.ctrl('c');

    // Paste at the end of the quote.
    for _ in 0..4 {
        app.key(KeyCode::Down);
    }
    app.key(KeyCode::End);
    app.ctrl('v');

    let svg = app.svg();
    assert_eq!(
        svg.matches(r#"font-weight="bold">essentials"#).count(),
        2,
        "the pasted copy of \"essentials\" must still be bold"
    );
    assert_eq!(
        svg.matches(r#"font-style="italic">long"#).count(),
        2,
        "the pasted copy of \"long\" must still be italic"
    );
    assert_svg("paste_preserves_inline_styles", &mut app);
}

#[test]
fn ctrl_o_opens_file_dialog_and_lists_directory() {
    let mut app = sample_app();
    app.ctrl('o');
    app.type_text("tests/fixtures/");
    assert_svg("file_dialog_lists_directory", &mut app);
}

#[test]
fn file_dialog_tab_completes_path() {
    let mut app = sample_app();
    app.ctrl('o');
    app.type_text("tests/fixtures/al");
    app.key(KeyCode::Tab);
    assert_svg("file_dialog_tab_completed", &mut app);
}

#[test]
fn open_dialog_loads_selected_file() {
    let mut app = sample_app();
    app.ctrl('o');
    app.type_text("tests/fixtures/");
    // Highlight alpha.ftml (the entry after the sub/ directory).
    app.key(KeyCode::Down);
    app.key(KeyCode::Down);
    app.key(KeyCode::Enter);
    assert_svg("file_dialog_opened_file", &mut app);
}

#[test]
fn open_dialog_asks_before_discarding_unsaved_changes() {
    let mut app = sample_app();
    app.type_text("Summer ");
    app.ctrl('o');
    app.type_text("tests/fixtures/beta.md");
    app.key(KeyCode::Enter);
    assert_svg("file_dialog_unsaved_changes_warning", &mut app);
    app.key(KeyCode::Enter);
    assert_svg("file_dialog_discarded_and_opened", &mut app);
}

#[test]
fn esc_cancels_file_dialog() {
    let mut app = sample_app();
    app.ctrl('o');
    app.key(KeyCode::Esc);
    app.type_text("Summer ");
    assert_svg("file_dialog_cancelled", &mut app);
}

/// Open the Save As dialog through the File menu.
fn open_save_as_dialog(app: &mut TestApp) {
    app.key_with(KeyCode::Char('f'), KeyModifiers::ALT);
    app.key(KeyCode::Down); // Open...
    app.key(KeyCode::Down); // Save
    app.key(KeyCode::Down); // Save As...
    app.key(KeyCode::Enter);
}

#[test]
fn untitled_document_shows_untitled_in_status_bar() {
    let mut app = TestApp::untitled(WIDTH, HEIGHT, sample_document());
    assert_svg("untitled_status_bar", &mut app);
}

#[test]
fn ctrl_n_replaces_document_with_untitled_one() {
    let mut app = sample_app();
    app.ctrl('n');
    assert_svg("new_document", &mut app);
}

#[test]
fn ctrl_n_asks_before_discarding_unsaved_changes() {
    let mut app = sample_app();
    app.type_text("Summer ");
    app.ctrl('n');
    assert!(
        app.svg().contains("Summer"),
        "the first Ctrl+N must only warn, not discard the document"
    );
    assert_svg("new_document_unsaved_warning", &mut app);
    app.ctrl('n');
    assert_svg("new_document_after_confirm", &mut app);
}

#[test]
fn typing_after_new_document_warning_requires_another_warning() {
    let mut app = sample_app();
    app.type_text("Summer ");
    app.ctrl('n');
    // Editing after the warning invalidates it: the next Ctrl+N warns again.
    app.type_text("and Winter ");
    app.ctrl('n');
    assert!(
        app.svg().contains("Winter"),
        "Ctrl+N after further edits must warn again instead of discarding"
    );
}

#[test]
fn saving_untitled_document_opens_save_as_dialog() {
    let dir = std::env::temp_dir().join(format!("pure-untitled-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let target = dir.join("named.ftml");
    let _ = std::fs::remove_file(&target);

    let mut app = TestApp::untitled(WIDTH, HEIGHT, sample_document());
    app.ctrl('s');
    assert!(
        app.svg().contains("Save As"),
        "Ctrl+S on an untitled document must open the Save As dialog"
    );

    app.type_text(target.to_str().expect("utf-8 temp path"));
    app.key(KeyCode::Enter);
    let contents = std::fs::read_to_string(&target).expect("file saved under the typed name");
    assert!(
        contents.contains("Packing List"),
        "saved file must contain the document, got: {contents}"
    );

    // Now that the document has a name, Ctrl+S saves directly.
    app.type_text("More ");
    app.ctrl('s');
    assert!(
        !app.svg().contains("Save As"),
        "Ctrl+S on a named document must save without a dialog"
    );
    let contents = std::fs::read_to_string(&target).expect("file saved again");
    assert!(
        contents.contains("More"),
        "the second save must write the edited document, got: {contents}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_as_via_menu_writes_new_file() {
    let dir = std::env::temp_dir().join(format!("pure-save-as-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let target = dir.join("saved.ftml");
    let _ = std::fs::remove_file(&target);

    let mut app = sample_app();
    open_save_as_dialog(&mut app);
    // Replace the suggested "test.ftml" with the target path.
    app.ctrl('w');
    app.type_text(target.to_str().expect("utf-8 temp path"));
    app.key(KeyCode::Enter);

    let contents = std::fs::read_to_string(&target).expect("file saved under the new name");
    assert!(
        contents.contains("Packing List"),
        "saved file must contain the document, got: {contents}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_as_requires_confirmation_before_overwriting() {
    let dir = std::env::temp_dir().join(format!("pure-overwrite-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let target = dir.join("existing.md");
    std::fs::write(&target, "old contents").expect("seed existing file");

    let mut app = sample_app();
    open_save_as_dialog(&mut app);
    app.ctrl('w');
    app.type_text(target.to_str().expect("utf-8 temp path"));
    app.key(KeyCode::Enter);
    assert_eq!(
        std::fs::read_to_string(&target).expect("file readable"),
        "old contents",
        "the first Enter must only warn, not overwrite"
    );

    app.key(KeyCode::Enter);
    let contents = std::fs::read_to_string(&target).expect("file overwritten");
    assert!(
        contents.starts_with("# Packing List"),
        "saving as .md must write Markdown, got: {contents}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn paste_rebuilds_cut_list_items() {
    let mut app = sample_app();
    // Select both list items and cut them.
    app.key(KeyCode::Down);
    app.key(KeyCode::Down);
    app.key(KeyCode::Home);
    app.key_with(KeyCode::Down, KeyModifiers::SHIFT);
    app.key_with(KeyCode::End, KeyModifiers::SHIFT);
    app.ctrl('x');
    assert_svg("cut_removes_list_items", &mut app);

    // Pasting them right back restores the bullet list.
    app.ctrl('v');
    assert_svg("paste_rebuilds_cut_list_items", &mut app);
}

/// Unindenting a continuation paragraph out of the middle of a list splits
/// the list, which changes the number of root paragraphs. The cached layout
/// must be fully rebuilt: previously the incremental update left the screen
/// truncated and the cursor misplaced until the cursor was moved away and
/// back.
#[test]
fn unindent_split_renders_fully_with_cursor_in_place() {
    let document = ftml! {
        ul {
            li { p { "Alpha" } }
            li { p { "" } }
            li { p { "Beta" } }
        }
        p { "Tail" }
    };
    let mut app = TestApp::new(40, 14, document);
    app.key(KeyCode::Down); // onto the empty item
    app.ctrl('p'); // continuation paragraph within the item
    app.ctrl('['); // unindent: splits the list at the new paragraph

    // The whole document must be on screen right away.
    let lines = app.buffer_lines();
    let row_of = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not on screen: {lines:#?}"))
    };
    let empty_bullet_row = lines
        .iter()
        .position(|line| line.trim_end() == "•")
        .expect("empty list item visible");
    let beta_row = row_of("• Beta");
    row_of("• Alpha");
    row_of("Tail");

    // The cursor sits on the new empty paragraph between the list halves...
    let cursor = app.cursor_position().expect("cursor shown");
    assert_eq!(cursor.x, 0, "cursor at start of the empty paragraph");
    assert!(
        (usize::from(cursor.y)) > empty_bullet_row && (usize::from(cursor.y)) < beta_row,
        "cursor row {} not between empty item (row {empty_bullet_row}) and Beta (row {beta_row})",
        cursor.y
    );

    // ... and moving away and back lands on exactly the same spot.
    app.key(KeyCode::Down);
    app.key(KeyCode::Up);
    assert_eq!(app.cursor_position(), Some(cursor));
}

fn link_document() -> Document {
    ftml! {
        p { "Read the " link { "https://example.test" "manual" } " carefully." }
    }
}

#[test]
fn ctrl_k_opens_edit_dialog_on_existing_link() {
    let mut app = TestApp::new(WIDTH, HEIGHT, link_document());
    // Move the cursor into the link text ("Read the " is nine characters).
    for _ in 0..11 {
        app.key(KeyCode::Right);
    }
    app.ctrl('k');
    assert_svg("link_dialog_edit_existing", &mut app);
}

#[test]
fn link_dialog_creates_link_from_selection() {
    let mut app = sample_app();
    // Select "Packing" in the heading.
    for _ in 0..7 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }
    app.ctrl('k');
    assert_svg("link_dialog_new_from_selection", &mut app);

    // Fill in the URL and apply; "Packing" becomes a rendered link.
    app.key(KeyCode::Tab);
    app.type_text("https://example.test");
    app.key(KeyCode::Enter);
    assert_svg("link_created_from_selection", &mut app);
}

#[test]
fn link_dialog_focuses_open_button() {
    let mut app = TestApp::new(WIDTH, HEIGHT, link_document());
    for _ in 0..11 {
        app.key(KeyCode::Right);
    }
    app.ctrl('k');
    // Tab past the Text and URL fields onto the Open button.
    app.key(KeyCode::Tab);
    app.key(KeyCode::Tab);
    assert_svg("link_dialog_open_focused", &mut app);
}

#[test]
fn clearing_link_target_removes_the_link() {
    let mut app = TestApp::new(WIDTH, HEIGHT, link_document());
    for _ in 0..11 {
        app.key(KeyCode::Right);
    }
    app.ctrl('k');
    // Focus the URL field, jump to its end, and erase it.
    app.key(KeyCode::Tab);
    app.ctrl('e');
    for _ in 0..30 {
        app.key(KeyCode::Backspace);
    }
    app.key(KeyCode::Enter);
    assert_svg("link_unlinked", &mut app);
}

#[test]
fn link_dialog_focuses_save_button() {
    let mut app = TestApp::new(WIDTH, HEIGHT, link_document());
    for _ in 0..11 {
        app.key(KeyCode::Right);
    }
    app.ctrl('k');
    // Tab to the Save button: Text -> URL -> Open -> Cancel -> Save.
    for _ in 0..4 {
        app.key(KeyCode::Tab);
    }
    assert_svg("link_dialog_save_focused", &mut app);
}

#[test]
fn space_activates_the_cancel_button() {
    let mut app = TestApp::new(WIDTH, HEIGHT, link_document());
    for _ in 0..11 {
        app.key(KeyCode::Right);
    }
    app.ctrl('k');
    assert!(
        app.buffer_lines()
            .iter()
            .any(|line| line.contains("Edit Link"))
    );
    // Tab to Cancel: Text -> URL -> Open -> Cancel, then activate with Space.
    for _ in 0..3 {
        app.key(KeyCode::Tab);
    }
    app.key(KeyCode::Char(' '));
    assert!(
        !app.buffer_lines()
            .iter()
            .any(|line| line.contains("Edit Link")),
        "Space on Cancel should dismiss the dialog"
    );
}

#[test]
fn space_on_open_button_keeps_dialog_and_reports_opening() {
    let mut app = TestApp::new(WIDTH, HEIGHT, link_document());
    for _ in 0..11 {
        app.key(KeyCode::Right);
    }
    app.ctrl('k');
    // Tab to Open: Text -> URL -> Open, then activate with Space.
    app.key(KeyCode::Tab);
    app.key(KeyCode::Tab);
    app.key(KeyCode::Char(' '));
    let lines = app.buffer_lines();
    assert!(
        lines.iter().any(|line| line.contains("Edit Link")),
        "Open keeps the dialog open"
    );
    assert!(
        lines.iter().any(|line| line.contains("Opening")),
        "Open reports progress in the status line"
    );
}
