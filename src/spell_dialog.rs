//! State of the modal spell-check dialog (F7): the misspelling under review,
//! the suggested corrections, and the editable replacement.
//!
//! The dialog walks the document forwards, one misspelling at a time. It holds
//! no dictionary and performs no edits: [`crate::app::App`] scans the document,
//! hands each misspelling here via [`SpellDialogState::new`] /
//! [`SpellDialogState::show`], and applies whatever the user picks. The running
//! tally survives those hand-offs so the closing status line can report what
//! the pass did.
//!
//! Tab / Shift-Tab move focus between the replacement field and the buttons,
//! Up / Down pick a suggestion (filling the field with it), and the app owns the
//! accelerators: Enter replaces, Esc closes.

use crate::spell::{ContextWindow, Misspelling};

/// A focusable element of the dialog: the replacement field, or one of the
/// buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellControl {
    /// The editable replacement ("Change to").
    ChangeTo,
    Replace,
    ReplaceAll,
    Add,
    Ignore,
    IgnoreAll,
    Close,
}

impl SpellControl {
    /// Whether this element is a button (rather than the text field).
    pub fn is_button(self) -> bool {
        !matches!(self, SpellControl::ChangeTo)
    }
}

/// Tab order: the replacement field, then the buttons as they're laid out —
/// left to right, top row before bottom.
const FOCUS_ORDER: [SpellControl; 7] = [
    SpellControl::ChangeTo,
    SpellControl::Replace,
    SpellControl::ReplaceAll,
    SpellControl::Add,
    SpellControl::Ignore,
    SpellControl::IgnoreAll,
    SpellControl::Close,
];

/// What a spell-check pass has done so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpellTally {
    pub replaced: usize,
    pub ignored: usize,
    pub added: usize,
}

impl SpellTally {
    pub fn is_empty(self) -> bool {
        self.replaced == 0 && self.ignored == 0 && self.added == 0
    }

    /// The tally as status-bar text: `2 replaced, 1 ignored`.
    pub fn summary(self) -> String {
        let mut parts = Vec::new();
        if self.replaced > 0 {
            parts.push(format!("{} replaced", self.replaced));
        }
        if self.ignored > 0 {
            parts.push(format!("{} ignored", self.ignored));
        }
        if self.added > 0 {
            parts.push(format!("{} added to dictionary", self.added));
        }
        parts.join(", ")
    }
}

pub struct SpellDialogState {
    /// The misspelling under review.
    current: Misspelling,
    /// Excerpt of the paragraph around it, for the context line.
    context: ContextWindow,
    /// Misspellings left to review, including the current one.
    remaining: usize,
    suggestions: Vec<String>,
    /// Index into `suggestions`, or `None` when there are none.
    selected: Option<usize>,
    /// The text "Replace" will insert; seeded from the selected suggestion and
    /// editable, so a word the dictionary can't guess can still be fixed here.
    replacement: String,
    /// Cursor position in `replacement`, as a char index.
    replacement_cursor: usize,
    focus: SpellControl,
    tally: SpellTally,
}

impl SpellDialogState {
    /// Open the dialog on `current`, the first of `remaining` misspellings.
    pub fn new(
        current: Misspelling,
        context: ContextWindow,
        suggestions: Vec<String>,
        remaining: usize,
    ) -> Self {
        let mut dialog = Self {
            current,
            context,
            remaining,
            suggestions,
            selected: None,
            replacement: String::new(),
            replacement_cursor: 0,
            focus: SpellControl::ChangeTo,
            tally: SpellTally::default(),
        };
        dialog.seed_replacement();
        dialog
    }

    /// Move on to the next misspelling, keeping the tally and the focus.
    pub fn show(
        &mut self,
        current: Misspelling,
        context: ContextWindow,
        suggestions: Vec<String>,
        remaining: usize,
    ) {
        self.current = current;
        self.context = context;
        self.suggestions = suggestions;
        self.remaining = remaining;
        self.seed_replacement();
    }

    /// Preselect the best suggestion and put it in the replacement field. With
    /// no suggestions the field starts on the misspelled word itself, so it can
    /// be edited into shape.
    fn seed_replacement(&mut self) {
        self.selected = if self.suggestions.is_empty() {
            None
        } else {
            Some(0)
        };
        self.replacement = self
            .suggestions
            .first()
            .cloned()
            .unwrap_or_else(|| self.current.word.clone());
        self.replacement_cursor = self.replacement.chars().count();
    }

