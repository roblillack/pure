use std::ops::{Deref, DerefMut};

use ratatui::layout::Rect;

use crate::editor::{CursorPointer, DocumentEditor};
use crate::render::{
    CursorVisualPosition, DirectCursorTracking, RenderCache, RenderResult, render_document_direct,
};

/// EditorDisplay wraps a DocumentEditor and manages all visual/rendering concerns.
/// This includes cursor movement in visual space, wrapping, and rendering.
#[derive(Debug)]
pub struct EditorDisplay {
    editor: DocumentEditor,
    render_cache: RenderCache,
    visual_positions: Vec<CursorDisplay>,
    last_cursor_visual: Option<CursorVisualPosition>,
    preferred_column: Option<u16>,
    cursor_following: bool,
    last_view_height: usize,
    last_total_lines: usize,
    last_text_area: Rect,
}

impl EditorDisplay {
    /// Create a new EditorDisplay with the given editor
    pub fn new(editor: DocumentEditor) -> Self {
        Self {
            editor,
            render_cache: RenderCache::new(),
            visual_positions: Vec::new(),
            last_cursor_visual: None,
            preferred_column: None,
            cursor_following: true,
            last_view_height: 1,
            last_total_lines: 0,
            last_text_area: Rect::default(),
        }
    }

    /// Get the current visual positions
    pub fn visual_positions(&self) -> &[CursorDisplay] {
        &self.visual_positions
    }

    /// Get the last cursor visual position
    pub fn last_cursor_visual(&self) -> Option<CursorVisualPosition> {
        self.last_cursor_visual
    }

    /// Get the preferred column
    pub fn preferred_column(&self) -> Option<u16> {
        self.preferred_column
    }

    /// Set the preferred column
    pub fn set_preferred_column(&mut self, column: Option<u16>) {
        self.preferred_column = column;
    }

    /// Check if cursor following is enabled
    pub fn cursor_following(&self) -> bool {
        self.cursor_following
    }

    /// Set cursor following mode
    pub fn set_cursor_following(&mut self, following: bool) {
        self.cursor_following = following;
    }

    /// Detach cursor follow
    pub fn detach_cursor_follow(&mut self) {
        self.cursor_following = false;
    }

    /// Get last view height
    pub fn last_view_height(&self) -> usize {
        self.last_view_height
    }

    /// Get last total lines
    pub fn last_total_lines(&self) -> usize {
        self.last_total_lines
    }

    /// Get last text area
    pub fn last_text_area(&self) -> Rect {
        self.last_text_area
    }

    /// Clear render cache (called when document changes)
    pub fn clear_render_cache(&mut self) {
        self.render_cache.clear();
    }

    /// Render the document at the given width and update internal state
    pub fn render_document(
        &mut self,
        _width: usize,
        wrap_width: usize,
        left_padding: usize,
        selection: Option<(CursorPointer, CursorPointer)>,
        _cursor_sentinel: char,
        _selection_start_sentinel: char,
        _selection_end_sentinel: char,
    ) -> RenderResult {
        // Use direct rendering - no document cloning needed!
        let cursor_pointer = self.editor.cursor_pointer();

        // Get reveal tags for reveal codes mode
        // TODO: Optimize this - we currently need to call clone_with_markers just for reveal_tags
        // In the future, extract reveal tag generation into a separate non-cloning function
        let reveal_tags = if self.editor.reveal_codes() {
            let (_, _, tags, _) = self
                .editor
                .clone_with_markers('\u{F8FF}', None, '\u{F8FE}', '\u{F8FD}');
            tags
        } else {
            Vec::new()
        };

        let result = render_document_direct(
            self.editor.document(),
            wrap_width,
            left_padding,
            &reveal_tags,
            DirectCursorTracking {
                cursor: Some(&cursor_pointer),
                selection: selection.as_ref().map(|(start, end)| (start, end)),
                track_all_positions: true, // Track all positions for cursor_map
            },
            Some(&mut self.render_cache),
        );

        // Update internal state from render result
        self.visual_positions = result
            .cursor_map
            .iter()
            .cloned()
            .map(|(pointer, position)| CursorDisplay { pointer, position })
            .collect();

        self.last_cursor_visual = result.cursor;
        if self.preferred_column.is_none() {
            self.preferred_column = result.cursor.map(|p| p.column);
        }

        result
    }

