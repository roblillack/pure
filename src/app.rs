//! The interactive editor application: state, drawing, and event handling.
//!
//! This lives in the library (rather than the `pure` binary) so that tests can
//! drive the full application — key and mouse events through real event
//! handling, rendered via `ratatui`'s `TestBackend` — without a terminal.
//!
//! Rendering and editing are delegated to the shared `tdoc-editor` crate
//! ([`StructuredRichDisplay`] + [`StructuredEditor`]); this module is the
//! terminal frontend: a [`RatatuiDrawContext`] backend, key/mouse mapping,
//! window chrome (status bar, scrollbar, menus, dialogs), and file I/O.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use crossterm::{
    clipboard::CopyToClipboard,
    cursor::SetCursorStyle,
    event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use tdoc::ftml::{Writer, parse};
use tdoc::{Document, InlineStyle, ParagraphType, gemini, html, markdown};
use tdoc_editor::draw_context::DrawContext;
use tdoc_editor::richtext::tree_path::TreePath;
use tdoc_editor::{BlockType, DocumentPosition, StructuredRichDisplay, UndoKind};

use crate::file_dialog::{FileDialogKind, FileDialogResult, FileDialogState};
use crate::link_dialog::{LinkDialogState, LinkField};
use crate::menu_bar::{
    AppAction, MENU_BAR, MenuBarEntry, MenuBarState, menu_title_offset, menu_with_accel,
};
use crate::ratatui_draw_context::{RatatuiDrawContext, terminal_theme};
use crate::theme::Theme;

const STATUS_TIMEOUT: Duration = Duration::from_secs(4);
const DOUBLE_CLICK_TIMEOUT: Duration = Duration::from_millis(400);
const MOUSE_SCROLL_LINES: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentFormat {
    Ftml,
    Markdown,
    Html,
    Gemini,
}

impl DocumentFormat {
    /// Picks the document format from a file's extension. Unknown (or missing)
    /// extensions default to FTML, Pure's native format.
    fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());
        match ext.as_deref() {
            Some("md") | Some("markdown") | Some("mkd") | Some("mdown") | Some("mdtxt") => {
                DocumentFormat::Markdown
            }
            Some("html") | Some("htm") | Some("xhtml") => DocumentFormat::Html,
            Some("gmi") | Some("gemini") => DocumentFormat::Gemini,
            _ => DocumentFormat::Ftml,
        }
    }
}

pub fn load_document(path: &PathBuf) -> Result<(Document, DocumentFormat, Option<String>)> {
    let format = DocumentFormat::from_path(path);
    if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let parsed = match format {
            DocumentFormat::Ftml => parse(std::io::Cursor::new(content))
                .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> { Box::new(err) }),
            DocumentFormat::Markdown => markdown::parse(std::io::Cursor::new(content)),
            DocumentFormat::Html => html::parse(std::io::Cursor::new(content)),
            DocumentFormat::Gemini => gemini::parse(std::io::Cursor::new(content)),
        };
        match parsed {
            Ok(doc) => Ok((doc, format, None)),
            Err(err) => {
                let message = format!("Parse error: {err}. Starting with empty document.");
                Ok((Document::new(), format, Some(message)))
            }
        }
    } else {
        Ok((Document::new(), format, Some("New document".to_string())))
    }
}

// ---------------------------------------------------------------------------
// Context menu (model-agnostic): types, entry builders, navigation.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum MenuAction {
    SetParagraphType(ParagraphType),
    ToggleChecklistItem,
    ApplyInlineStyle(InlineStyle),
    EditLink,
    IndentMore,
    IndentLess,
    Cut,
    Copy,
    Paste,
}

#[derive(Clone, Copy)]
struct MenuShortcut {
    key: char,
    requires_shift: bool,
}

impl MenuShortcut {
    const fn new(key: char) -> Self {
        Self {
            key,
            requires_shift: false,
        }
    }

    const fn with_shift(key: char) -> Self {
        Self {
            key,
            requires_shift: true,
        }
    }

    fn matches(&self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        match code {
            KeyCode::Char(ch) if ch == self.key => {
                if self.requires_shift {
                    modifiers == KeyModifiers::SHIFT
                } else {
                    modifiers.is_empty()
                }
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy)]
struct MenuItem {
    label: &'static str,
    action: Option<MenuAction>,
    shortcut: Option<MenuShortcut>,
}

impl MenuItem {
    fn enabled_with_shortcut(
        label: &'static str,
        action: MenuAction,
        shortcut: MenuShortcut,
    ) -> Self {
        Self {
            label,
            action: Some(action),
            shortcut: Some(shortcut),
        }
    }

    fn disabled_with_shortcut(label: &'static str, shortcut: MenuShortcut) -> Self {
        Self {
            label,
            action: None,
            shortcut: Some(shortcut),
        }
    }

    fn enabled(label: &'static str, action: MenuAction) -> Self {
        Self {
            label,
            action: Some(action),
            shortcut: None,
        }
    }

    fn is_enabled(&self) -> bool {
        self.action.is_some()
    }
}

enum MenuEntry {
    Section(&'static str),
    Separator,
    Item(MenuItem),
}

struct ContextMenuState {
    entries: Vec<MenuEntry>,
    selected_index: usize,
}

impl ContextMenuState {
    fn new(entries: Vec<MenuEntry>) -> Self {
        let selected_index = entries
            .iter()
            .enumerate()
            .find(|(_, entry)| matches!(entry, MenuEntry::Item(item) if item.is_enabled()))
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        Self {
            entries,
            selected_index,
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as i32;
        let mut idx = self.selected_index as i32;
        for _ in 0..len {
            idx = (idx + delta).rem_euclid(len);
            if matches!(self.entries[idx as usize], MenuEntry::Item(_)) {
                self.selected_index = idx as usize;
                break;
            }
        }
    }

    fn current_item(&self) -> Option<&MenuItem> {
        match self.entries.get(self.selected_index) {
            Some(MenuEntry::Item(item)) => Some(item),
            _ => None,
        }
    }

    fn current_action(&self) -> Option<MenuAction> {
        self.current_item().and_then(|item| item.action)
    }

    fn entries(&self) -> &[MenuEntry] {
        &self.entries
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn shortcut_action(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> (bool, Option<MenuAction>) {
        for (idx, entry) in self.entries.iter().enumerate() {
            if let MenuEntry::Item(item) = entry
                && let Some(shortcut) = item.shortcut
                && shortcut.matches(code, modifiers)
            {
                self.selected_index = idx;
                return (true, item.action);
            }
        }
        (false, None)
    }
}

fn build_context_menu_entries(
    checklist_state: Option<bool>,
    has_selection: bool,
    can_indent: bool,
    allow_paragraph_change: bool,
    can_paste: bool,
) -> Vec<MenuEntry> {
    let mut entries = Vec::new();

    if let Some(is_checked) = checklist_state {
        let label = if is_checked {
            "Uncheck Item"
        } else {
            "Check Item"
        };
        entries.push(MenuEntry::Item(MenuItem::enabled(
            label,
            MenuAction::ToggleChecklistItem,
        )));
        entries.push(MenuEntry::Separator);
    }

    if can_indent {
        entries.push(MenuEntry::Section("Structure"));
        entries.push(MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Indent more",
            MenuAction::IndentMore,
            MenuShortcut::new(']'),
        )));
        entries.push(MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Indent less",
            MenuAction::IndentLess,
            MenuShortcut::new('['),
        )));
        entries.push(MenuEntry::Separator);
    }

    entries.extend(default_context_menu_entries(
        has_selection,
        allow_paragraph_change,
        can_paste,
    ));
    entries
}

fn paragraph_type_item(
    label: &'static str,
    pt: ParagraphType,
    key: char,
    allow: bool,
) -> MenuEntry {
    MenuEntry::Item(if allow {
        MenuItem::enabled_with_shortcut(label, MenuAction::SetParagraphType(pt), MenuShortcut::new(key))
    } else {
        MenuItem::disabled_with_shortcut(label, MenuShortcut::new(key))
    })
}

fn inline_style_item(
    label: &'static str,
    style: InlineStyle,
    shortcut: MenuShortcut,
    enabled: bool,
) -> MenuEntry {
    MenuEntry::Item(if enabled {
        MenuItem::enabled_with_shortcut(label, MenuAction::ApplyInlineStyle(style), shortcut)
    } else {
        MenuItem::disabled_with_shortcut(label, shortcut)
    })
}

fn default_context_menu_entries(
    has_selection: bool,
    allow_paragraph_change: bool,
    can_paste: bool,
) -> Vec<MenuEntry> {
    let a = allow_paragraph_change;
    vec![
        MenuEntry::Section("Paragraph type"),
        paragraph_type_item("Text", ParagraphType::Text, '0', a),
        paragraph_type_item("Heading 1", ParagraphType::Header1, '1', a),
        paragraph_type_item("Heading 2", ParagraphType::Header2, '2', a),
        paragraph_type_item("Heading 3", ParagraphType::Header3, '3', a),
        paragraph_type_item("Quote", ParagraphType::Quote, '5', a),
        paragraph_type_item("Code", ParagraphType::CodeBlock, '6', a),
        paragraph_type_item("Numbered List", ParagraphType::OrderedList, '7', a),
        paragraph_type_item("Bullet List", ParagraphType::UnorderedList, '8', a),
        paragraph_type_item("Checklist", ParagraphType::Checklist, '9', a),
        MenuEntry::Separator,
        MenuEntry::Section("Inline style"),
        inline_style_item("Bold", InlineStyle::Bold, MenuShortcut::new('b'), has_selection),
        inline_style_item("Italic", InlineStyle::Italic, MenuShortcut::new('i'), has_selection),
        inline_style_item("Underline", InlineStyle::Underline, MenuShortcut::new('u'), has_selection),
        inline_style_item("Code", InlineStyle::Code, MenuShortcut::with_shift('C'), has_selection),
        inline_style_item("Highlight", InlineStyle::Highlight, MenuShortcut::with_shift('H'), has_selection),
        inline_style_item("Strikethrough", InlineStyle::Strike, MenuShortcut::with_shift('X'), has_selection),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Edit Link...",
            MenuAction::EditLink,
            MenuShortcut::new('k'),
        )),
        inline_style_item("Clear Formatting", InlineStyle::None, MenuShortcut::new('\\'), has_selection),
        MenuEntry::Separator,
        MenuEntry::Section("Copy & paste"),
        MenuEntry::Item(if has_selection {
            MenuItem::enabled_with_shortcut("Cut", MenuAction::Cut, MenuShortcut::new('x'))
        } else {
            MenuItem::disabled_with_shortcut("Cut", MenuShortcut::new('x'))
        }),
        MenuEntry::Item(if has_selection {
            MenuItem::enabled_with_shortcut("Copy", MenuAction::Copy, MenuShortcut::new('c'))
        } else {
            MenuItem::disabled_with_shortcut("Copy", MenuShortcut::new('c'))
        }),
        MenuEntry::Item(if can_paste {
            MenuItem::enabled_with_shortcut("Paste", MenuAction::Paste, MenuShortcut::new('v'))
        } else {
            MenuItem::disabled_with_shortcut("Paste", MenuShortcut::new('v'))
        }),
    ]
}

