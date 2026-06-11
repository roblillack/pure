//! SVG snapshot tests for full-app interactions.
//!
//! Each test drives the real `App` through synthetic key/mouse events and
//! snapshots the rendered terminal as SVG (see [`crate::test_harness`]).
//! Review changed snapshots with `cargo insta review`; the `.snap.svg` files
//! under `src/snapshots/` open directly in a browser.

use crossterm::event::{KeyCode, KeyModifiers};
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
