# Pure Architecture Documentation

## Editor, Renderer, and Display System

This document provides a comprehensive overview of the editor, renderer, and display system used in Pure to render tdoc documents with wrapping, cursor tracking, selection, and caching.

---

## Glossary of Terms

### Document Model Terms

- **Document**: A tdoc `Document` structure containing a tree of paragraphs
- **Paragraph**: A structural element in the document (text, header, list, checklist, etc.)
- **Span**: A piece of formatted text within a paragraph (can be nested for inline styles)
- **ParagraphPath**: A hierarchical path identifying a paragraph's position in the document tree (e.g., root index, child index, entry index, checklist item indices)
- **SpanPath**: A path of indices identifying a span within a paragraph's content

### Cursor and Position Terms

- **Segment**: A navigable portion of the document that the cursor can be positioned within. Each segment represents a contiguous range of cursor positions within a span. Segments are the atomic units of cursor navigation in the editor.
- **SegmentKind**: The type of segment, which can be:
  - `Text`: Regular text content within a span
  - `RevealStart(InlineStyle)`: Start boundary of an inline style (only in reveal codes mode)
  - `RevealEnd(InlineStyle)`: End boundary of an inline style (only in reveal codes mode)
- **SegmentRef**: A reference to a segment containing:
  - `paragraph_path`: Location of the paragraph
  - `span_path`: Location of the span within the paragraph
  - `len`: Number of cursor positions in the segment
  - `kind`: The segment kind (Text, RevealStart, or RevealEnd)
- **CursorPointer**: A logical position in the document identifying a specific location via:
  - `paragraph_path`: Which paragraph
  - `span_path`: Which span within the paragraph
  - `offset`: Character offset within the segment (0 to len)
  - `segment_kind`: Type of segment (Text, RevealStart, RevealEnd)
- **CursorVisualPosition**: A visual screen position containing:
  - `line`: Visual line number (0-based)
  - `column`: Visual column number (0-based, includes padding and prefixes)
  - `content_line`: Content line number (excludes blank separator lines)
  - `content_column`: Content column number (excludes prefixes like bullets)
- **CursorDisplay**: Pairs a `CursorPointer` with its `CursorVisualPosition`
- **Preferred Column**: The column the cursor tries to maintain when moving vertically

### Rendering Terms

- **Fragment**: A piece of text to be rendered with style and width information
- **FragmentKind**: Type of fragment (Word, Whitespace, RevealTag)
- **DirectFragment**: Fragment with attached position events during rendering
- **Wrapping**: Breaking long lines into multiple visual lines to fit within `wrap_width`
- **Wrap Width**: The maximum width in characters for a line before wrapping
- **Left Padding**: The number of spaces added to the left of all rendered lines
- **Line Prefix**: Text added to the start of lines (e.g., "• " for bullets, "[ ] " for checklist items)
- **Continuation Prefix**: Prefix for wrapped continuation lines (usually spaces to align with first line)

### Tracking and Events

- **DirectCursorTracking**: Parameters for cursor tracking during rendering:
  - `cursor`: Optional cursor pointer to track
  - `selection`: Optional selection start/end pointers
  - `track_all_positions`: Whether to track all cursor positions for the cursor map
- **TextEvent**: An event marking a position in rendered text (cursor, selection start/end, or position marker)
- **Cursor Map**: A mapping from all `CursorPointer` positions to their `CursorVisualPosition`
- **Reveal Codes Mode**: A mode showing inline style markers (e.g., `[Bold>text<Bold]`)

### Caching Terms

- **RenderCache**: A cache of rendered paragraphs keyed by content hash, wrap width, and left padding
- **ParagraphCacheKey**: Cache key containing paragraph index, content hash, wrap width, and left padding
- **CachedParagraphRender**: Cached rendering result containing rendered lines and metrics
- **Active Paragraph**: A paragraph containing the cursor or selection (not cached)
- **Cache Hit/Miss**: Whether a paragraph's rendering was found in cache or needed re-rendering

---

## Major Components

### 1. DocumentEditor (`src/editor.rs`)

**Purpose**: Manages the document content and logical cursor position.

**Responsibilities**:
- Maintains the `Document` structure
- Manages cursor position as a `CursorPointer`
- Maintains a list of `SegmentRef` representing all navigable segments in the document
- Handles text editing operations (insert, delete, etc.)
- Manages paragraph structure operations (indent, unindent, convert types)
- Supports "reveal codes" mode to show inline style boundaries
- Rebuilds segments when document structure changes

