# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While pre-1.0, the minor version is bumped for breaking changes.

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Added

- A general block model for containers (quotes and lists). The context menu gains
  a **Wrap inside…** entry (`Esc .`) that wraps the current paragraph or selection
  in a new container of your choice (Quote, Numbered/Bullet List, Checklist)
  while **preserving** the inner paragraph types — so you can, for example, nest a
  heading inside a quote on purpose. Its counterpart, **Select parent** (`Esc ,`),
  retargets a short menu at the enclosing container so you can convert it to
  another kind, unwrap it, or climb another level up. `[` (and `Shift+Tab`) now
  also lifts a paragraph out of a quote, not just out of a list.

### Fixed

- Converting a paragraph to a quote (`Esc 5`) now **converts** it — turning a
  heading into a plain quote — instead of nesting the heading inside the quote,
  matching how lists already behave. A container that holds a single text
  paragraph now behaves like a leaf of the container's type for block-type
  changes, so `Esc 5` (quote), `Esc 8` (bullet), `Esc 0` (text) round-trip
  cleanly, and the status-bar breadcrumb's rightmost entry shows that effective
  type. Converting a paragraph to a list now merges it with adjacent same-kind
  lists into one list.
- Changing the list type with a **selection spanning two or more items of one
  list** now carves just those items out into a list of the chosen kind, splitting
  the original list into two or three, instead of converting the whole list. (A
  plain cursor still converts the whole list.)
- **Ctrl+P** now inserts a *continuation paragraph* inside the current list item
  (another paragraph in the same item) instead of behaving like Enter and
  starting a new item.
- **Shift+Enter** and **Ctrl+Enter** insert a hard line break again: Pure now
  enables the terminal's keyboard-enhancement protocol where available, so these
  combinations are delivered as distinct keys instead of a plain Enter. (Ctrl+J
  remains a fallback for terminals without the protocol.)
- Nested list items now render with the proper per-level indentation in the
  terminal (the cell backend previously collapsed all nesting to a flat indent).
- Content inside a list (continuation paragraphs, code blocks) now aligns with the
  item's text rather than a fixed bullet width — most visibly in numbered lists
  with two-digit numbers, where continuations were previously mis-indented and the
  number padding was inconsistent across items.
- **Tab** / **Esc ]** ("Indent more") on a top-level paragraph that follows a
  container now nests it into that container — as a new list item, a new checklist
  item, or a child of the preceding quote — instead of just inserting spaces.
  (Previously this only worked on existing list items.) A paragraph sandwiched
  between two same-kind lists is joined into a single merged list. This also works
  for a multi-paragraph selection, and for paragraphs *before* a list (they are
  prepended to it) as well as after.
- Indenting a list item under a sibling that already contains a sublist now merges
  it into that sublist even when the kinds differ (a bullet indented under an item
  ending in a numbered sublist joins the numbered list, and vice versa), instead
  of creating a second sublist next to it. The first item of a list that directly
  follows another list can now be indented (Tab) straight into that preceding
  list, merging under its last item. This includes selecting one or more items of
  a checklist that follows a bullet/numbered list and pressing Tab: they nest
  under that list's last item as a sub-checklist, keeping their checkboxes.
  **Shift+Tab** on such nested checklist items reverses this, lifting them back
  out to a top-level checklist (rather than converting them into text paragraphs).

### Removed

- **Tab** no longer inserts whitespace as a fallback. It is now dedicated to list
  and paragraph structure (Tab indents / Shift+Tab unindents); with nothing to
  indent it does nothing rather than inserting spaces.

### Changed

- The document layout and editing engine has been carved out of Pure into a new
  shared crate, `rutle` (`rutle = "0.1.0"` on crates.io), replacing Pure's
  homegrown layouter. Pure and its sibling editor Piki now build on the same
  structured-editor/layout core; both resolve the identical crates.io
  `tdoc 0.11.0`, so `tdoc::Document` values cross the crate boundary unchanged.
  On the Pure side this retires roughly 26,000 lines of bespoke editor code —
  the entire `src/editor/` module, `editor_display.rs`, and `render.rs`, along
  with their test suites — replaced by a thin ratatui adapter
  (`ratatui_draw_context.rs`) that paints rutle's layout into the terminal.
  Rendering, cursor movement, selection, reveal codes, and table display were
  brought back to visual parity with the previous implementation, and all SVG
  snapshots updated to match. (#40)
- Cursor navigation and redraw are substantially faster. Measured with a new
  end-to-end benchmark (`examples/bench_cursor.rs`) that drives the real
  application headlessly — pressing Down from the top to the bottom of a
  document, each press a full `handle_event` plus redraw, timing the median
  per-keystroke cost — the shared core is ~2.3–2.9x faster per keystroke than
  the old layouter across every document and reveal-codes combination (e.g.
  USER-GUIDE.md at 1098 lines: 1.76 ms/key vs. 5.09 ms/key; README.md at 262
  lines: 0.46 ms/key vs. 1.06 ms/key). Two follow-up optimizations cut the cost
  much further — making rutle's `resize()`/horizontal-padding updates idempotent
  so an unchanged frame no longer throws away the layout cache, and memoizing
  the status-bar word count so it is no longer recomputed over the whole
  document every frame — bringing every tested case under 300 µs per keystroke
  (down from up to ~1.95 ms): USER-GUIDE.md 257 µs, ARCHITECTURE.md 227 µs,
  README.md 173 µs, each a touch faster again with reveal codes on. (#40)

### Misc

- Replaced the old Criterion micro-benchmark harness (`benches/performance`)
  with `examples/bench_cursor.rs`, which measures cursor responsiveness through
  the public `App` API over a headless `TestBackend`. Because it touches only
  public API, it also compiles unchanged on the pre-rutle codebase, so the two
  implementations can be benchmarked head-to-head in a `git worktree`. (#40)

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
