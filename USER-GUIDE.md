# Pure User's Guide

A modern terminal-based word processor for editing FTML and Markdown documents

---

## Getting Started Documentation for Pure

Pure is a modern terminal-based word processor that brings structured document editing to the command line. Unlike traditional text editors that work with plain text, Pure works with semantic document elements—headings, lists, quotes, code blocks—allowing you to focus on content rather than formatting syntax.

### What You Need to Know

Pure uses two document formats:

**FTML (Formatted Text Markup Language)** is Pure's native format. FTML is a strict subset of HTML5, designed for simplicity and ease of processing. When you save a document in Pure, it's saved as valid HTML that can be opened in any web browser.

**Markdown** is supported for import and export. You can open existing Markdown files in Pure, and export your FTML documents to Markdown format.

### Documentation for Pure

This User's Guide provides comprehensive information about using Pure. The guide is organized into several sections:

#### The Basics

The Basics section introduces you to Pure's interface, fundamental concepts, and essential keyboard commands. Read this section to understand how Pure works and to learn the most important keys you'll use every day.

#### A Brief Lesson

The Brief Lesson walks you through creating, editing, and formatting a complete document. This hands-on tutorial helps you get comfortable with Pure's workflow.

#### Reference

The alphabetically-arranged, task-oriented sections in the Reference provide detailed information about specific features. Use the Reference when you need to know how to accomplish a particular task or when you want to learn more about a feature.

### Installation

Pure can be installed from source or from Crates.io.

#### From Crates.io

If you have Rust installed, install Pure with:

```
cargo install pure
```

#### From Source

To build and install from source:

```
git clone https://github.com/roblillack/pure.git
cd pure
cargo build --release
cargo install --path .
```

Your system must have Rust and Cargo installed. Contact your system administrator if you need assistance with installation.

---

## The Basics

### Starting Pure

Once Pure has been installed, you are ready to start the program.

First, open your terminal. Then follow one of the procedures below:

**To edit an existing document:**

```
pure document.ftml
```

**To create a new document:**

```
pure newfile.ftml
```

You can also open Markdown files:

```
pure document.md
```

### The Clean Screen

When you start Pure, you see the editing screen.

```
┌────────────────────────────────────────────────────────────┐
│                                                            │
│                                                            │
│                                                            │
│     Cursor →                                               │
│                                                            │
│                                                            │
│                                                            │
│                                                            │
└────────────────────────────────────────────────────────────┘
  Document (1 of 1)  │  Line 1  │  Position 1
         Status line
```

The **cursor** points to the current position in your document. It shows where text will be inserted when you type.

The **status line** displays information about your document and cursor position:

- The document number if you have multiple documents open
- The current line number
- The cursor position on the line

When you save a document, the status line displays the filename.

### Document Structure

Pure documents are made up of **paragraphs**. Each paragraph has a type:

- **Text** - Regular paragraphs for body text
- **Heading 1, 2, 3** - Section headings at different levels
- **Quote** - Block quotations
- **Code Block** - Preformatted code or monospaced text
- **Numbered List** - Ordered list items
- **Bullet List** - Unordered list items
- **Checklist** - Task list items with checkboxes

Within paragraphs, you can apply **inline styles** to text:

- **Bold** - Strong emphasis
- **Italic** - Emphasis
- **Underline** - Underlined text
- **Strikethrough** - Deleted or deprecated text
- **Highlight** - Highlighted text
- **Code** - Inline code or monospaced text
- **Hyperlink** - Clickable links

### Keys to Know

**Backspace**
Erases characters to the immediate left of the cursor as you type.

**Delete**
Deletes the character at the cursor position.

**Ctrl+Q**
Exits Pure. You will be prompted to save any unsaved changes.

**Ctrl+S**
Saves the current document.

**Esc** or **Ctrl+Space**
Opens the context menu, which provides quick access to formatting options and paragraph types.

**F9**
Toggles Reveal Codes mode, which displays the underlying formatting structure of your document.

**Arrow Keys**
Move the cursor up, down, left, and right.

**Mouse**
Pure supports mouse interaction including clicking to position the cursor, selecting text, double-clicking to select words, triple-clicking to select paragraphs, and scrolling to navigate through your document.

**Return** or **Enter**
Inserts a paragraph break, creating a new paragraph of the same type as the current one.

**Shift+Enter** or **Ctrl+Enter**
Inserts a newline within the current paragraph without creating a new paragraph.

### Function Keys

Pure uses keyboard shortcuts to access features quickly. All features can be accessed through keyboard shortcuts, keeping your hands on the keyboard for efficient editing.

For a complete list of keyboard shortcuts, see the Keyboard Shortcuts section later in this guide.

---

## Getting Help

If you need help while using Pure, several resources are available:

**README File**
The README file contains essential information about Pure, including installation instructions, basic usage, and keyboard shortcuts. View it on GitHub or in the Pure source directory.

