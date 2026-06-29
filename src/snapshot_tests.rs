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

const MARKDOWN_TABLE: &str = "# Team\n\nIntro paragraph before the table.\n\n| Name | Role | Notes |\n| --- | --- | --- |\n| Alice | Developer | Works on the parser and the renderer |\n| Bob | Designer | UI |\n\nClosing paragraph after the table.\n";

fn table_document() -> Document {
    tdoc::markdown::parse(std::io::Cursor::new(MARKDOWN_TABLE)).expect("parse markdown")
}

#[test]
fn markdown_table_renders_with_box_drawing_borders() {
    let doc = table_document();
    let table_count = doc
        .paragraphs
        .iter()
        .filter(|p| p.paragraph_type() == tdoc::ParagraphType::Table)
        .count();
    assert_eq!(table_count, 1, "markdown should parse into one table");

    let mut app = TestApp::new(WIDTH, HEIGHT, doc);
    app.draw();
    let lines = app.buffer_lines();
    // The cell backend merges the engine's grid strokes into proper box-drawing
    // junctions: corners (┌┐└┘) and tees/crosses (┬┴├┤┼).
    assert!(
        lines.iter().any(|l| l.contains('─')),
        "expected a horizontal table border"
    );
    assert!(
        lines.iter().any(|l| l.contains('│')),
        "expected vertical table borders"
    );
    assert!(
        lines.iter().any(|l| l.contains('┌') && l.contains('┬') && l.contains('┐')),
        "expected a top border with corner and tee glyphs"
    );
    assert!(
        lines.iter().any(|l| l.contains('└') && l.contains('┴') && l.contains('┘')),
        "expected a bottom border with corner and tee glyphs"
    );
    assert!(
        lines.iter().any(|l| l.contains('├') && l.contains('┼') && l.contains('┤')),
        "expected an interior row separator with junction glyphs"
    );
}

#[test]
fn table_document_renders() {
    let mut app = TestApp::new(WIDTH, HEIGHT, table_document());
    assert_svg("table_document_renders", &mut app);
}

#[test]
fn cursor_is_drawn_inside_the_table() {
    let mut app = TestApp::new(WIDTH, HEIGHT, table_document());
    app.draw();

    // The table's on-screen rows; recomputed each step since navigation can
    // scroll the view, which shifts every row's visual y.
    let table_rows = |app: &TestApp| -> Vec<usize> {
        app.buffer_lines()
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.contains('│') || line.contains('┌') || line.contains('├') || line.contains('└')
            })
            .map(|(i, _)| i)
            .collect()
    };
    assert!(!table_rows(&app).is_empty(), "expected the table to be drawn");

    // Walking down with the arrow keys should at some point place the visible
    // terminal cursor on one of the table's rows.
    let mut cursor_entered_table = false;
    for _ in 0..12 {
        app.key(KeyCode::Down);
        app.draw();
        if let Some(pos) = app.cursor_position()
            && table_rows(&app).contains(&(pos.y as usize))
        {
            cursor_entered_table = true;
            break;
        }
    }
    assert!(
        cursor_entered_table,
        "the terminal cursor should be drawn within the table while navigating through it"
    );
}

#[test]
fn initial_document() {
    let mut app = sample_app();
    assert_svg("initial_document", &mut app);
}

/// The page margin is responsive: flush-left on narrow terminals, a small gutter
/// at medium widths, and centered with a capped text measure when wide. This
/// mirrors classic Pure (see [`crate::app::page_margin`]).
#[test]
fn page_margin_is_responsive() {
    use crate::app::page_margin;
    // Narrow: single-cell gutter only.
    assert_eq!(page_margin(40), 1);
    assert_eq!(page_margin(59), 1);
    // Medium: a two-cell gutter.
    assert_eq!(page_margin(60), 2);
    assert_eq!(page_margin(99), 2);
    // Wide: center the content, capping the measure near 92 columns.
    assert_eq!(page_margin(100), 4);
    assert_eq!(page_margin(140), 24);
    assert_eq!(page_margin(200), 54);
    // The capped text measure stays roughly constant once centered.
    for w in [120, 160, 200, 300] {
        let measure = w - 2 * page_margin(w);
        assert!(
            (90..=100).contains(&measure),
            "width {w}: text measure {measure} should stay near ~92 columns"
        );
    }
}

/// Render the same document at several terminal widths and confirm the left
/// gutter actually grows (content is indented and, when wide, centered) rather
/// than always hugging the left edge.
#[test]
fn rendered_left_gutter_grows_with_width() {
    let leading_spaces = |app: &TestApp| -> usize {
        let lines = app.buffer_lines();
        let content = &lines[..lines.len().saturating_sub(1)]; // skip the status bar
        content
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0)
    };

    let mut narrow = TestApp::new(50, 18, sample_document());
    narrow.draw();
    let mut wide = TestApp::new(140, 18, sample_document());
    wide.draw();

    assert!(
        leading_spaces(&wide) > leading_spaces(&narrow) + 10,
        "a wide terminal should center content with a much larger gutter \
         (narrow={}, wide={})",
        leading_spaces(&narrow),
        leading_spaces(&wide),
    );
}