**Key Methods**:
- `new(document)`: Creates editor from a document
- `document()`: Returns reference to the document
- `cursor_pointer()`: Returns the current cursor position
- `rebuild_segments()`: Rebuilds the segment list (called after structural changes)
- `ensure_cursor_selectable()`: Ensures cursor is on a valid segment
- `move_left/right/up/down()`: Logical cursor movement through segments
- `insert_char()`, `delete_char()`: Text editing
- `set_reveal_codes()`: Toggle reveal codes mode

**State Management**:
- Maintains segments list synchronized with document structure
- Updates cursor position after edits
- Invalidates segments when structure changes

### 2. EditorDisplay (`src/editor_display.rs`)

**Purpose**: Wraps `DocumentEditor` and manages all visual/rendering concerns.

**Responsibilities**:
- Owns a `DocumentEditor` instance
- Maintains visual state (cursor visual position, preferred column, visual positions map)
- Provides visual cursor movement (respects wrapping)
- Tracks cursor following mode (whether view should follow cursor)
- Coordinates with renderer to get visual positions
- Maps mouse clicks to cursor positions
- Manages render cache lifecycle

**Key Fields**:
- `editor`: The underlying `DocumentEditor`
- `render_cache`: Cache of rendered paragraphs
- `visual_positions`: Vector mapping all cursor pointers to visual positions
- `last_cursor_visual`: The last known visual position of the cursor
- `preferred_column`: Column to maintain during vertical movement
- `cursor_following`: Whether the view should follow the cursor
- `last_view_height`: Height of the viewport (for page jumps)
- `last_total_lines`: Total rendered lines
- `last_text_area`: The text area rect from the last render

**Key Methods**:
- `new(editor)`: Creates display wrapper around an editor
- `render_document(wrap_width, left_padding, selection)`: Renders the document and updates visual state
- `update_after_render(text_area, total_lines)`: Updates tracking state after drawing
- `move_cursor_vertical(delta)`: Moves cursor up/down by visual lines
- `move_page(direction)`: Moves cursor by a page
- `move_to_visual_line_start/end()`: Moves to start/end of visual line
- `pointer_from_mouse(column, row, scroll_top)`: Converts mouse click to cursor pointer
- `clear_render_cache()`: Clears the render cache (called when document changes)
- `set_reveal_codes()`: Overrides to clear cache when toggling reveal codes

**Visual Movement Algorithm**:
1. Get current cursor visual position from `visual_positions` map
2. Calculate target visual line (current line + delta)
3. Use `preferred_column` to find the closest position on target line
4. If target line has no positions, search nearby lines
5. Update cursor using `editor.move_to_pointer()`
6. Fall back to logical movement if visual positions unavailable

**Deref Pattern**: Implements `Deref` and `DerefMut` to `DocumentEditor`, allowing direct access to editor methods.

### 3. DirectRenderer (`src/render.rs`)

**Purpose**: Renders the document into styled lines with wrapping and cursor tracking.

**Responsibilities**:
- Traverses the document structure paragraph by paragraph
- Collects text fragments with styles from nested spans
- Performs line wrapping based on `wrap_width`
- Tracks cursor and selection positions during rendering
- Generates cursor map (all positions → visual positions)
- Applies paragraph-specific formatting (headers, lists, code blocks, quotes)
- Manages render cache to avoid re-rendering unchanged paragraphs

**Rendering Pipeline**:
1. **Document Traversal**: Iterate through paragraphs
2. **Fragment Collection**: Convert spans to fragments with styles
3. **Position Tracking**: Mark fragments with cursor/selection events
4. **Wrapping**: Break fragments into lines using `wrap_fragments()`
5. **Line Assembly**: Combine fragments into styled `Line` objects
6. **Event Processing**: Convert fragment events to visual positions
7. **Result Construction**: Build `RenderResult` with lines, cursor, and cursor map

**Key Fields**:
- `wrap_width`: Maximum line width
- `wrap_limit`: Effective wrap limit (wrap_width - 1)
- `left_padding`: Spaces added to the left of all lines
- `lines`: Accumulated rendered lines
- `line_metrics`: Metadata for each line (whether it counts as content)
- `cursor_pointer/selection_start/selection_end`: Positions being tracked
- `track_all_positions`: Whether to build a complete cursor map
- `current_paragraph_path`: Current position during traversal
- `marker_to_pointer`: Maps marker IDs to cursor pointers for cursor map
- `cache`: Optional render cache

