//! The demo "camera": drives the real app through the headless test harness
//! and records every keypress as one GIF frame.
//!
//! Each simulated key goes through [`TestApp::key_with`] (real event
//! handling, real rendering), the resulting terminal buffer is rendered to
//! SVG by the snapshot harness, rasterized with resvg, and appended to the
//! GIF with a delay that matches the kind of interaction: quick between
//! typed characters, a long beat while a menu is open. Identical consecutive
//! frames are merged, and every frame after the first only encodes the
//! rectangle of pixels that actually changed, so the GIF stays small.
//!
//! Set `PURE_DEMO_FRAMES=<dir>` to additionally dump every frame as a PNG
//! for debugging a script.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyModifiers};
use gif::{DisposalMethod, Encoder, Frame, Repeat};
use pure_tui::menu_bar::{MENU_BAR, MenuBarEntry, menu_with_accel};
use pure_tui::test_harness::TestApp;
use resvg::{tiny_skia, usvg};
use tdoc::Document;

/// How long a frame stays on screen, by the interaction that produced it,
/// in GIF time units (10ms).
mod pace {
    /// Between typed characters.
    pub const TYPE: u32 = 7;
    /// After a typed space — typists pause between words.
    pub const SPACE: u32 = 12;
    /// After typed punctuation.
    pub const PUNCTUATION: u32 = 30;
    /// Plain cursor movement.
    pub const MOVE: u32 = 25;
    /// Watching a selection grow.
    pub const SELECT: u32 = 50;
    /// Reading a freshly opened menu.
    pub const MENU: u32 = 120;
    /// Stepping through menu items.
    pub const STEP: u32 = 50;
    /// Letting an action's effect sink in.
    pub const ACTION: u32 = 90;
    /// The opening frame: an empty editor.
    pub const FIRST: u32 = 100;
    /// The closing frame before the GIF loops.
    pub const LAST: u32 = 500;
}

struct RecordedFrame {
    rgba: Vec<u8>,
    delay: u32,
}

/// Records a scripted Pure session into an animated GIF.
pub struct Recorder {
    app: TestApp,
    options: usvg::Options<'static>,
    frames: Vec<RecordedFrame>,
    width: u16,
    height: u16,
    dump_dir: Option<PathBuf>,
}