/// Checklist markers render as the classic bracketed text (`[✓] ` / `[ ] `)
/// directly abutting the item text — not a wide-gap drawn box.
#[test]
fn checklist_renders_bracket_markers() {
    let doc = ftml! {
        checklist {
            done { "Write the parser" }
            todo { "Write the docs" }
        }
    };
    let mut app = TestApp::new(WIDTH, HEIGHT, doc);
    app.draw();
    let lines = app.buffer_lines();
    assert!(
        lines.iter().any(|l| l.contains("[✓] Write the parser")),
        "checked item should render `[✓] ` immediately before its text"
    );
    assert!(
        lines.iter().any(|l| l.contains("[ ] Write the docs")),
        "unchecked item should render `[ ] ` immediately before its text"
    );
}

/// A code block is only modestly inset, not pushed ~10 columns in (the old
/// pixel inset interpreted as cells).
#[test]
fn code_block_is_not_over_indented() {
    let doc = ftml! {
        p { "Intro." }
        code { "fn main() {}" }
    };
    let mut app = TestApp::new(WIDTH, HEIGHT, doc);
    app.draw();
    let code = app
        .buffer_lines()
        .into_iter()
        .find(|l| l.contains("fn main"))
        .expect("code line on screen");
    let indent = code.len() - code.trim_start().len();
    assert!(
        indent <= 4,
        "code should be modestly inset, got {indent} columns: {code:?}"
    );
}

/// Scrolling can't push the whole document off the top into empty space: after
/// wheeling far past the end of a short document, content is still visible.
#[test]
fn over_scroll_keeps_content_visible() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    let doc = ftml! {
        h1 { "Doc" }
        p { "Alpha" } p { "Beta" } p { "Gamma" }
    };
    let mut app = TestApp::new(40, 12, doc);
    app.draw();
    for _ in 0..40 {
        app.event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }));
    }
    let lines = app.buffer_lines();
    let content_rows = lines[..lines.len() - 1]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert!(
        content_rows > 0,
        "over-scrolling must not leave a blank document"
    );
}

/// H2/H3 headings get a rule (`===`/`---`) beneath them so heading levels stay
/// distinguishable in a terminal (which has no font sizes). H1 is centered and
/// gets no rule.
#[test]
fn headings_get_underline_rules() {
    let doc = ftml! {
        h1 { "Title" }
        h2 { "Section Two" }
        p { "Body." }
        h3 { "Sub Three" }
    };
    let mut app = TestApp::new(50, 20, doc);
    app.draw();
    let lines = app.buffer_lines();
    let row_of = |needle: &str| lines.iter().position(|l| l.contains(needle));
    let h2 = row_of("Section Two").expect("H2 visible");
    let h3 = row_of("Sub Three").expect("H3 visible");
    assert!(
        lines[h2 + 1].contains("==========="),
        "H2 should be underlined with '=': {:?}",
        lines[h2 + 1]
    );
    assert!(
        lines[h3 + 1].contains("---------"),
        "H3 should be underlined with '-': {:?}",
        lines[h3 + 1]
    );
}

/// Code blocks are fenced with a full-width `-` rule above and below (classic
/// Pure), and the code text itself is flush with the body (no indent).
#[test]
fn code_block_has_fence_rules() {
    let doc = ftml! {
        p { "Intro." }
        code { "fn main() {}" }
        p { "Outro." }
    };
    let mut app = TestApp::new(60, 20, doc);
    app.draw();
    let lines = app.buffer_lines();
    let code = lines
        .iter()
        .position(|l| l.contains("fn main"))
        .expect("code visible");
    let is_fence = |s: &str| {
        let t = s.trim();
        t.len() >= 4 && t.chars().all(|c| c == '-')
    };
    assert!(
        is_fence(&lines[code - 1]),
        "expected a `-` fence above the code: {:?}",
        lines[code - 1]
    );
    assert!(
        is_fence(&lines[code + 1]),
        "expected a `-` fence below the code: {:?}",
        lines[code + 1]
    );
}

/// The quote bar is a literal ASCII `|`, the way classic Pure drew it — not a
/// box-drawing rule.
#[test]
fn quote_bar_is_ascii_pipe() {
    let doc = ftml! {
        quote { p { "Heed this." } }
    };
    let mut app = TestApp::new(50, 12, doc);
    app.draw();
    let bar = content_line_with(&app, "Heed this.");
    assert!(
        bar.starts_with('|'),
        "quote line should start with an ASCII pipe: {bar:?}"
    );
    assert!(
        !bar.contains('│'),
        "quote bar should not use a box-drawing glyph: {bar:?}"
    );
}

