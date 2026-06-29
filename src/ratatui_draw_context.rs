//! A terminal/cell backend for `tdoc_editor`'s [`DrawContext`].
//!
//! The shared layout engine in `tdoc-editor` is written against a pixel-oriented
//! [`DrawContext`] (FLTK is the reference backend). A terminal works in character
//! cells, not pixels, so this adapter collapses the pixel model onto a cell grid:
//!
//! - `text_width` returns the Unicode **display width** (columns) of the text,
//!   ignoring font/size — so the engine wraps in columns.
//! - `text_height` is always `1` (one row). Pair this with a [`Theme`] whose
//!   `line_height` is `1` (see [`terminal_theme`]) so vertical advance is one row
//!   per line.
//! - `draw_text` / `draw_rect_filled` write directly into a Ratatui [`Buffer`].
//!
//! This lets Pure render and drive the *shared* editor in a real terminal buffer,
//! which is what the integration test exercises.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use tdoc_editor::draw_context::{DrawContext, FontStyle, FontType};
use tdoc_editor::theme::Theme;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A [`Theme`] with terminal-cell metrics: one row per line and no oversized
/// pixel padding. Colors are inherited from [`Theme::default`].
///
/// `font_size` is forced to `0` on every font: the engine positions text at a
/// pixel baseline `font_size` below the line top (`draw_y = line_top +
/// font_size`), which is meaningless on a cell grid. With `font_size == 0` the
/// baseline coincides with the line's row, so each line lands on exactly one row.
pub fn terminal_theme() -> Theme {
    let mut t = Theme {
        line_height: 1,
        // Quote bars share the gray of the other structural marks.
        quote_bar_color: 0x808080FF,
        // No page gutter: classic Pure had none, and a heading's top margin
        // (below) supplies the gap when the document opens with one.
        // `padding_horizontal` is a baseline; the app overrides it per-frame with
        // a responsive page margin (see `app::page_margin`).
        padding_vertical: 0,
        padding_horizontal: 1,
        quote_bar_width: 1,
        // Block spacing is driven entirely by `classic_block_spacing` (the gap
        // before a block is the max of the base gap and the adjacent margins),
        // so the per-block trailing-spacing fields are all zero — otherwise the
        // additive model would stack on top of the classic one.
        heading_top_margin: 0,
        heading_bottom_margin: 0,
        paragraph_spacing: 0,
        list_item_spacing: 0,
        quote_spacing: 0,
        // One row of padding above/below code hosts the fence rules.
        code_block_padding: 1,
        quote_indent: 2,
        quote_bar_offset: 0,
        table_cell_padding_h: 1,
        table_cell_padding_v: 0,
        // Underline/strikethrough become cell attributes, not separate lines.
        text_decoration_lines: false,
        // Classic Pure centered the document title (level-1 heading).
        center_level1_headings: true,
        // Classic Pure put code flush with the body text and set it apart with
        // fences, not an indent.
        code_block_indent: 0,
        // Render checkboxes as `[✓] `/`[ ] ` text, the way classic Pure did.
        checkbox_text: true,
        // A terminal has no font sizes, so mark H2/H3 with `===`/`---` rules and
        // fence code blocks with `---` rules; both are drawn in gray.
        heading_underline: true,
        code_block_fence: true,
        structural_color: 0x808080FF,
        // Collapse-by-max block margins with classic heading sizes.
        classic_block_spacing: true,
        // Classic Pure wrapped text one column shy of the content width.
        wrap_width_reduction: 1,
        // Classic Pure's quote bar was a literal `|`, not a box-drawing rule.
        quote_bar_as_text: true,
        ..Theme::default()
    };
    for fs in [
        &mut t.header_level_1,
        &mut t.header_level_2,
        &mut t.header_level_3,
        &mut t.plain_text,
        &mut t.quote_text,
        &mut t.code_text,
    ] {
        fs.font_size = 0;
    }
    // Classic Pure drew code and quote text in the default body color (no blue
    // code, no dim italic quotes) and left the quote's emphasis to inline spans.
    t.code_text.font_color = t.plain_text.font_color;
    t.quote_text.font_color = t.plain_text.font_color;
    t.quote_text.font_style = FontStyle::Regular;
    // A solid light header fill would hide the (terminal-default) header text;
    // drop it so table headers read as bold text on the terminal background.
    t.table_header_background = t.background_color;
    t
}

fn to_color(rgba: u32) -> Color {
    let r = ((rgba >> 24) & 0xFF) as u8;
    let g = ((rgba >> 16) & 0xFF) as u8;
    let b = ((rgba >> 8) & 0xFF) as u8;
    Color::Rgb(r, g, b)
}

