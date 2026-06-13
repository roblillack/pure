//! Modal dialog for creating and editing hyperlinks, used by the editor's
//! "Edit Link..." command.
//!
//! The dialog holds two single-line text fields — the link's visible text and
//! its target URL — plus Open, Cancel, and Save buttons. Tab / Shift-Tab (and
//! Up / Down) move focus between the fields and the buttons; typing edits
//! whichever field has focus, and Space activates a focused button. The
//! surrounding [`crate::app::App`] owns the accelerators (Enter saves, Esc
//! cancels) and decides what saving means: it writes the text and target back
//! onto the document, creating, retargeting, or (when the URL is cleared)
//! removing the link.

/// Which part of the dialog currently has focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkField {
    Text,
    Target,
    Open,
    Cancel,
    Save,
}

impl LinkField {
    /// Whether this element is a button (rather than a text field).
    pub fn is_button(self) -> bool {
        matches!(self, LinkField::Open | LinkField::Cancel | LinkField::Save)
    }
}

pub struct LinkDialogState {
    /// The link's visible text.
    text: String,
    /// Cursor position in `text`, as a char index.
    text_cursor: usize,
    /// The link's target URL.
    target: String,
    /// Cursor position in `target`, as a char index.
    target_cursor: usize,
    focus: LinkField,
    /// Whether an existing link is being edited (vs. a new one created); only
    /// affects the dialog title.
    editing: bool,
}

impl LinkDialogState {
    pub fn new(text: String, target: String, editing: bool) -> Self {
        let text_cursor = text.chars().count();
        let target_cursor = target.chars().count();
        Self {
            text,
            text_cursor,
            target,
            target_cursor,
            // A fresh link starts on the text field; when editing an existing
            // one the text is usually fine and the target is what changes, but
            // starting on the text field keeps the behaviour predictable.
            focus: LinkField::Text,
            editing,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn focus(&self) -> LinkField {
        self.focus
    }

    pub fn editing(&self) -> bool {
        self.editing
    }

    /// Cursor char index within the focused text field, or `None` when a
    /// button has focus (no text caret to show).
    pub fn active_cursor(&self) -> Option<usize> {
        match self.focus {
            LinkField::Text => Some(self.text_cursor),
            LinkField::Target => Some(self.target_cursor),
            _ => None,
        }
    }

    /// Move focus to the next element, wrapping Save → Text.
    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            LinkField::Text => LinkField::Target,
            LinkField::Target => LinkField::Open,
            LinkField::Open => LinkField::Cancel,
            LinkField::Cancel => LinkField::Save,
            LinkField::Save => LinkField::Text,
        };
    }