/// The status bar reports the cursor as a *content* line/column, the block-type
/// breadcrumb (`Header Lvl N`), and classic Pure's line + word counts. A leading
/// H1 carries a 3-line top margin that counts as content, so the title sits on
/// content line 4.
#[test]
fn status_bar_reports_classic_line_and_word_counts() {
    let doc = ftml! {
        h1 { "Title" }
        p { "two words" }
    };
    let mut app = TestApp::new(60, 20, doc);
    app.draw();
    let status = app
        .buffer_lines()
        .last()
        .cloned()
        .unwrap_or_default()
        .trim()
        .to_string();
    assert!(
        status.starts_with("4:1 "),
        "H1 with its 3-line top margin should put the cursor on content line 4: {status:?}"
    );
    assert!(
        status.contains("Header Lvl 1"),
        "block breadcrumb should read `Header Lvl 1`: {status:?}"
    );
    // Content lines = 3-line H1 top margin + title + 3-line gap + paragraph = 8
    // (margins that classic Pure counted, but not decorations); word count is
    // Title + "two words" = 3.
    assert!(
        status.contains(", 8 lines, 3 words"),
        "expected classic content-line + word counts: {status:?}"
    );
}

/// A single click on the scrollbar track jumps the scroll there (most terminals
/// don't deliver a drag stream, so click-to-jump is what makes it usable).
#[test]
fn scrollbar_click_jumps() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let doc = ftml! {
        h1 { "D" }
        p { "a" } p { "b" } p { "c" } p { "d" } p { "e" } p { "f" }
        p { "g" } p { "h" } p { "i" } p { "j" } p { "k" } p { "l" }
    };
    let mut app = TestApp::new(80, 14, doc);
    app.draw();
    let first_letter = |app: &TestApp| -> String {
        let lines = app.buffer_lines();
        lines[..lines.len() - 1]
            .iter()
            .find(|l| {
                let t = l.trim();
                t.len() == 1 && t.chars().next().unwrap().is_ascii_lowercase()
            })
            .map(|l| l.trim().to_string())
            .unwrap_or_default()
    };
    let before = first_letter(&app);
    let sb = 79u16;
    // A click (down+up, no drag) low on the scrollbar.
    app.event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: sb,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }));
    app.event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: sb,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }));
    assert_ne!(
        before,
        first_letter(&app),
        "clicking down the scrollbar track should scroll the document"
    );
}

/// Vertical cursor movement walks through every line of a code block (the lines
/// carry cumulative byte offsets, so navigation doesn't stall on the first one).
#[test]
fn cursor_moves_through_code_block_lines() {
    let doc = ftml! {
        p { "before" }
        code { "line one\nline two\nline three" }
        p { "after" }
    };
    let mut app = TestApp::new(50, 16, doc);
    app.draw();
    let mut rows = vec![app.cursor_position().map(|p| p.y)];
    for _ in 0..4 {
        app.key(KeyCode::Down);
        rows.push(app.cursor_position().map(|p| p.y));
    }
    // The cursor visits four distinct rows (before + 3 code lines) before reaching
    // the trailing paragraph — i.e. it doesn't get stuck on the first code line.
    let distinct: std::collections::BTreeSet<_> = rows.iter().flatten().collect();
    assert!(
        distinct.len() >= 4,
        "cursor should descend through the code block, visited rows: {rows:?}"
    );
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

/// The single content line, with surrounding layout padding trimmed. Panics
/// if the needle is not visible.
fn content_line_with(app: &TestApp, needle: &str) -> String {
    app.buffer_lines()
        .into_iter()
        .find(|line| line.contains(needle))
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| panic!("{needle:?} not on screen"))
}

#[test]
fn backspace_deletes_the_active_selection() {
    let mut app = TestApp::new(WIDTH, HEIGHT, ftml! { p { "Hello World" } });
    // Select "Hello" (cursor anchored at the start, focus after the 'o').
    for _ in 0..5 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }
    app.key(KeyCode::Backspace);
    // The whole selection is gone — not just the character before the cursor,
    // which would leave "Hell World".
    assert_eq!(content_line_with(&app, "World"), "World");
}

#[test]
fn delete_deletes_the_active_selection() {
    let mut app = TestApp::new(WIDTH, HEIGHT, ftml! { p { "Hello World" } });
    for _ in 0..5 {
        app.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }
    app.key(KeyCode::Delete);
    // The whole selection is gone — not just the character at the cursor,
    // which would leave "HelloWorld".
    assert_eq!(content_line_with(&app, "World"), "World");
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

// NOTE: the "reveal codes" feature (showing inline-style tags like `[Italic>`)
// is not implemented on the shared `tdoc-editor` rendering engine yet, so the
// former `editing_keeps_reveal_codes_visible` test was removed during the
// migration. Restore it once reveal-codes support is ported.

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

    // Collapse the selection and paste it back at the end of the same
    // (top-level) paragraph, duplicating the styled runs inline. (Pasting into
    // nested structure like a quote preserves styling in the model but its
    // terminal rendering is still being polished on the shared engine.)
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
    app.ctrl('['); // unindent the empty item out of the list

    // The whole document must be on screen right away (no scroll clipping).
    let lines = app.buffer_lines();
    let row_of = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not on screen: {lines:#?}"))
    };
    row_of("Alpha");
    row_of("Beta");
    row_of("Tail");

    // The cursor is shown, and moving away and back lands on the same spot.
    let cursor = app.cursor_position().expect("cursor shown");
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