/// Renders the shared layout engine into a Ratatui [`Buffer`].
///
/// Coordinates coming from the engine are treated as cell coordinates relative to
/// the display origin; `area` places that origin inside the buffer.
pub struct RatatuiDrawContext<'a> {
    buf: &'a mut Buffer,
    area: Rect,
    /// Active draw color (set via `set_color`, consumed by the next draw call).
    color: u32,
    /// Active text modifier derived from `set_font` (bold/italic).
    modifier: Modifier,
    /// Active decoration modifier from `set_underline`/`set_strikethrough`.
    deco: Modifier,
    /// The theme's "page" background — rendered as the terminal's default
    /// background (i.e. not painted) rather than a solid color, so the editor
    /// blends into the user's terminal instead of drawing a bright page.
    page_bg: u32,
    /// The theme's default text color — rendered as the terminal's default
    /// foreground ([`Color::Reset`]); accent colors (links, headings, …) are
    /// still drawn explicitly.
    default_fg: u32,
    /// Clip regions (engine coordinates); effective clip is their intersection.
    clip: Vec<(i32, i32, i32, i32)>,
    focus: bool,
}

impl<'a> RatatuiDrawContext<'a> {
    pub fn new(buf: &'a mut Buffer, area: Rect) -> Self {
        let theme = Theme::default();
        Self {
            buf,
            area,
            color: 0x000000FF,
            modifier: Modifier::empty(),
            deco: Modifier::empty(),
            page_bg: theme.background_color,
            default_fg: theme.plain_text.font_color,
            clip: Vec::new(),
            focus: true,
        }
    }

    pub fn with_focus(mut self, focus: bool) -> Self {
        self.focus = focus;
        self
    }

    /// Set which colors map to the terminal's defaults — the page background
    /// (left unpainted) and the body-text foreground (drawn as `Color::Reset`).
    /// Pass the active theme's `background_color` and `plain_text.font_color`.
    pub fn with_palette(mut self, page_bg: u32, default_fg: u32) -> Self {
        self.page_bg = page_bg;
        self.default_fg = default_fg;
        self
    }

    /// Foreground color for the active draw color, mapping the theme default to
    /// the terminal's default foreground.
    fn fg(&self) -> Color {
        if self.color == self.default_fg {
            Color::Reset
        } else {
            to_color(self.color)
        }
    }

    /// True if (x, y) in engine coordinates passes every active clip rect.
    fn in_clip(&self, x: i32, y: i32) -> bool {
        self.clip
            .iter()
            .all(|&(cx, cy, cw, ch)| x >= cx && x < cx + cw && y >= cy && y < cy + ch)
    }

    /// Write a single cell at engine coordinates (x, y), honoring clip + buffer
    /// bounds. `bg_only` leaves the existing glyph intact (for rectangle fills).
    fn put(&mut self, x: i32, y: i32, symbol: Option<&str>, bg_only: bool) {
        if !self.in_clip(x, y) {
            return;
        }
        let col = self.area.x as i32 + x;
        let row = self.area.y as i32 + y;
        if col < self.area.x as i32
            || col >= (self.area.x + self.area.width) as i32
            || row < self.area.y as i32
            || row >= (self.area.y + self.area.height) as i32
        {
            return;
        }
        let modifier = self.modifier | self.deco;
        let is_page_bg = self.color == self.page_bg;
        let fg = self.fg();
        if let Some(cell) = self.buf.cell_mut((col as u16, row as u16)) {
            if bg_only {
                // The page background is the terminal default — leave it unpainted
                // so the editor adopts the user's terminal colors. Other fills
                // (selection, highlight, table header) are drawn explicitly.
                cell.set_bg(if is_page_bg {
                    Color::Reset
                } else {
                    to_color(self.color)
                });
            } else if let Some(sym) = symbol {
                cell.set_symbol(sym);
                cell.set_style(Style::default().fg(fg).add_modifier(modifier));
            }
        }
    }

    /// Draw a box-drawing segment at (x, y), merging with any box glyph already
    /// in the cell so perpendicular grid lines join into ┼/├/┬/corner glyphs.
    fn put_line(&mut self, x: i32, y: i32, add: u8) {
        if !self.in_clip(x, y) {
            return;
        }
        let col = self.area.x as i32 + x;
        let row = self.area.y as i32 + y;
        if col < self.area.x as i32
            || col >= (self.area.x + self.area.width) as i32
            || row < self.area.y as i32
            || row >= (self.area.y + self.area.height) as i32
        {
            return;
        }
        let fg = self.fg();
        let modifier = self.modifier;
        if let Some(cell) = self.buf.cell_mut((col as u16, row as u16)) {
            let combined = add | box_mask(cell.symbol());
            cell.set_symbol(box_glyph(combined));
            cell.set_style(Style::default().fg(fg).add_modifier(modifier));
        }
    }
}

