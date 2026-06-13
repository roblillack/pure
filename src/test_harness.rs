//! Headless test harness: drives the full [`App`] — real event handling,
//! real rendering — against ratatui's `TestBackend`, and renders the
//! resulting cell buffer to deterministic SVG for `insta` snapshot tests.
//!
//! The SVG renderer maps the terminal cell grid 1:1 onto a fixed-size pixel
//! grid of background `<rect>`s and `<text>` runs, so snapshots capture
//! styling (colors, bold/italic, selection reverse-video, the cursor)
//! without rasterizing any fonts: the output is identical on every machine,
//! diffs as text, and opens in any browser.
//!
//! Snapshots live in `src/snapshots/*.snap.svg`. Review changes with
//! `cargo insta review`, or set `INSTA_UPDATE=always` to rewrite them.
//!
//! With the `recorder` feature the harness is also compiled into the
//! library itself, so `examples/demo` can drive the app headlessly and
//! record the README's `demo.gif` from the same SVG frames.

use std::fmt::Write as _;
use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::{Buffer, Cell},
    layout::Position,
    style::{Color, Modifier},
};
use tdoc::Document;

use crate::app::{App, DocumentFormat};

const FONT_SIZE: usize = 16;
const DEFAULT_FG: &str = "#d8d8d8";
const DEFAULT_BG: &str = "#101010";

/// Pixel geometry of one terminal cell in the rendered SVG.
#[derive(Clone, Copy)]
pub struct CellMetrics {
    /// Cell width in px.
    pub width: usize,
    /// Cell height in px — the line height.
    pub height: usize,
    /// Text baseline offset from the top of a cell row.
    pub baseline: usize,
}

impl Default for CellMetrics {
    /// The snapshot geometry: 20px rows leave breathing room around the
    /// 16px font, so snapshots stay legible with whatever monospace font
    /// the viewer's browser resolves.
    fn default() -> Self {
        Self {
            width: 10,
            height: 20,
            baseline: 15,
        }
    }
}

/// A `pure` application running against an in-memory terminal.
pub struct TestApp {
    app: App,
    terminal: Terminal<TestBackend>,
}

impl TestApp {
    /// Create an app showing `document` in a `width`×`height` cell terminal
    /// and render the first frame.
    pub fn new(width: u16, height: u16, document: Document) -> Self {
        Self::with_path(width, height, document, PathBuf::from("test.ftml"))
    }

    /// Like [`TestApp::new`], but with a custom file path (shown in the
    /// status bar).
    pub fn with_path(width: u16, height: u16, document: Document, path: PathBuf) -> Self {
        Self::build(width, height, document, Some(path))
    }

    /// Like [`TestApp::new`], but untitled — no backing file, as when Pure
    /// is started without an argument.
    pub fn untitled(width: u16, height: u16, document: Document) -> Self {
        Self::build(width, height, document, None)
    }

    fn build(width: u16, height: u16, document: Document, path: Option<PathBuf>) -> Self {
        let mut app = App::new(document, path, DocumentFormat::Ftml, None);
        app.set_interactive(false);
        let terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
        let mut test_app = Self { app, terminal };
        test_app.draw();
        test_app
    }

    /// Render one frame. The main loop redraws after every event, so
    /// [`TestApp::event`] does this automatically.
    pub fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| app.draw(frame))
            .expect("draw frame");
    }

    /// Feed one event through the real event handling and redraw.
    pub fn event(&mut self, event: Event) {
        self.app.handle_event(event).expect("handle event");
        self.draw();
    }

    pub fn key(&mut self, code: KeyCode) {
        self.key_with(code, KeyModifiers::NONE);
    }

    pub fn key_with(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.event(Event::Key(KeyEvent::new(code, modifiers)));
    }

    pub fn ctrl(&mut self, ch: char) {
        self.key_with(KeyCode::Char(ch), KeyModifiers::CONTROL);
    }

    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.key(KeyCode::Char(ch));
        }
    }

    /// Single left click (press + release). Like in a real terminal,
    /// repeated clicks at the same position within 400ms register as
    /// double/triple clicks.
    pub fn click(&mut self, column: u16, row: u16) {
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            self.event(Event::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }));
        }
    }

    /// The terminal cursor position after the last draw.
    pub fn cursor_position(&mut self) -> Option<Position> {
        self.terminal.get_cursor_position().ok()
    }

    /// Plain-text contents of the terminal buffer, one string per row.
    pub fn buffer_lines(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        let area = buffer.area();
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    /// Render the current frame to SVG.
    pub fn svg(&mut self) -> String {
        self.svg_with(CellMetrics::default())
    }

    /// Render the current frame to SVG with custom cell geometry.
    pub fn svg_with(&mut self, metrics: CellMetrics) -> String {
        let cursor = self.terminal.get_cursor_position().ok();
        buffer_to_svg(self.terminal.backend().buffer(), cursor, metrics)
    }
}

