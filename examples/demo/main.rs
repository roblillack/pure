//! The scripted demo recording embedded in the README.
//!
//! Re-record it after UI changes with:
//!
//! ```sh
//! cargo run --release --example demo --features recorder
//! ```
//!
//! Every call below simulates real key presses through the app's event
//! handling and captures one GIF frame per press, so the recording always
//! shows the editor's current behavior. Set `PURE_DEMO_FRAMES=<dir>` to
//! also dump each frame as a PNG while tuning the script.

mod recorder;

use recorder::Recorder;

fn main() -> anyhow::Result<()> {
    let mut demo = Recorder::start(80, 24);

    demo.write("Welcome to Pure");
    demo.context_menu('1'); // Heading 1
    demo.paragraph_break();
    demo.write(
        "This is a short demo, showing you how to use a word processor on the command line.",
    );
    demo.paragraph_break();
    demo.write("Pure supports:");
    demo.paragraph_break();
    demo.context_menu('8'); // Bullet List
    demo.write("Headings, lists, quotes & code blocks");
    demo.paragraph_break();
    demo.write("Bold, italic, underline & more");
    demo.paragraph_break();
    demo.write("Even WordPerfect-style reveal codes!");

    // Select "reveal codes" word by word and italicize it …
    demo.cursor_left();
    demo.shift_word_left();
    demo.shift_word_left();
    demo.context_menu('i'); // Italic

    // … then show the markup behind the document.
    demo.menu('v', "Reveal Codes");
    demo.pause(3000);

    demo.stop("demo.gif")
}
