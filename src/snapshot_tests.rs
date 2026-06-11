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