fn is_context_menu_shortcut(code: KeyCode, modifiers: KeyModifiers) -> bool {
    match code {
        KeyCode::Esc => modifiers.is_empty(),
        KeyCode::Char(' ') => modifiers.contains(KeyModifiers::CONTROL),
        _ => false,
    }
}

/// Launch the platform's default browser for `url`, detached.
fn open_in_browser(url: &str) -> io::Result<()> {
    use std::process::{Command, Stdio};

    let mut command = if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_child| ())
}

/// What a cut or copy leaves on the internal clipboard.
struct ClipboardContents {
    /// Plain-text rendering — offered to the system clipboard via OSC 52.
    text: String,
    /// The selection as a standalone document, so in-app paste restores
    /// structure/formatting. `None` falls paste back to `text`.
    fragment: Option<Document>,
}

/// Where an open link dialog writes its result.
enum LinkEdit {
    /// Replace the current selection with a link.
    Selection,
    /// Insert a new link at the cursor.
    Insert,
    /// Edit an existing link span.
    Existing { path: TreePath, index: usize },
}

#[derive(Clone, Debug)]
struct ScrollbarGeometry {
    knob_start: usize,
    knob_size: usize,
}

#[derive(Clone, Debug)]
enum DragState {
    Text,
    Scrollbar { anchor_within_knob: usize },
}

pub struct App {
    display: StructuredRichDisplay,
    theme: Theme,
    file_path: Option<PathBuf>,
    document_format: DocumentFormat,
    should_quit: bool,
    dirty: bool,
    status_message: Option<(String, Instant)>,
    clipboard: Option<ClipboardContents>,
    context_menu: Option<ContextMenuState>,
    menu_bar: Option<MenuBarState>,
    file_dialog: Option<FileDialogState>,
    link_dialog: Option<LinkDialogState>,
    link_edit: Option<LinkEdit>,
    /// Whether the next New command may discard unsaved changes.
    confirm_new: bool,
    /// Whether the viewport should keep the cursor visible (false after a
    /// manual scroll via wheel/Ctrl+arrows/scrollbar).
    follow_cursor: bool,
    last_click_instant: Option<Instant>,
    last_click_position: Option<(u16, u16)>,
    last_click_button: Option<MouseButton>,
    mouse_click_count: u8,
    drag_state: Option<DragState>,
    last_text_area: Rect,
    last_viewport_height: usize,
    last_total_lines: usize,
    last_scrollbar_column: u16,
    /// Cursor position (1-based visual line/column) recorded at the last draw,
    /// for the status bar.
    cursor_line: usize,
    cursor_col: usize,
    interactive: bool,
}

impl App {
    pub fn new(
        document: Document,
        path: Option<PathBuf>,
        format: DocumentFormat,
        initial_status: Option<String>,
    ) -> Self {
        let mut display = StructuredRichDisplay::new(0, 0, 80, 24);
        display.set_theme(terminal_theme());
        // The terminal draws its own hardware caret; don't double-render.
        display.set_cursor_visible(false);
        display.editor_mut().set_tdoc(document);
        display.editor_mut().reset_undo_history();

        Self {
            display,
            theme: Theme::new(),
            file_path: path,
            document_format: format,
            should_quit: false,
            dirty: false,
            status_message: initial_status.map(|msg| (msg, Instant::now())),
            clipboard: None,
            context_menu: None,
            menu_bar: None,
            file_dialog: None,
            link_dialog: None,
            link_edit: None,
            confirm_new: false,
            follow_cursor: true,
            last_click_instant: None,
            last_click_position: None,
            last_click_button: None,
            mouse_click_count: 0,
            drag_state: None,
            last_text_area: Rect::new(0, 0, 80, 24),
            last_viewport_height: 0,
            last_total_lines: 0,
            last_scrollbar_column: 0,
            cursor_line: 1,
            cursor_col: 1,
            interactive: true,
        }
    }

