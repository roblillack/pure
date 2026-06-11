# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While pre-1.0, the minor version is bumped for breaking changes.

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Added

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
[Unreleased]: https://github.com/roblillack/pure/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/roblillack/pure/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/roblillack/pure/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/roblillack/pure/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/roblillack/pure/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/roblillack/pure/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/roblillack/pure/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/roblillack/pure/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/roblillack/pure/releases/tag/v0.1.0
