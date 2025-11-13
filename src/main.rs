use std::{
    env, fs, io,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};
use tdoc::{Document, ParagraphType, parse, writer::Writer};

mod editor;
mod render;

use editor::{CursorPointer, DocumentEditor};
use render::{CursorVisualPosition, RenderResult, render_document};

const SENTINEL: char = '\u{F8FF}';
const STATUS_TIMEOUT: Duration = Duration::from_secs(4);

fn main() -> Result<()> {
    run()
}

fn column_distance(a: u16, b: u16) -> u16 {
    if a >= b { a - b } else { b - a }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(path_arg) = args.next() else {
        eprintln!("Usage: cargo run -- <file.ftml>");
        return Ok(());
    };
    let path = PathBuf::from(path_arg);

    let (document, initial_status) = load_document(&path)?;
    let mut app = App::new(document, path, initial_status);

    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal backend")?;
    terminal.clear().ok();

    let res = run_app(&mut terminal, &mut app).context("application error");

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    res
}

fn load_document(path: &PathBuf) -> Result<(Document, Option<String>)> {
    if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        match parse(std::io::Cursor::new(content)) {
            Ok(doc) => Ok((doc, None)),
            Err(err) => {
                let message = format!("Parse error: {err}. Starting with empty document.");
                Ok((Document::new(), Some(message)))
            }
        }
    } else {
        Ok((Document::new(), Some("New document".to_string())))
    }
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    while !app.should_quit() {
        terminal
            .draw(|frame| app.draw(frame))
            .context("failed to draw frame")?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout).context("event poll failed")? {
            let evt = event::read().context("failed to read event")?;
            app.handle_event(evt)?;
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
enum MenuAction {
    SetParagraphType(ParagraphType),
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
    const fn enabled_with_shortcut(
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

    const fn disabled_with_shortcut(label: &'static str, shortcut: MenuShortcut) -> Self {
        Self {
            label,
            action: None,
            shortcut: Some(shortcut),
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
    fn new() -> Self {
        let entries = build_context_menu_entries();
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
            if let MenuEntry::Item(item) = entry {
                if let Some(shortcut) = item.shortcut {
                    if shortcut.matches(code, modifiers) {
                        self.selected_index = idx;
                        return (true, item.action);
                    }
                }
            }
        }
        (false, None)
    }
}

fn build_context_menu_entries() -> Vec<MenuEntry> {
    vec![
        MenuEntry::Section("Paragraph type"),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Text",
            MenuAction::SetParagraphType(ParagraphType::Text),
            MenuShortcut::new('0'),
        )),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Heading 1",
            MenuAction::SetParagraphType(ParagraphType::Header1),
            MenuShortcut::new('1'),
        )),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Heading 2",
            MenuAction::SetParagraphType(ParagraphType::Header2),
            MenuShortcut::new('2'),
        )),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Heading 3",
            MenuAction::SetParagraphType(ParagraphType::Header3),
            MenuShortcut::new('3'),
        )),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Quote",
            MenuAction::SetParagraphType(ParagraphType::Quote),
            MenuShortcut::new('5'),
        )),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Code",
            MenuAction::SetParagraphType(ParagraphType::CodeBlock),
            MenuShortcut::new('6'),
        )),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Numbered List",
            MenuAction::SetParagraphType(ParagraphType::OrderedList),
            MenuShortcut::new('7'),
        )),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Bullet List",
            MenuAction::SetParagraphType(ParagraphType::UnorderedList),
            MenuShortcut::new('8'),
        )),
        MenuEntry::Item(MenuItem::enabled_with_shortcut(
            "Checklist",
            MenuAction::SetParagraphType(ParagraphType::Checklist),
            MenuShortcut::new('9'),
        )),
        MenuEntry::Separator,
        MenuEntry::Section("Inline style"),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Bold",
            MenuShortcut::new('b'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Italic",
            MenuShortcut::new('i'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Underline",
            MenuShortcut::new('u'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Code",
            MenuShortcut::with_shift('C'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Highlight",
            MenuShortcut::with_shift('H'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Strikethrough",
            MenuShortcut::with_shift('X'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Edit Link...",
            MenuShortcut::new('k'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Clear Formatting",
            MenuShortcut::new('\\'),
        )),
        MenuEntry::Separator,
        MenuEntry::Section("Copy & paste"),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Cut",
            MenuShortcut::new('x'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Copy",
            MenuShortcut::new('c'),
        )),
        MenuEntry::Item(MenuItem::disabled_with_shortcut(
            "Paste",
            MenuShortcut::new('v'),
        )),
    ]
}

fn is_context_menu_shortcut(code: KeyCode, modifiers: KeyModifiers) -> bool {
    match code {
        KeyCode::Esc => modifiers.is_empty(),
        KeyCode::Char(' ') | KeyCode::Char('p') => modifiers.contains(KeyModifiers::CONTROL),
        _ => false,
    }
}

struct App {
    editor: DocumentEditor,
    file_path: PathBuf,
    scroll_top: usize,
    last_view_height: usize,
    should_quit: bool,
    dirty: bool,
    status_message: Option<(String, Instant)>,
    visual_positions: Vec<CursorDisplay>,
    last_cursor_visual: Option<CursorVisualPosition>,
    preferred_column: Option<u16>,
    context_menu: Option<ContextMenuState>,
}

impl App {
    fn new(document: Document, path: PathBuf, initial_status: Option<String>) -> Self {
        let mut editor = DocumentEditor::new(document);
        editor.ensure_cursor_selectable();

        Self {
            editor,
            file_path: path,
            scroll_top: 0,
            last_view_height: 1,
            should_quit: false,
            dirty: false,
            status_message: initial_status.map(|msg| (msg, Instant::now())),
            visual_positions: Vec::new(),
            last_cursor_visual: None,
            preferred_column: None,
            context_menu: None,
        }
    }

    fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        if area.height == 0 || area.width == 0 {
            return;
        }

        let status_height = if area.height > 1 { 2 } else { 1 };
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(status_height)])
            .split(area);

        let editor_area = vertical[0];
        let status_area = vertical[1];

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(editor_area);
        let text_area = horizontal[0];
        let scrollbar_area = horizontal[1];

        let render = self.render_document(text_area.width.max(1) as usize);

        self.visual_positions = render
            .cursor_map
            .iter()
            .cloned()
            .map(|(pointer, position)| CursorDisplay { pointer, position })
            .collect();
        let cursor_visual = render.cursor;
        self.last_cursor_visual = cursor_visual;
        if self.preferred_column.is_none() {
            self.preferred_column = cursor_visual.map(|p| p.column);
        }

        let viewport_height = text_area.height as usize;
        self.last_view_height = viewport_height.max(1);
        self.adjust_scroll(&render, viewport_height);

        let paragraph = Paragraph::new(Text::from(render.lines.clone()))
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::NONE))
            .scroll((self.scroll_top as u16, 0));
        frame.render_widget(paragraph, text_area);

        let mut scrollbar_state = ScrollbarState::new(render.total_lines).position(self.scroll_top);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);

        if let Some(cursor) = cursor_visual {
            if cursor.line >= self.scroll_top
                && cursor.line < self.scroll_top + viewport_height
                && text_area.width > 0
            {
                let cursor_y = text_area.y + (cursor.line - self.scroll_top) as u16;
                let cursor_x = text_area.x + cursor.column.min(text_area.width - 1);
                frame.set_cursor_position(Position::new(cursor_x, cursor_y));
            }
        }

        let status_text = self.status_line(render.total_lines);
        let status_widget = Paragraph::new(Line::from(Span::styled(status_text, Style::default())))
            .block(Block::default().borders(Borders::TOP));
        frame.render_widget(status_widget, status_area);

        if self.context_menu.is_some() {
            self.render_context_menu(frame, area);
        }
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
        let shortcut_width = if has_shortcuts {
            max_shortcut_width.max(1)
        } else {
            0
        };
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
        let popup_style = Style::default().bg(Color::Black).fg(Color::White);

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
                        if let Some(shortcut) = item.shortcut {
                            format!(
                                "{label:<label_width$}{gap}{shortcut:>shortcut_width$}",
                                label = item.label,
                                label_width = max_label_width,
                                gap = gap,
                                shortcut = shortcut.key,
                                shortcut_width = shortcut_width,
                            )
                        } else {
                            format!(
                                "{label:<label_width$}{gap}{empty:>shortcut_width$}",
                                label = item.label,
                                label_width = max_label_width,
                                gap = gap,
                                empty = "",
                                shortcut_width = shortcut_width,
                            )
                        }
                    } else {
                        item.label.to_string()
                    };
                    let style = if item.is_enabled() {
                        Style::default()
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    items.push(ListItem::new(Line::from(Span::styled(content, style))));
                }
            }
        }

        let mut state = ListState::default();
        state.select(Some(menu.selected_index()));

        let list = List::new(items)
            .highlight_style(Style::default().bg(Color::White).fg(Color::Black))
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

    fn open_context_menu(&mut self) {
        self.context_menu = Some(ContextMenuState::new());
    }

    fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    fn handle_context_menu_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        if self.context_menu.is_none() {
            return false;
        }

        match code {
            KeyCode::Esc => {
                self.close_context_menu();
                true
            }
            KeyCode::Up => {
                if let Some(menu) = self.context_menu.as_mut() {
                    menu.move_selection(-1);
                }
                true
            }
            KeyCode::Down => {
                if let Some(menu) = self.context_menu.as_mut() {
                    menu.move_selection(1);
                }
                true
            }
            KeyCode::Enter => {
                if let Some(action) = self
                    .context_menu
                    .as_ref()
                    .and_then(|menu| menu.current_action())
                {
                    if self.execute_menu_action(action) {
                        self.close_context_menu();
                    }
                }
                true
            }
            KeyCode::Char(' ') | KeyCode::Char('p')
                if modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.close_context_menu();
                true
            }
            KeyCode::Char(_) => {
                if let Some(menu) = self.context_menu.as_mut() {
                    let (handled, action) = menu.shortcut_action(code, modifiers);
                    if handled {
                        if let Some(action) = action {
                            if self.execute_menu_action(action) {
                                self.close_context_menu();
                            }
                        }
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn execute_menu_action(&mut self, action: MenuAction) -> bool {
        match action {
            MenuAction::SetParagraphType(kind) => {
                if self.editor.set_paragraph_type(kind) {
                    self.mark_dirty();
                    self.preferred_column = None;
                }
                true
            }
        }
    }

    fn status_line(&mut self, total_lines: usize) -> String {
        self.prune_status_message();
        let cursor_details = self.cursor_status_text();
        if let Some((message, _)) = &self.status_message {
            return format!("{cursor_details} | {message}");
        }

        let marker = if self.dirty { "*" } else { "" };
        format!(
            "{} | {}{} | Lines: {} | Ctrl-S save | Ctrl-Q quit",
            cursor_details,
            self.file_path.display(),
            marker,
            total_lines
        )
    }

    fn prune_status_message(&mut self) {
        if let Some((_, instant)) = &self.status_message {
            if instant.elapsed() > STATUS_TIMEOUT {
                self.status_message = None;
            }
        }
    }

    fn adjust_scroll(&mut self, render: &RenderResult, viewport_height: usize) {
        let viewport = viewport_height.max(1);
        let max_scroll = render
            .total_lines
            .saturating_sub(viewport)
            .min(render.total_lines);
        if self.scroll_top > max_scroll {
            self.scroll_top = max_scroll;
        }
        if let Some(cursor) = &render.cursor {
            if cursor.line < self.scroll_top {
                self.scroll_top = cursor.line;
            } else if cursor.line >= self.scroll_top + viewport_height {
                let target = cursor.line.saturating_add(1);
                self.scroll_top = target.saturating_sub(viewport);
            }
        }
        if self.scroll_top > max_scroll {
            self.scroll_top = max_scroll;
        }
    }

    fn move_cursor_vertical(&mut self, delta: i32) {
        if self.visual_positions.is_empty() {
            return;
        }

        let pointer = self.editor.cursor_pointer();
        let current_position = self
            .visual_positions
            .iter()
            .find(|entry| entry.pointer == pointer)
            .map(|entry| entry.position)
            .or(self.last_cursor_visual);

        let Some(current) = current_position else {
            return;
        };

        let desired_column = self.preferred_column.unwrap_or(current.column);

        let max_line = self
            .visual_positions
            .iter()
            .map(|entry| entry.position.line)
            .max()
            .unwrap_or(0);

        let mut target_line = current.line as i32 + delta;
        if target_line < 0 {
            target_line = 0;
        } else if target_line > max_line as i32 {
            target_line = max_line as i32;
        }

        let target_line_usize = target_line as usize;

        let destination = self
            .closest_pointer_on_line(target_line_usize, desired_column)
            .or_else(|| self.search_nearest_line(target_line_usize, delta, desired_column));

        if let Some(dest) = destination {
            if self.editor.move_to_pointer(&dest.pointer) {
                self.preferred_column = Some(desired_column);
                self.last_cursor_visual = Some(dest.position);
            } else {
                self.last_cursor_visual = Some(dest.position);
            }
        }
    }

    fn move_to_visual_line_start(&mut self) {
        self.preferred_column = None;

        if self.visual_positions.is_empty() {
            self.editor.move_to_segment_start();
            return;
        }

        let pointer = self.editor.cursor_pointer();
        let current = self
            .visual_positions
            .iter()
            .find(|entry| entry.pointer == pointer)
            .map(|entry| entry.position)
            .or(self.last_cursor_visual);

        let Some(current_position) = current else {
            self.editor.move_to_segment_start();
            return;
        };

        let destination = self
            .visual_positions
            .iter()
            .filter(|entry| entry.position.line == current_position.line)
            .cloned()
            .min_by_key(|entry| {
                (
                    entry.position.content_column as usize,
                    entry.position.column as usize,
                    entry.pointer.offset,
                )
            });

        if let Some(target) = destination {
            if self.editor.move_to_pointer(&target.pointer) {
                self.last_cursor_visual = Some(target.position);
            } else {
                self.last_cursor_visual = Some(target.position);
            }
        } else {
            self.editor.move_to_segment_start();
        }
    }

    fn move_to_visual_line_end(&mut self) {
        self.preferred_column = None;

        if self.visual_positions.is_empty() {
            self.editor.move_to_segment_end();
            return;
        }

        let pointer = self.editor.cursor_pointer();
        let current = self
            .visual_positions
            .iter()
            .find(|entry| entry.pointer == pointer)
            .map(|entry| entry.position)
            .or(self.last_cursor_visual);

        let Some(current_position) = current else {
            self.editor.move_to_segment_end();
            return;
        };

        let destination = self
            .visual_positions
            .iter()
            .filter(|entry| entry.position.line == current_position.line)
            .cloned()
            .max_by_key(|entry| {
                (
                    entry.position.content_column as usize,
                    entry.position.column as usize,
                    entry.pointer.offset,
                )
            });

        if let Some(target) = destination {
            if self.editor.move_to_pointer(&target.pointer) {
                self.last_cursor_visual = Some(target.position);
            } else {
                self.last_cursor_visual = Some(target.position);
            }
        } else {
            self.editor.move_to_segment_end();
        }
    }

    fn closest_pointer_on_line(&self, line: usize, column: u16) -> Option<CursorDisplay> {
        self.visual_positions
            .iter()
            .filter(|entry| entry.position.line == line)
            .min_by_key(|entry| column_distance(entry.position.column, column))
            .cloned()
    }

    fn search_nearest_line(
        &self,
        start_line: usize,
        delta: i32,
        column: u16,
    ) -> Option<CursorDisplay> {
        if delta == 0 {
            return None;
        }
        let max_line = self
            .visual_positions
            .iter()
            .map(|entry| entry.position.line)
            .max()
            .unwrap_or(0);

        let mut distance = 1usize;
        loop {
            if delta < 0 {
                if let Some(line) = start_line.checked_sub(distance) {
                    if let Some(found) = self.closest_pointer_on_line(line, column) {
                        return Some(found);
                    }
                } else {
                    break;
                }
            } else {
                let line = start_line + distance;
                if line > max_line {
                    break;
                }
                if let Some(found) = self.closest_pointer_on_line(line, column) {
                    return Some(found);
                }
            }

            if distance > max_line.saturating_add(1) {
                break;
            }
            distance += 1;
        }

        None
    }

    fn insert_paragraph_break(&mut self) -> bool {
        self.editor.insert_paragraph_break()
    }

    fn render_document(&self, width: usize) -> RenderResult {
        let (clone, markers, _) = self.editor.clone_with_markers(SENTINEL);
        render_document(&clone, width, &markers, SENTINEL)
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            if self.handle_context_menu_key(code, modifiers) {
                return Ok(());
            }

            if self.context_menu.is_some() {
                return Ok(());
            }

            if is_context_menu_shortcut(code, modifiers) {
                if self.context_menu.is_some() {
                    self.close_context_menu();
                } else {
                    self.open_context_menu();
                }
                return Ok(());
            }

            match (code, modifiers) {
                (KeyCode::Char('q'), m) if m.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true;
                }
                (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
                    self.save()?;
                }
                (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true;
                }
                (KeyCode::Left, _) => {
                    if self.editor.move_left() {
                        self.preferred_column = None;
                    }
                }
                (KeyCode::Right, _) => {
                    if self.editor.move_right() {
                        self.preferred_column = None;
                    }
                }
                (KeyCode::Home, _) => {
                    self.move_to_visual_line_start();
                }
                (KeyCode::End, _) => {
                    self.move_to_visual_line_end();
                }
                (KeyCode::Char('a'), m) if m.contains(KeyModifiers::CONTROL) => {
                    self.move_to_visual_line_start();
                }
                (KeyCode::Char('e'), m) if m.contains(KeyModifiers::CONTROL) => {
                    self.move_to_visual_line_end();
                }
                (KeyCode::Backspace, _) => {
                    if self.editor.backspace() {
                        self.mark_dirty();
                        self.preferred_column = None;
                    }
                }
                (KeyCode::Delete, _) => {
                    if self.editor.delete() {
                        self.mark_dirty();
                        self.preferred_column = None;
                    }
                }
                (KeyCode::Enter, m) => {
                    if m.contains(KeyModifiers::SHIFT) || m.contains(KeyModifiers::CONTROL) {
                        if self.editor.insert_char('\n') {
                            self.mark_dirty();
                            self.preferred_column = None;
                        }
                    } else {
                        if self.insert_paragraph_break() {
                            self.mark_dirty();
                            self.preferred_column = None;
                        }
                    }
                }
                (KeyCode::Tab, _) => {
                    if self.editor.insert_char('\t') {
                        self.mark_dirty();
                        self.preferred_column = None;
                    }
                }
                (KeyCode::Char('j'), m) if m.contains(KeyModifiers::CONTROL) => {
                    if self.editor.insert_char('\n') {
                        self.mark_dirty();
                        self.preferred_column = None;
                    }
                }
                (KeyCode::Char(ch), m)
                    if !m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::ALT) =>
                {
                    if self.editor.insert_char(ch) {
                        self.mark_dirty();
                        self.preferred_column = None;
                    }
                }
                (KeyCode::Up, m) if m.contains(KeyModifiers::CONTROL) => {
                    self.scroll_top = self.scroll_top.saturating_sub(self.last_view_height);
                }
                (KeyCode::Up, _) => {
                    self.move_cursor_vertical(-1);
                }
                (KeyCode::Down, m) if m.contains(KeyModifiers::CONTROL) => {
                    self.scroll_top += self.last_view_height;
                }
                (KeyCode::Down, _) => {
                    self.move_cursor_vertical(1);
                }
                (KeyCode::PageUp, _) => {
                    self.scroll_top = self.scroll_top.saturating_sub(self.last_view_height.max(1));
                }
                (KeyCode::PageDown, _) => {
                    self.scroll_top += self.last_view_height.max(1);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn on_tick(&mut self) {
        self.prune_status_message();
    }

    fn save(&mut self) -> Result<()> {
        let writer = Writer::new();
        let contents = writer
            .write_to_string(self.editor.document())
            .context("failed to render FTML")?;
        fs::write(&self.file_path, contents)
            .with_context(|| format!("failed to write {}", self.file_path.display()))?;

        self.dirty = false;
        self.status_message = Some(("Saved".to_string(), Instant::now()));
        Ok(())
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn cursor_status_text(&self) -> String {
        let position_text = if let Some(position) = self.last_cursor_visual {
            let line = position.content_line + 1;
            let column = usize::from(position.content_column) + 1;
            format!("[{},{}]", line, column)
        } else {
            "[?,?]".to_string()
        };
        let mut parts = vec![position_text];
        if let Some(labels) = self.editor.cursor_breadcrumbs() {
            if !labels.is_empty() {
                parts.push(labels.join(" > "));
            }
        }
        parts.join(" ")
    }
}

#[derive(Clone)]
struct CursorDisplay {
    pointer: CursorPointer,
    position: CursorVisualPosition,
}