const BOX_UP: u8 = 1;
const BOX_DOWN: u8 = 2;
const BOX_LEFT: u8 = 4;
const BOX_RIGHT: u8 = 8;

/// Direction bitmask (up/down/left/right) for an existing box-drawing glyph.
///
/// The lone-direction stubs (`╵╷╴╶`) matter: a grid line's endpoint is painted
/// as a single inward direction so that, when the perpendicular line is drawn
/// over it, the two merge into the correct corner/junction. Reading the stub
/// back as its true single direction (rather than a full `│`/`─`) is what keeps
/// corners from collapsing into `┼`/`├`.
fn box_mask(sym: &str) -> u8 {
    match sym {
        "─" => BOX_LEFT | BOX_RIGHT,
        "│" => BOX_UP | BOX_DOWN,
        "╵" => BOX_UP,
        "╷" => BOX_DOWN,
        "╴" => BOX_LEFT,
        "╶" => BOX_RIGHT,
        "┌" => BOX_DOWN | BOX_RIGHT,
        "┐" => BOX_DOWN | BOX_LEFT,
        "└" => BOX_UP | BOX_RIGHT,
        "┘" => BOX_UP | BOX_LEFT,
        "├" => BOX_UP | BOX_DOWN | BOX_RIGHT,
        "┤" => BOX_UP | BOX_DOWN | BOX_LEFT,
        "┬" => BOX_DOWN | BOX_LEFT | BOX_RIGHT,
        "┴" => BOX_UP | BOX_LEFT | BOX_RIGHT,
        "┼" => BOX_UP | BOX_DOWN | BOX_LEFT | BOX_RIGHT,
        _ => 0,
    }
}

/// The box-drawing glyph for a direction bitmask.
fn box_glyph(mask: u8) -> &'static str {
    match mask {
        m if m == BOX_LEFT | BOX_RIGHT => "─",
        m if m == BOX_UP | BOX_DOWN => "│",
        m if m == BOX_DOWN | BOX_RIGHT => "┌",
        m if m == BOX_DOWN | BOX_LEFT => "┐",
        m if m == BOX_UP | BOX_RIGHT => "└",
        m if m == BOX_UP | BOX_LEFT => "┘",
        m if m == BOX_UP | BOX_DOWN | BOX_RIGHT => "├",
        m if m == BOX_UP | BOX_DOWN | BOX_LEFT => "┤",
        m if m == BOX_DOWN | BOX_LEFT | BOX_RIGHT => "┬",
        m if m == BOX_UP | BOX_LEFT | BOX_RIGHT => "┴",
        m if m == BOX_UP | BOX_DOWN | BOX_LEFT | BOX_RIGHT => "┼",
        // Lone-direction stubs: an endpoint that hasn't met its perpendicular
        // line yet (in a finished table grid these are always merged away).
        m if m == BOX_UP => "╵",
        m if m == BOX_DOWN => "╷",
        m if m == BOX_LEFT => "╴",
        m if m == BOX_RIGHT => "╶",
        _ => "│",
    }
}

