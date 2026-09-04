# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While pre-1.0, the minor version is bumped for breaking changes.

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Added

- **Configuration file** — an optional TOML config at `~/.config/pure/config.toml`
  (honoring `XDG_CONFIG_HOME`); a missing, unreadable, or invalid file falls back
  to defaults. First setting: `caret_affinity` (default `true`) — an extra caret
  stop at inline-style/link boundaries that controls whether text you type there
  joins the run; set `false` to step across such boundaries in a single press. (#40)
- **Block model for containers** (quotes and lists): **Wrap inside…** (`Esc .`)
  wraps the paragraph or selection in a new container (Quote, Numbered/Bullet
  List, Checklist) while preserving the inner paragraph types; **Select parent**
  (`Esc ,`) targets the enclosing container to convert it, unwrap it, or climb a
  level. `[` / `Shift+Tab` now also lifts a paragraph out of a quote, not just a list. (#40)
- **Horizontal rules** — a thematic break, via **Insert > Horizontal Rule**. A
  rule always sits at the top level: inserting one mid-paragraph splits the
  paragraph around it, and from inside a list or quote it lands below that whole
  block. The caret continues below the rule. Backspace and Delete remove it, on
  the rule itself and from the edge of the block above or below. A rule holds no
  text, so the context menu's paragraph types are disabled while the caret rests
  on one, and the status bar names it "Horizontal Rule". The terminal draws
  tdoc's centered `───── • ─────` ornament, since a cell grid cannot draw a
  sub-cell line. (#42)
- **Definition lists** — terms paired with their definitions, via the context
  menu's **Definition List** entry below Checklist (no number shortcut: `0`–`9`
  are all taken). Terms render bold and definitions indent beneath them; a
  definition holds whole paragraphs, so anything can go inside one. Round-trips
  through HTML (`<dl>`/`<dt>`/`<dd>`) and Markdown.

  **Enter** alternates between the two halves, so a glossary is typed straight
  through: at the end of a term it opens the definition, at the end of a
  definition it starts the next term, and on an empty term it leaves the list.
  **Ctrl+P** adds another paragraph to the current definition, as it does inside
  a list item. **Tab** folds a term, with its definition, into the definition
  above; **Shift+Tab** turns a definition into the next term, taking the
  paragraphs below it along. The two are exact opposites.

  Picking any **other paragraph type** lifts that line out of the list instead
  of converting the list in place: a definition leaves as a new paragraph below
  the list (its term keeps an empty definition), and a term takes its definition
  along only when no other term is left to head it. The rest stays a list, and
  lists **split and rejoin** as lines leave and return, so a round trip through
  another type never leaves a seam. Picking **Definition List** again dissolves
  the whole list. Previously a term ignored the menu and a definition changed
  type in place.

  **Known limitation:** formats without these elements degrade them on save.
  FTML drops a rule and flattens a definition list into plain paragraphs;
  Gemtext writes a `---` line and plain text instead. The text survives, the
  structure does not, and Pure does not warn about this yet. `.html` and `.md`
  keep both. (#42)

### Changed

- **Editor/layout engine carved out to the shared `rutle` crate**, replacing
  Pure's homegrown layouter. Pure and its sibling editor Piki now share one
  structured-editor/layout core, and both resolve the same `tdoc` so
  `tdoc::Document` crosses the crate boundary unchanged. Retires ~26,000 lines
  (`src/editor/`, `editor_display.rs`, `render.rs`, and their tests), replaced by
  a thin ratatui adapter (`ratatui_draw_context.rs`). Rendering, cursor movement,
  selection, reveal codes, and tables are at visual parity; SVG snapshots updated.
  (#40)
- **Cursor navigation and redraw are much faster** — ~2.3–2.9× per keystroke
  versus the old layouter (e.g. USER-GUIDE.md: 1.76 vs. 5.09 ms/key). Two
  follow-ups — making `resize()`/padding updates idempotent so an unchanged frame
  keeps the layout cache, and memoizing the status-bar word count — bring every
  tested case under 300 µs/key (down from up to ~1.95 ms). Measured by the new
  end-to-end `examples/bench_cursor.rs`. (#40)
- **`tdoc` bumped to `0.12` and `rutle` to `0.6.0`** for
  `Paragraph::HorizontalRule` and `Paragraph::DefinitionList`. rutle now draws
  reveal-codes tags as WordPerfect-style boxes in pixel backends; the terminal
  keeps the bracketed `[Bold>` / `<Bold]` text tags, so nothing changes on
  screen. (#42)
- The status bar now advertises the formatting menu as **`Esc:Format`**, ahead
  of `F10:Menu ^S:Save ^Q:Quit`; on a narrow terminal it is the first hint to
  go. The popup's title is now **Format** (was "Context Menu"), matching the
  Format drop-down. (#43)

### Fixed

- The status bar's **word count** now reaches into every place a paragraph can
  hold text. It previously stopped at one level of checklist nesting and skipped
  tables entirely, so **table cells counted as zero words**. Definition-list
  terms and definitions are counted too, at any nesting. (#42)
- The status bar's **line count** accounts for the blank rows a horizontal rule
  reserves, keeping its line numbers in step with what the engine laid out. (#42)
- The format round-trip tests used one fixed temp path per extension, so two
  tests saving the same extension could race. Each call now gets its own file. (#42)
- Converting a paragraph to a quote (`Esc 5`) now **converts** it (a heading
  becomes a plain quote) instead of nesting it, matching lists. A single-text
  container acts as a leaf, so `Esc 5`/`8`/`0` round-trip and the breadcrumb shows
  the effective type. Converting to a list merges with an adjacent same-kind list. (#40)
- Changing list type over a **selection spanning two or more items** carves just
  those items out into a new list (splitting the original) instead of converting
  the whole list; a plain cursor still converts the whole list. (#40)
- **Ctrl+P** now inserts a *continuation paragraph* in the current item instead of
  starting a new item. (#40)
- **Enter** on an empty nested line no longer dissolves the item: an empty trailing
  line becomes a new empty item, and repeated Enter steps out one level at a time
  (out of the list, then out of an enclosing quote). An empty line in a quote exits
  the quote instead of adding another blank quoted line. (#40)
- **Shift+Enter** / **Ctrl+Enter** insert a hard line break again — Pure enables
  the terminal's keyboard-enhancement protocol where available (Ctrl+J remains a
  fallback). (#40)
- Nested list items now use proper per-level indentation in the terminal
  (previously collapsed to a flat indent). (#40)
- List content (continuation paragraphs, code blocks) aligns with the item's text
  rather than a fixed bullet width — fixing misindented continuations and
  inconsistent number padding in two-digit numbered lists. (#40)
- **Tab** / **Esc ]** on a paragraph that follows a container now nests it into
  that container (list item, checklist item, or quote child) instead of inserting
  spaces — including inside a quote, for multi-paragraph selections, and for
  paragraphs *before* a list (prepended); a paragraph between two same-kind lists
  is merged into one. (#40)
- **Tab** on the first item of a list directly after a quote pulls it **into** the
  quote as a nested list (bullet/number preserved), removing the emptied outer
  list; **Shift+Tab** on a list item inside a quote reverses this, lifting it out
  while keeping it a list item. (Enter on an empty item, and toggling a list off,
  still produce a plain paragraph.) (#40)
- Indenting a list item under a sibling that already has a sublist merges into it
  even across kinds; the first item of a list following another list can Tab
  straight into that list; checklist items after a bullet/numbered list nest as a
  sub-checklist (checkboxes kept), and **Shift+Tab** lifts them back to a
  top-level checklist rather than to text. (#40)

### Removed

- **Tab** no longer inserts whitespace as a fallback — it is dedicated to structure
  (indent / Shift+Tab unindent) and does nothing when there is nothing to indent. (#40)

### Misc

- Replaced the Criterion micro-benchmark harness (`benches/performance`) with
  `examples/bench_cursor.rs`, which drives the public `App` API over a headless
  `TestBackend` — so it also builds on the pre-rutle codebase for head-to-head
  comparison in a `git worktree`. (#40)

## [0.6.0] - 2026-06-24

### Added

- HTML and Gemini import/export. Opening a `.html`/`.htm`/`.xhtml` or
  `.gmi`/`.gemini` file now parses it with tdoc's dedicated HTML or Gemini
  parser instead of the FTML parser, and saving writes the matching format —
  HTML as a complete, standalone styled page that opens directly in a browser,
  Gemini as Gemtext. The format follows the file's extension (including on Save
  As), joining the existing Markdown and FTML support. Formatting that a target
  format can't represent (e.g. embedded images in HTML) is dropped on save;
  only FTML is guaranteed to round-trip losslessly.
- Preliminary read-only table support. Tables in opened documents (e.g.
  Markdown `| ... |` tables) are rendered with tdoc's ANSI formatter as a
  box-drawn, multi-line block. The cursor can be positioned anywhere within a
  table — vertical and horizontal navigation pass transparently through it —
  but its contents cannot yet be edited: typing, deletion, paragraph breaks,
  type changes, restyling, and linking are all blocked inside a table, and
  adjacent backspace/delete will not merge away or remove the block. Tables
  round-trip unchanged on save.

## [0.5.0] - 2026-06-13

### Added

- Link editing: "Edit Link..." in the context menu (or Ctrl+K) opens a modal
  dialog with the link's visible text and target URL plus Open, Cancel, and
  Save buttons. With the cursor inside a link it edits that link; over a
  selection it turns the selected text into a link; otherwise it inserts a new
  one. Clearing the URL removes the link, leaving the text in place. Tab and
  Shift+Tab move between the fields and buttons, Space activates the focused
  button, and Enter always saves while Esc always cancels. The Open button
  launches the URL in the system browser (`xdg-open`/`open`/`start`). Links
  remain non-clickable in the editor itself, so a click only places the
  cursor — handy for editing a link in place.
- Open... (Ctrl+O) and Save As... in the File menu. Both show a modal file
  dialog: a path input with shell-style Tab completion above a live listing
  of the directory it points into, navigable with the arrow keys (Enter
  descends into directories). Opening loads FTML or Markdown based on the
  extension and starts a new document for nonexistent paths; Save As writes
  in the format of the new extension, so saving a `.md` copy of an `.ftml`
  document converts it. Destructive accepts — opening over unsaved changes,
  overwriting another file — need a confirming second Enter. (#34)
- New (Ctrl+N) in the File menu starts an untitled document, and Pure can
  now be started without a filename argument to do the same. Untitled
  documents show "Untitled" in the status bar, and saving one opens the
  Save As dialog to ask for a name first. With unsaved changes, New warns
  in the status line and only a repeated New discards them. (#34)
- Clipboard support: Ctrl+X/Ctrl+C cut/copy the selection and Ctrl+V pastes,
  all also available in the Edit and context menus. Ctrl+C therefore no
  longer quits Pure; use Ctrl+Q for that. The internal clipboard keeps the selection
  as document structure, so pasting within Pure restores inline styles,
  paragraph types, and list structure. Copied text also reaches the system
  clipboard — as plain text, with blank lines between paragraphs — through
  the terminal with the OSC 52 escape sequence, and pasting from other
  applications works through the terminal's own paste shortcut (bracketed
  paste), arriving as a single undoable edit that turns blank lines back
  into paragraph breaks. (#33)
- Inline styles now stack: applying a style to already-styled text layers it
  on top instead of replacing it, so e.g. bold and highlight combine and
  render together. Reveal codes show the nesting, and deleting a tag there
  removes just that style while the styles stacked inside it survive. (#32)
- A menu bar in the typical TUI style: File, Edit, Format, and View menus
  across the top of the screen, opened with F10 or an Alt+letter accelerator
  and driven with the keyboard. (#29)
- Basic undo/redo support (Ctrl+Z / Ctrl+Y). Consecutive typing, deleting, and
  backspacing coalesce into single undo steps. (#26)
- An SVG snapshot testing harness: tests drive the real application headlessly
  through synthetic key and mouse events and snapshot the rendered terminal as
  deterministic SVG, so styling, selection, and cursor placement diff as text
  and open in any browser. (#27)
- We're now automatically adding release notes using the CI. (#30)

### Fixed

- Applying an inline style to a selection no longer makes the cursor jump to
  the beginning of the selection; it stays at the position it had before. (#28)
- With reveal codes shown, pressing Backspace directly behind a start tag like
  `[Italic>` removes the formatting again instead of merging the paragraph
  into the previous one, and the cursor stays at the position of the removed
  tag. The same root cause made Backspace at a style-span boundary merge
  paragraphs with reveal codes hidden, too. (#28)
- Inserting or deleting a character in a line that shows reveal codes later in
  the line no longer hides those codes (while keeping the formatting) until
  the next full re-render. (#28)
- Changing the indent level of paragraphs near nested lists no longer leaves
  behind empty list items that could not be removed. This covered several
  cases, including indenting the first item of a nested list, indenting a sole
  list item into a preceding quote, and unindenting the only child of a quote.
  (#35)
- The Format menu's "Indent more" is now enabled only when indenting would
  actually do something, matching what the operation performs. (#35)
- Unindenting a paragraph from the middle of a list now splits the list at that
  spot instead of dropping the paragraph below the entire list, where it
  appeared to vanish. (#35)
- Splitting a list by unindenting now redraws the whole document immediately
  with the cursor in the right place, instead of leaving the view truncated and
  the cursor misplaced until moving away and back. (#35)
- Pressing Backspace or Delete with an active selection now deletes the whole
  selection instead of just the single character next to the cursor. The
  word-wise variants (Ctrl/Alt+Backspace and Ctrl/Alt+Delete) do the same.
  (#37)

### Misc

- The README's demo GIF is now recorded automatically: `examples/demo`
  scripts a Pure session as a series of simulated key presses, renders every
  frame through the SVG snapshot harness, and assembles the result into
  `demo.gif` — so the demo can be re-recorded with one command whenever the
  interface changes. Without the `recorder` feature, `cargo run --example
  demo` plays the same script live in the terminal instead. (#31)

## [0.4.2] - 2026-03-01

### Fixed

- Cursor movement around reveal-code tags.

## [0.4.1] - 2026-02-28

### Changed

- Upgraded tdoc without its remote feature, dropping the SSL dependencies.

## [0.4.0] - 2026-01-11

### Changed

- Upgraded tdoc for transparent Markdown frontmatter support and a Markdown
  code block fix.

## [0.3.0] - 2025-12-03

### Added

- Support for selections spanning multiple paragraphs. (#21)

### Changed

- Improved styling of headers and nested lists. (#22)
- Improved default colors: highlights, selection, scrollbar, and the menus'
  structural characters. (#17)
- The cursor is a blinking underscore while a selection is active. (#16)

### Fixed

- Coloring of structural characters in nested quotes. (#20)
- Rendering of indented checklists. (#19)
- Cursor positioning with mouse clicks. (#18)

## [0.2.2] - 2025-11-29

### Changed

- Improved mouse selection speed and accuracy. (#15)

### Fixed

- Setting paragraph types that involve structural changes. (#14)

## [0.2.1] - 2025-11-29

### Fixed

- Joining and splitting paragraphs. (#12)

## [0.2.0] - 2025-11-28

### Added

- New scrollbar implementation. (#5)

### Changed

- Refactored the layouting algorithm for incremental updates and improved
  cursor and selection tracking. (#11)
- Improved status bar styling. (#6)

### Fixed

- Cursor jumping, by caching target cursor positions correctly. (#4)

## [0.1.0] - 2025-11-25

Initial release.

<!-- next-url -->
[Unreleased]: https://github.com/roblillack/pure/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/roblillack/pure/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/roblillack/pure/compare/v0.4.2...v0.5.0
[0.4.2]: https://github.com/roblillack/pure/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/roblillack/pure/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/roblillack/pure/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/roblillack/pure/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/roblillack/pure/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/roblillack/pure/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/roblillack/pure/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/roblillack/pure/releases/tag/v0.1.0
