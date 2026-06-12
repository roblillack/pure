//! Live playback backend: replays the scripted key presses in the real
//! terminal, at the same pace the recording would use, so the demo can be
//! watched like a screencast. Press q, Esc, or Ctrl+C to stop early.

use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use pure_tui::app::{App, DocumentFormat};
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect};
use tdoc::Document;

/// Plays a scripted Pure session in the real terminal.
pub struct Backend {
    app: App,
    terminal: Terminal<CrosstermBackend<Stdout>>,
    aborted: bool,
}

impl Backend {
    /// Put the terminal into raw mode and show an empty document. Playback
    /// renders into a fixed `columns`×`rows` viewport so the layout — and
    /// with it every scripted cursor move — matches the recording exactly,
    /// whatever size the real terminal has.
    pub fn start(columns: u16, rows: u16) -> Self {
        let app = App::new(
            Document::new(),
            PathBuf::from("demo.md"),
            DocumentFormat::Markdown,
            None,
        );
        if let Ok((real_columns, real_rows)) = crossterm::terminal::size()
            && (real_columns < columns || real_rows < rows)
        {
            eprintln!("note: the demo is sized for {columns}x{rows} and will be clipped");
        }
        enable_raw_mode().expect("enable raw mode");
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).expect("enter alternate screen");
        let terminal = Terminal::with_options(
            CrosstermBackend::new(stdout),
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, columns, rows)),
            },
        )
        .expect("create terminal");
        let mut backend = Self {
            app,
            terminal,
            aborted: false,
        };
        backend.draw();
        backend
    }

    /// Feed one key press through the real event handling and redraw.
    pub fn press(&mut self, code: KeyCode, modifiers: KeyModifiers, delay: u32) {
        if self.aborted {
            return;
        }
        self.app
            .handle_event(Event::Key(KeyEvent::new(code, modifiers)))
            .expect("handle event");
        self.draw();
        self.hold(delay);
    }

    /// Wait `cs` (10ms units), keeping the screen responsive: resizes
    /// redraw, and q / Esc / Ctrl+C abort the playback.
    pub fn hold(&mut self, cs: u32) {
        let deadline = Instant::now() + Duration::from_millis(u64::from(cs) * 10);
        while !self.aborted {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            if !event::poll(deadline - now).unwrap_or(false) {
                continue;
            }
            match event::read() {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                    let ctrl_c = key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL);
                    if ctrl_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                        self.aborted = true;
                    }
                }
                Ok(Event::Resize(..)) => self.draw(),
                _ => {}
            }
        }
    }

    /// Restore the terminal. `path` only applies to recording.
    pub fn finish(mut self, _path: &Path) -> Result<()> {
        disable_raw_mode().ok();
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            SetCursorStyle::DefaultUserShape
        )
        .ok();
        self.terminal.show_cursor().ok();
        println!(
            "Demo {}.",
            if self.aborted { "aborted" } else { "finished" }
        );
        Ok(())
    }

    fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| app.draw(frame))
            .expect("draw frame");
    }
}