    /// Update tracking state after rendering (called from draw)
    pub fn update_after_render(&mut self, text_area: Rect, total_lines: usize) {
        self.last_text_area = text_area;
        self.last_total_lines = total_lines;
        self.last_view_height = (text_area.height as usize).max(1);
    }

    /// Move cursor vertically by delta lines
    pub fn move_cursor_vertical(&mut self, delta: i32) {
        if self.visual_positions.is_empty() {
            return;
        }

        let pointer = self.editor.cursor_pointer();
        let current_position = self
            .visual_positions
            .iter()
            .find(|entry| entry.pointer == pointer)
            .map(|entry| entry.position)
            .or(self.last_cursor_visual);

        let Some(current) = current_position else {
            return;
        };

        let desired_column = self.preferred_column.unwrap_or(current.column);

        let max_line = self
            .visual_positions
            .iter()
            .map(|entry| entry.position.line)
            .max()
            .unwrap_or(0);

        let mut target_line = current.line as i32 + delta;
        if target_line < 0 {
            target_line = 0;
        } else if target_line > max_line as i32 {
            target_line = max_line as i32;
        }

        let target_line_usize = target_line as usize;

        let destination = self
            .closest_pointer_on_line(target_line_usize, desired_column)
            .or_else(|| self.search_nearest_line(target_line_usize, delta, desired_column));

        if let Some(dest) = destination {
            if self.editor.move_to_pointer(&dest.pointer) {
                self.preferred_column = Some(desired_column);
                self.last_cursor_visual = Some(dest.position);
            } else {
                self.last_cursor_visual = Some(dest.position);
            }
        }
    }

    /// Calculate the page jump distance based on viewport height
    pub fn page_jump_distance(&self) -> i32 {
        let viewport = self.last_view_height.max(1);
        let approx = ((viewport as f32) * 0.9).round() as usize;
        approx.max(1) as i32
    }

    /// Move by a page in the given direction (-1 for up, 1 for down)
    pub fn move_page(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        let distance = self.page_jump_distance();
        self.move_cursor_vertical(distance * direction);
    }

    /// Move cursor to the start of the current visual line
    pub fn move_to_visual_line_start(&mut self) {
        self.preferred_column = None;

        if self.visual_positions.is_empty() {
            self.editor.move_to_segment_start();
            return;
        }

        let pointer = self.editor.cursor_pointer();
        let current = self
            .visual_positions
            .iter()
            .find(|entry| entry.pointer == pointer)
            .map(|entry| entry.position)
            .or(self.last_cursor_visual);

        let Some(current_position) = current else {
            self.editor.move_to_segment_start();
            return;
        };

        let destination = self
            .visual_positions
            .iter()
            .filter(|entry| entry.position.line == current_position.line)
            .cloned()
            .min_by_key(|entry| {
                (
                    entry.position.content_column as usize,
                    entry.position.column as usize,
                    entry.pointer.offset,
                )
            });

        if let Some(target) = destination {
            if self.editor.move_to_pointer(&target.pointer) {
                self.last_cursor_visual = Some(target.position);
            } else {
                self.last_cursor_visual = Some(target.position);
            }
        } else {
            self.editor.move_to_segment_start();
        }
    }

    /// Move cursor to the end of the current visual line
    pub fn move_to_visual_line_end(&mut self) {
        self.preferred_column = None;

        if self.visual_positions.is_empty() {
            self.editor.move_to_segment_end();
            return;
        }

        let pointer = self.editor.cursor_pointer();
        let current = self
            .visual_positions
            .iter()
            .find(|entry| entry.pointer == pointer)
            .map(|entry| entry.position)
            .or(self.last_cursor_visual);

        let Some(current_position) = current else {
            self.editor.move_to_segment_end();
            return;
        };

        let destination = self
            .visual_positions
            .iter()
            .filter(|entry| entry.position.line == current_position.line)
            .cloned()
            .max_by_key(|entry| {
                (
                    entry.position.content_column as usize,
                    entry.position.column as usize,
                    entry.pointer.offset,
                )
            });

        if let Some(target) = destination {
            if self.editor.move_to_pointer(&target.pointer) {
                self.last_cursor_visual = Some(target.position);
            } else {
                self.last_cursor_visual = Some(target.position);
            }
        } else {
            self.editor.move_to_segment_end();
        }
    }