**User's Guide**
This manual (the document you're reading now) describes how each feature works and provides step-by-step instructions for common tasks.

**Online Documentation**
Visit https://github.com/roblillack/pure for additional documentation, tutorials, and examples.

---

## A Brief Lesson

This lesson walks you through creating and formatting a simple document. Follow along to learn Pure's basic features.

### Creating the Document

After you start Pure, you are ready to start typing. If you make any mistakes, use Backspace to erase them.

When a line fills with text, the cursor automatically moves to the next line. This automatic wrapping is known as **word wrapping**.

**Type the following text without pressing Return at the end of each line:**

```
Study Abroad Program

I wanted to follow up on our discussion about starting a study abroad
program. I just returned from a conference in Illinois and I think I've
come up with an outline that will make us all very happy.
```

Notice that you didn't need to press Return at the end of each line. Pure wraps text automatically.

### Creating Headings

The first line should be a heading. Let's change its paragraph type.

1. Move the cursor to the first line (Study Abroad Program).

2. Press **Esc** to open the context menu.

3. Press **1** to change the paragraph to Heading 1.

The text is now formatted as a major heading.

### Editing the Document

You just created your first document, but you may want to make some changes.

#### Moving the Cursor

Move the cursor using the arrow keys on your keyboard:

- **Up/Down** arrows move between lines
- **Left/Right** arrows move between characters
- **Ctrl+Left/Ctrl+Right** move by words
- **Home** or **Ctrl+A** moves to the start of the line
- **End** or **Ctrl+E** moves to the end of the line

**Move the cursor to the word "very" in the last sentence.**

### Applying Inline Formatting

You can emphasize important words by applying inline styles.

#### Making Text Bold

1. Position your cursor at the beginning of the word "very".

2. Press **Esc** to open the context menu.

3. Navigate to "Toggle bold" using the arrow keys and press **Enter**.
   (You can also press **b** directly from the context menu)

4. Type **very** again. The new text appears in bold.

5. Press **Esc** and toggle bold again to turn off bold formatting.

Alternatively, you can select existing text and apply formatting to it:

1. Delete the word you just typed (we'll apply formatting to the existing word).

2. Position your cursor at the start of "very".

3. Move the cursor to the end of "very" while selecting. (Selection features are under development).

4. Press **Esc** and toggle bold to make the selected text bold.

### Creating Lists

Let's add a list of items to your document.

1. Press **Ctrl+P** to create a new paragraph at the same level as the current one.

2. Type: **We should consider the following locations:**

3. Press **Enter** to create a new paragraph.

4. Press **Esc**, then press **8** to change to a bullet list.

5. Type: **Italy - Focus on Renaissance art and architecture**

6. Press **Enter**. Pure automatically creates another bullet list item.

7. Type: **Japan - Study language and modern culture**

8. Press **Enter** and type: **Costa Rica - Environmental science program**

### Creating a Checklist

You can also create checklists for tasks or to-do items.

1. Press **Ctrl+P** to create a new paragraph.

2. Type: **Action items:**

3. Press **Enter** to create a new paragraph.

4. Press **Esc**, then press **9** to change to a checklist item.

5. Type: **Draft program proposal**

6. Press **Enter** and type: **Research partner universities**

7. Press **Enter** and type: **Prepare budget estimates**

To check off an item, position your cursor on the item and use the context menu to toggle its checked state.

### Saving Your Document

Now that you've created a document, save it:

1. Press **Ctrl+S**.

2. If this is a new document, you'll be prompted for a filename.

3. Type: **study-abroad.ftml** and press **Enter**.

Your document is saved.

### Exiting Pure

When you finish working on your document:

1. Press **Ctrl+Q**.

2. If you have unsaved changes, Pure will ask if you want to save.

3. Press **y** to save or **n** to exit without saving.

---

## Reference

The following sections provide detailed, alphabetically-arranged information about Pure's features.

### Backspace

**Purpose:** Deletes the character immediately to the left of the cursor.

**Keyboard Shortcut:** Backspace

#### To delete characters backward:

1. Position the cursor immediately after the character you want to delete.

2. Press **Backspace**.

The character is deleted and text to the right moves left to fill the space.

#### To delete a word backward:

1. Position the cursor at the end of or within the word you want to delete.

2. Press **Ctrl+W**, **Ctrl+Backspace**, or **Alt+Backspace**.

The word is deleted. If the cursor is in the middle of a word, text from the cursor to the beginning of the word is deleted.

#### Additional Information

When you delete at the beginning of a paragraph, the current paragraph merges with the previous paragraph. This removes the paragraph break.

---

### Bold, Italic, and Other Inline Styles

**Purpose:** Apply formatting to text within a paragraph to add emphasis or meaning.

**Keyboard Shortcut:** Esc (to open context menu), then navigate to style option

#### Available Inline Styles:

- **Bold** - Strong emphasis
- **Italic** - Emphasis
- **Underline** - Underlined text
- **Strikethrough** - Deleted or crossed-out text
- **Highlight** - Highlighted or marked text
- **Code** - Inline code or monospaced text
- **Hyperlink** - Web links (coming soon)

#### To apply an inline style to new text:

1. Press **Esc** to open the context menu.

2. Navigate to the style you want to apply (e.g., "Toggle bold").

3. Press **Enter** to activate the style.

4. Type your text. It appears with the formatting applied.

5. Press **Esc** and select the same style again to turn it off.

#### To apply an inline style to existing text:

1. Select the text you want to format. (Selection features are under development)

2. Press **Esc** to open the context menu.

3. Navigate to the style you want to apply.

4. Press **Enter**.

The formatting is applied to the selected text.

#### To remove inline formatting:

Use the context menu to toggle off styles, or use the "Clear formatting" option to remove all formatting from selected text. (Coming soon)

#### Additional Information

You can combine multiple inline styles. For example, text can be both bold and italic simultaneously.

In Reveal Codes mode (F9), you can see the exact boundaries of styled text, making it easier to edit formatting precisely.

---

### Checklists

**Purpose:** Create interactive task lists with checkable items.

**Keyboard Shortcut:** Esc, then 9 (from context menu)

#### To create a checklist:

1. Position the cursor where you want the checklist to begin.

2. Press **Esc** to open the context menu.

3. Press **9** to change the current paragraph to a checklist item.

4. Type the text for your first task.

5. Press **Enter** to create another checklist item.

6. Repeat steps 4-5 for each item.

#### To check or uncheck an item:

1. Position the cursor on the checklist item.

2. Press **Esc** to open the context menu.

3. Select the option to toggle the item's checked state.

4. Press **Enter**.

The item's checkbox updates to reflect its new state.

#### To convert a checklist back to regular text:

1. Position the cursor on a checklist item.

2. Press **Esc**, then press **0** to convert to a text paragraph.

#### Additional Information

Each checklist item can be checked or unchecked independently. Checklist items are saved with their checked/unchecked state in the FTML format.

When exporting to Markdown, checked items appear as `[x]` and unchecked items as `[ ]`.

---

### Code Blocks

**Purpose:** Display preformatted code or monospaced text.

**Keyboard Shortcut:** Esc, then 6 (from context menu)

#### To create a code block:

1. Position the cursor where you want the code block.

2. Press **Esc** to open the context menu.

3. Press **6** to change to a code block.

4. Type or paste your code.

5. Press **Shift+Enter** or **Ctrl+Enter** to create new lines within the code block.

6. Press **Ctrl+P** to create a new paragraph after the code block.

#### To convert text to a code block:

1. Position the cursor in the paragraph you want to convert.

2. Press **Esc**, then press **6**.

The paragraph becomes a code block with monospaced formatting.

#### Additional Information

Code blocks preserve exact spacing and indentation. Unlike regular text paragraphs, code blocks do not wrap—long lines extend beyond the visible area.

Use code blocks for programming code, command-line examples, or any text where exact formatting must be preserved.

For inline code within a paragraph (like variable names), use the inline code style instead of a code block.

---

### Context Menu

**Purpose:** Quick access to formatting options and paragraph types.

**Keyboard Shortcut:** Esc or Ctrl+Space

#### To open the context menu:

Press **Esc** or **Ctrl+Space**.

The context menu appears, showing available formatting options and paragraph types.

#### To navigate the context menu:

- Press **Up/Down** arrow keys to move between menu items
- Press **Enter** to execute the selected action
- Press **Esc** to close the menu without making a change

#### Quick Paragraph Type Changes:

When the context menu is open, you can press a number key to quickly change the current paragraph's type:

- **0** - Text paragraph
- **1** - Heading 1
- **2** - Heading 2
- **3** - Heading 3
- **5** - Quote
- **6** - Code block
- **7** - Numbered list
- **8** - Bullet list
- **9** - Checklist

#### Inline Styles (Current and Planned):

The context menu provides access to inline text formatting:

- Bold
- Italic
- Underline
- Strikethrough
- Highlight
- Code
- Hyperlink (coming soon)
- Clear formatting (coming soon)

#### Additional Information

The context menu is Pure's primary interface for formatting. All formatting features can be accessed through the context menu, making it easy to discover and use features without memorizing complex keyboard shortcuts.

---

### Cursor Movement

**Purpose:** Navigate through your document efficiently.

#### Character-by-Character Movement:

**Left Arrow** - Move one character to the left
**Right Arrow** - Move one character to the right

#### Word-by-Word Movement:

**Ctrl+Left** - Move to the beginning of the previous word
**Ctrl+Right** - Move to the beginning of the next word

#### Line-by-Line Movement:

**Up Arrow** - Move to the previous line
**Down Arrow** - Move to the next line

#### Beginning and End of Line:

**Home** or **Ctrl+A** - Move to the start of the current visual line
**End** or **Ctrl+E** - Move to the end of the current visual line

#### Page-by-Page Movement:

**PageUp** or **Ctrl+Up** - Scroll up one page
**PageDown** or **Ctrl+Down** - Scroll down one page

#### Additional Information

Word-by-word movement (Ctrl+Left/Right) is especially useful for navigating and editing quickly. The cursor stops at the beginning of each word and skips over whitespace.

Visual lines may differ from logical paragraphs. A single paragraph can span multiple visual lines due to word wrapping. Home and End move within the visual line, not the entire paragraph.

---

### Delete Text

**Purpose:** Remove characters, words, or paragraphs from your document.

#### To delete the character at the cursor:

Press **Delete**.

The character at the cursor position is removed.

#### To delete the character before the cursor:

Press **Backspace**.

The character immediately to the left is removed.

#### To delete a word forward:

1. Position the cursor at the beginning of or within the word.

2. Press **Ctrl+Delete** or **Alt+Delete**.

Text from the cursor to the end of the word is deleted.

#### To delete a word backward:

1. Position the cursor at the end of or within the word.

2. Press **Ctrl+W**, **Ctrl+Backspace**, or **Alt+Backspace**.

Text from the cursor to the beginning of the word is deleted.

#### To delete a paragraph break:

1. Position the cursor at the end of a paragraph.

2. Press **Delete**.

The paragraph break is removed and the next paragraph merges with the current one.

Alternatively:

1. Position the cursor at the beginning of a paragraph.

2. Press **Backspace**.

The current paragraph merges with the previous one.

#### Additional Information

When you delete a paragraph break between two different paragraph types (for example, merging a heading into a text paragraph), the resulting merged paragraph takes the type of the paragraph where the cursor is positioned.

Undo functionality is planned for future releases. Currently, deleted text cannot be recovered.

---

### Document Format

**Purpose:** Understand how Pure stores and represents documents.

#### FTML Format

Pure's native format is FTML (Formatted Text Markup Language), a strict subset of HTML5. When you save a document with a `.ftml` extension, Pure creates a valid HTML5 file.

FTML documents contain:

- A standard HTML5 document structure
- Semantic paragraph elements (h1-h3, p, blockquote, pre, ul, ol)
- Inline formatting elements (strong, em, u, del, mark, code, a)
- Proper nesting and structure

#### Advantages of FTML:

**Browser Compatible** - FTML files can be opened directly in any web browser for viewing or printing.

**Version Control Friendly** - FTML's line-based structure works well with Git and other version control systems.

**Simple and Predictable** - FTML allows only one way to express each formatting concept, making it easy to process programmatically.

**Future-Proof** - As valid HTML5, FTML files remain accessible even without Pure.

#### Markdown Format

Pure can import and export Markdown files. When you open a `.md` file, Pure converts it to its internal FTML representation. When you export to Markdown, Pure converts FTML to Markdown syntax.

#### Supported Markdown Features:

- Headings (# ## ###)
- Bold (**text**) and italic (_text_)
- Ordered and unordered lists
- Checklists (- [ ] item)
- Code blocks (`code`)
- Block quotes (> text)
- Links [text](url)

#### Limitations:

Some Pure features may not have direct Markdown equivalents. When exporting to Markdown, these features may be approximated or simplified.

---

### Exit

**Purpose:** Close Pure and return to the terminal.

**Keyboard Shortcut:** Ctrl+Q

#### To exit Pure:

1. Press **Ctrl+Q**.

2. If you have unsaved changes, Pure prompts: "Save changes?"

3. Press **y** to save changes before exiting.
   Or press **n** to exit without saving.

If you choose to save, you may be prompted for a filename if the document hasn't been saved before.

#### To exit without being prompted to save:

Ensure you've saved your document with Ctrl+S before exiting. If there are no unsaved changes, Pure exits immediately when you press Ctrl+Q.

#### Additional Information

If you have made changes to your document since the last save, Pure will always give you the opportunity to save before exiting. This prevents accidental loss of work.

Currently, Pure works with one document at a time. Future versions may support multiple documents, in which case the exit process will ensure all documents are saved.

---

### FTML Format

**Purpose:** Understand Pure's native document format.

FTML (Formatted Text Markup Language) is a lightweight document format designed for simplicity and ease of processing. As a strict subset of HTML5, FTML remains fully compatible with standard web technologies while being far easier to parse and work with programmatically.

#### Key Features:

**Simple Structure** - FTML includes only essential formatting options, avoiding the complexity of full HTML.

**HTML-Compatible** - Every valid FTML document is a valid HTML5 document that can be opened in any web browser.

**Diffable** - FTML is designed to work well with version control systems. Documents are line-based and formatted consistently.

**Unambiguous** - FTML typically allows only one way to express a formatting concept, making documents predictable and easy to process.

#### FTML Elements:

**Paragraph Types:**

- `<p>` - Text paragraphs
- `<h1>`, `<h2>`, `<h3>` - Headings
- `<blockquote>` - Quotations
- `<pre>` - Code blocks
- `<ul><li>` - Bullet lists
- `<ol><li>` - Numbered lists
- `<ul class="checklist"><li>` - Checklist items

**Inline Styles:**

- `<strong>` - Bold
- `<em>` - Italic
- `<u>` - Underline
- `<del>` - Strikethrough
- `<mark>` - Highlight
- `<code>` - Inline code
- `<a>` - Hyperlinks

#### Example FTML Document:

```html
<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>Document Title</title>
  </head>
  <body>
    <h1>Study Abroad Program</h1>
    <p>
      I wanted to follow up on our discussion about starting a
      <strong>study abroad program</strong>.
    </p>
    <ul>
      <li>Italy - Focus on Renaissance art</li>
      <li>Japan - Study language and culture</li>
    </ul>
  </body>
</html>
```

#### Additional Information

For the full FTML specification and the accompanying Rust library, see the tdoc project at https://github.com/roblillack/tdoc.

---

### Headings

**Purpose:** Create hierarchical document structure with headings.

**Keyboard Shortcuts:**

- Esc, then 1 (Heading 1)
- Esc, then 2 (Heading 2)
- Esc, then 3 (Heading 3)

#### To create a heading:

1. Type your heading text or position the cursor in an existing paragraph.

2. Press **Esc** to open the context menu.

3. Press **1** for Heading 1, **2** for Heading 2, or **3** for Heading 3.

The paragraph becomes a heading at the selected level.

#### Heading Levels:

**Heading 1** - The highest level, typically used for document titles or major sections.

**Heading 2** - Subsections within major sections.

**Heading 3** - Sub-subsections or minor headings.

#### To convert a heading back to regular text:

1. Position the cursor in the heading.

2. Press **Esc**, then press **0**.

The heading becomes a regular text paragraph.

#### Additional Information

Use headings to create a logical document structure. Headings make documents easier to scan and understand.

In FTML, headings are represented as `<h1>`, `<h2>`, and `<h3>` elements. When exported to Markdown, they become `#`, `##`, and `###` respectively.

Headings can contain inline formatting like bold and italic text.

---

### Hyperlinks

**Purpose:** Create clickable links to web pages or other documents.

**Status:** Hyperlinks are supported in FTML documents and can be created and edited. Full editing interface is under development.

#### Hyperlink Structure:

A hyperlink consists of two parts:

- **Link text** - The visible text the user clicks
- **URL** - The destination address

#### Current Capabilities:

Pure can read and display existing hyperlinks in FTML documents. The link text appears in your document and the URL is preserved in the file.

#### Planned Features:

Future versions of Pure will include:

- Interactive link creation through the context menu
- Link editing to change URLs
- Visual indication of links in the editor
- Quick keyboard shortcut for adding links (Esc, then k)

#### Additional Information:

In FTML, hyperlinks are represented as `<a href="url">text</a>` elements.

In Markdown export, hyperlinks appear as `[text](url)`.

---

### Insert Text

**Purpose:** Add text to your document.

#### To insert text:

1. Position the cursor where you want to add text.

2. Type your text.

By default, Pure is in Insert mode. New text is inserted at the cursor position, and existing text moves to the right.

#### Automatic Word Wrapping:

You don't need to press Enter at the end of each line. Pure automatically wraps text to the next line when the current line fills. This is called word wrapping.

Press Enter only when you want to start a new paragraph.

#### To insert a newline within a paragraph:

Press **Shift+Enter** or **Ctrl+Enter**.

This creates a new line within the current paragraph without starting a new paragraph.

#### To insert a tab:

Press **Tab**.

A tab character is inserted at the cursor position.

#### To insert a paragraph break:

Press **Enter** or **Return**.

A new paragraph of the same type as the current paragraph is created.

#### To insert a paragraph break as a sibling:

Press **Ctrl+P**.

This is useful when you're inside a nested structure (like a list item that contains multiple paragraphs) and want to create a new item at the same level.

#### Additional Information:

Pure always operates in Insert mode. There is no Typeover mode where new text replaces existing text.

---

### Lists, Ordered and Unordered

**Purpose:** Create structured lists of items.

**Keyboard Shortcuts:**

- Esc, then 7 (Numbered/Ordered list)
- Esc, then 8 (Bullet/Unordered list)

#### To create a bullet list:

1. Position the cursor where you want the list to begin.

2. Press **Esc** to open the context menu.

3. Press **8** to change to a bullet list item.

4. Type the text for your first item.

5. Press **Enter** to create another list item.

6. Repeat steps 4-5 for each item.

#### To create a numbered list:

1. Position the cursor where you want the list to begin.

2. Press **Esc** to open the context menu.

3. Press **7** to change to a numbered list item.

4. Type the text for your first item.

5. Press **Enter** to create another numbered item.

6. Repeat steps 4-5 for each item.

#### To end a list:

1. At the end of your last list item, press **Enter** to create a new item.

2. Press **Esc**, then press **0** to convert it to a regular text paragraph.

The list ends and you can continue with regular text.

#### To convert between list types:

1. Position the cursor in any list item.

2. Press **Esc** and select the new list type (7 for numbered, 8 for bullet).

The entire list converts to the new type.

#### Additional Information:

Lists can be nested (lists within lists). Position your cursor in a list item and create nested content as needed.

List items can contain multiple paragraphs. Use Shift+Enter to create line breaks within an item, or use standard paragraph formatting for complex list items.

In FTML, bullet lists are `<ul><li>` and numbered lists are `<ol><li>`. When exporting to Markdown, bullet lists use `-` or `*` and numbered lists use `1.`, `2.`, etc.

Adjacent lists of the same type are automatically merged. To create separate lists, place a non-list paragraph between them.

---

### Markdown Support

**Purpose:** Import and export Markdown documents.

Pure provides full support for opening and saving Markdown files, allowing you to work with existing Markdown documents or export your work to Markdown format.

#### To open a Markdown file:

```
pure document.md
```

Pure converts the Markdown to its internal FTML representation, allowing you to edit it with all of Pure's features.

#### To export to Markdown:

Currently, you must manually specify the `.md` extension when saving. Future versions may provide an explicit export command.

When saving as `.md`, Pure converts the FTML structure to Markdown syntax.

#### Supported Markdown Features:

**Headings**
`#` → Heading 1
`##` → Heading 2
`###` → Heading 3

**Emphasis**
`**bold**` or `__bold__` → Bold
`*italic*` or `_italic_` → Italic

**Lists**
`- item` or `* item` → Bullet list
`1. item` → Numbered list
`- [ ] item` → Unchecked checklist item
`- [x] item` → Checked checklist item

**Code**
`` `code` `` → Inline code
` ```code block``` ` → Code block

**Quotes**
`> quoted text` → Block quote

**Links**
`[text](url)` → Hyperlink

#### Limitations:

Some Pure features may not have exact Markdown equivalents:

- Underline, strikethrough, and highlight may not be preserved in all Markdown variants
- Complex nested structures may be simplified
- Some formatting details may be lost in conversion

Some Markdown features are not yet supported:

- Images
- Tables
- Inline HTML

#### Additional Information:

Pure uses FTML as its primary format because FTML preserves all formatting information precisely. If you work primarily with Markdown files, consider keeping your original `.md` files under version control and using Pure to edit working copies.

---

### Mouse Support

**Purpose:** Use your mouse or trackpad to navigate, select, and scroll through documents.

Pure provides comprehensive mouse support, allowing you to work with your documents using both keyboard shortcuts and mouse interactions.

#### To position the cursor with the mouse:

Click anywhere in your document. The cursor moves to the clicked position.

#### To select text with the mouse:

1. Click and hold at the starting position.

2. Drag the mouse to the end of the text you want to select.

3. Release the mouse button.

The selected text is highlighted and can be formatted using the context menu.

#### To select a word:

Double-click on any word.

The entire word is selected instantly, making it easy to apply formatting or replace the word.

#### To select a paragraph:

Triple-click anywhere in a paragraph.

The entire paragraph is selected, allowing you to quickly format or replace large blocks of text.

#### To scroll through your document:

Use your mouse wheel or trackpad scrolling gestures to move up and down through your document.

Scrolling moves the viewport without changing the cursor position, allowing you to browse through your document while keeping your current editing position.

#### Working with selections:

Once text is selected, you can:

- Press **Esc** to open the context menu and apply formatting (bold, italic, etc.)
- Type new text to replace the selection
- Press **Delete** or **Backspace** to remove the selected text
- Use arrow keys to deselect and position the cursor

#### Additional Information:

Mouse support in Pure works alongside keyboard shortcuts. You can seamlessly switch between mouse and keyboard as needed for your workflow.

Mouse support includes:

- **Click** - Position cursor
- **Click and drag** - Select text
- **Double-click** - Select word
- **Triple-click** - Select paragraph
- **Scroll** - Navigate through document

Selection boundaries respect formatting structure. When you select text, you can see exactly what will be affected by formatting changes, especially when used with Reveal Codes mode (F9).

---

### Newlines and Paragraph Breaks

**Purpose:** Control document structure by inserting newlines and paragraph breaks.

Pure distinguishes between newlines (line breaks within a paragraph) and paragraph breaks (which create new paragraphs).

#### To insert a paragraph break:

Press **Enter** or **Return**.

A new paragraph is created. The new paragraph has the same type as the current paragraph (e.g., if you're in a text paragraph, you get a new text paragraph; if you're in a bullet list, you get a new bullet item).

#### To insert a newline within a paragraph:

Press **Shift+Enter** or **Ctrl+Enter**.

A line break is inserted in the current paragraph. This moves the cursor to a new line but doesn't create a new paragraph.

Use newlines when you want to break a line in the middle of a paragraph, such as:

- Line breaks in poetry or lyrics
- Breaking long list items into multiple lines
- Formatting addresses

#### To insert a paragraph break as a sibling:

Press **Ctrl+P**.

This creates a new paragraph at the same structural level as the current paragraph. This is especially useful inside nested structures.

For example, if you're editing a paragraph inside a list item, **Enter** would create a new paragraph inside the same list item, while **Ctrl+P** creates a new list item.

#### Additional Information:

Understanding the difference between newlines and paragraph breaks is key to mastering Pure.

In FTML:

- Paragraph breaks create new HTML elements (`<p>`, `<li>`, etc.)
- Newlines within paragraphs are represented as `<br>` elements

In Markdown:

- Paragraph breaks are blank lines
- Newlines are two spaces at the end of a line followed by a line break

In Reveal Codes mode (F9), you can see exactly where paragraph breaks occur in your document structure.

---

### Open a Document

**Purpose:** Load an existing FTML or Markdown document for editing.

#### To open a document when starting Pure:

```
pure filename.ftml
```

or

```
pure filename.md
```

Pure starts and loads the specified document.

#### To open a document from within Pure:

The Retrieve feature for opening documents while Pure is running is planned for a future release. Currently, you must specify the filename when starting Pure.

#### Supported File Types:

**FTML files** (.ftml, .html) - Pure's native format
**Markdown files** (.md, .markdown) - Converted to FTML on load

#### Additional Information:

If the file doesn't exist, Pure creates a new empty document with the specified filename. This allows you to start a new document simply by specifying a filename that doesn't exist yet.

Pure determines the file format by the file extension. Files ending in `.md` or `.markdown` are treated as Markdown; all others are treated as FTML/HTML.

If you open a file that is not valid FTML or Markdown, Pure may display an error or show unexpected results.

---

### Paragraph Types

**Purpose:** Change the type and formatting of paragraphs.

Pure documents are composed of paragraphs, and each paragraph has a type that determines its structure and appearance.

#### Available Paragraph Types:

**Text** (Esc, 0) - Regular body paragraphs

**Heading 1** (Esc, 1) - Major section headings

**Heading 2** (Esc, 2) - Subsection headings

**Heading 3** (Esc, 3) - Minor headings

**Quote** (Esc, 5) - Block quotations

**Code Block** (Esc, 6) - Preformatted code

**Numbered List** (Esc, 7) - Ordered list items

**Bullet List** (Esc, 8) - Unordered list items

**Checklist** (Esc, 9) - Task items with checkboxes

#### To change a paragraph type:

1. Position the cursor in the paragraph you want to change.

2. Press **Esc** to open the context menu.

3. Press the number corresponding to the desired type (see list above).

The paragraph immediately changes to the new type.

#### Additional Information:

When you change a paragraph type, the text content is preserved but the structural formatting changes.

Some paragraph types have special behaviors:

- **Lists** automatically create new list items when you press Enter
- **Headings** typically display in larger or bold text
- **Code blocks** preserve exact spacing and use monospaced fonts
- **Quotes** may be visually indented

Changing between list types (numbered, bullet, checklist) converts the entire list, not just the current item.

Changing from a list type to a non-list type (like text or heading) may restructure nested content.

---

### Quotes

**Purpose:** Format block quotations.

**Keyboard Shortcut:** Esc, then 5

#### To create a quote:

1. Position the cursor where you want the quote.

2. Press **Esc** to open the context menu.

3. Press **5** to change to a quote paragraph.

4. Type the quoted text.

5. Press **Enter** to continue the quote on a new line.

6. To end the quote, press **Esc** then **0** to convert to regular text.

#### To convert existing text to a quote:

1. Position the cursor in the paragraph you want to quote.

2. Press **Esc**, then press **5**.

The paragraph becomes a block quote.

#### Additional Information:

Block quotes are typically used for:

- Quotations from other sources
- Excerpts from documents
- Highlighted or offset text

In FTML, quotes are represented as `<blockquote>` elements.

In Markdown export, quotes appear with `>` at the beginning of each line.

Quotes can contain inline formatting (bold, italic, etc.) and can span multiple paragraphs.

---

### Reveal Codes

**Purpose:** Display the underlying structure and formatting of your document.

**Keyboard Shortcut:** F9

Reveal Codes mode shows the internal structure of your document, making formatting boundaries and styles visible. This is especially useful when you need to understand exactly where formatting begins and ends.

#### To toggle Reveal Codes:

Press **F9**.

The display changes to show formatting codes alongside your text.

Press **F9** again to return to normal editing mode.

#### What You See in Reveal Codes:

In Reveal Codes mode, inline styles appear as visible tags in your document:

- `[Bold>` and `<Bold]` - Bold formatting boundaries
- `[Italic>` and `<Italic]` - Italic formatting boundaries
- `[Underline>` and `<Underline]` - Underline boundaries
- `[Code>` and `<Code]` - Inline code boundaries
- `[Highlight>` and `<Highlight]` - Highlight boundaries
- `[Strikethrough>` and `<Strikethrough]` - Strikethrough boundaries

#### Using Reveal Codes:

Reveal Codes helps you:

**Understand Formatting** - See exactly where styles start and end

**Fix Formatting Problems** - Identify unwanted or incorrect formatting

**Edit Precisely** - Position your cursor exactly at formatting boundaries

**Learn FTML Structure** - Understand how your document is structured internally

#### To edit in Reveal Codes mode:

You can edit text normally while in Reveal Codes mode. The visible codes help you see exactly where you're inserting text and what formatting will apply.

To position your cursor at a specific formatting boundary, use the arrow keys to navigate to the code markers.

#### Additional Information:

Reveal Codes is inspired by WordPerfect's famous Reveal Codes feature, which allowed users to see and edit the underlying formatting codes in their documents.

The visible codes in Pure are representations of the FTML structure. They show where HTML tags like `<b>`, `<i>`, and `<code>` begin and end in the underlying document.

---

### Save a Document

**Purpose:** Write your document to disk.

**Keyboard Shortcut:** Ctrl+S

#### To save a document:

1. Press **Ctrl+S**.

2. If this is a new document that hasn't been saved before, Pure prompts you for a filename.

3. Type a filename and press **Enter**.

The document is saved to disk.

#### To save with a new name:

Currently, Pure doesn't have a "Save As" feature. To save a document with a new name:

1. Exit Pure (Ctrl+Q).

2. Copy the file to a new name using your terminal.

3. Open the new file with Pure.

A dedicated "Save As" feature is planned for future releases.

#### Filename Extensions:

**For FTML documents:** Use the `.ftml` or `.html` extension

```
study-abroad.ftml
```

**For Markdown documents:** Use the `.md` extension

```
notes.md
```

Pure determines the save format based on the file extension.

#### Additional Information:

Pure saves your document in the format indicated by the filename extension. If you open a `.md` file, edit it, and save it, Pure saves it back as Markdown.

If you want to convert a Markdown document to FTML, open the `.md` file and save it with a `.ftml` extension.

Pure saves documents with proper UTF-8 encoding, ensuring international characters are preserved correctly.

There are no automatic saves or backup copies. Remember to save frequently (Ctrl+S) to avoid losing work.

---

## Keyboard Shortcuts Reference

### Navigation

**Left** / **Right** - Move cursor character by character

**Ctrl+Left** / **Ctrl+Right** - Move cursor word by word

**Up** / **Down** - Move cursor line by line

**Home** / **Ctrl+A** - Move to start of visual line

**End** / **Ctrl+E** - Move to end of visual line

**PageUp** / **PageDown** - Scroll viewport up/down by one page

**Ctrl+Up** / **Ctrl+Down** - Scroll viewport up/down by one page

### Editing

**Enter** - Insert paragraph break

**Shift+Enter** / **Ctrl+Enter** - Insert newline within paragraph

**Ctrl+J** - Insert newline character

**Ctrl+P** - Insert paragraph break as sibling

**Tab** - Insert tab character

**Backspace** - Delete character before cursor

**Delete** - Delete character after cursor

**Ctrl+W** / **Ctrl+Backspace** / **Alt+Backspace** - Delete word backward

**Ctrl+Delete** / **Alt+Delete** - Delete word forward

### File Operations

**Ctrl+S** - Save document

**Ctrl+Q** - Quit editor

### Context Menu

**Esc** / **Ctrl+Space** - Open/close context menu

**Up** / **Down** - Navigate menu items

**Enter** - Execute selected menu action

### Paragraph Types (from Context Menu)

**0** - Text paragraph

**1** - Heading 1

**2** - Heading 2

**3** - Heading 3

**5** - Quote

**6** - Code block

**7** - Numbered list

**8** - Bullet list

**9** - Checklist

### Special Features

**F9** - Toggle Reveal Codes

---

## About This Guide

This User's Guide was created to help you get the most out of Pure. The guide is modeled after classic word processor documentation, with a focus on clear, task-oriented instructions.

### Getting More Help

**GitHub Repository**
https://github.com/roblillack/pure

Visit the repository for:

- Latest version information
- Bug reports and feature requests
- Source code
- Contributing guidelines

**FTML Specification**
https://github.com/roblillack/tdoc

Learn about the FTML format and the tdoc library that powers Pure's document handling.

### About Pure

Pure is developed by Rob Lillack and contributors. It is released under the MIT License.

Pure aims to bring the power and efficiency of structured document editing to the terminal, making it easy to create well-formatted documents without leaving the command line.

---

_This guide covers Pure version 0.1.x. Features and shortcuts may change in future versions._
