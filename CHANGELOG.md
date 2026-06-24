# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While pre-1.0, the minor version is bumped for breaking changes.

<!-- next-header -->

## [Unreleased] - ReleaseDate

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