impl DrawContext for RatatuiDrawContext<'_> {
    fn set_color(&mut self, color: u32) {
        self.color = color;
    }

    fn set_font(&mut self, _font: FontType, style: FontStyle, _size: u8) {
        // A terminal cell has one font, but bold/italic map to cell modifiers.
        self.modifier = match style {
            FontStyle::Regular => Modifier::empty(),
            FontStyle::Bold => Modifier::BOLD,
            FontStyle::Italic => Modifier::ITALIC,
            FontStyle::BoldItalic => Modifier::BOLD | Modifier::ITALIC,
        };
    }

    fn draw_text(&mut self, text: &str, x: i32, y: i32) {
        let mut col = x;
        for ch in text.chars() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if w == 0 {
                continue; // skip zero-width / control chars
            }
            let mut buf = [0u8; 4];
            self.put(col, y, Some(ch.encode_utf8(&mut buf)), false);
            col += w as i32;
        }
    }

    fn draw_rect_filled(&mut self, x: i32, y: i32, w: i32, h: i32) {
        for dy in 0..h {
            for dx in 0..w {
                self.put(x + dx, y + dy, None, true);
            }
        }
    }

    fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        // Horizontal/vertical runs draw box-drawing glyphs, merging at crossings
        // into the right junction (┼ ├ ┬ ┌ …) so table grids look connected.
        //
        // The engine speaks in pixels, where a stroke's two endpoints are real
        // corners — table grids are drawn corner-to-corner and must paint *both*
        // endpoints (inclusive) so the junctions join. A quote bar, however, is a
        // *span*: the engine draws it from a line's top to its bottom edge
        // (`y..y + line.height`), where the bottom edge is the next row's top, not
        // a cell to paint. On the coarse cell grid a one-row-tall span (height 1)
        // would otherwise bleed into the row below, so treat a unit span as the
        // single cell it covers.
        if y1 == y2 {
            // Horizontal stroke. Each endpoint carries only its inward direction
            // so that, when merged with the verticals drawn first, grid corners
            // become ┌┐└┘ and edge crossings become ┬┴├┤ instead of all ┼.
            let (a, b) = (x1.min(x2), x1.max(x2));
            for x in a..=b {
                let mut mask = BOX_LEFT | BOX_RIGHT;
                if x == a {
                    mask &= !BOX_LEFT;
                }
                if x == b {
                    mask &= !BOX_RIGHT;
                }
                self.put_line(x, y1, mask);
            }
        } else if x1 == x2 {
            let (a, b) = (y1.min(y2), y1.max(y2));
            if b - a == 1 {
                // Unit-height span (quote bar): paint only the row it occupies.
                self.put_line(x1, a, BOX_UP | BOX_DOWN);
            } else {
                for y in a..=b {
                    let mut mask = BOX_UP | BOX_DOWN;
                    if y == a {
                        mask &= !BOX_UP;
                    }
                    if y == b {
                        mask &= !BOX_DOWN;
                    }
                    self.put_line(x1, y, mask);
                }
            }
        }
    }

    fn set_underline(&mut self, on: bool) {
        self.deco.set(Modifier::UNDERLINED, on);
    }

    fn set_strikethrough(&mut self, on: bool) {
        self.deco.set(Modifier::CROSSED_OUT, on);
    }

    fn draw_checkbox(&mut self, x: i32, y: i32, _size: i32, checked: bool) {
        // A drawn square collapses to garbage in one cell; stamp a glyph instead.
        self.put(x, y, Some(if checked { "☑" } else { "☐" }), false);
    }

    fn text_width(&mut self, text: &str, _f: FontType, _s: FontStyle, _sz: u8) -> f64 {
        UnicodeWidthStr::width(text) as f64
    }

    fn text_height(&self, _f: FontType, _s: FontStyle, _sz: u8) -> i32 {
        1
    }

    fn text_descent(&self, _f: FontType, _s: FontStyle, _sz: u8) -> i32 {
        0
    }

    fn push_clip(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.clip.push((x, y, w, h));
    }

    fn pop_clip(&mut self) {
        self.clip.pop();
    }

    fn color_average(&self, c1: u32, c2: u32, weight: f32) -> u32 {
        let w = weight.clamp(0.0, 1.0);
        let mix = |s1: u32, s2: u32| -> u32 {
            (s1 as f32 * w + s2 as f32 * (1.0 - w)).round() as u32 & 0xFF
        };
        let r = mix((c1 >> 24) & 0xFF, (c2 >> 24) & 0xFF);
        let g = mix((c1 >> 16) & 0xFF, (c2 >> 16) & 0xFF);
        let b = mix((c1 >> 8) & 0xFF, (c2 >> 8) & 0xFF);
        let a = mix(c1 & 0xFF, c2 & 0xFF);
        (r << 24) | (g << 16) | (b << 8) | a
    }

    fn color_contrast(&self, fg: u32, bg: u32) -> u32 {
        // Pick black or white text depending on background luminance.
        let r = ((bg >> 24) & 0xFF) as f32;
        let g = ((bg >> 16) & 0xFF) as f32;
        let b = ((bg >> 8) & 0xFF) as f32;
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        if luma > 140.0 {
            0x000000FF
        } else if luma > 0.0 {
            0xFFFFFFFF
        } else {
            fg
        }
    }

    fn color_inactive(&self, c: u32) -> u32 {
        // Blend halfway toward mid-gray.
        self.color_average(c, 0x808080FF, 0.5)
    }

    fn has_focus(&self) -> bool {
        self.focus
    }

    fn is_active(&self) -> bool {
        true
    }
}
