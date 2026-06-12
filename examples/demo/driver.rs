//! The demo script vocabulary, shared by both playback backends.
//!
//! Each verb turns into one or more simulated key presses with human
//! pacing. The backend decides what a key press means: with the `recorder`
//! feature it is rasterized into a GIF frame (see `recorder.rs`); without
//! it, the demo plays out live in the real terminal (see `player.rs`).

#[cfg(feature = "recorder")]
#[path = "recorder.rs"]
mod backend;
#[cfg(not(feature = "recorder"))]
#[path = "player.rs"]
mod backend;

use std::path::Path;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use pure_tui::menu_bar::{MENU_BAR, MenuBarEntry, menu_with_accel};

/// How long the result of an interaction stays on screen, by the kind of
/// interaction that produced it, in 10ms steps (the GIF delay unit).
mod pace {
    /// Between typed characters.
    pub const TYPE: u32 = 2;
    /// After a typed space — typists pause between words.
    pub const SPACE: u32 = 10;
    /// After typed punctuation.
    pub const PUNCTUATION: u32 = 30;
    /// Plain cursor movement.
    pub const MOVE: u32 = 25;
    /// Watching a selection grow.
    pub const SELECT: u32 = 50;
    /// Reading a freshly opened menu.
    pub const MENU: u32 = 100;
    /// Stepping through menu items.
    pub const STEP: u32 = 50;
    /// Letting an action's effect sink in.
    pub const ACTION: u32 = 90;
    /// The opening state: an empty editor.
    pub const FIRST: u32 = 100;
    /// The closing state before the GIF loops / the player exits.
    pub const LAST: u32 = 500;
}

/// A scripted Pure session: records to a GIF (`--features recorder`) or
/// plays live in the terminal.
pub struct Demo {
    backend: backend::Backend,
}

// The interaction verbs form the script vocabulary; not every demo uses all
// of them.
#[allow(dead_code)]
impl Demo {
    /// Start a session with an empty document. `columns`×`rows` is the
    /// recorded terminal size; live playback uses the real terminal as is.
    pub fn start(columns: u16, rows: u16) -> Self {
        let mut backend = backend::Backend::start(columns, rows);
        backend.hold(pace::FIRST);
        Self { backend }
    }

    /// Type text character by character, pacing word and sentence breaks
    /// like a human typist.
    pub fn write(&mut self, text: &str) {
        for ch in text.chars() {
            let delay = match ch {
                ' ' => pace::SPACE,
                '.' | ',' | ';' | ':' | '!' | '?' => pace::PUNCTUATION,
                _ => pace::TYPE,
            };
            self.press(KeyCode::Char(ch), KeyModifiers::NONE, delay);
        }
    }

    /// Open the context menu and trigger the entry with this shortcut
    /// character (e.g. `'1'` for "Heading 1", `'i'` for "Italic"). Shifted
    /// shortcuts like Highlight's `'H'` are written in their uppercase form.
    pub fn context_menu(&mut self, shortcut: char) {
        self.press(KeyCode::Esc, KeyModifiers::NONE, pace::MENU);
        let modifiers = if shortcut.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        self.press(KeyCode::Char(shortcut), modifiers, pace::ACTION);
    }

    /// Open the context menu and trigger the entry, but faster.
    pub fn context_menu_fast(&mut self, shortcut: char) {
        self.press(KeyCode::Esc, KeyModifiers::NONE, pace::PUNCTUATION);
        let modifiers = if shortcut.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        self.press(KeyCode::Char(shortcut), modifiers, pace::PUNCTUATION);
    }

    /// Open a menu by its Alt accelerator and activate the entry with this
    /// label, stepping down to it like a user would.
    pub fn menu(&mut self, accel: char, label: &str) {
        let menu =
            menu_with_accel(accel).unwrap_or_else(|| panic!("no menu with accelerator '{accel}'"));
        let entries = MENU_BAR[menu].entries;
        let is_item = |entry: &&MenuBarEntry| matches!(entry, MenuBarEntry::Item(_));
        let first = entries
            .iter()
            .position(|e| matches!(e, MenuBarEntry::Item(item) if item.action.is_some()))
            .expect("menu has an enabled item");
        let target = entries
            .iter()
            .position(|e| matches!(e, MenuBarEntry::Item(item) if item.label == label))
            .unwrap_or_else(|| panic!("menu has no item labelled '{label}'"));
        assert!(
            target >= first,
            "'{label}' sits above the initially selected item"
        );

        self.press(KeyCode::Char(accel), KeyModifiers::ALT, pace::MENU);
        let steps = entries[first..=target].iter().filter(is_item).count() - 1;
        for _ in 0..steps {
            self.press(KeyCode::Down, KeyModifiers::NONE, pace::STEP);
        }
        self.press(KeyCode::Enter, KeyModifiers::NONE, pace::ACTION);
    }

    pub fn paragraph_break(&mut self) {
        self.press(KeyCode::Enter, KeyModifiers::NONE, pace::ACTION);
    }

    pub fn cursor_left(&mut self) {
        self.press(KeyCode::Left, KeyModifiers::NONE, pace::MOVE);
    }

    pub fn cursor_right(&mut self) {
        self.press(KeyCode::Right, KeyModifiers::NONE, pace::MOVE);
    }

    pub fn cursor_up(&mut self) {
        self.press(KeyCode::Up, KeyModifiers::NONE, pace::MOVE);
    }

    pub fn cursor_down(&mut self) {
        self.press(KeyCode::Down, KeyModifiers::NONE, pace::MOVE);
    }

    pub fn home(&mut self) {
        self.press(KeyCode::Home, KeyModifiers::NONE, pace::MOVE);
    }

    pub fn end(&mut self) {
        self.press(KeyCode::End, KeyModifiers::NONE, pace::MOVE);
    }

    /// Jump one word to the left.
    pub fn word_left(&mut self) {
        self.press(KeyCode::Left, KeyModifiers::CONTROL, pace::MOVE);
    }

    /// Jump one word to the right.
    pub fn word_right(&mut self) {
        self.press(KeyCode::Right, KeyModifiers::CONTROL, pace::MOVE);
    }

    pub fn backspace(&mut self) {
        self.press(KeyCode::Backspace, KeyModifiers::NONE, pace::ACTION);
    }

    /// Extend the selection by one word to the left.
    pub fn shift_word_left(&mut self) {
        self.press(
            KeyCode::Left,
            KeyModifiers::SHIFT | KeyModifiers::CONTROL,
            pace::SELECT,
        );
    }

    /// Extend the selection by one word to the right.
    pub fn shift_word_right(&mut self) {
        self.press(
            KeyCode::Right,
            KeyModifiers::SHIFT | KeyModifiers::CONTROL,
            pace::SELECT,
        );
    }

    pub fn ctrl(&mut self, ch: char) {
        self.press(KeyCode::Char(ch), KeyModifiers::CONTROL, pace::ACTION);
    }

    /// Linger on the current state for an extra `ms` milliseconds.
    pub fn pause(&mut self, ms: u32) {
        self.backend.hold(ms / 10);
    }

    /// Hold the final state, then finish up: write the GIF to `path` when
    /// recording, or restore the terminal when playing live.
    pub fn stop(mut self, path: impl AsRef<Path>) -> Result<()> {
        self.backend.hold(pace::LAST);
        self.backend.finish(path.as_ref())
    }

    fn press(&mut self, code: KeyCode, modifiers: KeyModifiers, delay: u32) {
        self.backend.press(code, modifiers, delay);
    }
}