**Fragment Collection Process**:
```
For each paragraph:
  For each span:
    Tokenize text into words and whitespace
    For each character position:
      Check if it matches cursor/selection
      Add position events to fragment
    Add styled fragment to list
  Apply trimming (remove leading/trailing whitespace)
```

**Wrapping Algorithm** (in `wrap_fragments()`):
1. Start with line prefix
2. For each fragment:
   - If whitespace: Add to pending (may be discarded at line breaks)
   - If word/tag:
     - Check if it fits on current line
     - If not, start new line with continuation prefix
     - If word is too long for any line, split it
3. Consume pending whitespace when committing words
4. Build `LineOutput` with segments and position events

**Cache Strategy**:
- Cache key: `(paragraph_index, content_hash, wrap_width, left_padding)`
- Cache hit: Paragraph unchanged and not active (no cursor/selection)
- Cache miss: Paragraph changed or contains cursor/selection
- Eviction: Clear entire cache when size exceeds limit
- Active paragraphs: Never cached (contain cursor styling)

### 4. RenderCache (`src/render.rs`)

**Purpose**: Caches rendered paragraph results to avoid redundant re-rendering.

**Responsibilities**:
- Stores rendered lines and metrics by cache key
- Tracks cache hit/miss statistics
- Evicts entries when cache grows too large
- Computes content hashes for paragraphs

**Key Methods**:
- `new()`: Creates empty cache with default max size (50,000 paragraphs)
- `get(key)`: Retrieves cached render if available (records hit/miss)
- `insert(key, value)`: Stores rendered result (evicts if cache full)
- `clear()`: Clears entire cache
- `invalidate_paragraph(index)`: Removes cache entries for specific paragraph

**Hashing Strategy**:
- Recursively hashes paragraph type, content spans, children, entries, and checklist items
- Includes inline styles and text content
- Hash changes trigger cache miss and re-render

**Performance Characteristics**:
- Large documents: High hit rate for unchanged paragraphs
- Scrolling: Active paragraph (with cursor) always misses, others hit
- Editing: Only edited paragraph and its neighbors miss
- Resizing: All paragraphs miss (wrap_width changes)

---

## Typical Use-Case Interactions

### Use Case 1: Initial Document Render

**Scenario**: Application starts and needs to display a document.

**Flow**:
1. **Create Editor**: `DocumentEditor::new(document)`
   - Initializes document structure
   - Builds segment list via `rebuild_segments()`
   - Places cursor at first selectable position via `ensure_cursor_selectable()`

2. **Create Display**: `EditorDisplay::new(editor)`
   - Wraps editor
   - Initializes empty render cache
   - Sets up visual state tracking

3. **First Render**: `display.render_document(wrap_width, left_padding, None)`
   - Calls `render_document_direct()` with cursor tracking
   - DirectRenderer traverses document:
     - For each paragraph: Collect fragments, check cache (all misses)
     - Wrap fragments into lines
     - Track cursor position during rendering
     - Store results in cache
   - Returns `RenderResult` with:
     - `lines`: Styled lines for display
     - `cursor`: Visual position of cursor
     - `cursor_map`: Map of all positions
   - EditorDisplay updates internal state:
     - `visual_positions` from cursor_map
     - `last_cursor_visual` from cursor
     - `preferred_column` initialized

4. **Draw to Screen**: Application uses `RenderResult.lines`
   - Calls `display.update_after_render(text_area, total_lines)`
   - EditorDisplay stores viewport information

**Cache State**: All paragraphs cached except the one with cursor.

**Result**: Document displayed with cursor at initial position.

### Use Case 2: Typing Text

**Scenario**: User types a character at the cursor.

**Flow**:
1. **Insert Character**: `display.insert_char('a')`
   - Deref calls `editor.insert_char('a')`
   - Editor inserts character at cursor position
   - Updates document structure
   - Moves cursor forward
   - Rebuilds segments if structure changed

2. **Clear Cache**: `display.clear_render_cache()`
   - Clears entire render cache (document content changed)
   - Ensures fresh render on next frame

