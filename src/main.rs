use std::{
    collections::HashMap,
    env,
    fs,
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame, Terminal,
};
use tdoc::{
    formatter::{Formatter, FormattingStyle},
    parse,
    writer::Writer,
    Document,
};

mod ansi;
mod editor;

use ansi::{parse_ansi, CursorVisualPosition, ParseResult};
use editor::{CursorPointer, DocumentEditor};

const SENTINEL: char = '\u{F8FF}';
const STATUS_TIMEOUT: Duration = Duration::from_secs(4);

fn main() -> Result<()> {
    run()
}

fn column_distance(a: u16, b: u16) -> u16 {
    if a >= b {
        a - b
    } else {
        b - a
    }
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

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
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

        let render = self.render_document(text_area.width.max(1) as usize);

        self.visual_positions = render.cursor_map.clone();
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

        let mut scrollbar_state =
            ScrollbarState::new(render.total_lines).position(self.scroll_top);
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
        let status_widget =
            Paragraph::new(Line::from(Span::styled(status_text, Style::default()))).block(
                Block::default().borders(Borders::TOP),
            );
        frame.render_widget(status_widget, status_area);
    }

    fn status_line(&mut self, total_lines: usize) -> String {
        self.prune_status_message();
        if let Some((message, _)) = &self.status_message {
            return message.clone();
        }

        let marker = if self.dirty { "*" } else { "" };
        format!(
            "{}{} | Lines: {} | Ctrl-S save | Ctrl-Q quit",
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

        let desired_column = self
            .preferred_column
            .unwrap_or(current.column);

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

    fn closest_pointer_on_line(
        &self,
        line: usize,
        column: u16,
    ) -> Option<CursorDisplay> {
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
        let (clone, markers, inserted_cursor) = self.editor.clone_with_markers(SENTINEL);
        let mut buffer = Vec::new();
        {
            let writer = VecWriter {
                buffer: &mut buffer,
            };
            let mut formatter = Formatter::new(writer, {
                let mut style = FormattingStyle::ansi();
                style.wrap_width = width.max(1);
                style
            });
            if formatter.write_document(&clone).is_err() {
                return RenderResult::empty();
            }
        }

        let ansi_output = match String::from_utf8(buffer) {
            Ok(text) => text,
            Err(_) => return RenderResult::empty(),
        };

        let ParseResult {
            lines,
            cursor,
            total_lines,
            markers: display_markers,
        } = parse_ansi(&ansi_output, SENTINEL);

        let mut marker_positions: HashMap<usize, CursorVisualPosition> = display_markers
            .into_iter()
            .collect();

        let mut cursor_map = Vec::new();
        for marker in markers {
            if let Some(position) = marker_positions.remove(&marker.id) {
                cursor_map.push(CursorDisplay {
                    pointer: marker.pointer,
                    position,
                });
            }
        }

        if inserted_cursor && cursor.is_none() {
            RenderResult::from_parts(lines, None, total_lines, cursor_map)
        } else {
            RenderResult::from_parts(lines, cursor, total_lines, cursor_map)
        }
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
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
                    self.editor.move_to_segment_start();
                    self.preferred_column = None;
                }
                (KeyCode::End, _) => {
                    self.editor.move_to_segment_end();
                    self.preferred_column = None;
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
                    self.scroll_top = self
                        .scroll_top
                        .saturating_sub(self.last_view_height.max(1));
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
}

struct VecWriter<'a> {
    buffer: &'a mut Vec<u8>,
}

impl<'a> Write for VecWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct CursorDisplay {
    pointer: CursorPointer,
    position: CursorVisualPosition,
}

struct RenderResult {
    lines: Vec<Line<'static>>,
    cursor: Option<CursorVisualPosition>,
    total_lines: usize,
    cursor_map: Vec<CursorDisplay>,
}

impl RenderResult {
    fn empty() -> Self {
        Self {
            lines: vec![Line::from("")],
            cursor: None,
            total_lines: 1,
            cursor_map: Vec::new(),
        }
    }

    fn from_parts(
        lines: Vec<Line<'static>>,
        cursor: Option<CursorVisualPosition>,
        total_lines: usize,
        cursor_map: Vec<CursorDisplay>,
    ) -> Self {
        let total = total_lines.max(1);
        let lines = if lines.is_empty() {
            vec![Line::from("")]
        } else {
            lines
        };
        Self {
            lines,
            cursor,
            total_lines: total,
            cursor_map,
        }
    }
}