    /// Find the closest pointer on a given line to a target column
    fn closest_pointer_on_line(&self, line: usize, column: u16) -> Option<CursorDisplay> {
        self.visual_positions
            .iter()
            .filter(|entry| entry.position.line == line)
            .min_by_key(|entry| column_distance(entry.position.column, column))
            .cloned()
    }

    /// Search for the nearest line with content, starting from start_line and moving in delta direction
    fn search_nearest_line(
        &self,
        start_line: usize,
        delta: i32,
        column: u16,
    ) -> Option<CursorDisplay> {
        if delta == 0 {
            return None;
        }
        let max_line = self
            .visual_positions
            .iter()
            .map(|entry| entry.position.line)
            .max()
            .unwrap_or(0);

        let mut distance = 1usize;
        loop {
            if delta < 0 {
                if let Some(line) = start_line.checked_sub(distance) {
                    if let Some(found) = self.closest_pointer_on_line(line, column) {
                        return Some(found);
                    }
                } else {
                    break;
                }
            } else {
                let line = start_line + distance;
                if line > max_line {
                    break;
                }
                if let Some(found) = self.closest_pointer_on_line(line, column) {
                    return Some(found);
                }
            }

            if distance > max_line.saturating_add(1) {
                break;
            }
            distance += 1;
        }

        None
    }

    /// Find the closest pointer near a line (searching up and down if not found on exact line)
    fn closest_pointer_near_line(&self, line: usize, column: u16) -> Option<CursorDisplay> {
        if self.visual_positions.is_empty() {
            return None;
        }
        if let Some(hit) = self.closest_pointer_on_line(line, column) {
            return Some(hit);
        }
        let max_line = self
            .visual_positions
            .iter()
            .map(|entry| entry.position.line)
            .max()
            .unwrap_or(0);
        let mut distance = 1usize;
        while line.checked_sub(distance).is_some() || line + distance <= max_line {
            if let Some(prev) = line.checked_sub(distance) {
                if let Some(hit) = self.closest_pointer_on_line(prev, column) {
                    return Some(hit);
                }
            }
            let next = line + distance;
            if next <= max_line {
                if let Some(hit) = self.closest_pointer_on_line(next, column) {
                    return Some(hit);
                }
            } else if line.checked_sub(distance).is_none() {
                break;
            }
            distance += 1;
        }
        None
    }

    /// Convert mouse coordinates to a cursor pointer
    pub fn pointer_from_mouse(
        &self,
        column: u16,
        row: u16,
        scroll_top: usize,
    ) -> Option<CursorDisplay> {
        if self.visual_positions.is_empty() {
            return None;
        }
        let area = self.last_text_area;
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let max_x = area.x.saturating_add(area.width);
        let max_y = area.y.saturating_add(area.height);
        if column < area.x || column >= max_x || row < area.y || row >= max_y {
            return None;
        }
        let line = scroll_top.saturating_add((row - area.y) as usize);
        let relative_column = column.saturating_sub(area.x);
        self.closest_pointer_near_line(line, relative_column)
    }

    /// Get the start and end boundaries of a visual line
    pub fn visual_line_boundaries(&self, line: usize) -> Option<(CursorDisplay, CursorDisplay)> {
        let mut entries: Vec<_> = self
            .visual_positions
            .iter()
            .filter(|entry| entry.position.line == line)
            .cloned()
            .collect();
        if entries.is_empty() {
            return None;
        }
        entries.sort_by_key(|entry| {
            (
                entry.position.content_column as usize,
                entry.position.column as usize,
                entry.pointer.offset,
            )
        });
        let start = entries.first()?.clone();
        let end = entries.last()?.clone();
        Some((start, end))
    }

    /// Focus on a specific display position
    pub fn focus_display(&mut self, display: &CursorDisplay) {
        if self.editor.move_to_pointer(&display.pointer) {
            self.last_cursor_visual = Some(display.position);
            self.preferred_column = Some(display.position.column);
            self.cursor_following = true;
        }
    }