3. **Next Render**: `display.render_document(...)`
   - All paragraphs are cache misses (cache was cleared)
   - Paragraphs are re-rendered and re-cached
   - Cursor map updated with new positions
   - Returns new `RenderResult`

4. **Update Display**: Visual state updated, screen redrawn

**Optimization Note**: Could optimize to only invalidate affected paragraph, but current approach clears entire cache for simplicity.

**Result**: Character appears, cursor advances.

### Use Case 3: Vertical Cursor Movement (Down Arrow)

**Scenario**: User presses down arrow key.

**Flow**:
1. **Move Cursor**: `display.move_cursor_vertical(1)`
   - Retrieves current cursor position from `visual_positions` map
   - Gets `preferred_column` (or current column if none)
   - Calculates target line: `current_line + 1`

2. **Find Target Position**:
   - Calls `closest_pointer_on_line(target_line, preferred_column)`
   - Searches `visual_positions` for entries on target line
   - Finds closest match by column distance
   - If target line empty, calls `search_nearest_line()` to find nearest line with content

3. **Move Editor Cursor**:
   - If destination found: `editor.move_to_pointer(&dest.pointer)`
     - Editor updates cursor to logical position
     - Updates `last_cursor_visual` and `preferred_column`
   - If same position (edge case): Falls back to `editor.move_down()` for logical movement
   - If no destination: Falls back to logical movement

4. **Next Render**: Cursor appears on new visual line
   - Render cache hits for unchanged paragraphs
   - Only paragraphs with cursor (old and new) are re-rendered

**Preferred Column Behavior**: Maintained across vertical movements, allowing cursor to "remember" its column when moving through short lines.

**Result**: Cursor moves down one visual line, respecting wrapping.

### Use Case 4: Window Resize

**Scenario**: User resizes the terminal, changing available width.

**Flow**:
1. **Application Detects Resize**: New viewport dimensions calculated
   - New `wrap_width` computed from terminal width

2. **Render with New Width**: `display.render_document(new_wrap_width, left_padding, None)`
   - Cache keys include `wrap_width`, so all cache lookups miss
   - All paragraphs re-rendered with new wrap width
   - Text reflows to fit new width
   - Cursor map rebuilt with new visual positions
   - All cache entries updated with new wrap width

3. **Update Viewport**: `display.update_after_render(new_text_area, total_lines)`
   - Updates `last_view_height` for page jumps
   - Updates `last_text_area` for mouse click handling

**Result**: Document reflows to new width, cursor remains at same logical position but may move visually.

### Use Case 5: Scrolling Through Large Document

**Scenario**: User scrolls down through a 10,000-paragraph document.

**Flow**:
1. **Cursor Moves**: Via `move_cursor_vertical()` or page jumps
   - Cursor moves to new paragraph
   - Visual positions update

2. **Render**: `display.render_document(...)`
   - Paragraphs before cursor: Cache hit (unchanged, no cursor)
   - Paragraph with cursor: Cache miss (active paragraph)
   - Paragraphs after cursor: Cache hit or miss depending on first render
   - Renderer uses cache for most paragraphs:
     - Retrieves cached lines and metrics
     - Skips fragment collection and wrapping
     - Advances line index
   - Only active paragraph fully rendered

3. **Performance**:
   - First scroll through document: High miss rate (building cache)
   - Subsequent scrolls: High hit rate (90%+ for large documents)
   - Render time dominated by active paragraph + cache lookups

**Cache Hit Rate**: Typically 95%+ when scrolling, assuming stable document.

**Result**: Smooth scrolling even in large documents due to caching.

### Use Case 6: Mouse Click to Position Cursor

**Scenario**: User clicks at screen coordinates (column=15, row=8).

**Flow**:
1. **Convert Click**: `display.pointer_from_mouse(column, row, scroll_top)`
   - Adjusts row by `scroll_top` to get absolute visual line
   - Adjusts column by text area offset
   - Calculates target visual line: `scroll_top + (row - text_area.y)`

2. **Find Nearest Position**:
   - Calls `closest_pointer_near_line(visual_line, relative_column)`
   - Searches `visual_positions` for entries on that line
   - Finds closest position by column distance
   - If line empty, searches nearby lines (alternating above/below)
   - Returns `CursorDisplay` with pointer and position

