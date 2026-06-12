//! Modal file dialog with shell-style tab completion, used by the File
//! menu's Open... and Save As... commands.
//!
//! The dialog is a single-line path input above a live listing of the
//! directory the input points into, filtered by the typed file name prefix.
//! Tab completes like a shell (longest common prefix, unique directories
//! gain a trailing slash), Up/Down move through the listing, and Enter
//! either descends into the selected directory or accepts a file. The
//! surrounding [`crate::app::App`] decides what accepting a path means and
//! uses [`FileDialogState::pending_confirm`] to require a second Enter for
//! destructive accepts (overwriting a file, discarding unsaved changes).

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDialogKind {
    Open,
    SaveAs,
}

pub struct Candidate {
    /// File or directory name within the input's directory (no slash).
    pub name: String,
    pub is_dir: bool,
}

/// What pressing Enter did: the dialog either stays open (descended into a
/// directory, nothing to accept) or yields the chosen path.
pub enum FileDialogResult {
    Pending,
    Accept(PathBuf),
}

pub struct FileDialogState {
    kind: FileDialogKind,
    input: String,
    /// Cursor position in the input, as a char index.
    cursor: usize,
    candidates: Vec<Candidate>,
    /// Highlighted candidate; `None` keeps focus on the input line.
    selected: Option<usize>,
    /// Path whose accept needs confirming with a second Enter. Set by the
    /// app, cleared by any edit or selection change.
    pending_confirm: Option<PathBuf>,
}

impl FileDialogState {
    pub fn new(kind: FileDialogKind, initial_input: String) -> Self {
        let mut state = Self {
            kind,
            cursor: initial_input.chars().count(),
            input: initial_input,
            candidates: Vec::new(),
            selected: None,
            pending_confirm: None,
        };
        state.refresh();
        state
    }

    pub fn kind(&self) -> FileDialogKind {
        self.kind
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    /// Cursor position as a char index into [`FileDialogState::input`].
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn pending_confirm(&self) -> Option<&Path> {
        self.pending_confirm.as_deref()
    }

    pub fn set_pending_confirm(&mut self, path: PathBuf) {
        self.pending_confirm = Some(path);
    }

    fn char_len(&self) -> usize {
        self.input.chars().count()
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_index)
            .map(|(index, _)| index)
            .unwrap_or(self.input.len())
    }

    /// Split the input at the last slash into the directory part (with the
    /// slash) and the file name prefix being typed.
    fn split_input(&self) -> (String, String) {
        match self.input.rfind('/') {
            Some(pos) => (
                self.input[..=pos].to_string(),
                self.input[pos + 1..].to_string(),
            ),
            None => (String::new(), self.input.clone()),
        }
    }

    /// Re-list the directory the input points into, keeping only entries
    /// matching the typed prefix. Hidden entries stay hidden until the
    /// prefix itself starts with a dot.
    fn refresh(&mut self) {
        self.candidates.clear();
        let (dir, prefix) = self.split_input();
        let dir_path = if dir.is_empty() {
            PathBuf::from(".")
        } else {
            expand_tilde(&dir)
        };
        let Ok(entries) = fs::read_dir(&dir_path) else {
            return;
        };
        let show_hidden = prefix.starts_with('.');
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            if !name.starts_with(&prefix) {
                continue;
            }
            let is_dir = entry.path().is_dir();
            self.candidates.push(Candidate { name, is_dir });
        }
        self.candidates.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    fn edited(&mut self) {
        self.selected = None;
        self.pending_confirm = None;
        self.refresh();
    }

    fn set_input(&mut self, input: String) {
        self.input = input;
        self.cursor = self.char_len();
        self.edited();
    }