// The interaction verbs form the script vocabulary; not every demo uses all
// of them.
#[allow(dead_code)]
impl Recorder {
    /// Start a session with an empty document in a `columns`×`rows` terminal.
    pub fn start(columns: u16, rows: u16) -> Self {
        let app = TestApp::with_path(columns, rows, Document::new(), PathBuf::from("demo.ftml"));

        let mut options = usvg::Options::default();
        let fontdb = options.fontdb_mut();
        fontdb.load_system_fonts();
        fontdb.set_monospace_family("DejaVu Sans Mono");
        let have_font = fontdb.faces().any(|face| {
            face.families
                .iter()
                .any(|(name, _)| name == "DejaVu Sans Mono")
        });
        if !have_font {
            eprintln!(
                "warning: font 'DejaVu Sans Mono' not found; \
                 the recording will use a fallback font"
            );
        }

        let dump_dir = std::env::var_os("PURE_DEMO_FRAMES").map(PathBuf::from);
        if let Some(dir) = &dump_dir {
            fs::create_dir_all(dir).expect("create frame dump directory");
        }

        let mut recorder = Self {
            app,
            options,
            frames: Vec::new(),
            width: 0,
            height: 0,
            dump_dir,
        };
        recorder.capture(pace::FIRST);
        recorder
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
    /// character (e.g. `'1'` for "Heading 1", `'i'` for "Italic").
    pub fn context_menu(&mut self, shortcut: char) {
        self.press(KeyCode::Esc, KeyModifiers::NONE, pace::MENU);
        let modifiers = if shortcut.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        self.press(KeyCode::Char(shortcut), modifiers, pace::ACTION);
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

    /// Hold the current frame for an extra `ms` milliseconds.
    pub fn pause(&mut self, ms: u32) {
        if let Some(last) = self.frames.last_mut() {
            last.delay += ms / 10;
        }
    }

    /// Hold the final frame, then encode all frames into a GIF at `path`.
    pub fn stop(mut self, path: impl AsRef<Path>) -> Result<()> {
        self.pause(pace::LAST * 10);
        let path = path.as_ref();
        encode_gif(path, &self.frames, self.width, self.height)
            .with_context(|| format!("failed to write {}", path.display()))?;

        let seconds = self.frames.iter().map(|f| f.delay).sum::<u32>() as f32 / 100.0;
        let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        println!(
            "{}: {} frames, {seconds:.1}s, {}K",
            path.display(),
            self.frames.len(),
            bytes.div_ceil(1024),
        );
        Ok(())
    }

    fn press(&mut self, code: KeyCode, modifiers: KeyModifiers, delay: u32) {
        self.app.key_with(code, modifiers);
        self.capture(delay);
    }

    /// Rasterize the current frame and append it, or — when nothing visible
    /// changed — extend the previous frame's delay instead.
    fn capture(&mut self, delay: u32) {
        let svg = self.app.svg();
        let tree = usvg::Tree::from_str(&svg, &self.options).expect("harness SVG parses");
        let size = tree.size().to_int_size();
        let mut pixmap =
            tiny_skia::Pixmap::new(size.width(), size.height()).expect("nonzero frame size");
        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );

        // The SVG paints an opaque background, so every pixel is opaque and
        // the premultiplied buffer equals straight RGBA.
        let rgba = pixmap.take();
        (self.width, self.height) = (size.width() as u16, size.height() as u16);

        if let Some(last) = self.frames.last_mut()
            && last.rgba == rgba
        {
            last.delay += delay;
            return;
        }

        if let Some(dir) = &self.dump_dir {
            let pixmap = tiny_skia::Pixmap::from_vec(rgba.clone(), size).expect("frame size");
            let path = dir.join(format!("frame-{:04}.png", self.frames.len()));
            pixmap.save_png(path).expect("dump frame PNG");
        }

        self.frames.push(RecordedFrame { rgba, delay });
    }
}

fn encode_gif(path: &Path, frames: &[RecordedFrame], width: u16, height: u16) -> Result<()> {
    if frames.is_empty() {
        bail!("no frames recorded");
    }
    let file = BufWriter::new(File::create(path)?);
    let mut encoder = Encoder::new(file, width, height, &[])?;
    encoder.set_repeat(Repeat::Infinite)?;

    let mut previous: Option<&RecordedFrame> = None;
    for frame in frames {
        // First frame ships whole; later ones only the changed rectangle.
        let region = match previous {
            None => Region {
                left: 0,
                top: 0,
                width,
                height,
            },
            Some(prev) => changed_region(&prev.rgba, &frame.rgba, width)
                // Equal frames are merged during capture; keep a 1×1 patch
                // as a fallback so the delay still gets encoded.
                .unwrap_or(Region {
                    left: 0,
                    top: 0,
                    width: 1,
                    height: 1,
                }),
        };
        let previous_rgba = previous.map(|prev| prev.rgba.as_slice());
        encoder.write_frame(&gif_frame(
            &frame.rgba,
            previous_rgba,
            width,
            &region,
            frame.delay,
        ))?;
        previous = Some(frame);
    }
    Ok(())
}

/// A rectangle of cropped pixels, in whole-frame coordinates.
struct Region {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
}

/// The bounding box of all pixels that differ between two frames.
fn changed_region(a: &[u8], b: &[u8], width: u16) -> Option<Region> {
    let stride = width as usize * 4;
    let (mut top, mut bottom) = (None, 0);
    for (y, (row_a, row_b)) in a
        .chunks_exact(stride)
        .zip(b.chunks_exact(stride))
        .enumerate()
    {
        if row_a != row_b {
            top.get_or_insert(y);
            bottom = y;
        }
    }
    let top = top?;

    let (mut left, mut right) = (width as usize - 1, 0);
    for y in top..=bottom {
        let row_a = &a[y * stride..(y + 1) * stride];
        let row_b = &b[y * stride..(y + 1) * stride];
        for x in 0..width as usize {
            if row_a[x * 4..x * 4 + 4] != row_b[x * 4..x * 4 + 4] {
                left = left.min(x);
                right = right.max(x);
            }
        }
    }

    Some(Region {
        left: left as u16,
        top: top as u16,
        width: (right - left + 1) as u16,
        height: (bottom - top + 1) as u16,
    })
}

fn gif_frame(
    rgba: &[u8],
    previous: Option<&[u8]>,
    width: u16,
    region: &Region,
    delay: u32,
) -> Frame<'static> {
    let stride = width as usize * 4;
    let crop = |buffer: &[u8]| {
        let mut cropped = Vec::with_capacity(region.width as usize * region.height as usize * 4);
        for y in region.top..region.top + region.height {
            let start = y as usize * stride + region.left as usize * 4;
            cropped.extend_from_slice(&buffer[start..start + region.width as usize * 4]);
        }
        cropped
    };
    let mut cropped = crop(rgba);
    let cropped_previous = previous.map(crop);

    let mut frame = indexed_frame(&cropped, cropped_previous.as_deref(), region)
        .unwrap_or_else(|| Frame::from_rgba_speed(region.width, region.height, &mut cropped, 10));
    frame.left = region.left;
    frame.top = region.top;
    frame.delay = delay.min(u16::MAX as u32) as u16;
    frame.dispose = DisposalMethod::Keep;
    frame
}

/// Index the pixels against an exact palette, mapping pixels unchanged from
/// the previous frame to a shared transparent index — the previous frame
/// shows through, so runs of "no change" compress to almost nothing.
/// Lossless, and faster than quantizing. Returns `None` for frames with
/// more than 255 distinct colors (anti-aliased text in a full frame can
/// exceed that); the caller falls back to quantization.
fn indexed_frame(rgba: &[u8], previous: Option<&[u8]>, region: &Region) -> Option<Frame<'static>> {
    const TRANSPARENT: u8 = 0;
    let mut lookup: HashMap<[u8; 3], u8> = HashMap::new();
    let mut palette = vec![0, 0, 0];
    let mut pixels = Vec::with_capacity(rgba.len() / 4);
    for (i, pixel) in rgba.chunks_exact(4).enumerate() {
        if let Some(previous) = previous
            && previous[i * 4..i * 4 + 4] == *pixel
        {
            pixels.push(TRANSPARENT);
            continue;
        }
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let index = match lookup.get(&rgb) {
            Some(&index) => index,
            None => {
                if lookup.len() == 255 {
                    return None;
                }
                let index = lookup.len() as u8 + 1;
                lookup.insert(rgb, index);
                palette.extend_from_slice(&rgb);
                index
            }
        };
        pixels.push(index);
    }
    Some(Frame {
        width: region.width,
        height: region.height,
        palette: Some(palette),
        transparent: previous.is_some().then_some(TRANSPARENT),
        buffer: pixels.into(),
        ..Frame::default()
    })
}