3. **Move Cursor**: If position found:
   - Calls `display.focus_display(&cursor_display)`
   - Moves editor cursor via `editor.move_to_pointer()`
   - Updates visual state
   - Enables cursor following

4. **Next Render**: Cursor appears at clicked location

**Fallback**: If click is outside text area or no valid position found, returns `None` and cursor doesn't move.

**Result**: Cursor jumps to clicked position.

### Use Case 7: Reveal Codes Mode

**Scenario**: User toggles reveal codes mode to see inline style markers.

**Flow**:
1. **Enable Reveal Codes**: `display.set_reveal_codes(true)`
   - Calls `editor.set_reveal_codes(true)`
   - Editor rebuilds segments to include reveal tag segments
   - Clears render cache (reveal tags change rendering)

2. **Generate Reveal Tags**:
   - Editor calls `clone_with_markers()` to generate reveal tag references
   - Creates `RevealTagRef` for each inline style boundary
   - Assigns unique IDs to tags

3. **Render with Tags**: `display.render_document(...)`
   - Passes reveal tags to renderer
   - Renderer inserts reveal tag fragments (e.g., `[Bold>`, `<Bold]`)
   - Tags styled distinctly (yellow on blue background)
   - Cursor can be positioned at tag boundaries
   - Tags have zero `content_width` (don't affect content column tracking)

4. **Navigation**: Cursor can move through reveal tags
   - `editor.move_left/right()` stops at tag boundaries
   - Visual movement skips over tags (uses `content_column`)

5. **Disable Reveal Codes**: `display.set_reveal_codes(false)`
   - Editor rebuilds segments without tags
   - Cache cleared again
   - Next render shows normal text

**Result**: Inline styles become visible as bracketed tags, helping users understand document structure.

### Use Case 8: Text Selection Rendering

**Scenario**: User selects text from one position to another.

**Flow**:
1. **Track Selection**: Application maintains selection start and end as `CursorPointer`

2. **Render with Selection**: `display.render_document(wrap_width, left_padding, Some((start, end)))`
   - Passes selection bounds to renderer
   - DirectRenderer tracks selection start/end events
   - During fragment processing:
     - Marks fragments between start/end with selection events
   - During line building:
     - Splits fragments at selection boundaries
     - Applies reverse video style to selected segments
     - Tracks `selection_active` flag across line breaks

3. **Selection Styling**:
   - Selected text: `style.add_modifier(Modifier::REVERSED)`
   - Non-selected text: Normal style
   - Selection can span multiple lines and paragraphs

4. **Cache Behavior**:
   - Paragraphs with selection are active (not cached)
   - Selection changes trigger re-render of affected paragraphs

**Result**: Selected text appears highlighted with reverse video.

### Use Case 9: Page Up/Page Down

**Scenario**: User presses Page Down.

**Flow**:
1. **Calculate Jump Distance**: `display.page_jump_distance()`
   - Uses `last_view_height` (stored from `update_after_render()`)
   - Calculates 90% of viewport: `(viewport * 0.9).round()`
   - Returns distance as `i32`

2. **Move by Page**: `display.move_page(1)` (1 for down, -1 for up)
   - Calls `move_cursor_vertical(distance)`
   - Uses standard vertical movement logic
   - Respects preferred column

3. **Result**: Cursor jumps approximately one screen, less a little overlap for context.

**Rationale**: 90% jump provides visual continuity between pages.

### Use Case 10: Editing in Checklist

**Scenario**: User navigates to an empty checklist item and types.

**Flow**:
1. **Navigate to Item**: `display.move_cursor_vertical(1)`
   - Cursor moves to checklist item
   - Editor cursor points to checklist item path with nested indices
   - `visual_positions` includes position for empty item (zero-width fragment)

2. **Render Empty Item**:
   - Renderer detects empty checklist item content
   - Creates zero-width fragment with position events at offset 0
   - Renders "[ ] " prefix followed by empty line
   - Cursor appears after prefix

3. **Insert Character**: `display.insert_char('T')`
   - Editor inserts character into checklist item content
   - Updates document structure
   - Clears cache

4. **Next Render**:
   - Checklist item now has content: "[ ] T"
   - Cursor map includes all character positions
   - Visual cursor appears after 'T'

**Result**: Typing works smoothly in empty checklist items.

---

## Re-rendering Triggers

The system re-renders the document in several scenarios:

### 1. Content Changes
- **Trigger**: User edits text, inserts/deletes paragraphs, changes styles
- **Handler**: `EditorDisplay::clear_render_cache()` called explicitly
- **Cache Impact**: Entire cache cleared
- **Next Render**: All paragraphs re-rendered and re-cached

### 2. Cursor Movement
- **Trigger**: User moves cursor via keyboard or mouse
- **Handler**: Cursor pointer updated in editor
- **Cache Impact**:
  - Previous active paragraph becomes cacheable
  - New active paragraph bypasses cache
- **Next Render**: Only cursor paragraph re-rendered, others from cache

### 3. Window Resize
- **Trigger**: Terminal dimensions change
- **Handler**: New wrap_width passed to `render_document()`
- **Cache Impact**: All cache keys different (include wrap_width), all misses
- **Next Render**: All paragraphs re-rendered with new width, cache repopulated

### 4. Toggle Reveal Codes
- **Trigger**: User enables/disables reveal codes mode
- **Handler**: `EditorDisplay::set_reveal_codes()` clears cache
- **Cache Impact**: Entire cache cleared (rendering completely different)
- **Next Render**: All paragraphs re-rendered, segments rebuilt

### 5. Scrolling (No Re-render)
- **Trigger**: User scrolls viewport
- **Handler**: Application adjusts scroll offset
- **Cache Impact**: None (document not re-rendered)
- **Render**: Uses existing `RenderResult.lines`, slicing for visible range

---

## Performance Optimizations

### 1. Render Caching
- **Benefit**: Avoids re-rendering unchanged paragraphs
- **Hit Rate**: 90-99% for large documents during scrolling/navigation
- **Trade-off**: Memory usage (stores rendered lines and metrics)

### 2. Content Hash Tracking
- **Benefit**: Detects actual content changes, not just edits elsewhere
- **Implementation**: Hash paragraph type, spans, children, entries recursively
- **Cache Key**: Includes hash to detect subtle changes

### 3. Active Paragraph Skip
- **Benefit**: Cursor styling always fresh without cache invalidation
- **Implementation**: Check if paragraph contains cursor/selection before cache lookup
- **Trade-off**: Active paragraph always re-rendered

### 4. Lazy Position Tracking
- **Benefit**: Cursor map only built when `track_all_positions=true`
- **Usage**: Enabled in EditorDisplay for visual navigation, disabled in simpler renders
- **Trade-off**: Slight overhead for tracking, but necessary for mouse clicks and visual movement

### 5. Fragment Reuse
- **Benefit**: Wrapping algorithm reuses fragments without cloning
- **Implementation**: Fragments moved through pipeline, split only when necessary
- **Trade-off**: More complex lifetime management

### 6. Incremental Segment Updates
- **Benefit**: Segments only rebuilt on structural changes, not content edits
- **Implementation**: `rebuild_segments()` called selectively
- **Trade-off**: Must ensure segments stay synchronized with document

---

## Limitations and Future Improvements

### Current Limitations

1. **Cache Eviction Strategy**: Clears entire cache when full, could use LRU
2. **Partial Invalidation**: Cache cleared entirely on edits, could invalidate only affected paragraphs
3. **Reveal Tag Overhead**: Still requires `clone_with_markers()` to generate tags (optimization opportunity)
4. **Fragment Splitting**: Long words split mid-word with no hyphenation
5. **Content Hash Recursion**: Deep nesting could impact hash performance

### Potential Improvements

1. **LRU Cache**: Implement least-recently-used eviction instead of full clear
2. **Incremental Invalidation**: Track dirty paragraphs and invalidate selectively
3. **Reveal Tag Extraction**: Generate reveal tags without document cloning
4. **Hyphenation**: Add word hyphenation for long words
5. **Parallel Rendering**: Render paragraphs in parallel for very large documents
6. **Viewport Rendering**: Only render visible paragraphs + buffer
7. **Memoized Hashing**: Cache paragraph hashes to avoid recomputation

---

## Thread Safety

The current implementation is **not thread-safe**:
- `DocumentEditor`, `EditorDisplay`, and `RenderCache` assume single-threaded access
- Rendering modifies internal state (cursor tracking, cache)
- Document editing mutates shared structures

For multi-threaded use:
- Synchronize access with `Mutex` or similar
- Consider immutable document snapshots for rendering
- Separate read-only rendering from editing operations