    pub fn insert_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let at = self.byte_index(self.cursor);
        self.input.insert(at, ch);
        self.cursor += 1;
        self.edited();
    }

    /// Insert pasted text, dropping control characters (newlines included).
    pub fn insert_str(&mut self, text: &str) {
        let filtered: String = text.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }
        let at = self.byte_index(self.cursor);
        self.input.insert_str(at, &filtered);
        self.cursor += filtered.chars().count();
        self.edited();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_index(self.cursor - 1);
        let end = self.byte_index(self.cursor);
        self.input.replace_range(start..end, "");
        self.cursor -= 1;
        self.edited();
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.char_len() {
            return;
        }
        let start = self.byte_index(self.cursor);
        let end = self.byte_index(self.cursor + 1);
        self.input.replace_range(start..end, "");
        self.edited();
    }

    /// Delete back to the start of the current path component (or the whole
    /// previous component when the cursor sits right behind its slash).
    pub fn delete_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.input.chars().collect();
        let mut target = self.cursor;
        while target > 0 && chars[target - 1] == '/' {
            target -= 1;
        }
        while target > 0 && chars[target - 1] != '/' {
            target -= 1;
        }
        let start = self.byte_index(target);
        let end = self.byte_index(self.cursor);
        self.input.replace_range(start..end, "");
        self.cursor = target;
        self.edited();
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_len());
    }

    pub fn move_cursor_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor = self.char_len();
    }

    /// Move the listing highlight. The cycle includes "no selection": moving
    /// past either end puts focus back on the input line, so Enter accepts
    /// the typed path again.
    pub fn move_selection(&mut self, delta: i32) {
        if self.candidates.is_empty() {
            return;
        }
        self.pending_confirm = None;
        let len = self.candidates.len() as i32;
        let next = match self.selected {
            None if delta >= 0 => Some(0),
            None => Some(len - 1),
            Some(current) => {
                let next = current as i32 + delta.signum();
                (0..len).contains(&next).then_some(next)
            }
        };
        self.selected = next.map(|index| index as usize);
    }

    /// Shell-style completion: a unique match completes fully (directories
    /// gain a trailing slash), multiple matches complete to their longest
    /// common prefix.
    pub fn complete(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        let (dir, prefix) = self.split_input();
        if let [candidate] = &self.candidates[..] {
            let mut input = format!("{dir}{}", candidate.name);
            if candidate.is_dir {
                input.push('/');
            }
            self.set_input(input);
            return;
        }
        let mut lcp = self.candidates[0].name.clone();
        for candidate in &self.candidates[1..] {
            lcp = common_prefix(&lcp, &candidate.name);
            if lcp.is_empty() {
                break;
            }
        }
        if lcp.chars().count() > prefix.chars().count() {
            self.set_input(format!("{dir}{lcp}"));
        }
    }

    /// Apply Enter: descend into the selected (or typed) directory, or
    /// resolve the chosen file path for the app to act on.
    pub fn enter(&mut self) -> FileDialogResult {
        if let Some(index) = self.selected {
            let (dir, _) = self.split_input();
            let candidate = &self.candidates[index];
            let mut input = format!("{dir}{}", candidate.name);
            if candidate.is_dir {
                input.push('/');
                self.set_input(input);
                return FileDialogResult::Pending;
            }
            self.set_input(input);
            return FileDialogResult::Accept(self.resolved_path());
        }

        if self.input.is_empty() {
            return FileDialogResult::Pending;
        }
        let path = self.resolved_path();
        if path.is_dir() {
            if !self.input.ends_with('/') {
                let input = format!("{}/", self.input);
                self.set_input(input);
            }
            return FileDialogResult::Pending;
        }
        FileDialogResult::Accept(path)
    }

    /// The typed path with a leading `~` expanded.
    pub fn resolved_path(&self) -> PathBuf {
        expand_tilde(&self.input)
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~"
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home);
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

fn common_prefix(a: &str, b: &str) -> String {
    a.chars()
        .zip(b.chars())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog_with_input(input: &str) -> FileDialogState {
        FileDialogState::new(FileDialogKind::Open, input.to_string())
    }

    #[test]
    fn lists_directory_filtered_by_prefix() {
        let dialog = dialog_with_input("tests/fixtures/");
        let names: Vec<&str> = dialog
            .candidates()
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect();
        assert_eq!(names, ["sub", "alpha.ftml", "beta.md"]);
        assert!(dialog.candidates()[0].is_dir);

        let dialog = dialog_with_input("tests/fixtures/b");
        let names: Vec<&str> = dialog
            .candidates()
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect();
        assert_eq!(names, ["beta.md"]);
    }

    #[test]
    fn tab_completes_unique_match_and_descends_into_directories() {
        let mut dialog = dialog_with_input("tests/fixtures/al");
        dialog.complete();
        assert_eq!(dialog.input(), "tests/fixtures/alpha.ftml");

        let mut dialog = dialog_with_input("tests/fixtures/su");
        dialog.complete();
        assert_eq!(dialog.input(), "tests/fixtures/sub/");
        assert_eq!(dialog.candidates().len(), 3); // the nested* fixtures
    }

    #[test]
    fn tab_completes_to_longest_common_prefix() {
        let mut dialog = dialog_with_input("tests/fixtures/sub/nested-");
        dialog.complete();
        assert_eq!(dialog.input(), "tests/fixtures/sub/nested-cop");
        assert_eq!(dialog.candidates().len(), 2);
    }

    #[test]
    fn enter_descends_into_typed_directory() {
        let mut dialog = dialog_with_input("tests/fixtures");
        assert!(matches!(dialog.enter(), FileDialogResult::Pending));
        assert_eq!(dialog.input(), "tests/fixtures/");
    }

    #[test]
    fn enter_accepts_selected_file() {
        let mut dialog = dialog_with_input("tests/fixtures/alpha");
        dialog.move_selection(1);
        match dialog.enter() {
            FileDialogResult::Accept(path) => {
                assert_eq!(path, PathBuf::from("tests/fixtures/alpha.ftml"));
            }
            FileDialogResult::Pending => panic!("expected the selected file to be accepted"),
        }
    }

    #[test]
    fn selection_cycles_through_input_focus() {
        let mut dialog = dialog_with_input("tests/fixtures/");
        assert_eq!(dialog.selected(), None);
        dialog.move_selection(1);
        assert_eq!(dialog.selected(), Some(0));
        dialog.move_selection(-1);
        assert_eq!(dialog.selected(), None);
        dialog.move_selection(-1);
        assert_eq!(dialog.selected(), Some(2));
        dialog.move_selection(1);
        assert_eq!(dialog.selected(), None);
    }

    #[test]
    fn editing_input_resets_selection_and_confirmation() {
        let mut dialog = dialog_with_input("tests/fixtures/");
        dialog.move_selection(1);
        dialog.set_pending_confirm(PathBuf::from("tests/fixtures/alpha.ftml"));
        dialog.insert_char('a');
        assert_eq!(dialog.selected(), None);
        assert_eq!(dialog.pending_confirm(), None);
        assert_eq!(dialog.input(), "tests/fixtures/a");
    }
}