    pub fn set_interactive(&mut self, interactive: bool) {
        self.interactive = interactive;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn has_status_message(&self) -> bool {
        self.status_message.is_some()
    }

    pub fn on_tick(&mut self) {
        self.prune_status_message();
    }

    fn prune_status_message(&mut self) {
        if let Some((_, at)) = &self.status_message
            && at.elapsed() >= STATUS_TIMEOUT
        {
            self.status_message = None;
        }
    }

    fn status(&mut self, message: impl Into<String>) {
        self.status_message = Some((message.into(), Instant::now()));
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.confirm_new = false;
    }

    fn has_selection(&self) -> bool {
        self.display.editor().selection().is_some()
    }

    /// Run a closure with a throwaway [`DrawContext`] sized to the last editor
    /// viewport — needed by the engine's layout-dependent operations (visual
    /// cursor movement, ensure-visible) when no frame is being drawn.
    fn with_ctx<R>(&mut self, f: impl FnOnce(&mut StructuredRichDisplay, &mut dyn DrawContext) -> R) -> R {
        let area = self.last_text_area;
        let (page_bg, default_fg) = self.palette();
        let mut buf = Buffer::empty(area);
        let mut ctx = RatatuiDrawContext::new(&mut buf, area).with_palette(page_bg, default_fg);
        f(&mut self.display, &mut ctx)
    }

    /// The editor theme's page background and default text color, mapped to the
    /// terminal's defaults by [`RatatuiDrawContext`].
    fn palette(&self) -> (u32, u32) {
        let theme = self.display.theme();
        (theme.background_color, theme.plain_text.font_color)
    }

    fn after_edit(&mut self, kind: UndoKind) {
        self.display.editor_mut().commit_undo_step(kind, Instant::now());
        self.mark_dirty();
        self.follow_cursor = true;
    }

    // ----- editing actions -------------------------------------------------

    fn insert_char(&mut self, ch: char) {
        let _ = self.display.editor_mut().insert_text(&ch.to_string());
        self.after_edit(UndoKind::Typing);
    }

    fn insert_str(&mut self, text: &str) {
        let _ = self.display.editor_mut().insert_text(text);
        self.after_edit(UndoKind::Typing);
    }

    fn insert_paragraph_break(&mut self) {
        let _ = self.display.editor_mut().insert_newline();
        self.after_edit(UndoKind::Other);
    }

    fn insert_line_break(&mut self) {
        let _ = self.display.editor_mut().insert_hard_break();
        self.after_edit(UndoKind::Other);
    }

    fn backspace(&mut self) {
        let _ = self.display.editor_mut().delete_backward();
        self.after_edit(UndoKind::Deleting);
    }

    fn delete_forward(&mut self) {
        let _ = self.display.editor_mut().delete_forward();
        self.after_edit(UndoKind::Deleting);
    }

    fn delete_word_backward(&mut self) {
        let _ = self.display.editor_mut().delete_word_backward();
        self.after_edit(UndoKind::Deleting);
    }

    fn delete_word_forward(&mut self) {
        let _ = self.display.editor_mut().delete_word_forward();
        self.after_edit(UndoKind::Deleting);
    }

    fn apply_inline_style(&mut self, style: InlineStyle) {
        let editor = self.display.editor_mut();
        let result = match style {
            InlineStyle::Bold => editor.toggle_bold(),
            InlineStyle::Italic => editor.toggle_italic(),
            InlineStyle::Underline => editor.toggle_underline(),
            InlineStyle::Code => editor.toggle_code(),
            InlineStyle::Highlight => editor.toggle_highlight(),
            InlineStyle::Strike => editor.toggle_strikethrough(),
            InlineStyle::None => editor.clear_formatting(),
            InlineStyle::Link => Ok(()),
        };
        if result.is_ok() {
            self.after_edit(UndoKind::Other);
        }
    }

    fn set_paragraph_type(&mut self, pt: ParagraphType) {
        let block = match pt {
            ParagraphType::Text => BlockType::Paragraph,
            ParagraphType::Header1 => BlockType::Heading { level: 1 },
            ParagraphType::Header2 => BlockType::Heading { level: 2 },
            ParagraphType::Header3 => BlockType::Heading { level: 3 },
            ParagraphType::Quote => BlockType::BlockQuote,
            ParagraphType::CodeBlock => BlockType::CodeBlock { language: None },
            ParagraphType::OrderedList => BlockType::ListItem {
                ordered: true,
                number: None,
                checkbox: None,
                depth: 0,
            },
            ParagraphType::UnorderedList => BlockType::ListItem {
                ordered: false,
                number: None,
                checkbox: None,
                depth: 0,
            },
            ParagraphType::Checklist => BlockType::ListItem {
                ordered: false,
                number: None,
                checkbox: Some(false),
                depth: 0,
            },
            ParagraphType::Table => return,
        };
        if self.display.editor_mut().set_block_type(block).is_ok() {
            self.after_edit(UndoKind::Other);
        }
    }

    fn indent(&mut self) {
        if self.display.editor_mut().indent_list_item().is_ok() {
            self.after_edit(UndoKind::Other);
        }
    }

    fn unindent(&mut self) {
        if self.display.editor_mut().outdent_list_item().is_ok() {
            self.after_edit(UndoKind::Other);
        }
    }

    fn toggle_checklist_item(&mut self) {
        if self.display.editor_mut().toggle_current_checkmark().is_ok() {
            self.after_edit(UndoKind::Other);
        }
    }

    fn undo(&mut self) {
        if self.display.editor_mut().undo() {
            self.follow_cursor = true;
            self.mark_dirty();
        } else {
            self.status("Nothing to undo");
        }
    }

    fn redo(&mut self) {
        if self.display.editor_mut().redo() {
            self.follow_cursor = true;
            self.mark_dirty();
        } else {
            self.status("Nothing to redo");
        }
    }

    // ----- clipboard -------------------------------------------------------

    fn copy_to_system_clipboard(&mut self, text: &str) {
        if self.interactive {
            execute!(io::stdout(), CopyToClipboard::to_clipboard_from(text)).ok();
        }
    }

    fn copy_selection(&mut self) -> bool {
        if !self.has_selection() {
            self.status("Nothing selected");
            return false;
        }
        let text = self.display.editor().get_selection_text();
        let fragment = self.display.editor().get_selection_document();
        self.copy_to_system_clipboard(&text);
        self.clipboard = Some(ClipboardContents { text, fragment });
        self.status("Copied to clipboard");
        true
    }

    fn cut_selection(&mut self) -> bool {
        if !self.copy_selection() {
            return false;
        }
        let _ = self.display.editor_mut().delete_selection();
        self.after_edit(UndoKind::Other);
        self.status("Cut to clipboard");
        true
    }

    fn paste_from_clipboard(&mut self) {
        let Some(contents) = &self.clipboard else {
            self.status("Nothing to paste — use the terminal's paste shortcut instead");
            return;
        };
        match &contents.fragment {
            Some(doc) => {
                let doc = doc.clone();
                let _ = self.display.editor_mut().insert_document(&doc);
            }
            None => {
                let text = contents.text.clone();
                let _ = self.display.editor_mut().insert_text(&text);
            }
        }
        self.after_edit(UndoKind::Other);
    }

    fn paste_text(&mut self, text: &str) {
        let _ = self.display.editor_mut().insert_text(text);
        self.after_edit(UndoKind::Other);
    }

    // ----- cursor movement -------------------------------------------------

    fn move_horizontal(&mut self, right: bool, word: bool, extend: bool) {
        let editor = self.display.editor_mut();
        match (right, word, extend) {
            (true, false, false) => editor.move_cursor_right(),
            (true, false, true) => editor.move_cursor_right_extend(),
            (false, false, false) => editor.move_cursor_left(),
            (false, false, true) => editor.move_cursor_left_extend(),
            (true, true, false) => editor.move_word_right(),
            (true, true, true) => editor.move_word_right_extend(),
            (false, true, false) => editor.move_word_left(),
            (false, true, true) => editor.move_word_left_extend(),
        }
        self.follow_cursor = true;
    }

    fn move_vertical(&mut self, down: bool, extend: bool) {
        self.with_ctx(|d, ctx| {
            if down {
                d.move_cursor_visual_down(extend, ctx);
            } else {
                d.move_cursor_visual_up(extend, ctx);
            }
        });
        self.follow_cursor = true;
    }

    fn move_line_start(&mut self, extend: bool) {
        self.with_ctx(|d, ctx| d.move_cursor_visual_line_start(extend, ctx));
        self.follow_cursor = true;
    }

    fn move_line_end(&mut self, extend: bool) {
        self.with_ctx(|d, ctx| d.move_cursor_visual_line_end_precise(extend, ctx));
        self.follow_cursor = true;
    }

    fn move_page(&mut self, down: bool, extend: bool) {
        let steps = self.last_viewport_height.max(1).saturating_sub(1).max(1);
        self.with_ctx(|d, ctx| {
            for _ in 0..steps {
                if down {
                    d.move_cursor_visual_down(extend, ctx);
                } else {
                    d.move_cursor_visual_up(extend, ctx);
                }
            }
        });
        self.follow_cursor = true;
    }

    fn scroll_by(&mut self, delta: i32) {
        let next = (self.display.scroll_offset() + delta).max(0);
        self.display.set_scroll(next);
        self.follow_cursor = false;
    }

    // ----- links -----------------------------------------------------------

    fn open_link_dialog(&mut self) {
        if let Some(((path, index), url)) = self.display.find_link_near_cursor() {
            self.link_edit = Some(LinkEdit::Existing { path, index });
            self.link_dialog = Some(LinkDialogState::new(String::new(), url, true));
        } else if self.has_selection() {
            let text = self.display.editor().get_selection_text();
            self.link_edit = Some(LinkEdit::Selection);
            self.link_dialog = Some(LinkDialogState::new(text, String::new(), false));
        } else {
            self.link_edit = Some(LinkEdit::Insert);
            self.link_dialog = Some(LinkDialogState::new(String::new(), String::new(), false));
        }
    }

    fn accept_link_dialog(&mut self) {
        let Some(dialog) = self.link_dialog.take() else {
            return;
        };
        let text = dialog.text().to_string();
        let target = dialog.target().to_string();
        let edit = self.link_edit.take();
        let result = match edit {
            Some(LinkEdit::Existing { path, index }) => {
                if target.is_empty() {
                    self.display.editor_mut().remove_link_at(path, index)
                } else {
                    let label = if text.is_empty() { target.clone() } else { text };
                    self.display.editor_mut().edit_link_at(path, index, &target, &label)
                }
            }
            Some(LinkEdit::Selection) => {
                if target.is_empty() {
                    Ok(())
                } else {
                    let label = if text.is_empty() { target.clone() } else { text };
                    self.display.editor_mut().replace_selection_with_link(&target, &label)
                }
            }
            Some(LinkEdit::Insert) | None => {
                if target.is_empty() {
                    Ok(())
                } else {
                    let label = if text.is_empty() { target.clone() } else { text };
                    self.display.editor_mut().insert_link_at_cursor(&target, &label)
                }
            }
        };
        if result.is_ok() {
            self.after_edit(UndoKind::Other);
        }
    }

    fn cancel_link_dialog(&mut self) {
        self.link_dialog = None;
        self.link_edit = None;
    }

    // ----- context menu ----------------------------------------------------

    fn open_context_menu(&mut self) {
        let checklist_state = match self.display.editor().current_block_type() {
            BlockType::ListItem {
                checkbox: Some(checked),
                ..
            } => Some(checked),
            _ => None,
        };
        let block = self.display.editor().current_block_type();
        let can_indent = matches!(block, BlockType::ListItem { .. });
        let allow_paragraph_change = !matches!(block, BlockType::Table { .. });
        let entries = build_context_menu_entries(
            checklist_state,
            self.has_selection(),
            can_indent,
            allow_paragraph_change,
            self.clipboard.is_some(),
        );
        self.context_menu = Some(ContextMenuState::new(entries));
    }

    fn execute_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::SetParagraphType(pt) => self.set_paragraph_type(pt),
            MenuAction::ToggleChecklistItem => self.toggle_checklist_item(),
            MenuAction::ApplyInlineStyle(style) => self.apply_inline_style(style),
            MenuAction::EditLink => {
                self.open_link_dialog();
                return; // keep dialog open
            }
            MenuAction::IndentMore => self.indent(),
            MenuAction::IndentLess => self.unindent(),
            MenuAction::Cut => {
                self.cut_selection();
            }
            MenuAction::Copy => {
                self.copy_selection();
            }
            MenuAction::Paste => self.paste_from_clipboard(),
        }
    }

    // ----- menu bar / app actions -----------------------------------------

    fn execute_app_action(&mut self, action: AppAction) -> Result<()> {
        match action {
            AppAction::New => self.new_document(),
            AppAction::Open => self.open_file_dialog(FileDialogKind::Open),
            AppAction::Save => self.save()?,
            AppAction::SaveAs => self.open_file_dialog(FileDialogKind::SaveAs),
            AppAction::Quit => self.should_quit = true,
            AppAction::Undo => self.undo(),
            AppAction::Redo => self.redo(),
            AppAction::Cut => {
                self.cut_selection();
            }
            AppAction::Copy => {
                self.copy_selection();
            }
            AppAction::Paste => self.paste_from_clipboard(),
            AppAction::InsertLineBreak => self.insert_line_break(),
            AppAction::InsertSiblingParagraph => self.insert_paragraph_break(),
            AppAction::FormattingMenu => self.open_context_menu(),
            AppAction::ToggleRevealCodes => {
                self.status("Reveal codes is not available in this build");
            }
        }
        Ok(())
    }

    // ----- file I/O --------------------------------------------------------

    fn new_document(&mut self) {
        if self.dirty && !self.confirm_new {
            self.confirm_new = true;
            self.status("Unsaved changes — select New again to discard them");
            return;
        }
        self.replace_document(Document::new(), None, DocumentFormat::Ftml);
        self.status("New document");
    }

    fn replace_document(
        &mut self,
        document: Document,
        path: Option<PathBuf>,
        format: DocumentFormat,
    ) {
        self.display.editor_mut().set_tdoc(document);
        self.display.editor_mut().reset_undo_history();
        self.display.set_scroll(0);
        self.file_path = path;
        self.document_format = format;
        self.dirty = false;
        self.confirm_new = false;
        self.follow_cursor = true;
    }

    fn open_file(&mut self, path: PathBuf) {
        match load_document(&path) {
            Ok((document, format, message)) => {
                let label = message.unwrap_or_else(|| format!("Opened {}", path.display()));
                self.replace_document(document, Some(path), format);
                self.status(label);
            }
            Err(err) => self.status(format!("Open failed: {err}")),
        }
    }

    pub fn save(&mut self) -> Result<()> {
        let Some(path) = self.file_path.clone() else {
            self.open_file_dialog(FileDialogKind::SaveAs);
            return Ok(());
        };
        let document = self.display.editor().tdoc();
        let result: Result<()> = (|| {
            match self.document_format {
                DocumentFormat::Ftml => {
                    let text = Writer::new()
                        .write_to_string(document)
                        .map_err(|err| anyhow::anyhow!("{err}"))?;
                    fs::write(&path, text)?;
                }
                DocumentFormat::Markdown => {
                    let mut out: Vec<u8> = Vec::new();
                    markdown::write(&mut out, document).map_err(|err| anyhow::anyhow!("{err}"))?;
                    fs::write(&path, out)?;
                }
                DocumentFormat::Html => {
                    let mut out: Vec<u8> = Vec::new();
                    html::write_document(&mut out, document)
                        .map_err(|err| anyhow::anyhow!("{err}"))?;
                    fs::write(&path, out)?;
                }
                DocumentFormat::Gemini => {
                    let mut out: Vec<u8> = Vec::new();
                    gemini::write(&mut out, document).map_err(|err| anyhow::anyhow!("{err}"))?;
                    fs::write(&path, out)?;
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.dirty = false;
                self.status(format!("Saved {}", path.display()));
                Ok(())
            }
            Err(err) => {
                self.status(format!("Save failed: {err}"));
                Ok(())
            }
        }
    }

    fn save_as(&mut self, path: PathBuf) {
        let previous_path = self.file_path.take();
        let previous_format = self.document_format;
        self.document_format = DocumentFormat::from_path(&path);
        self.file_path = Some(path);
        if self.save().is_err() || self.file_path.is_none() {
            self.file_path = previous_path;
            self.document_format = previous_format;
        }
    }

    fn open_file_dialog(&mut self, kind: FileDialogKind) {
        let initial = match kind {
            // Open starts in the current file's directory; Save As pre-fills the
            // full path so it can be tweaked. Untitled documents start empty.
            FileDialogKind::Open => self
                .file_path
                .as_ref()
                .and_then(|p| p.parent())
                .filter(|d| !d.as_os_str().is_empty())
                .map(|d| format!("{}/", d.display()))
                .unwrap_or_default(),
            FileDialogKind::SaveAs => self
                .file_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        };
        self.file_dialog = Some(FileDialogState::new(kind, initial));
    }

    fn accept_file_dialog(&mut self, path: PathBuf) {
        let Some(dialog) = self.file_dialog.as_mut() else {
            return;
        };
        let kind = dialog.kind();
        // Destructive actions warn once: the first Enter records a pending
        // confirmation, a second Enter on the same path goes through.
        let needs_confirmation = match kind {
            FileDialogKind::Open => self.dirty,
            FileDialogKind::SaveAs => {
                self.file_path.as_deref() != Some(path.as_path()) && path.exists()
            }
        };
        if needs_confirmation && dialog.pending_confirm() != Some(path.as_path()) {
            dialog.set_pending_confirm(path);
            return;
        }

        self.file_dialog = None;
        match kind {
            FileDialogKind::Open => self.open_file(path),
            FileDialogKind::SaveAs => self.save_as(path),
        }
    }

    // ----- drawing ---------------------------------------------------------

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        if area.height == 0 || area.width == 0 {
            return;
        }

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let editor_area = vertical[0];
        let status_area = vertical[1];

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(editor_area);
        let text_area = horizontal[0];
        let scrollbar_area = horizontal[1];

        self.last_text_area = text_area;
        self.last_viewport_height = text_area.height as usize;
        self.last_scrollbar_column = scrollbar_area.x;

        self.display.resize(
            text_area.x as i32,
            text_area.y as i32,
            text_area.width as i32,
            text_area.height as i32,
        );

        let follow = self.follow_cursor;
        let (page_bg, default_fg) = self.palette();
        let mut cursor_pos: Option<(i32, i32)> = None;
        {
            let buf = frame.buffer_mut();
            let mut ctx = RatatuiDrawContext::new(buf, area).with_palette(page_bg, default_fg);
            if follow {
                self.display.ensure_cursor_visible(&mut ctx);
            }
            self.display.draw(&mut ctx);
            cursor_pos = self.display.cursor_screen_position(&mut ctx).or(cursor_pos);
        }

        self.last_total_lines = self.display.content_height().max(0) as usize;

        // Record the cursor's 1-based visual line/column for the status bar.
        if let Some((x, y)) = cursor_pos {
            let ph = self.display.theme().padding_horizontal;
            self.cursor_col = (x - text_area.x as i32 - ph + 1).max(1) as usize;
            self.cursor_line =
                (y - text_area.y as i32 + self.display.scroll_offset() + 1).max(1) as usize;
        }

        self.draw_scrollbar(frame, scrollbar_area);

        let overlay_active = self.context_menu.is_some()
            || self.menu_bar.is_some()
            || self.file_dialog.is_some()
            || self.link_dialog.is_some();

        if !overlay_active
            && let Some((x, y)) = cursor_pos
            && x >= 0
            && y >= 0
        {
            frame.set_cursor_position(Position::new(x as u16, y as u16));
            if self.interactive {
                let cursor_style = if self.has_selection() {
                    SetCursorStyle::BlinkingUnderScore
                } else {
                    SetCursorStyle::DefaultUserShape
                };
                execute!(io::stdout(), cursor_style).ok();
            }
        }

        let status = self.status_line(status_area.width as usize);
        let status_widget = Paragraph::new(status).style(self.theme.status_bar_style());
        frame.render_widget(status_widget, status_area);

        if self.context_menu.is_some() {
            self.render_context_menu(frame, area);
        }
        if self.menu_bar.is_some() {
            self.render_menu_bar(frame, area);
        }
        if self.file_dialog.is_some() {
            self.render_file_dialog(frame, area);
        }
        if self.link_dialog.is_some() {
            self.render_link_dialog(frame, area);
        }
    }

    fn status_line(&mut self, width: usize) -> Line<'static> {
        self.prune_status_message();

        if let Some((message, _)) = &self.status_message {
            return Line::from(format!(" {message}"));
        }

        let name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        let dirty = if self.dirty { "*" } else { "" };
        let format = match self.document_format {
            DocumentFormat::Ftml => "FTML",
            DocumentFormat::Markdown => "Markdown",
            DocumentFormat::Html => "HTML",
            DocumentFormat::Gemini => "Gemini",
        };
        let lines = self.last_total_lines.max(1);
        let block = block_type_label(self.display.editor().current_block_type());
        let pos = format!(" {}:{} ", self.cursor_line, self.cursor_col);

        // The filename keeps its own accent color; the rest inherits the bar style.
        let info = format!("{dirty} · {block} · {lines} lines · {format}");
        let hints = " F10:Menu ^S:Save ^Q:Quit";
        let left_len = pos.chars().count() + name.chars().count() + info.chars().count();

        let mut spans = vec![
            Span::raw(pos),
            Span::styled(name, self.theme.filename_style()),
            Span::raw(info),
        ];
        // Right-align the shortcut hints only when there is room; otherwise drop
        // them so the left half isn't jammed/truncated.
        if left_len + hints.chars().count() <= width {
            spans.push(Span::raw(" ".repeat(width - left_len - hints.chars().count())));
            spans.push(Span::raw(hints));
        }
        Line::from(spans)
    }

    fn scrollbar_geometry(&self) -> Option<ScrollbarGeometry> {
        let viewport = self.last_viewport_height;
        let total = self.last_total_lines;
        if viewport == 0 || total <= viewport {
            return None;
        }
        let knob_size = ((viewport * viewport) / total).clamp(1, viewport);
        let knob_travel = viewport - knob_size;
        let max_scroll = total - viewport;
        let scroll = (self.display.scroll_offset().max(0) as usize).min(max_scroll);
        let knob_start = if max_scroll == 0 {
            0
        } else {
            scroll * knob_travel / max_scroll
        };
        Some(ScrollbarGeometry {
            knob_start,
            knob_size,
        })
    }

    fn draw_scrollbar(&self, frame: &mut Frame, area: Rect) {
        if area.height == 0 || self.last_total_lines <= self.last_viewport_height {
            return;
        }
        let Some(geometry) = self.scrollbar_geometry() else {
            return;
        };
        let knob_end = geometry.knob_start + geometry.knob_size;
        for row in 0..self.last_viewport_height.min(area.height as usize) {
            let y = area.y + row as u16;
            let style = if row >= geometry.knob_start && row < knob_end {
                self.theme
                    .scrollbar_knob_style()
                    .add_modifier(Modifier::REVERSED)
            } else {
                self.theme.scrollbar_track_style()
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(" ", style))),
                Rect::new(area.x, y, 1, 1),
            );
        }
    }

    fn render_file_dialog(&self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.file_dialog else {
            return;
        };
        if area.width < 20 || area.height < 8 {
            return;
        }
        let popup_style = self.theme.menu_style();
        let width = 60.min(area.width.saturating_sub(4));
        let list_rows = dialog.candidates().len().clamp(1, 8) as u16;
        let height = (list_rows + 5).min(area.height.saturating_sub(2));
        let popup_area = Rect::new(
            area.x + (area.width.saturating_sub(width)) / 2,
            area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup_area);

        let title = match dialog.kind() {
            FileDialogKind::Open => "Open File",
            FileDialogKind::SaveAs => "Save As",
        };
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(popup_style)
            .border_style(Style::default().fg(Color::Gray));
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);
        if inner.width < 3 || inner.height < 4 {
            return;
        }

        let input_area = Rect::new(inner.x + 1, inner.y, inner.width - 2, 1);
        let visible = input_area.width as usize;
        let skip = (dialog.cursor() + 1).saturating_sub(visible);
        let shown: String = dialog.input().chars().skip(skip).take(visible).collect();
        frame.render_widget(Paragraph::new(shown).style(popup_style), input_area);
        frame.set_cursor_position(Position::new(
            input_area.x + (dialog.cursor() - skip) as u16,
            input_area.y,
        ));

        let separator = "─".repeat(inner.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                separator,
                Style::default().fg(Color::DarkGray),
            )))
            .style(popup_style),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );

        let list_area = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(3));
        if dialog.candidates().is_empty() {
            frame.render_widget(
                Paragraph::new(" (no matching files)")
                    .style(popup_style.patch(self.theme.menu_disabled_style())),
                list_area,
            );
        } else {
            let items: Vec<ListItem> = dialog
                .candidates()
                .iter()
                .map(|candidate| {
                    let (name, style) = if candidate.is_dir {
                        (
                            format!(" {}/", candidate.name),
                            Style::default().add_modifier(Modifier::BOLD),
                        )
                    } else {
                        (format!(" {}", candidate.name), Style::default())
                    };
                    ListItem::new(Line::from(Span::styled(name, style)))
                })
                .collect();
            let mut list_state = ListState::default();
            list_state.select(dialog.selected());
            let list = List::new(items)
                .highlight_style(self.theme.menu_selected_style())
                .style(popup_style);
            frame.render_stateful_widget(list, list_area, &mut list_state);
        }

        let footer_area = Rect::new(inner.x + 1, inner.y + inner.height - 1, inner.width - 2, 1);
        let footer = if dialog.pending_confirm().is_some() {
            let warning = match dialog.kind() {
                FileDialogKind::Open => "Unsaved changes — press Enter again to discard them",
                FileDialogKind::SaveAs => "File exists — press Enter again to overwrite",
            };
            Span::styled(warning, Style::default().fg(Color::LightYellow))
        } else {
            let action = match dialog.kind() {
                FileDialogKind::Open => "open",
                FileDialogKind::SaveAs => "save",
            };
            Span::styled(
                format!("Tab: complete  Enter: {action}  Esc: cancel"),
                self.theme.menu_disabled_style(),
            )
        };
        frame.render_widget(
            Paragraph::new(Line::from(footer)).style(popup_style),
            footer_area,
        );
    }

    fn render_link_dialog(&self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.link_dialog else {
            return;
        };
        if area.width < 24 || area.height < 9 {
            return;
        }
        let popup_style = self.theme.menu_style();
        let width = 56.min(area.width.saturating_sub(4));
        let height = 9.min(area.height.saturating_sub(2));
        let popup_area = Rect::new(
            area.x + (area.width.saturating_sub(width)) / 2,
            area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup_area);
        let title = if dialog.editing() { "Edit Link" } else { "Insert Link" };
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(popup_style)
            .border_style(Style::default().fg(Color::Gray));
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);
        if inner.width < 8 || inner.height < 5 {
            return;
        }

        let field = |label: &str, value: &str| -> String { format!("{label} {value}") };
        let text_area = Rect::new(inner.x + 1, inner.y + 1, inner.width - 2, 1);
        let target_area = Rect::new(inner.x + 1, inner.y + 3, inner.width - 2, 1);
        frame.render_widget(
            Paragraph::new(field("Text:  ", dialog.text())).style(popup_style),
            text_area,
        );
        frame.render_widget(
            Paragraph::new(field("Target:", dialog.target())).style(popup_style),
            target_area,
        );

        let buttons = match dialog.focus() {
            LinkField::Open => "[ Open ]  Cancel   Save ",
            LinkField::Cancel => "  Open  [ Cancel ] Save ",
            LinkField::Save => "  Open    Cancel  [ Save ]",
            _ => "  Open    Cancel   Save ",
        };
        frame.render_widget(
            Paragraph::new(buttons).style(popup_style),
            Rect::new(inner.x + 1, inner.y + inner.height - 2, inner.width - 2, 1),
        );

        if let Some(cursor) = dialog.active_cursor() {
            let (base, label_len) = match dialog.focus() {
                LinkField::Text => (text_area, 7),
                LinkField::Target => (target_area, 7),
                _ => (text_area, 7),
            };
            frame.set_cursor_position(Position::new(
                base.x + (label_len + cursor) as u16,
                base.y,
            ));
        }
    }

    fn render_menu_bar(&self, frame: &mut Frame, area: Rect) {
        let Some(state) = &self.menu_bar else {
            return;
        };
        if area.width == 0 || area.height == 0 {
            return;
        }
        let bar_style = self.theme.menu_bar_style();
        let bar_area = Rect::new(area.x, area.y, area.width, 1);

        let mut spans = vec![Span::styled(" ", bar_style)];
        for (index, menu) in MENU_BAR.iter().enumerate() {
            let selected = index == state.selected_menu();
            let (text_style, accel_style) = if selected {
                (
                    self.theme.menu_bar_selected_style(),
                    self.theme.menu_bar_selected_accel_style(),
                )
            } else {
                (bar_style, self.theme.menu_bar_accel_style())
            };
            let accel_start = menu
                .title
                .char_indices()
                .nth(menu.accel_index)
                .map(|(byte, ch)| (byte, ch.len_utf8()))
                .unwrap_or((0, 0));
            let before = &menu.title[..accel_start.0];
            let accel = &menu.title[accel_start.0..accel_start.0 + accel_start.1];
            let after = &menu.title[accel_start.0 + accel_start.1..];
            spans.push(Span::styled(" ", text_style));
            spans.push(Span::styled(before, text_style));
            spans.push(Span::styled(accel, accel_style));
            spans.push(Span::styled(after, text_style));
            spans.push(Span::styled(" ", text_style));
        }
        frame.render_widget(Clear, bar_area);
        frame.render_widget(Paragraph::new(Line::from(spans)).style(bar_style), bar_area);

        if let Some(selected_item) = state.dropdown_item() {
            self.render_menu_dropdown(frame, area, state.selected_menu(), selected_item);
        }
    }

    fn render_menu_dropdown(
        &self,
        frame: &mut Frame,
        area: Rect,
        menu_index: usize,
        selected_item: usize,
    ) {
        if area.width < 5 || area.height < 4 {
            return;
        }
        let menu = &MENU_BAR[menu_index];
        let rows: Vec<Option<(String, Option<&'static str>, bool)>> = menu
            .entries
            .iter()
            .map(|entry| match entry {
                MenuBarEntry::Separator => None,
                MenuBarEntry::Item(item) => {
                    Some((item.label.to_string(), item.shortcut, item.action.is_some()))
                }
            })
            .collect();

        let max_label_width = rows
            .iter()
            .flatten()
            .map(|(label, _, _)| label.chars().count())
            .max()
            .unwrap_or(0);
        let max_shortcut_width = rows
            .iter()
            .flatten()
            .filter_map(|(_, shortcut, _)| shortcut.map(|s| s.chars().count()))
            .max()
            .unwrap_or(0);
        let gap_width = if max_shortcut_width > 0 { 2 } else { 0 };
        let content_width = max_label_width + gap_width + max_shortcut_width;

        let width = ((content_width + 4) as u16).min(area.width);
        let height = (menu.entries.len() as u16 + 2).min(area.height.saturating_sub(1));
        let mut x = area.x + menu_title_offset(menu_index) as u16;
        if x + width > area.right() {
            x = area.right().saturating_sub(width);
        }
        let popup_area = Rect::new(x, area.y + 1, width, height);
        frame.render_widget(Clear, popup_area);

        let popup_style = self.theme.menu_style();
        let separator_width = popup_area.width.saturating_sub(2) as usize;
        let mut items = Vec::new();
        for row in &rows {
            match row {
                None => {
                    let line = "─".repeat(separator_width);
                    items.push(ListItem::new(Line::from(Span::styled(
                        line,
                        Style::default().fg(Color::DarkGray),
                    ))));
                }
                Some((label, shortcut, enabled)) => {
                    let content = format!(
                        " {label:<max_label_width$}{gap}{shortcut:>max_shortcut_width$} ",
                        gap = " ".repeat(gap_width),
                        shortcut = shortcut.unwrap_or(""),
                    );
                    let style = if *enabled {
                        Style::default()
                    } else {
                        self.theme.menu_disabled_style()
                    };
                    items.push(ListItem::new(Line::from(Span::styled(content, style))));
                }
            }
        }
        let highlight_style = match rows.get(selected_item) {
            Some(Some((_, _, false))) => self.theme.menu_selected_disabled_style(),
            _ => self.theme.menu_selected_style(),
        };
        let mut list_state = ListState::default();
        list_state.select(Some(selected_item));
        let list = List::new(items)
            .highlight_style(highlight_style)
            .style(popup_style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(popup_style)
                    .border_style(Style::default().fg(Color::Gray)),
            );
        frame.render_stateful_widget(list, popup_area, &mut list_state);
    }

    fn render_context_menu(&self, frame: &mut Frame, area: Rect) {
        let Some(menu) = &self.context_menu else {
            return;
        };
        if area.width < 3 || area.height < 3 {
            return;
        }

        let mut max_label_width = 0usize;
        let mut max_section_width = 0usize;
        let mut has_shortcuts = false;
        let mut max_shortcut_width = 0usize;
        for entry in menu.entries() {
            match entry {
                MenuEntry::Item(item) => {
                    max_label_width = max_label_width.max(item.label.chars().count());
                    if let Some(shortcut) = item.shortcut {
                        has_shortcuts = true;
                        max_shortcut_width = max_shortcut_width.max(shortcut.key.len_utf8());
                    }
                }
                MenuEntry::Section(title) => {
                    max_section_width = max_section_width.max(title.chars().count());
                }
                MenuEntry::Separator => {}
            }
        }

        let gap_width = if has_shortcuts { 2 } else { 0 };
        let shortcut_width = if has_shortcuts { max_shortcut_width.max(1) } else { 0 };
        let item_width = if has_shortcuts {
            max_label_width + gap_width + shortcut_width
        } else {
            max_label_width
        };
        let base_content_width = item_width.max(max_section_width);
        let content_width = base_content_width as u16;
        let min_width = 10.min(area.width);
        let width = (content_width + 4).min(area.width).max(min_width);
        let desired_height = (menu.entries().len() as u16 + 2).min(area.height);
        let height = desired_height.max(3.min(area.height));
        let popup_area = Rect::new(
            area.x + (area.width.saturating_sub(width)) / 2,
            area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup_area);

        let separator_width = popup_area.width.saturating_sub(4).max(4) as usize;
        let popup_style = self.theme.menu_style();
        let mut items = Vec::new();
        let gap = if has_shortcuts { "  " } else { "" };
        for entry in menu.entries() {
            match entry {
                MenuEntry::Section(title) => {
                    items.push(ListItem::new(Line::from(Span::styled(
                        *title,
                        popup_style.add_modifier(Modifier::BOLD),
                    ))));
                }
                MenuEntry::Separator => {
                    let line = "─".repeat(separator_width);
                    items.push(ListItem::new(Line::from(Span::styled(
                        line,
                        Style::default().fg(Color::DarkGray),
                    ))));
                }
                MenuEntry::Item(item) => {
                    let content = if has_shortcuts {
                        let shortcut = item
                            .shortcut
                            .map(|s| s.key.to_string())
                            .unwrap_or_default();
                        format!(
                            "{label:<label_width$}{gap}{shortcut:>shortcut_width$}",
                            label = item.label,
                            label_width = max_label_width,
                        )
                    } else {
                        item.label.to_string()
                    };
                    let style = if item.is_enabled() {
                        Style::default()
                    } else {
                        self.theme.menu_disabled_style()
                    };
                    items.push(ListItem::new(Line::from(Span::styled(content, style))));
                }
            }
        }
        let mut state = ListState::default();
        state.select(Some(menu.selected_index()));
        let highlight_style = match &menu.entries()[menu.selected_index()] {
            MenuEntry::Item(item) if !item.is_enabled() => {
                self.theme.menu_selected_disabled_style()
            }
            _ => self.theme.menu_selected_style(),
        };
        let list = List::new(items)
            .highlight_style(highlight_style)
            .style(popup_style)
            .block(
                Block::default()
                    .title("Context Menu")
                    .borders(Borders::ALL)
                    .style(popup_style)
                    .border_style(Style::default().fg(Color::Gray)),
            );
        frame.render_stateful_widget(list, popup_area, &mut state);
    }

    // ----- event handling --------------------------------------------------

    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) => self.handle_key(key)?,
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) => {
                if self.file_dialog.is_some() || self.link_dialog.is_some() {
                    self.handle_dialog_paste(&text);
                } else {
                    self.paste_text(&text);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_dialog_paste(&mut self, text: &str) {
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        if let Some(dialog) = &mut self.file_dialog {
            dialog.insert_str(&clean);
        } else if let Some(dialog) = &mut self.link_dialog {
            dialog.insert_str(&clean);
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind == KeyEventKind::Release {
            return Ok(());
        }

        // Modal overlays consume keys first.
        if self.file_dialog.is_some() {
            return self.handle_file_dialog_key(key);
        }
        if self.link_dialog.is_some() {
            self.handle_link_dialog_key(key);
            return Ok(());
        }
        if self.context_menu.is_some() {
            self.handle_context_menu_key(key);
            return Ok(());
        }
        if self.menu_bar.is_some() {
            return self.handle_menu_bar_key(key);
        }

        let code = key.code;
        let m = key.modifiers;
        let ctrl = m.contains(KeyModifiers::CONTROL);
        let shift = m.contains(KeyModifiers::SHIFT);
        let alt = m.contains(KeyModifiers::ALT);

        // Menu activation.
        if code == KeyCode::F(10) {
            self.menu_bar = Some(MenuBarState::new());
            return Ok(());
        }
        if alt && let KeyCode::Char(ch) = code {
            if let Some(index) = menu_with_accel(ch.to_ascii_lowercase()) {
                self.menu_bar = Some(MenuBarState::open_at(index));
                return Ok(());
            }
        }

        match (code, ctrl, alt) {
            // File / app
            (KeyCode::Char('q'), true, _) => self.should_quit = true,
            (KeyCode::Char('s'), true, _) => self.save()?,
            (KeyCode::Char('o'), true, _) => self.open_file_dialog(FileDialogKind::Open),
            (KeyCode::Char('n'), true, _) => self.new_document(),
            (KeyCode::Char('z'), true, _) => self.undo(),
            (KeyCode::Char('y'), true, _) => self.redo(),
            (KeyCode::Char('c'), true, _) => {
                self.copy_selection();
            }
            (KeyCode::Char('x'), true, _) => {
                self.cut_selection();
            }
            (KeyCode::Char('v'), true, _) => self.paste_from_clipboard(),
            (KeyCode::Char('k'), true, _) => self.open_link_dialog(),
            // Esc and Ctrl+Space open the formatting/context menu.
            (KeyCode::Char(' '), true, _) => self.open_context_menu(),
            (KeyCode::Esc, _, _) => self.open_context_menu(),
            (KeyCode::Char('p'), true, _) => self.insert_paragraph_break(),
            (KeyCode::Char('j'), true, _) => self.insert_line_break(),

            // Word delete
            (KeyCode::Char('w'), true, _) => self.delete_word_backward(),
            (KeyCode::Backspace, true, _) | (KeyCode::Backspace, _, true) => {
                self.delete_word_backward()
            }
            (KeyCode::Delete, true, _) | (KeyCode::Delete, _, true) => self.delete_word_forward(),

            // Scrolling (no cursor move)
            (KeyCode::Up, true, _) => self.scroll_by(-(self.last_viewport_height as i32).max(1)),
            (KeyCode::Down, true, _) => self.scroll_by((self.last_viewport_height as i32).max(1)),

            // Word movement
            (KeyCode::Left, true, _) => self.move_horizontal(false, true, shift),
            (KeyCode::Right, true, _) => self.move_horizontal(true, true, shift),

            // Emacs-style line nav
            (KeyCode::Char('a'), true, _) => self.move_line_start(false),
            (KeyCode::Char('e'), true, _) => self.move_line_end(false),

            // Editing
            (KeyCode::Enter, false, false) if shift => self.insert_line_break(),
            (KeyCode::Enter, true, _) => self.insert_line_break(),
            (KeyCode::Enter, false, false) => self.insert_paragraph_break(),
            (KeyCode::Backspace, false, false) => self.backspace(),
            (KeyCode::Delete, false, false) => self.delete_forward(),
            (KeyCode::Tab, false, false) => self.tab(false),
            (KeyCode::BackTab, _, _) => self.tab(true),

            // Plain movement
            (KeyCode::Left, false, false) => self.move_horizontal(false, false, shift),
            (KeyCode::Right, false, false) => self.move_horizontal(true, false, shift),
            (KeyCode::Up, false, false) => self.move_vertical(false, shift),
            (KeyCode::Down, false, false) => self.move_vertical(true, shift),
            (KeyCode::Home, false, false) => self.move_line_start(shift),
            (KeyCode::End, false, false) => self.move_line_end(shift),
            (KeyCode::PageUp, false, false) => self.move_page(false, shift),
            (KeyCode::PageDown, false, false) => self.move_page(true, shift),

            (KeyCode::Char(ch), false, false) => self.insert_char(ch),
            (KeyCode::Char(ch), false, _) if shift => self.insert_char(ch),
            _ => {}
        }
        Ok(())
    }

    fn tab(&mut self, back: bool) {
        let in_list = matches!(
            self.display.editor().current_block_type(),
            BlockType::ListItem { .. }
        );
        if in_list {
            if back {
                self.unindent();
            } else {
                self.indent();
            }
        } else if !back {
            self.insert_str("    ");
        }
    }

    fn handle_context_menu_key(&mut self, key: KeyEvent) {
        let code = key.code;
        let m = key.modifiers;
        // Toggle/close shortcuts.
        if is_context_menu_shortcut(code, m) {
            self.context_menu = None;
            return;
        }
        match code {
            KeyCode::Up => {
                if let Some(menu) = &mut self.context_menu {
                    menu.move_selection(-1);
                }
            }
            KeyCode::Down => {
                if let Some(menu) = &mut self.context_menu {
                    menu.move_selection(1);
                }
            }
            KeyCode::Enter => {
                let action = self.context_menu.as_ref().and_then(|m| m.current_action());
                self.context_menu = None;
                if let Some(action) = action {
                    self.execute_menu_action(action);
                }
            }
            _ => {
                let result = self
                    .context_menu
                    .as_mut()
                    .map(|menu| menu.shortcut_action(code, m));
                if let Some((matched, action)) = result
                    && matched
                {
                    self.context_menu = None;
                    if let Some(action) = action {
                        self.execute_menu_action(action);
                    }
                }
            }
        }
    }

    fn handle_menu_bar_key(&mut self, key: KeyEvent) -> Result<()> {
        let code = key.code;
        match code {
            KeyCode::Esc | KeyCode::F(10) => self.menu_bar = None,
            KeyCode::Left => {
                if let Some(state) = &mut self.menu_bar {
                    state.move_menu(-1);
                }
            }
            KeyCode::Right => {
                if let Some(state) = &mut self.menu_bar {
                    state.move_menu(1);
                }
            }
            KeyCode::Up => {
                if let Some(state) = &mut self.menu_bar {
                    state.move_item(-1);
                }
            }
            KeyCode::Down => {
                if let Some(state) = &mut self.menu_bar {
                    if state.dropdown_item().is_none() {
                        state.open_dropdown();
                    } else {
                        state.move_item(1);
                    }
                }
            }
            KeyCode::Enter => {
                let action = self.menu_bar.as_ref().and_then(|s| s.selected_action());
                if let Some(action) = action {
                    self.menu_bar = None;
                    self.execute_app_action(action)?;
                } else if let Some(state) = &mut self.menu_bar {
                    state.open_dropdown();
                }
            }
            KeyCode::Char(ch) => {
                if let Some(index) = menu_with_accel(ch.to_ascii_lowercase()) {
                    if let Some(state) = &mut self.menu_bar {
                        state.select_menu(index);
                        state.open_dropdown();
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_file_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        let code = key.code;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (code, ctrl) {
            (KeyCode::Esc, _) => self.file_dialog = None,
            (KeyCode::Enter, _) => {
                let result = self.file_dialog.as_mut().map(|d| d.enter());
                if let Some(FileDialogResult::Accept(path)) = result {
                    self.accept_file_dialog(path);
                }
            }
            (KeyCode::Tab, _) => {
                if let Some(d) = &mut self.file_dialog {
                    d.complete();
                }
            }
            (KeyCode::Up, _) => {
                if let Some(d) = &mut self.file_dialog {
                    d.move_selection(-1);
                }
            }
            (KeyCode::Down, _) => {
                if let Some(d) = &mut self.file_dialog {
                    d.move_selection(1);
                }
            }
            (KeyCode::Left, _) => {
                if let Some(d) = &mut self.file_dialog {
                    d.move_cursor_left();
                }
            }
            (KeyCode::Right, _) => {
                if let Some(d) = &mut self.file_dialog {
                    d.move_cursor_right();
                }
            }
            (KeyCode::Char('a'), true) => {
                if let Some(d) = &mut self.file_dialog {
                    d.move_cursor_start();
                }
            }
            (KeyCode::Char('e'), true) => {
                if let Some(d) = &mut self.file_dialog {
                    d.move_cursor_end();
                }
            }
            (KeyCode::Char('w'), true) => {
                if let Some(d) = &mut self.file_dialog {
                    d.delete_word_backward();
                }
            }
            (KeyCode::Backspace, _) => {
                if let Some(d) = &mut self.file_dialog {
                    d.backspace();
                }
            }
            (KeyCode::Delete, _) => {
                if let Some(d) = &mut self.file_dialog {
                    d.delete();
                }
            }
            (KeyCode::Char(ch), false) => {
                if !ch.is_control()
                    && let Some(d) = &mut self.file_dialog
                {
                    d.insert_char(ch);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_link_dialog_key(&mut self, key: KeyEvent) {
        let code = key.code;
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match code {
            KeyCode::Esc => self.cancel_link_dialog(),
            KeyCode::Enter => self.accept_link_dialog(),
            KeyCode::Tab | KeyCode::Down => {
                if let Some(d) = &mut self.link_dialog {
                    d.focus_next();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(d) = &mut self.link_dialog {
                    d.focus_prev();
                }
            }
            KeyCode::Char(' ') if self.link_field_is_button() => self.activate_link_button(),
            KeyCode::Left => {
                if let Some(d) = &mut self.link_dialog {
                    d.move_cursor_left();
                }
            }
            KeyCode::Right => {
                if let Some(d) = &mut self.link_dialog {
                    d.move_cursor_right();
                }
            }
            KeyCode::Char('a') if ctrl => {
                if let Some(d) = &mut self.link_dialog {
                    d.move_cursor_start();
                }
            }
            KeyCode::Char('e') if ctrl => {
                if let Some(d) = &mut self.link_dialog {
                    d.move_cursor_end();
                }
            }
            KeyCode::Char('w') if ctrl => {
                if let Some(d) = &mut self.link_dialog {
                    d.delete_word_backward();
                }
            }
            KeyCode::Backspace => {
                if let Some(d) = &mut self.link_dialog {
                    d.backspace();
                }
            }
            KeyCode::Delete => {
                if let Some(d) = &mut self.link_dialog {
                    d.delete();
                }
            }
            KeyCode::Char(ch) => {
                if (!ch.is_control() || shift)
                    && let Some(d) = &mut self.link_dialog
                {
                    d.insert_char(ch);
                }
            }
            _ => {}
        }
    }

    fn link_field_is_button(&self) -> bool {
        self.link_dialog
            .as_ref()
            .map(|d| d.focus().is_button())
            .unwrap_or(false)
    }

    fn activate_link_button(&mut self) {
        let Some(dialog) = &self.link_dialog else {
            return;
        };
        match dialog.focus() {
            LinkField::Open => {
                let target = dialog.target().to_string();
                if target.is_empty() {
                    self.status("No link target to open");
                } else {
                    if self.interactive {
                        open_in_browser(&target).ok();
                    }
                    self.status(format!("Opening {target}…"));
                }
            }
            LinkField::Cancel => self.cancel_link_dialog(),
            LinkField::Save => self.accept_link_dialog(),
            _ => {}
        }
    }

    // ----- mouse -----------------------------------------------------------

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        // Overlays ignore mouse for now.
        if self.file_dialog.is_some() || self.link_dialog.is_some() {
            return;
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_by(-MOUSE_SCROLL_LINES),
            MouseEventKind::ScrollDown => self.scroll_by(MOUSE_SCROLL_LINES),
            MouseEventKind::Down(MouseButton::Left) => self.mouse_down(mouse.column, mouse.row),
            MouseEventKind::Drag(MouseButton::Left) => self.mouse_drag(mouse.column, mouse.row),
            MouseEventKind::Up(MouseButton::Left) => self.drag_state = None,
            _ => {}
        }
    }

    fn mouse_down(&mut self, column: u16, row: u16) {
        // Scrollbar?
        if column == self.last_scrollbar_column
            && let Some(geometry) = self.scrollbar_geometry()
        {
            let rel = row.saturating_sub(self.last_text_area.y) as usize;
            let anchor = rel.saturating_sub(geometry.knob_start);
            self.drag_state = Some(DragState::Scrollbar {
                anchor_within_knob: anchor.min(geometry.knob_size.saturating_sub(1)),
            });
            return;
        }

        let Some(pos) = self.position_at(column, row) else {
            return;
        };

        let now = Instant::now();
        let same_spot = self.last_click_position == Some((column, row))
            && self.last_click_button == Some(MouseButton::Left)
            && self
                .last_click_instant
                .map(|t| now.duration_since(t) < DOUBLE_CLICK_TIMEOUT)
                .unwrap_or(false);
        self.mouse_click_count = if same_spot {
            (self.mouse_click_count % 3) + 1
        } else {
            1
        };
        self.last_click_instant = Some(now);
        self.last_click_position = Some((column, row));
        self.last_click_button = Some(MouseButton::Left);

        match self.mouse_click_count {
            2 => self.display.editor_mut().select_word_at(pos),
            3 => self.display.editor_mut().select_line_at(pos),
            _ => {
                self.display.editor_mut().set_cursor(pos);
                self.drag_state = Some(DragState::Text);
            }
        }
        self.follow_cursor = false;
    }

    fn mouse_drag(&mut self, column: u16, row: u16) {
        match self.drag_state.clone() {
            Some(DragState::Scrollbar { anchor_within_knob }) => {
                self.scrollbar_drag(row, anchor_within_knob);
            }
            Some(DragState::Text) => {
                if let Some(pos) = self.position_at(column, row) {
                    self.display.editor_mut().extend_selection_to(pos);
                }
            }
            None => {}
        }
    }

    fn scrollbar_drag(&mut self, row: u16, anchor_within_knob: usize) {
        let Some(geometry) = self.scrollbar_geometry() else {
            return;
        };
        let viewport = self.last_viewport_height;
        let total = self.last_total_lines;
        if viewport == 0 || total <= viewport {
            return;
        }
        let rel = row.saturating_sub(self.last_text_area.y) as usize;
        let knob_travel = viewport.saturating_sub(geometry.knob_size).max(1);
        let new_start = rel.saturating_sub(anchor_within_knob).min(knob_travel);
        let max_scroll = total - viewport;
        let scroll = new_start * max_scroll / knob_travel;
        self.display.set_scroll(scroll as i32);
        self.follow_cursor = false;
    }

    /// Map a terminal cell to a document position via the engine (widget-relative
    /// coordinates). Returns `None` for clicks outside the editor text area.
    fn position_at(&mut self, column: u16, row: u16) -> Option<DocumentPosition> {
        let area = self.last_text_area;
        if column < area.x
            || column >= area.x + area.width
            || row < area.y
            || row >= area.y + area.height
        {
            return None;
        }
        let x = (column - area.x) as i32;
        let y = (row - area.y) as i32;
        Some(self.display.xy_to_position(x, y))
    }
}

fn block_type_label(block: BlockType) -> &'static str {
    match block {
        BlockType::Paragraph => "Text",
        BlockType::Heading { level: 1 } => "Heading 1",
        BlockType::Heading { level: 2 } => "Heading 2",
        BlockType::Heading { .. } => "Heading 3",
        BlockType::CodeBlock { .. } => "Code",
        BlockType::BlockQuote => "Quote",
        BlockType::ListItem {
            checkbox: Some(_), ..
        } => "Checklist",
        BlockType::ListItem { ordered: true, .. } => "Numbered List",
        BlockType::ListItem { .. } => "Bullet List",
        BlockType::Table { .. } => "Table",
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod app_tests;
