//! Recording backend: the demo "camera". Drives the real app through the
//! headless test harness and records every keypress as one GIF frame.
//!
//! Each simulated key goes through [`TestApp::key_with`] (real event
//! handling, real rendering), the resulting terminal buffer is rendered to
//! SVG by the snapshot harness, rasterized with resvg, and appended to the
//! GIF with the delay the script vocabulary assigns. Identical consecutive
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
use pure_tui::test_harness::{CellMetrics, TestApp};
use resvg::{tiny_skia, usvg};
use tdoc::Document;

/// Tighter rows than the snapshot default: 18px is DejaVu Sans Mono's
/// natural line height at 16px, so the box-drawing characters of menu
/// borders connect cleanly across rows instead of leaving gaps.
const METRICS: CellMetrics = CellMetrics {
    width: 10,
    height: 18,
    baseline: 14,
};

struct RecordedFrame {
    rgba: Vec<u8>,
    delay: u32,
}

/// Records a scripted Pure session into an animated GIF.
pub struct Backend {
    app: TestApp,
    options: usvg::Options<'static>,
    frames: Vec<RecordedFrame>,
    width: u16,
    height: u16,
    dump_dir: Option<PathBuf>,
}

impl Backend {
    /// Start a session with an empty document in a `columns`×`rows`
    /// terminal and capture the first frame.
    pub fn start(columns: u16, rows: u16) -> Self {
        let app = TestApp::with_path(columns, rows, Document::new(), PathBuf::from("demo.md"));

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

        let mut backend = Self {
            app,
            options,
            frames: Vec::new(),
            width: 0,
            height: 0,
            dump_dir,
        };
        backend.capture(0);
        backend
    }

    /// Feed one key press through the real event handling and capture the
    /// resulting frame.
    pub fn press(&mut self, code: KeyCode, modifiers: KeyModifiers, delay: u32) {
        self.app.key_with(code, modifiers);
        self.capture(delay);
    }

    /// Extend the time the current frame stays on screen by `cs` (10ms
    /// units).
    pub fn hold(&mut self, cs: u32) {
        if let Some(last) = self.frames.last_mut() {
            last.delay += cs;
        }
    }

    /// Encode all captured frames into a GIF at `path`.
    pub fn finish(self, path: &Path) -> Result<()> {
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

    /// Rasterize the current frame and append it, or — when nothing visible
    /// changed — extend the previous frame's delay instead.
    fn capture(&mut self, delay: u32) {
        let svg = self.app.svg_with(METRICS);
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
