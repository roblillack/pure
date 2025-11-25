# Pure

[![Build Status](https://github.com/roblillack/pure/workflows/build-lint-test/badge.svg)](https://github.com/roblillack/pure/actions)
[![Crates.io](https://img.shields.io/crates/v/pure-tui.svg)](https://crates.io/crates/pure-tui)

A modern terminal-based word processor for editing FTML and Markdown documents.

Pure brings structured document editing to the command line. Unlike traditional text editors that work with plain text, Pure works with semantic document elements—headings, lists, quotes, code blocks—allowing you to focus on content rather than formatting syntax.

> **Note**: Pure is currently in alpha. While the core functionality is stable and usable for daily work, some features are still under development and you may encounter rough edges. Please report any issues you find!

## Quick Start

```bash
# Install from crates.io
cargo install pure-tui

# Open or create a document
pure document.ftml

# Or work with Markdown
pure notes.md
```

## Features

### Document Structure

Pure documents are made up of **paragraphs**, each with a specific type:

**Paragraph Types:**
- **Text** - Regular body paragraphs
- **Headings** (H1, H2, H3) - Section headings at different levels
- **Quote** - Block quotations
- **Code Block** - Preformatted code or monospaced text
- **Numbered List** - Ordered list items
- **Bullet List** - Unordered list items
- **Checklist** - Task items with checkboxes (`[ ]` and `[x]`)

**Inline Styles:**
- **Bold** - Strong emphasis
- **Italic** - Emphasis
- **Underline** - Underlined text
- **Strikethrough** - Deleted or deprecated text
- **Highlight** - Highlighted text
- **Code** - Inline code or monospaced text
- **Hyperlink** - Clickable links

### Visual Editing

Pure provides an intuitive editing experience:

- **Word Wrapping**: Automatic text flow without manual line breaks
- **Mouse Support**: Click to position cursor, drag to select, double-click to select words, triple-click for paragraphs
- **Reveal Codes**: Press F9 to see the underlying formatting structure (inspired by WordPerfect)
- **Context Menu**: Press Esc to access all formatting options
- **Real-time Rendering**: See your formatted document as you type

### Format Support

- **Native FTML**: Full support for reading and writing FTML documents
- **Markdown**: Import and export Markdown files with round-trip support for most features
- **HTML Export**: FTML documents are valid HTML5 and can be opened in any browser

### Keyboard-Driven Workflow

Pure is designed for efficiency with comprehensive keyboard shortcuts:

- **Ctrl+S** - Save document
- **Ctrl+Q** - Quit (with save prompt)
- **Esc** - Open context menu
- **F9** - Toggle reveal codes
- **Ctrl+P** - Create new paragraph at same level
- **Ctrl+J** - Insert line break within paragraph (useful for addresses, poetry, etc.)
- **Arrow keys** - Navigate (Ctrl+Left/Right for word jumps)
- **Context menu shortcuts** - Quick paragraph type changes (0-9)

See the [User Guide](USER-GUIDE.md) for comprehensive documentation.

## Installation

### From Crates.io

If you have Rust installed:

```bash
cargo install pure-tui
```

### From Source

```bash
git clone https://github.com/roblillack/pure.git
cd pure
cargo build --release
cargo install --path .
```

### Binary Releases

Pre-built binaries for Linux, macOS, and Windows are available on the [releases page](https://github.com/roblillack/pure/releases).

## Usage

### Creating and Editing Documents

```bash
# Start with a new FTML document
pure newfile.ftml

# Open an existing document
pure document.ftml

# Work with Markdown
pure notes.md

# Open and convert from HTML
pure webpage.html
```

### Essential Keyboard Shortcuts

**Navigation:**
- Arrow keys - Move cursor
- Ctrl+Left/Right - Jump by word
- Home/End - Start/end of line
- PageUp/PageDown - Scroll by page

**Editing:**
- Enter - New paragraph
- Shift+Enter - Newline within paragraph
- Backspace/Delete - Remove text
- Ctrl+W - Delete word backward

**Formatting:**
- Esc - Open context menu
- 0-9 (in menu) - Change paragraph type
- b/i/u/s (in menu) - Toggle inline styles

**File Operations:**
- Ctrl+S - Save
- Ctrl+Q - Quit

**Special Features:**
- F9 - Reveal codes mode

## What is FTML?

**FTML (Formatted Text Markup Language)** is Pure's native document format. It's a strict subset of HTML5, designed for simplicity and ease of processing. When you save a document in Pure, it's saved as valid HTML that can be opened in any web browser.

FTML provides the essential features needed for rich text documents—such as paragraph structures, headings, lists, and inline styles—without the complexity of full HTML or Markdown. It's ideal for straightforward text content like notes, documentation, memos, and help files.

**Key features:**

- **Simple structure**: Only the most essential formatting options
- **HTML-compatible**: Valid FTML is valid HTML5
- **Diffable**: Designed to work well with version control
- **Unambiguous**: Usually only one way to express something

For the full FTML specification and the underlying library, see [tdoc](https://github.com/roblillack/tdoc).

## Architecture

Pure is built on top of [tdoc](https://github.com/roblillack/tdoc), a comprehensive Rust library for handling FTML, Markdown, HTML, and Gemini documents. The editor architecture features:

- **Efficient Rendering**: Paragraph-level caching for smooth performance with large documents
- **Visual Cursor Tracking**: Maintains cursor position across wrapping and reformatting
- **Structural Editing**: Direct manipulation of document structure, not just text
- **Reveal Codes**: Inspired by WordPerfect, showing exact formatting boundaries

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed technical documentation.

## Implementation Status

Pure is under active development. Current status:

**Core Editing:**
- [x] Text input with full Unicode support
- [x] Cursor navigation (keyboard and mouse)
- [x] Text selection (visual selection with mouse and keyboard)
- [x] Word wrapping with dynamic reflow

**Paragraph Types:**
- [x] Text paragraphs
- [x] Headings (H1, H2, H3)
- [x] Ordered lists with nesting
- [x] Unordered lists with nesting
- [x] Checklists with checkboxes
- [x] Block quotes with nesting
- [x] Code blocks

**Inline Styles:**
- [x] Bold
- [x] Italic
- [x] Underline
- [x] Strikethrough
- [x] Highlight
- [x] Inline code
- [ ] Hyperlinks (displayed but not editable yet)

**File Operations:**
- [x] FTML reading and writing
- [x] Markdown import and export
- [x] HTML import (basic)

**User Interface:**
- [x] Context menu (Esc)
- [x] Reveal codes mode (F9)
- [x] Mouse support (click, drag, select, scroll)
- [x] Status bar with document info

**Advanced Features:**
- [ ] Undo/Redo
- [ ] Find and replace
- [ ] Multiple documents with tabs
- [ ] System clipboard integration
- [ ] Interactive hyperlink editing

## Documentation

- **[User Guide](USER-GUIDE.md)** - Comprehensive documentation for all features
- **[Architecture](ARCHITECTURE.md)** - Technical overview of editor internals
- **[FTML Specification](https://github.com/roblillack/ftml)** - Format details
- **[tdoc Library](https://github.com/roblillack/tdoc)** - Document handling library

## Building from Source

```bash
# Clone the repository
git clone https://github.com/roblillack/pure.git
cd pure

# Build release version
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench

# The binary will be in target/release/pure
./target/release/pure
```

## Use Cases

Pure is ideal for:

- **Email Writing**: Compose rich-formatted emails with structure and style. Pure integrates seamlessly with TUI-based email clients like [Elma](https://github.com/roblillack/elma)
- **Technical Writing**: Documentation with code examples and structured content
- **Note-Taking**: Quick notes with proper formatting and organization
- **Meeting Notes**: Structured notes with checklists and action items
- **Terminal Workflows**: Document editing without leaving the command line
- **Git-Friendly Documents**: Version-controlled content with clean diffs
- **Markdown Alternative**: Structured editing instead of syntax memorization

## Philosophy

Pure embraces several design principles:

1. **Structure over Syntax**: Work with document elements, not markup syntax
2. **Keyboard Efficiency**: Every feature accessible via keyboard shortcuts
3. **Visual Clarity**: See your formatted document, not raw markup (unless you want to with F9)
4. **Format Preservation**: Round-trip between FTML and Markdown without loss
5. **Terminal Native**: Built for terminal use, not a web editor in disguise
6. **Git Friendly**: FTML format designed for version control

## Related Projects

- **[tdoc](https://github.com/roblillack/tdoc)** - The document handling library powering Pure
- **[ftml (Go)](https://github.com/roblillack/ftml)** - Original FTML implementation in Go

## License

MIT

## Contributing

Contributions are welcome! Pure is under active development. Please see the [issues page](https://github.com/roblillack/pure/issues) for planned features and known bugs.

Areas where contributions would be especially valuable:

- Undo/redo implementation
- Clipboard integration
- Find and replace functionality
- Additional export formats
- Performance optimizations
- Documentation improvements

## Acknowledgments

Pure is inspired by classic word processors like WordPerfect, bringing their structured editing approach to modern terminal environments. The FTML format and document handling are powered by [tdoc](https://github.com/roblillack/tdoc).