    pub fn current(&self) -> &Misspelling {
        &self.current
    }

    pub fn context(&self) -> &ContextWindow {
        &self.context
    }

    pub fn remaining(&self) -> usize {
        self.remaining
    }

    pub fn suggestions(&self) -> &[String] {
        &self.suggestions
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    /// Cursor char index in the replacement field, or `None` while a button has
    /// focus (no text caret to show).
    pub fn cursor(&self) -> Option<usize> {
        match self.focus {
            SpellControl::ChangeTo => Some(self.replacement_cursor),
            _ => None,
        }
    }

    pub fn focus(&self) -> SpellControl {
        self.focus
    }

    pub fn tally(&self) -> SpellTally {
        self.tally
    }

    /// Whether "Replace" can do anything: an empty replacement, or one equal to
    /// the misspelled word, is a no-op.
    pub fn can_replace(&self) -> bool {
        let replacement = self.replacement.trim();
        !replacement.is_empty() && replacement != self.current.word
    }

    pub fn count_replaced(&mut self, words: usize) {
        self.tally.replaced += words;
    }

    pub fn count_ignored(&mut self, words: usize) {
        self.tally.ignored += words;
    }

    pub fn count_added(&mut self) {
        self.tally.added += 1;
    }

    pub fn focus_next(&mut self) {
        self.move_focus(1);
    }

    pub fn focus_prev(&mut self) {
        self.move_focus(-1);
    }

    fn move_focus(&mut self, delta: i32) {
        let len = FOCUS_ORDER.len() as i32;
        let current = FOCUS_ORDER
            .iter()
            .position(|control| *control == self.focus)
            .unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len) as usize;
        self.focus = FOCUS_ORDER[next];
    }

    /// Move the suggestion selection by `delta`, filling the replacement field
    /// with the newly selected word. Stops at either end rather than wrapping,
    /// so holding a cursor key doesn't cycle.
    pub fn select_suggestion(&mut self, delta: i32) {
        if self.suggestions.is_empty() {
            return;
        }
        let last = self.suggestions.len() as i32 - 1;
        let current = self.selected.unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, last) as usize;
        self.selected = Some(next);
        self.replacement = self.suggestions[next].clone();
        self.replacement_cursor = self.replacement.chars().count();
    }

    // ----- replacement field editing ---------------------------------------

    /// Whether the replacement field currently has focus (and so takes typing).
    fn editing(&self) -> bool {
        self.focus == SpellControl::ChangeTo
    }

    pub fn insert_char(&mut self, ch: char) {
        if ch.is_control() || !self.editing() {
            return;
        }
        let at = byte_index(&self.replacement, self.replacement_cursor);
        self.replacement.insert(at, ch);
        self.replacement_cursor += 1;
        // Typed text is the user's own correction, no longer a suggestion.
        self.selected = None;
    }

    pub fn backspace(&mut self) {
        if !self.editing() || self.replacement_cursor == 0 {
            return;
        }
        let start = byte_index(&self.replacement, self.replacement_cursor - 1);
        let end = byte_index(&self.replacement, self.replacement_cursor);
        self.replacement.replace_range(start..end, "");
        self.replacement_cursor -= 1;
        self.selected = None;
    }

    pub fn delete(&mut self) {
        if !self.editing() || self.replacement_cursor >= self.replacement.chars().count() {
            return;
        }
        let start = byte_index(&self.replacement, self.replacement_cursor);
        let end = byte_index(&self.replacement, self.replacement_cursor + 1);
        self.replacement.replace_range(start..end, "");
        self.selected = None;
    }

    pub fn move_cursor_left(&mut self) {
        if self.editing() {
            self.replacement_cursor = self.replacement_cursor.saturating_sub(1);
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.editing() {
            self.replacement_cursor =
                (self.replacement_cursor + 1).min(self.replacement.chars().count());
        }
    }

    pub fn move_cursor_start(&mut self) {
        if self.editing() {
            self.replacement_cursor = 0;
        }
    }

    pub fn move_cursor_end(&mut self) {
        if self.editing() {
            self.replacement_cursor = self.replacement.chars().count();
        }
    }
}

fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use rutle::tree_path::TreePath;

    use super::*;
    use crate::spell::context_window;

    fn misspelling(word: &str) -> Misspelling {
        Misspelling {
            path: TreePath::root(0),
            start: 0,
            end: word.len(),
            word: word.to_string(),
        }
    }

    fn dialog(word: &str, suggestions: &[&str]) -> SpellDialogState {
        let current = misspelling(word);
        let context = context_window(word, 0, word.len(), 40);
        SpellDialogState::new(
            current,
            context,
            suggestions.iter().map(|s| s.to_string()).collect(),
            1,
        )
    }

    #[test]
    fn the_best_suggestion_seeds_the_replacement() {
        let dialog = dialog("packk", &["pack", "packs", "peck"]);
        assert_eq!(dialog.selected(), Some(0));
        assert_eq!(dialog.replacement(), "pack");
        assert!(dialog.can_replace());
        assert_eq!(dialog.focus(), SpellControl::ChangeTo);
        assert_eq!(dialog.cursor(), Some(4));
    }

    #[test]
    fn without_suggestions_the_field_holds_the_word_itself() {
        let dialog = dialog("qwrtzp", &[]);
        assert_eq!(dialog.selected(), None);
        assert_eq!(dialog.replacement(), "qwrtzp");
        // Replacing a word with itself is a no-op, so the button stays inert
        // until the field is edited.
        assert!(!dialog.can_replace());
    }

    #[test]
    fn cursor_keys_pick_a_suggestion_and_stop_at_the_ends() {
        let mut dialog = dialog("packk", &["pack", "packs", "peck"]);
        dialog.select_suggestion(1);
        assert_eq!(dialog.selected(), Some(1));
        assert_eq!(dialog.replacement(), "packs");
        dialog.select_suggestion(1);
        dialog.select_suggestion(1);
        assert_eq!(
            dialog.selected(),
            Some(2),
            "selection stops at the last one"
        );
        dialog.select_suggestion(-9);
        assert_eq!(dialog.selected(), Some(0), "and at the first");
        assert_eq!(dialog.replacement(), "pack");
    }

    #[test]
    fn typing_edits_the_replacement_and_drops_the_suggestion() {
        let mut dialog = dialog("packk", &["pack", "packs"]);
        dialog.backspace();
        dialog.insert_char('t');
        assert_eq!(dialog.replacement(), "pact");
        assert_eq!(dialog.selected(), None, "the correction is now the user's");
    }

    #[test]
    fn focus_cycles_through_the_field_and_the_buttons() {
        let mut dialog = dialog("packk", &["pack"]);
        for expected in FOCUS_ORDER
            .iter()
            .skip(1)
            .chain([SpellControl::ChangeTo].iter())
        {
            dialog.focus_next();
            assert_eq!(dialog.focus(), *expected);
        }
        dialog.focus_prev();
        assert_eq!(dialog.focus(), SpellControl::Close);
        assert!(dialog.focus().is_button());
        assert_eq!(dialog.cursor(), None, "buttons carry no text caret");
    }

    #[test]
    fn typing_is_ignored_while_a_button_has_focus() {
        let mut dialog = dialog("packk", &["pack"]);
        dialog.focus_next();
        assert_eq!(dialog.focus(), SpellControl::Replace);
        dialog.insert_char('x');
        dialog.backspace();
        assert_eq!(dialog.replacement(), "pack");
    }

    #[test]
    fn showing_the_next_word_keeps_the_tally() {
        let mut dialog = dialog("packk", &["pack"]);
        dialog.count_replaced(1);
        dialog.count_ignored(1);
        let next = misspelling("essentails");
        dialog.show(
            next,
            context_window("essentails", 0, 10, 40),
            vec!["essentials".to_string()],
            1,
        );
        assert_eq!(dialog.current().word, "essentails");
        assert_eq!(dialog.replacement(), "essentials");
        assert_eq!(
            dialog.tally(),
            SpellTally {
                replaced: 1,
                ignored: 1,
                added: 0
            }
        );
        assert_eq!(dialog.tally().summary(), "1 replaced, 1 ignored");
    }
}
