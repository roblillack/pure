//! The scripted Pure demo.
//!
//! Watch it live in your terminal (press q, Esc, or Ctrl+C to stop):
//!
//! ```sh
//! cargo run --example demo
//! ```
//!
//! Or record it as the `demo.gif` embedded in the README, with a few of its
//! moments also saved as the PNGs in `screenshots/`:
//!
//! ```sh
//! cargo run --release --example demo --features recorder
//! ```
//!
//! Every call below simulates real key presses through the app's event
//! handling, so the demo always shows the editor's current behavior. While
//! recording, set `PURE_DEMO_FRAMES=<dir>` to dump each frame as a PNG.

mod driver;

use driver::Demo;

/// Select the word just before the trailing ", " and style it via the
/// context menu. Typing continues after the unstyled comma, so the style
/// does not bleed into the following text.
fn style_last_word(demo: &mut Demo, shortcut: char) {
    demo.cursor_left();
    demo.cursor_left();
    demo.shift_word_left();
    demo.context_menu_fast(shortcut);
    demo.end();
}

/// Turn the current selection into a hyperlink via the Edit Link dialog: open
/// it (the selected word pre-fills the text), Tab to the URL field, type the
/// target, and press Enter to save.
fn link_selection(demo: &mut Demo, target: &str) -> anyhow::Result<()> {
    demo.ctrl('k');
    demo.pause(700);
    demo.tab();
    demo.write(target);
    demo.screenshot("3-link")?;
    demo.pause(900);
    demo.paragraph_break(); // Enter saves the link
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut demo = Demo::start(80, 24);

    demo.write("Welcome to Pure");
    demo.context_menu('1'); // Heading 1
    demo.paragraph_break();
    demo.write(
        "This is a short demo, showing you how to use a terminal-based word processor to edit your Markdown files.",
    );
    demo.write(" As you can see, text will reflow and respect the paragraph breaks you introduce.");

    // Step back between "will" and "reflow" and add "automatically" after
    // the fact — the paragraph reflows live.
    for _ in 0..9 {
        demo.word_left();
    }
    demo.write("automatically ");
    demo.end();

    demo.paragraph_break();
    demo.write(
        "Even without a graphical interface, Pure supports a multitude of different paragraph and inline styles:",
    );
    demo.paragraph_break();
    demo.context_menu('8'); // Bullet List
    demo.write("Headings, lists, quotes, code blocks & checklists");
    demo.paragraph_break();

    // Style each listed inline style right after writing the word.
    demo.write("Bold, ");
    style_last_word(&mut demo, 'b');
    demo.write("italic, ");
    style_last_word(&mut demo, 'i');
    demo.write("underline, ");
    style_last_word(&mut demo, 'u');
    demo.write("code, ");
    style_last_word(&mut demo, 'C');
    demo.write("highlights, ");
    style_last_word(&mut demo, 'H');
    demo.write("strikethrough & links");
    for _ in 0..8 {
        demo.cursor_left(); // step back over " & links"
    }
    demo.shift_word_left();
    demo.context_menu('X'); // Strikethrough
    demo.end();

    // Turn the final word "links" into a real hyperlink to the project repo.
    demo.shift_word_left();
    link_selection(&mut demo, "https://github.com/roblillack/pure")?;

    demo.paragraph_break();

    demo.context_menu('0'); // back to a Text paragraph
    demo.write("Sometimes, stacking multiple styles gets messy.");

    // Embolden "multiple styles gets messy" …
    demo.cursor_left();
    for _ in 0..4 {
        demo.shift_word_left();
    }
    demo.context_menu('b'); // Bold

    // … then highlight "gets messy" on top of it — the highlight stacks
    // with the bold, nesting inside the bold span.
    demo.end();
    demo.cursor_left();
    demo.shift_word_left();
    demo.shift_word_left();
    demo.open_context_menu();
    demo.screenshot("2-context-menu")?;
    demo.pick('H'); // Highlight

    demo.end();
    demo.paragraph_break();
    demo.write(
        "Using Reveal Codes, you can clearly see where formatting is applied and even change it.",
    );

    demo.screenshot("1-document")?;

    // Show the markup behind the document …
    demo.menu('v', "Reveal Codes");
    demo.screenshot("4-reveal-codes")?;
    demo.pause(2000);

    // … and clean up the mess: walk up behind the <Bold] end tag and delete
    // it. The whole phrase loses its bold; the nested highlight stays.
    demo.cursor_up();
    demo.cursor_up();
    demo.end();
    demo.cursor_left();
    demo.backspace();
    demo.pause(3000);

    demo.stop("demo.gif")
}