/// The resolved visual style of a cell, with `REVERSED` already applied.
#[derive(Clone, PartialEq)]
struct CellStyle {
    fg: String,
    bg: String,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

fn cell_style(cell: &Cell) -> CellStyle {
    let mut fg = css_color(cell.fg, DEFAULT_FG);
    let mut bg = css_color(cell.bg, DEFAULT_BG);
    let modifier = cell.modifier;
    if modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    CellStyle {
        fg,
        bg,
        bold: modifier.contains(Modifier::BOLD),
        dim: modifier.contains(Modifier::DIM),
        italic: modifier.contains(Modifier::ITALIC),
        underline: modifier.contains(Modifier::UNDERLINED),
        strike: modifier.contains(Modifier::CROSSED_OUT),
    }
}

/// The 16 base ANSI colors (VS Code's terminal palette).
const ANSI16: [&str; 16] = [
    "#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
    "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6", "#29b8db", "#ffffff",
];

fn css_color(color: Color, default: &str) -> String {
    match color {
        Color::Reset => default.to_string(),
        Color::Black => ANSI16[0].to_string(),
        Color::Red => ANSI16[1].to_string(),
        Color::Green => ANSI16[2].to_string(),
        Color::Yellow => ANSI16[3].to_string(),
        Color::Blue => ANSI16[4].to_string(),
        Color::Magenta => ANSI16[5].to_string(),
        Color::Cyan => ANSI16[6].to_string(),
        Color::Gray => ANSI16[7].to_string(),
        Color::DarkGray => ANSI16[8].to_string(),
        Color::LightRed => ANSI16[9].to_string(),
        Color::LightGreen => ANSI16[10].to_string(),
        Color::LightYellow => ANSI16[11].to_string(),
        Color::LightBlue => ANSI16[12].to_string(),
        Color::LightMagenta => ANSI16[13].to_string(),
        Color::LightCyan => ANSI16[14].to_string(),
        Color::White => ANSI16[15].to_string(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(i) => indexed_color(i),
    }
}

/// Standard xterm-256 palette.
fn indexed_color(index: u8) -> String {
    match index {
        0..=15 => ANSI16[index as usize].to_string(),
        16..=231 => {
            let i = index - 16;
            let steps = [0u8, 95, 135, 175, 215, 255];
            let r = steps[(i / 36) as usize];
            let g = steps[((i % 36) / 6) as usize];
            let b = steps[(i % 6) as usize];
            format!("#{r:02x}{g:02x}{b:02x}")
        }
        232..=255 => {
            let v = 8 + 10 * (index - 232);
            format!("#{v:02x}{v:02x}{v:02x}")
        }
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render a ratatui cell buffer (plus optional cursor position) to SVG.
///
/// Cells of one row are coalesced into runs of equal style; each run emits a
/// background `<rect>` (when not the default background) and a `<text>` with
/// an exact `textLength`, so glyphs stay on the cell grid regardless of the
/// font the viewer resolves `monospace` to.
pub fn buffer_to_svg(buffer: &Buffer, cursor: Option<Position>, metrics: CellMetrics) -> String {
    let CellMetrics {
        width: cell_w,
        height: cell_h,
        baseline,
    } = metrics;
    let area = buffer.area;
    let width_px = area.width as usize * cell_w;
    let height_px = area.height as usize * cell_h;

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width_px}" height="{height_px}" viewBox="0 0 {width_px} {height_px}" font-family="'DejaVu Sans Mono', Menlo, Consolas, monospace" font-size="{FONT_SIZE}px">"#
    );
    let _ = writeln!(
        svg,
        r#"<rect width="100%" height="100%" fill="{DEFAULT_BG}"/>"#
    );

    for y in 0..area.height {
        // Coalesce the row into runs of (start cell, cell count, text, style)
        let mut runs: Vec<(usize, usize, String, CellStyle)> = Vec::new();
        for x in 0..area.width {
            let cell = buffer.cell(Position::new(x, y)).expect("cell in area");
            let style = cell_style(cell);
            match runs.last_mut() {
                Some((_, cells, text, last_style)) if *last_style == style => {
                    *cells += 1;
                    text.push_str(cell.symbol());
                }
                _ => runs.push((x as usize, 1, cell.symbol().to_string(), style)),
            }
        }

        let row_px = y as usize * cell_h;
        for (start, cells, text, style) in &runs {
            if style.bg != DEFAULT_BG {
                let _ = writeln!(
                    svg,
                    r#"<rect x="{}" y="{row_px}" width="{}" height="{cell_h}" fill="{}"/>"#,
                    start * cell_w,
                    cells * cell_w,
                    style.bg,
                );
            }
            if text.trim().is_empty() && !style.underline && !style.strike {
                continue;
            }
            let mut attrs = String::new();
            if style.bold {
                attrs.push_str(r#" font-weight="bold""#);
            }
            if style.italic {
                attrs.push_str(r#" font-style="italic""#);
            }
            match (style.underline, style.strike) {
                (true, true) => attrs.push_str(r#" text-decoration="underline line-through""#),
                (true, false) => attrs.push_str(r#" text-decoration="underline""#),
                (false, true) => attrs.push_str(r#" text-decoration="line-through""#),
                (false, false) => {}
            }
            if style.dim {
                attrs.push_str(r#" opacity="0.6""#);
            }
            let _ = writeln!(
                svg,
                r#"<text x="{}" y="{}" fill="{}" textLength="{}" lengthAdjust="spacingAndGlyphs" xml:space="preserve"{attrs}>{}</text>"#,
                start * cell_w,
                row_px + baseline,
                style.fg,
                cells * cell_w,
                xml_escape(text),
            );
        }
    }

    if let Some(position) = cursor {
        let _ = writeln!(
            svg,
            r##"<rect x="{}" y="{}" width="{cell_w}" height="{cell_h}" fill="#ffffff" fill-opacity="0.4"/>"##,
            position.x as usize * cell_w,
            position.y as usize * cell_h,
        );
    }

    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod snapshot_tests;