    /// Focus on a specific pointer
    pub fn focus_pointer(&mut self, pointer: &CursorPointer) {
        if self.editor.move_to_pointer(pointer) {
            if let Some(display) = self
                .visual_positions
                .iter()
                .find(|entry| &entry.pointer == pointer)
                .cloned()
            {
                self.last_cursor_visual = Some(display.position);
                self.preferred_column = Some(display.position.column);
            } else {
                self.last_cursor_visual = None;
                self.preferred_column = None;
            }
            self.cursor_following = true;
        }
    }
}

impl Deref for EditorDisplay {
    type Target = DocumentEditor;

    fn deref(&self) -> &Self::Target {
        &self.editor
    }
}

impl DerefMut for EditorDisplay {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.editor
    }
}

#[derive(Clone, Debug)]
pub struct CursorDisplay {
    pub pointer: CursorPointer,
    pub position: CursorVisualPosition,
}

fn column_distance(a: u16, b: u16) -> u16 {
    if a >= b { a - b } else { b - a }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::DocumentEditor;
    use tdoc::{Document, Paragraph};

    fn create_test_display() -> EditorDisplay {
        let mut doc = Document::new();
        doc.paragraphs.push(
            Paragraph::new_text().with_content(vec![tdoc::Span::new_text("First line of text")]),
        );
        doc.paragraphs.push(
            Paragraph::new_text().with_content(vec![tdoc::Span::new_text(
                "Second line with more content that might wrap",
            )]),
        );
        doc.paragraphs
            .push(Paragraph::new_text().with_content(vec![tdoc::Span::new_text("Third line")]));

        let mut editor = DocumentEditor::new(doc);
        editor.ensure_cursor_selectable();
        EditorDisplay::new(editor)
    }

    #[test]
    fn test_move_cursor_vertical_down() {
        let mut display = create_test_display();

        // Render to populate visual positions
        let _render = display.render_document(80, 80, 0, None, '\u{F8FF}', '\u{F8FE}', '\u{F8FD}');

        // Get initial cursor position
        let initial_pointer = display.cursor_pointer();

        // Move down one line
        display.move_cursor_vertical(1);

        let new_pointer = display.cursor_pointer();
        assert_ne!(initial_pointer, new_pointer, "Cursor should have moved");
    }

    #[test]
    fn test_move_cursor_vertical_up() {
        let mut display = create_test_display();

        // Render to populate visual positions
        let _render = display.render_document(80, 80, 0, None, '\u{F8FF}', '\u{F8FE}', '\u{F8FD}');

        // Move to second paragraph first
        display.move_cursor_vertical(1);
        let mid_pointer = display.cursor_pointer();

        // Move back up
        display.move_cursor_vertical(-1);

        let new_pointer = display.cursor_pointer();
        assert_ne!(mid_pointer, new_pointer, "Cursor should have moved up");
    }

    #[test]
    fn test_move_to_visual_line_start() {
        let mut display = create_test_display();

        // Render to populate visual positions
        let _render = display.render_document(80, 80, 0, None, '\u{F8FF}', '\u{F8FE}', '\u{F8FD}');

        // Move cursor to the middle of the line
        for _ in 0..5 {
            display.move_right();
        }

        let mid_offset = display.cursor_pointer().offset;
        assert!(mid_offset > 0, "Cursor should be in the middle");

        // Move to line start
        display.move_to_visual_line_start();

        let start_offset = display.cursor_pointer().offset;
        assert_eq!(start_offset, 0, "Cursor should be at start of line");
    }

    #[test]
    fn test_move_to_visual_line_end() {
        let mut display = create_test_display();

        // Render to populate visual positions
        let _render = display.render_document(80, 80, 0, None, '\u{F8FF}', '\u{F8FE}', '\u{F8FD}');

        let initial_offset = display.cursor_pointer().offset;

        // Move to line end
        display.move_to_visual_line_end();

        let end_offset = display.cursor_pointer().offset;
        assert!(
            end_offset > initial_offset,
            "Cursor should have moved to end"
        );

        // The first line is "First line of text" (19 characters)
        // The cursor should be placed at offset 19 (after the last character)
        let text = &display.document().paragraphs[0].content()[0].text;
        let expected_offset = text.len();
        assert_eq!(
            end_offset,
            expected_offset,
            "Cursor should be at offset {} (after last char '{}'), but is at offset {}. Text: '{}'",
            expected_offset,
            text.chars().last().unwrap_or(' '),
            end_offset,
            text
        );
    }

    #[test]
    fn test_page_jump_distance() {
        let mut display = create_test_display();
        display.last_view_height = 20;

        let distance = display.page_jump_distance();
        assert_eq!(
            distance, 18,
            "Page jump should be 90% of viewport (20 * 0.9 = 18)"
        );
    }

    #[test]
    fn test_move_page_down() {
        let mut display = create_test_display();
        display.last_view_height = 10;

        // Render to populate visual positions
        let _render = display.render_document(80, 80, 0, None, '\u{F8FF}', '\u{F8FE}', '\u{F8FD}');

        let initial_pointer = display.cursor_pointer();

        // Move page down
        display.move_page(1);

        let new_pointer = display.cursor_pointer();
        // Cursor should have attempted to move (even if it doesn't move far in a small document)
        assert!(
            new_pointer != initial_pointer || display.visual_positions().is_empty(),
            "Cursor should have attempted to move"
        );
    }

    #[test]
    fn test_preferred_column_preserved() {
        let mut display = create_test_display();

        // Render to populate visual positions
        let _render = display.render_document(80, 80, 0, None, '\u{F8FF}', '\u{F8FE}', '\u{F8FD}');

        // Move to the middle of the line
        for _ in 0..5 {
            display.move_right();
        }

        // Set preferred column explicitly
        display.set_preferred_column(Some(5));
        assert_eq!(display.preferred_column(), Some(5));

        // Move vertically - preferred column should be used
        display.move_cursor_vertical(1);
        assert_eq!(
            display.preferred_column(),
            Some(5),
            "Preferred column should be preserved"
        );
    }

    #[test]
    fn test_cursor_following_toggle() {
        let mut display = create_test_display();

        assert!(
            display.cursor_following(),
            "Cursor following should start as true"
        );

        display.detach_cursor_follow();
        assert!(
            !display.cursor_following(),
            "Cursor following should be false after detach"
        );

        display.set_cursor_following(true);
        assert!(
            display.cursor_following(),
            "Cursor following should be true after set"
        );
    }

    #[test]
    fn test_visual_line_boundaries() {
        let mut display = create_test_display();

        // Render to populate visual positions
        let _render = display.render_document(80, 80, 0, None, '\u{F8FF}', '\u{F8FE}', '\u{F8FD}');

        // Get boundaries of first visual line
        if let Some((start, end)) = display.visual_line_boundaries(0) {
            assert_eq!(start.position.line, 0, "Start should be on line 0");
            assert_eq!(end.position.line, 0, "End should be on line 0");
            assert!(
                start.pointer.offset <= end.pointer.offset,
                "Start offset should be <= end offset"
            );
        } else {
            panic!("Should have visual line boundaries for line 0");
        }
    }

    #[test]
    fn move_down_from_h2_to_checklist() {
        use crate::editor::{ParagraphPath, SegmentKind, SpanPath};
        use tdoc::parse;
        let content = std::fs::read_to_string("test.ftml").unwrap();
        let doc = parse(std::io::Cursor::new(content)).unwrap();
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Render to populate visual_positions
        let _ = display.render_document(80, 80, 0, None, '\0', '\0', '\0');

        // Find the H2 "Todos"
        let h2_path = ParagraphPath::new_root(1); // H1 is 0, H2 is 1
        let h2_pointer = CursorPointer {
            paragraph_path: h2_path.clone(),
            span_path: SpanPath::new(vec![0]),
            offset: 0,
            segment_kind: SegmentKind::Text,
        };
        assert!(display.move_to_pointer(&h2_pointer), "Could not move to H2");

        // Check we are where we think we are
        let cursor = display.cursor_pointer();
        assert_eq!(cursor.paragraph_path, h2_path);

        // Move down
        display.move_cursor_vertical(1);

        let cursor_after_move = display.cursor_pointer();

        let expected_path = {
            let mut path = ParagraphPath::new_root(2);
            path.push_checklist_item(vec![0]);
            path
        };

        assert_eq!(
            cursor_after_move.paragraph_path, expected_path,
            "Cursor should have moved to the first checklist item"
        );
    }
}