    /// Move focus to the previous element, wrapping Text → Save.
    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            LinkField::Text => LinkField::Save,
            LinkField::Target => LinkField::Text,
            LinkField::Open => LinkField::Target,
            LinkField::Cancel => LinkField::Open,
            LinkField::Save => LinkField::Cancel,
        };
    }

    /// The focused field's text and cursor, or `None` for a button.
    fn active_field_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.focus {
            LinkField::Text => Some((&mut self.text, &mut self.text_cursor)),
            LinkField::Target => Some((&mut self.target, &mut self.target_cursor)),
            _ => None,
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        if let Some((field, cursor)) = self.active_field_mut() {
            let at = byte_index(field, *cursor);
            field.insert(at, ch);
            *cursor += 1;
        }
    }

    /// Insert pasted text, dropping control characters (newlines included).
    pub fn insert_str(&mut self, text: &str) {
        let filtered: String = text.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }
        if let Some((field, cursor)) = self.active_field_mut() {
            let at = byte_index(field, *cursor);
            field.insert_str(at, &filtered);
            *cursor += filtered.chars().count();
        }
    }

    pub fn backspace(&mut self) {
        if let Some((field, cursor)) = self.active_field_mut() {
            if *cursor == 0 {
                return;
            }
            let start = byte_index(field, *cursor - 1);
            let end = byte_index(field, *cursor);
            field.replace_range(start..end, "");
            *cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        if let Some((field, cursor)) = self.active_field_mut() {
            if *cursor >= field.chars().count() {
                return;
            }
            let start = byte_index(field, *cursor);
            let end = byte_index(field, *cursor + 1);
            field.replace_range(start..end, "");
        }
    }

    /// Delete from the cursor back to the start of the previous whitespace-
    /// delimited word.
    pub fn delete_word_backward(&mut self) {
        if let Some((field, cursor)) = self.active_field_mut() {
            if *cursor == 0 {
                return;
            }
            let chars: Vec<char> = field.chars().collect();
            let mut target = *cursor;
            while target > 0 && chars[target - 1].is_whitespace() {
                target -= 1;
            }
            while target > 0 && !chars[target - 1].is_whitespace() {
                target -= 1;
            }
            let start = byte_index(field, target);
            let end = byte_index(field, *cursor);
            field.replace_range(start..end, "");
            *cursor = target;
        }
    }

    pub fn move_cursor_left(&mut self) {
        if let Some((_, cursor)) = self.active_field_mut() {
            *cursor = cursor.saturating_sub(1);
        }
    }

    pub fn move_cursor_right(&mut self) {
        if let Some((field, cursor)) = self.active_field_mut() {
            *cursor = (*cursor + 1).min(field.chars().count());
        }
    }

    pub fn move_cursor_start(&mut self) {
        if let Some((_, cursor)) = self.active_field_mut() {
            *cursor = 0;
        }
    }

    pub fn move_cursor_end(&mut self) {
        if let Some((field, cursor)) = self.active_field_mut() {
            *cursor = field.chars().count();
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
    use super::*;

    #[test]
    fn typing_edits_the_focused_field() {
        let mut dialog = LinkDialogState::new(String::new(), String::new(), false);
        dialog.insert_char('h');
        dialog.insert_char('i');
        assert_eq!(dialog.text(), "hi");
        assert_eq!(dialog.target(), "");

        dialog.focus_next();
        assert_eq!(dialog.focus(), LinkField::Target);
        dialog.insert_str("https://example.test");
        assert_eq!(dialog.target(), "https://example.test");
        assert_eq!(dialog.text(), "hi");
    }

    #[test]
    fn focus_cycles_through_fields_and_buttons() {
        let mut dialog = LinkDialogState::new(String::new(), String::new(), false);
        let order = [
            LinkField::Text,
            LinkField::Target,
            LinkField::Open,
            LinkField::Cancel,
            LinkField::Save,
        ];
        assert_eq!(dialog.focus(), LinkField::Text);
        // A full forward cycle visits each element and wraps back to Text.
        for expected in order
            .iter()
            .skip(1)
            .chain(std::iter::once(&LinkField::Text))
        {
            dialog.focus_next();
            assert_eq!(dialog.focus(), *expected);
        }
        assert_eq!(dialog.focus(), LinkField::Text);

        // The buttons carry no text caret; the fields do.
        dialog.focus_next();
        assert_eq!(dialog.focus(), LinkField::Target);
        assert_eq!(dialog.active_cursor(), Some(0));
        dialog.focus_next();
        assert_eq!(dialog.focus(), LinkField::Open);
        assert!(dialog.focus().is_button());
        assert_eq!(dialog.active_cursor(), None);
        dialog.focus_prev();
        assert_eq!(dialog.focus(), LinkField::Target);
    }

    #[test]
    fn editing_keys_are_ignored_on_the_open_button() {
        let mut dialog = LinkDialogState::new("hi".to_string(), "url".to_string(), true);
        dialog.focus = LinkField::Open;
        dialog.insert_char('x');
        dialog.backspace();
        assert_eq!(dialog.text(), "hi");
        assert_eq!(dialog.target(), "url");
    }

    #[test]
    fn delete_word_backward_stops_at_whitespace() {
        let mut dialog = LinkDialogState::new("one two three".to_string(), String::new(), false);
        dialog.delete_word_backward();
        assert_eq!(dialog.text(), "one two ");
        dialog.delete_word_backward();
        assert_eq!(dialog.text(), "one ");
    }
}
