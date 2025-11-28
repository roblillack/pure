use std::ops::{Deref, DerefMut};

use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::editor::{CursorPointer, DocumentEditor};
use crate::render::{
    CursorVisualPosition, DirectCursorTracking, ParagraphLineInfo, RenderResult, layout_paragraph,
    render_document_direct,
};

/// EditorDisplay wraps a DocumentEditor and manages all visual/rendering concerns.
/// This includes cursor movement in visual space, wrapping, and rendering.
#[derive(Debug)]
pub struct EditorDisplay {
    editor: DocumentEditor,
    /// Cached layout result - only re-rendered when document changes or viewport resizes
    layout: Option<RenderResult>,
    /// Parameters used for cached_layout (to detect when re-render is needed)
    wrap_width: usize,
    left_padding: usize,

    /// Last wrap_width used for rendering (needed for cache lookups)
    last_wrap_width: usize,
    /// Last left_padding used for rendering (needed for cache lookups)
    last_left_padding: usize,
    preferred_column: Option<u16>,
    cursor_following: bool,
    last_view_height: usize,
    last_total_lines: usize,
    last_text_area: Rect,
    /// Set to true when document changes to trigger re-render
    layout_dirty: bool,
    /// Track which paragraphs were last modified for incremental updates
    last_modified_paragraphs: Vec<usize>,
    /// Track the last selection to detect selection changes
    last_selection: Option<(CursorPointer, CursorPointer)>,
}

impl EditorDisplay {
    /// Create a new EditorDisplay with the given editor
    pub fn new(editor: DocumentEditor) -> Self {
        Self {
            editor,
            layout: None,
            wrap_width: 80,
            left_padding: 0,
            last_wrap_width: 80,
            last_left_padding: 0,
            preferred_column: None,
            cursor_following: true,
            last_view_height: 1,
            last_total_lines: 0,
            last_text_area: Rect::default(),
            layout_dirty: true,
            last_modified_paragraphs: Vec::new(),
            last_selection: None,
        }
    }

    /// Get all visual positions from paragraph_lines (for tests and legacy code)
    #[allow(dead_code)]
    pub fn visual_positions(&self) -> Vec<CursorDisplay> {
        if let Some(layout) = &self.layout {
            layout
                .paragraph_lines
                .iter()
                .flat_map(|info| {
                    info.positions.iter().map(|(pointer, position)| {
                        // Convert relative position to absolute
                        let mut absolute_pos = *position;
                        absolute_pos.line = info.start_line + position.line;
                        CursorDisplay {
                            pointer: pointer.clone(),
                            position: absolute_pos,
                        }
                    })
                })
                .collect()
        } else {
            vec![]
        }
    }

    pub fn get_layout(&self) -> &RenderResult {
        self.layout.as_ref().unwrap()
    }

    pub fn get_total_lines(&self) -> usize {
        if let Some(layout) = &self.layout {
            layout.total_lines
        } else {
            0
        }
    }

    pub fn get_content_lines(&self) -> usize {
        if let Some(layout) = &self.layout {
            layout.content_lines
        } else {
            0
        }
    }

    pub fn get_lines(&self) -> Option<Vec<Line<'static>>> {
        if let Some(layout) = &self.layout {
            Some(layout.lines.clone())
        } else {
            None
        }
    }

    /// Get the last cursor visual position
    pub fn cursor_visual(&self) -> Option<CursorVisualPosition> {
        if let Some(layout) = &self.layout {
            layout.cursor
        } else {
            None
        }
    }

    /// Update the cached cursor visual position after cursor movement
    /// Looks up the current logical cursor position in paragraph_lines and updates layout.cursor
    fn update_cursor_visual_position(&mut self) {
        let Some(layout) = &mut self.layout else {
            return;
        };

        let cursor_pointer = self.editor.cursor_pointer();

        // Search for the visual position of this pointer in paragraph_lines
        let found = layout.paragraph_lines.iter().find_map(|info| {
            info.positions
                .iter()
                .find(|(p, _)| p == &cursor_pointer)
                .map(|(_, pos)| {
                    // Convert relative position to absolute
                    let mut absolute_pos = *pos;
                    absolute_pos.line = info.start_line + pos.line;
                    absolute_pos
                })
        });

        layout.cursor = found;

        // If cursor wasn't found (because track_all_positions was false during rendering),
        // mark layout dirty and force a re-render to populate the cursor position
        if found.is_none() {
            self.layout_dirty = true;
            self.render_document(self.wrap_width, self.left_padding, None);
        }
    }

    /// Get cursor positions for a specific visual line.
    /// Positions are read directly from the paragraph_lines structure populated during rendering.
    fn get_positions_for_line(&self, line: usize) -> Vec<CursorDisplay> {
        // Find which paragraph contains this line
        let paragraph_info = self
            .layout
            .as_ref()
            .unwrap()
            .paragraph_lines
            .iter()
            .find(|info| line >= info.start_line && line <= info.end_line);

        let Some(info) = paragraph_info else {
            return Vec::new();
        };

        // Positions are stored relative to paragraph start, convert to absolute
        let relative_line = line.saturating_sub(info.start_line);

        info.positions
            .iter()
            .filter(|(_, position)| position.line == relative_line)
            .map(|(pointer, position)| {
                // Convert relative position back to absolute
                let mut absolute_pos = *position;
                absolute_pos.line = info.start_line + position.line;
                CursorDisplay {
                    pointer: pointer.clone(),
                    position: absolute_pos,
                }
            })
            .collect()
    }

    /// Get the preferred column
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn last_text_area(&self) -> Rect {
        self.last_text_area
    }

    /// Clear render cache (called when document changes)
    ///
    /// If specific paragraphs were modified (tracked in last_modified_paragraphs),
    /// tries to do an incremental update instead of marking the entire layout dirty.
    ///
    /// Returns true if an incremental update succeeded, false if a full re-render is needed.
    pub fn clear_render_cache(&mut self) -> bool {
        // Try incremental update if we know which paragraphs changed
        if !self.last_modified_paragraphs.is_empty() {
            let paragraphs_to_update = std::mem::take(&mut self.last_modified_paragraphs);
            let mut all_succeeded = true;

            for para_index in paragraphs_to_update {
                if !self.update_paragraph_layout(para_index) {
                    all_succeeded = false;
                    break;
                }
            }

            if all_succeeded {
                // Incremental update succeeded!
                return true;
            }
        }

        // Fall back to full re-render
        self.layout_dirty = true;
        self.last_modified_paragraphs.clear();
        false // Full re-render needed
    }

    /// Mark a specific paragraph as modified to enable incremental updates
    /// Note: This only sets the tracking, clear_render_cache() must be called separately
    pub fn mark_paragraph_modified(&mut self, paragraph_index: usize) {
        if !self.last_modified_paragraphs.contains(&paragraph_index) {
            self.last_modified_paragraphs.push(paragraph_index);
        }
    }

    pub fn force_full_relayout(&mut self) {
        self.last_modified_paragraphs.clear();
        self.layout_dirty = true;
    }

    /// Update layout for a single paragraph (incremental update)
    ///
    /// This is much faster than re-rendering the entire document when only one paragraph changed.
    /// Returns true if the update was successful, false if a full re-render is needed.
    pub fn update_paragraph_layout(&mut self, paragraph_index: usize) -> bool {
        // Need cached layout and valid parameters to do incremental update
        let Some(cached_layout) = self.layout.as_mut() else {
            return false;
        };

        // Get the paragraph
        let Some(paragraph) = self.editor.document().paragraphs.get(paragraph_index) else {
            return false;
        };

        // Find the paragraph info in cached layout
        let Some(para_info) = cached_layout
            .paragraph_lines
            .iter()
            .find(|info| info.paragraph_index == paragraph_index)
        else {
            return false;
        };

        let old_start_line = para_info.start_line;
        let old_end_line = para_info.end_line;
        let old_line_count = old_end_line - old_start_line + 1;

        // Skip reveal tags for incremental updates to avoid expensive document clone
        // Reveal codes will be updated on the next full render
        let reveal_tags = Vec::new();

        // Layout the paragraph
        let cursor_pointer = self.editor.cursor_pointer();
        let layout = layout_paragraph(
            paragraph,
            paragraph_index,
            crate::editor::ParagraphPath::new_root(paragraph_index),
            self.wrap_width,
            self.left_padding,
            "",
            &reveal_tags,
            DirectCursorTracking {
                cursor: Some(&cursor_pointer),
                selection: None,
                track_all_positions: true,
            },
        );

        let new_line_count = layout.line_count;
        let line_count_delta = new_line_count as isize - old_line_count as isize;

        // Replace the lines in the cached layout
        let lines_start = old_start_line;
        let lines_end = old_end_line + 1; // exclusive end
        cached_layout
            .lines
            .splice(lines_start..lines_end, layout.lines.clone());

        // Update paragraph_lines entry
        let para_info_index = cached_layout
            .paragraph_lines
            .iter()
            .position(|info| info.paragraph_index == paragraph_index)
            .unwrap();

        // Convert relative positions to absolute
        let new_positions: Vec<_> = layout
            .positions
            .iter()
            .map(|(pointer, pos)| {
                let mut absolute_pos = *pos;
                absolute_pos.line = old_start_line + pos.line;
                // Recompute content_line from line_metrics
                // For now, just use the line number (will be fixed in next full render)
                absolute_pos.content_line = absolute_pos.line;
                (pointer.clone(), absolute_pos)
            })
            .collect();

        cached_layout.paragraph_lines[para_info_index] = ParagraphLineInfo {
            paragraph_index,
            start_line: old_start_line,
            end_line: old_start_line + new_line_count.saturating_sub(1),
            positions: new_positions
                .iter()
                .map(|(pointer, pos)| {
                    let mut relative_pos = *pos;
                    relative_pos.line = pos.line.saturating_sub(old_start_line);
                    (pointer.clone(), relative_pos)
                })
                .collect(),
        };

        // If line count changed, adjust all subsequent paragraphs
        if line_count_delta != 0 {
            for info in cached_layout.paragraph_lines.iter_mut() {
                // Adjust paragraphs that come after the edited one (by index, not by line number)
                if info.paragraph_index > paragraph_index {
                    let _old_start = info.start_line;
                    let _old_end = info.end_line;
                    info.start_line = (info.start_line as isize + line_count_delta) as usize;
                    info.end_line = (info.end_line as isize + line_count_delta) as usize;
                }
            }

            // Update total lines
            cached_layout.total_lines =
                (cached_layout.total_lines as isize + line_count_delta) as usize;
        }

        // Update cursor if it's in the cached layout
        if let Some(ref mut cursor) = cached_layout.cursor {
            if cursor.line >= old_start_line && cursor.line <= old_end_line {
                // Cursor is in the updated paragraph - recompute from layout
                if let Some(layout_cursor) = layout.cursor {
                    cursor.line = old_start_line + layout_cursor.line;
                    cursor.column = layout_cursor.column;
                    cursor.content_column = layout_cursor.content_column;
                    // content_line will be recomputed on next full render
                }
            } else if cursor.line > old_end_line && line_count_delta != 0 {
                // Cursor is after the updated paragraph - adjust line number
                cursor.line = (cursor.line as isize + line_count_delta) as usize;
            } else {
            }
        }

        true
    }

    /// Set reveal codes mode and clear cache
    /// This overrides the Deref implementation to ensure cache is cleared
    pub fn set_reveal_codes(&mut self, enabled: bool) {
        self.editor.set_reveal_codes(enabled);
        self.layout_dirty = true;
    }

    /// Render the document at the given width and update internal state
    ///
    /// Uses cached layout if available and parameters unchanged. Only re-renders when:
    /// - Document content changed (layout_dirty=true)
    /// - Viewport dimensions changed (wrap_width or left_padding)
    /// - Selection changed
    /// - First render (cached_layout=None)
    ///
    /// Note: The layout should already be up-to-date because wrapper methods
    /// (insert_char, delete, etc.) automatically trigger incremental updates.
    pub fn render_document(
        &mut self,
        wrap_width: usize,
        left_padding: usize,
        selection: Option<(CursorPointer, CursorPointer)>,
    ) {
        // Check if selection has changed
        let selection_changed = self.last_selection != selection;

        // Check if we can reuse cached layout
        let needs_rerender = self.layout_dirty
            || self.layout.is_none()
            || self.wrap_width != wrap_width
            || self.left_padding != left_padding
            || selection_changed;

        if needs_rerender {
            self.render_document_internal(wrap_width, left_padding, selection.clone(), false);
            self.wrap_width = wrap_width;
            self.left_padding = left_padding;
            self.layout_dirty = false;
            self.last_selection = selection;
        } else {
            let result = self.layout.as_ref().unwrap().clone();
            // Update internal cursor state even when using cached layout
            if self.preferred_column.is_none() {
                self.preferred_column = result.cursor.map(|p| p.content_column);
            }

            // Return reference to cached layout - caller must clone if needed
            // But since paragraph_lines is already in EditorDisplay, no need for full clone
        }
    }

    /// Render the document with positions - always forces a re-render
    ///
    /// Use this after document edits to ensure changes are reflected.
    /// For just viewing/scrolling, use render_document() which caches the layout.
    pub fn render_document_with_positions(
        &mut self,
        wrap_width: usize,
        left_padding: usize,
        selection: Option<(CursorPointer, CursorPointer)>,
    ) {
        // Force re-render
        self.render_document_internal(wrap_width, left_padding, selection.clone(), true);
        self.wrap_width = wrap_width;
        self.left_padding = left_padding;
        self.layout_dirty = false;
        self.last_selection = selection;
    }

    /// Internal render implementation
    fn render_document_internal(
        &mut self,
        wrap_width: usize,
        left_padding: usize,
        selection: Option<(CursorPointer, CursorPointer)>,
        track_all_positions: bool,
    ) {
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
                track_all_positions,
            },
        );

        // Update internal state from render result
        // Always store paragraph_lines which now includes all cursor positions
        self.layout = Some(result);

        // Store rendering parameters for cache lookups
        self.last_wrap_width = wrap_width;
        self.last_left_padding = left_padding;

        if self.preferred_column.is_none() {
            self.preferred_column = self
                .layout
                .as_ref()
                .unwrap()
                .cursor
                .map(|p| p.content_column);
        }
    }

    /// Update tracking state after rendering (called from draw)
    pub fn update_after_render(&mut self, text_area: Rect) {
        self.last_text_area = text_area;
        self.last_total_lines = self.layout.as_ref().unwrap().total_lines;
        self.last_view_height = (text_area.height as usize).max(1);
    }

    /// Move cursor vertically by delta lines
    pub fn move_cursor_vertical(&mut self, delta: i32) {
        // If layout is stale, force a re-render before cursor movement
        // This can happen after paragraph breaks or structural changes
        if self.layout_dirty || self.layout.is_none() {
            // Force re-render using stored parameters
            self.render_document(self.wrap_width, self.left_padding, None);
        }

        // Use layout.cursor as the source of truth for current position
        let Some(current) = self.cursor_visual() else {
            // Fallback to logical cursor movement when visual position isn't available
            if delta < 0 {
                self.editor.move_up();
            } else if delta > 0 {
                self.editor.move_down();
            }
            return;
        };

        // Use content_column (without left padding) for consistent vertical movement
        let desired_column = self.preferred_column.unwrap_or(current.content_column);

        // Get max line from paragraph_lines (lightweight)
        let max_line = self
            .layout
            .as_ref()
            .unwrap()
            .paragraph_lines
            .last()
            .map(|p| p.end_line)
            .unwrap_or(0);

        let mut target_line = current.line as i32 + delta;
        if target_line < 0 {
            target_line = 0;
        } else if target_line > max_line as i32 {
            target_line = max_line as i32;
        }

        let target_line_usize = target_line as usize;

        let from_closest = self.closest_pointer_on_line(target_line_usize, desired_column);
        let destination = from_closest.or_else(|| {
            let result = self.search_nearest_line(target_line_usize, delta, desired_column);
            result
        });

        let pointer = self.editor.cursor_pointer();

        if let Some(dest) = destination {
            // Check if destination is the same as current position
            let is_same_position = dest.pointer == pointer;

            if is_same_position {
                // Destination is same as current - fall back to logical movement
                if delta < 0 {
                    self.editor.move_up();
                } else if delta > 0 {
                    self.editor.move_down();
                }
                self.preferred_column = None;
            } else if self.editor.move_to_pointer(&dest.pointer) {
                self.preferred_column = Some(desired_column);
            }
        } else {
            // Fallback: If visual-based movement failed, try logical cursor movement
            if delta < 0 {
                self.editor.move_up();
            } else if delta > 0 {
                self.editor.move_down();
            }
            self.preferred_column = None;
        }

        // Update the cached cursor visual position after movement
        self.update_cursor_visual_position();
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

        // If layout is stale, force re-render
        if self.layout_dirty || self.layout.is_none() {
            self.render_document(self.wrap_width, self.left_padding, None);
        }

        let Some(current_position) = self.cursor_visual() else {
            self.editor.move_to_segment_start();
            return;
        };

        let positions_on_line = self.get_positions_for_line(current_position.line);
        let destination = positions_on_line.into_iter().min_by_key(|entry| {
            (
                entry.position.content_column as usize,
                entry.position.column as usize,
                entry.pointer.offset,
            )
        });

        if let Some(target) = destination {
            self.editor.move_to_pointer(&target.pointer);
        } else {
            self.editor.move_to_segment_start();
        }

        // Update the cached cursor visual position after movement
        self.update_cursor_visual_position();
    }

    /// Move cursor to the end of the current visual line
    pub fn move_to_visual_line_end(&mut self) {
        self.preferred_column = None;

        // If layout is stale, force re-render
        if self.layout_dirty || self.layout.is_none() {
            self.render_document(self.wrap_width, self.left_padding, None);
        }

        let Some(current_position) = self.cursor_visual() else {
            self.editor.move_to_segment_end();
            return;
        };

        let positions_on_line = self.get_positions_for_line(current_position.line);
        let destination = positions_on_line.into_iter().max_by_key(|entry| {
            (
                entry.position.content_column as usize,
                entry.position.column as usize,
                entry.pointer.offset,
            )
        });

        if let Some(target) = destination {
            self.editor.move_to_pointer(&target.pointer);
        } else {
            self.editor.move_to_segment_end();
        }

        // Update the cached cursor visual position after movement
        self.update_cursor_visual_position();
    }

    /// Move cursor left by one character
    pub fn move_left(&mut self) -> bool {
        let result = self.editor.move_left();
        if result {
            self.update_cursor_visual_position();
        }
        result
    }

    /// Move cursor right by one character
    pub fn move_right(&mut self) -> bool {
        let result = self.editor.move_right();
        if result {
            self.update_cursor_visual_position();
        }
        result
    }

    /// Move cursor left by one word
    pub fn move_word_left(&mut self) -> bool {
        let result = self.editor.move_word_left();
        if result {
            self.update_cursor_visual_position();
        }
        result
    }

    /// Move cursor right by one word
    pub fn move_word_right(&mut self) -> bool {
        let result = self.editor.move_word_right();
        if result {
            self.update_cursor_visual_position();
        }
        result
    }

    /// Find the closest pointer on a given line to a target column
    /// Uses content_column (without left padding) for comparison
    fn closest_pointer_on_line(&self, line: usize, column: u16) -> Option<CursorDisplay> {
        // Get positions for this line on-demand
        let positions_on_line = self.get_positions_for_line(line);

        if positions_on_line.is_empty() {
            return None;
        }

        // Find the minimum content_column distance (without left padding)
        let min_distance = positions_on_line
            .iter()
            .map(|entry| column_distance(entry.position.content_column, column))
            .min()
            .unwrap();

        // Get all positions with the minimum distance
        let closest_positions: Vec<_> = positions_on_line
            .iter()
            .filter(|entry| column_distance(entry.position.content_column, column) == min_distance)
            .collect();

        if closest_positions.len() == 1 {
            return Some((*closest_positions[0]).clone());
        }

        // Multiple positions at the same visual column (nested inline styles)
        // If desired column < closest position: choose outermost (shallowest nesting)
        // If desired column >= closest position: choose innermost (deepest nesting)
        let closest_column = closest_positions[0].position.content_column;

        if column < closest_column {
            // Choose outermost position (smallest span path length)
            closest_positions
                .iter()
                .min_by_key(|entry| entry.pointer.span_path.indices.len())
                .map(|entry| (**entry).clone())
        } else {
            // Choose innermost position (largest span path length)
            closest_positions
                .iter()
                .max_by_key(|entry| entry.pointer.span_path.indices.len())
                .map(|entry| (**entry).clone())
        }
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
            .layout
            .as_ref()
            .unwrap()
            .paragraph_lines
            .last()
            .map(|p| p.end_line)
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
        // Try exact line first
        if let Some(hit) = self.closest_pointer_on_line(line, column) {
            return Some(hit);
        }

        // Get max line from paragraph_lines (lightweight)
        let max_line = if let Some(last_para) = self.layout.as_ref().unwrap().paragraph_lines.last()
        {
            last_para.end_line
        } else {
            0
        };
        let mut distance = 1usize;
        while line.checked_sub(distance).is_some() || line + distance <= max_line {
            if let Some(prev) = line.checked_sub(distance)
                && let Some(hit) = self.closest_pointer_on_line(prev, column)
            {
                return Some(hit);
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
        let mut entries = self.get_positions_for_line(line);
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
            self.preferred_column = Some(display.position.content_column);
            self.cursor_following = true;
            self.update_cursor_visual_position();
        }
    }

    /// Focus on a specific pointer
    pub fn focus_pointer(&mut self, pointer: &CursorPointer) {
        if self.editor.move_to_pointer(pointer) {
            // Search for the visual position of this pointer in paragraph_lines
            let found = self
                .layout
                .as_ref()
                .unwrap()
                .paragraph_lines
                .iter()
                .find_map(|info| {
                    info.positions
                        .iter()
                        .find(|(p, _)| p == pointer)
                        .map(|(_, pos)| {
                            // Convert relative position to absolute
                            let mut absolute_pos = *pos;
                            absolute_pos.line = info.start_line + pos.line;
                            absolute_pos
                        })
                });

            if let Some(position) = found {
                self.preferred_column = Some(position.content_column);
            } else {
                self.preferred_column = None;
            }
            self.cursor_following = true;
            self.update_cursor_visual_position();
        }
    }

    /// Insert a character at the cursor position with incremental layout update
    pub fn insert_char(&mut self, c: char) -> bool {
        let para_index = self.editor.cursor_pointer().paragraph_path.root_index();
        let result = self.editor.insert_char(c);
        if let Some(index) = para_index {
            self.mark_paragraph_modified(index);
        }
        self.clear_render_cache();
        result
    }

    /// Delete character before cursor with incremental layout update
    pub fn delete_char(&mut self) -> bool {
        let para_index = self.editor.cursor_pointer().paragraph_path.root_index();
        let result = self.editor.delete();
        if let Some(index) = para_index {
            self.mark_paragraph_modified(index);
        }
        self.clear_render_cache();
        result
    }

    /// Backspace (delete character before cursor) with incremental layout update
    pub fn backspace(&mut self) -> bool {
        let para_count_before = self.editor.document().paragraphs.len();
        let para_index_before = self.editor.cursor_pointer().paragraph_path.root_index();

        let result = self.editor.backspace();
        let para_count_after = self.editor.document().paragraphs.len();
        let para_index_after = self.editor.cursor_pointer().paragraph_path.root_index();

        // If paragraph count changed or cursor moved to different paragraph, force full re-render
        // (paragraph merge/split/removal affects structure and cursor tracking)
        if para_count_before != para_count_after || para_index_before != para_index_after {
            self.force_full_relayout();
        } else if let Some(index) = para_index_after {
            self.mark_paragraph_modified(index);
        }
        self.clear_render_cache();
        result
    }

    /// Delete character at cursor with incremental layout update
    pub fn delete(&mut self) -> bool {
        let para_count_before = self.editor.document().paragraphs.len();
        let para_index = self.editor.cursor_pointer().paragraph_path.root_index();

        // Check if next paragraph is a quote or list that might be affected by merge
        let next_para_needs_update = if let Some(idx) = para_index {
            if idx + 1 < para_count_before {
                matches!(
                    self.editor.document().paragraphs.get(idx + 1),
                    Some(tdoc::Paragraph::Quote { .. })
                        | Some(tdoc::Paragraph::OrderedList { .. })
                        | Some(tdoc::Paragraph::UnorderedList { .. })
                        | Some(tdoc::Paragraph::Checklist { .. })
                )
            } else {
                false
            }
        } else {
            false
        };

        let result = self.editor.delete();
        let para_count_after = self.editor.document().paragraphs.len();

        // If paragraph count changed, force full re-render (paragraph merge/split/removal)
        if para_count_before != para_count_after {
            self.force_full_relayout();
        } else if next_para_needs_update {
            // Merging with quote/list children affects both paragraphs
            // Mark both for incremental update
            self.last_modified_paragraphs.clear();
            if let Some(idx) = para_index {
                self.mark_paragraph_modified(idx);
                if idx + 1 < para_count_after {
                    self.mark_paragraph_modified(idx + 1);
                }
            }
        } else if let Some(index) = para_index {
            self.mark_paragraph_modified(index);
        }
        self.clear_render_cache();
        result
    }

    /// Delete word backward with incremental layout update
    pub fn delete_word_backward(&mut self) -> bool {
        let para_count_before = self.editor.document().paragraphs.len();
        let para_index = self.editor.cursor_pointer().paragraph_path.root_index();
        let result = self.editor.delete_word_backward();
        let para_count_after = self.editor.document().paragraphs.len();

        // If paragraph count changed, force full re-render (paragraph merge/split)
        if para_count_before != para_count_after {
            self.force_full_relayout();
        } else if let Some(index) = para_index {
            self.mark_paragraph_modified(index);
        }
        self.clear_render_cache();
        result
    }

    /// Delete word forward with incremental layout update
    pub fn delete_word_forward(&mut self) -> bool {
        let para_count_before = self.editor.document().paragraphs.len();
        let para_index = self.editor.cursor_pointer().paragraph_path.root_index();
        let result = self.editor.delete_word_forward();
        let para_count_after = self.editor.document().paragraphs.len();

        // If paragraph count changed, force full re-render (paragraph merge/split)
        if para_count_before != para_count_after {
            self.force_full_relayout();
        } else if let Some(index) = para_index {
            self.mark_paragraph_modified(index);
        }
        self.clear_render_cache();
        result
    }

    /// Insert paragraph break with layout update (requires full re-render as it affects multiple paragraphs)
    pub fn insert_paragraph_break(&mut self) -> bool {
        let result = self.editor.insert_paragraph_break();
        // Paragraph breaks affect structure - need full re-render
        self.force_full_relayout();
        self.clear_render_cache();
        result
    }

    /// Set paragraph type with layout update
    pub fn set_paragraph_type(&mut self, target: tdoc::ParagraphType) -> bool {
        let para_index = self.editor.cursor_pointer().paragraph_path.root_index();
        let result = self.editor.set_paragraph_type(target);
        // Changing paragraph type affects structure and rendering
        if let Some(index) = para_index {
            self.mark_paragraph_modified(index);
        }
        self.clear_render_cache();
        result
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
    a.abs_diff(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::DocumentEditor;
    use tdoc::{Document, Paragraph, ftml};

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
        let _render = display.render_document_with_positions(80, 0, None);

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
        let _render = display.render_document_with_positions(80, 0, None);

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
        let _render = display.render_document_with_positions(80, 0, None);

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
        let _render = display.render_document_with_positions(80, 0, None);

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
        let _render = display.render_document_with_positions(80, 0, None);

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
        let _render = display.render_document_with_positions(80, 0, None);

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
        let _render = display.render_document_with_positions(80, 0, None);

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
    fn test_moving_into_empty_checklist_items() {
        let doc = ftml! {
            h1 { "My Document" }
            checklist {
                done { "Task 1" }
                todo { }
            }
        };
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Render to populate visual_positions
        let _ = display.render_document_with_positions(80, 0, None);

        // Try to navigate down to reach the checklist
        display.move_cursor_vertical(1);
        let pos1 = display.cursor_pointer();
        assert_eq!(
            pos1.paragraph_path.numeric_steps(),
            vec![1, 0],
            "Should be at 2nd checklist paragraph"
        );
        assert_eq!(pos1.offset, 0, "Should be at start of checklist paragraph");

        display.move_cursor_vertical(1);
        let pos2 = display.cursor_pointer();
        assert_eq!(
            pos2.paragraph_path.numeric_steps(),
            vec![1, 1],
            "Should be at checklist paragraph"
        );
        assert_eq!(pos2.offset, 0, "Should be at start of checklist paragraph");
    }

    #[test]
    fn test_editing_empty_checklist_item() {
        let doc = ftml! {
            h1 { "My Document" }
            checklist {
                done { "Task 1" }
                todo { }
            }
        };
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Render to populate visual_positions
        let _ = display.render_document_with_positions(80, 0, None);

        // Try to navigate down to reach the checklist
        display.move_cursor_vertical(1);
        let pos1 = display.cursor_pointer();
        assert_eq!(
            pos1.paragraph_path.numeric_steps(),
            vec![1, 0],
            "Should be at 1st item"
        );

        display.move_cursor_vertical(1);
        let pos2 = display.cursor_pointer();
        assert_eq!(
            pos2.paragraph_path.numeric_steps(),
            vec![1, 1],
            "Should be at 2nd item"
        );

        assert!(display.insert_char('T'));
        assert!(display.insert_char('e'));
        assert!(display.insert_char('s'));
        assert!(display.insert_char('t'));
        let pos3 = display.cursor_pointer();
        assert_eq!(pos3.offset, 4, "Should be at end of item's text");
    }

    #[test]
    fn test_moving_into_empty_bullet_items() {
        let doc = ftml! {
            h1 { "My Document" }
            ul {
                li { p { "Task 1" } }
                li { }
            }
        };
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Render to populate visual_positions
        let _ = display.render_document_with_positions(80, 0, None);

        // Try to navigate down to reach the checklist
        display.move_cursor_vertical(1);
        let pos1 = display.cursor_pointer();
        assert_eq!(
            pos1.paragraph_path.numeric_steps(),
            vec![1, 0, 0],
            "Should be at 1st bullet paragraph's first child"
        );
        assert_eq!(pos1.offset, 0, "Should be at start of bullet paragraph");

        display.move_cursor_vertical(1);
        let pos2 = display.cursor_pointer();
        assert_eq!(
            pos2.paragraph_path.numeric_steps(),
            vec![1, 1],
            "Should be at 2nd bullet paragraph"
        );
        assert_eq!(pos2.offset, 0, "Should be at start of bullet paragraph");
    }

    #[test]
    fn test_editing_empty_bullet_paragraph() {
        let doc = ftml! {
            h1 { "My Document" }
            ul {
                li { p { "Task 1" } }
                li { }
            }
        };
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Render to populate visual_positions
        let _ = display.render_document_with_positions(80, 0, None);

        // Try to navigate down to reach the checklist
        display.move_cursor_vertical(1);
        let pos1 = display.cursor_pointer();
        assert_eq!(
            pos1.paragraph_path.numeric_steps(),
            vec![1, 0, 0],
            "Should be at 1st bullet paragraph's first child"
        );

        display.move_cursor_vertical(1);
        let pos2 = display.cursor_pointer();
        assert_eq!(
            pos2.paragraph_path.numeric_steps(),
            vec![1, 1],
            "Should be at 2nd bullet paragraph"
        );

        assert!(display.insert_char('T'));
        assert!(display.insert_char('e'));
        assert!(display.insert_char('s'));
        assert!(display.insert_char('t'));
        let pos3 = display.cursor_pointer();
        assert_eq!(pos3.offset, 4, "Should be at end of bullet paragraph");
    }

    #[test]
    fn test_empty_doc_has_cursor() {
        let doc = ftml! { p {} };
        let mut editor = DocumentEditor::new(doc);
        editor.ensure_cursor_selectable();
        let mut display = EditorDisplay::new(editor);
        let _ = display.render_document_with_positions(80, 0, None);
        let pos1 = display.cursor_pointer();
        assert_eq!(pos1.paragraph_path.numeric_steps(), vec![0]);
        // After ensure_cursor_selectable(), empty paragraphs get a placeholder span at index 0
        assert_eq!(pos1.span_path.indices(), vec![0]);
        assert_eq!(pos1.offset, 0);

        if let Some(vis1) = display.cursor_visual() {
            assert_eq!(vis1.line, 0);
            assert_eq!(vis1.column, 0);
        } else {
            panic!("No visible cursor")
        }
    }

    impl EditorDisplay {
        fn insert_text(&mut self, txt: &str) -> bool {
            for i in txt.chars() {
                if !self.insert_char(i) {
                    return false;
                }
            }

            true
        }

        fn get_pos(&mut self) -> Option<(usize, u16)> {
            let _ = self.render_document_with_positions(80, 0, None);

            self.cursor_visual().map(|v| (v.line, v.column))
        }

        fn get_content_pos(&mut self) -> Option<(usize, u16)> {
            let _ = self.render_document(80, 0, None);

            self.cursor_visual()
                .map(|v| (v.content_line, v.content_column))
        }

        fn get_txt(&mut self) -> String {
            self.render_document(80, 0, None);
            let mut s = String::new();
            for l in &self.layout.as_ref().unwrap().lines {
                for i in &l.spans {
                    s.push_str(&i.content);
                }
                s.push('\n');
            }
            s
        }
    }

    #[test]
    fn test_adding_two_checklist_items() {
        let doc = ftml! { p {} };
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));
        let _ = display.render_document_with_positions(80, 0, None);

        assert!(
            display.set_paragraph_type(tdoc::ParagraphType::Checklist),
            "Unable to set paragraph type"
        );
        assert!(
            display.insert_text("Test 123"),
            "unable to insert text in 1st paragraph"
        );
        assert_eq!(display.get_txt(), "[ ] Test 123\n");
        assert_eq!(display.get_pos(), Some((0, 12)));

        assert!(
            display.insert_paragraph_break(),
            "unable to insert paragraph break"
        );
        assert_eq!(display.get_txt(), "[ ] Test 123\n\n[ ] \n");
        assert_eq!(display.get_pos(), Some((2, 4)));

        assert!(
            display.insert_text("Test ABC"),
            "unable to insert text in 2nd paragraph"
        );
        assert_eq!(display.get_txt(), "[ ] Test 123\n\n[ ] Test ABC\n");
        assert_eq!(display.get_pos(), Some((2, 12)));
    }

    #[test]
    fn move_down_from_h2_to_checklist() {
        use crate::editor::{ParagraphPath, SegmentKind, SpanPath};
        use tdoc::parse;
        let content = std::fs::read_to_string("test.ftml").unwrap();
        let doc = parse(std::io::Cursor::new(content.clone())).unwrap();
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Render to populate visual_positions
        let _ = display.render_document_with_positions(80, 0, None);

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

    #[test]
    fn test_initial_cursor_navigation_in_test_ftml() {
        use tdoc::parse;

        let content = std::fs::read_to_string("test.ftml").unwrap();
        let doc = parse(std::io::Cursor::new(content)).unwrap();
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Render to populate visual_positions
        let _ = display.render_document_with_positions(80, 0, None);

        // Check initial position
        let _initial = display.cursor_pointer();

        // Try to navigate down to reach the checklist
        display.move_cursor_vertical(1);
        let _pos1 = display.cursor_pointer();

        display.move_cursor_vertical(1);
        let _pos2 = display.cursor_pointer();

        display.move_cursor_vertical(1);
        let pos3 = display.cursor_pointer();

        // Check if we reached a checklist item by looking at the debug representation
        let path_str = format!("{:?}", pos3.paragraph_path);
        let has_checklist = path_str.contains("ChecklistItem");

        assert!(
            has_checklist,
            "Should reach a checklist item after 3 down presses from initial position"
        );
    }

    #[test]
    fn regression_fallback_when_destination_equals_current() {
        // Regression test for bug where cursor couldn't move down when visual search
        // returned the same position (e.g., when at max_line with target beyond viewport)
        //
        // Bug scenario: User at H2 "Todos" presses down, but checklist items are beyond
        // visual_positions coverage. Visual search finds H2 again (same position), and
        // without fallback to logical movement, cursor stays stuck.
        //
        // This test verifies the fallback logic works: when destination == current position,
        // use logical cursor movement instead.
        use crate::editor::{ParagraphPath, SegmentKind, SpanPath};
        use tdoc::{ChecklistItem, Document, Paragraph, Span as DocSpan};

        let doc = Document::new().with_paragraphs(vec![
            Paragraph::new_header2().with_content(vec![DocSpan::new_text("Heading")]),
            Paragraph::new_checklist().with_checklist_items(vec![
                ChecklistItem::new(false).with_content(vec![DocSpan::new_text("Task")]),
            ]),
        ]);

        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Move to H2 without rendering (empty visual_positions)
        // This forces fallback to logical movement
        let h2_pointer = CursorPointer {
            paragraph_path: ParagraphPath::new_root(0),
            span_path: SpanPath::new(vec![0]),
            offset: 0,
            segment_kind: SegmentKind::Text,
        };
        assert!(display.move_to_pointer(&h2_pointer));

        // Try to move down with empty visual_positions
        // Should use fallback to move_down()
        display.move_cursor_vertical(1);

        let after = display.cursor_pointer();

        // Should have moved to checklist using logical movement
        let path_str = format!("{:?}", after.paragraph_path);
        assert!(
            path_str.contains("ChecklistItem"),
            "Should have used logical fallback to reach checklist, got: {:?}",
            after.paragraph_path
        );
    }

    #[test]
    fn fallback_to_logical_movement_when_visual_positions_incomplete() {
        use crate::editor::{ParagraphPath, SegmentKind, SpanPath};
        use tdoc::{ChecklistItem, Document, Paragraph, Span as DocSpan};

        // Create a document with heading and checklist
        let doc = Document::new().with_paragraphs(vec![
            Paragraph::new_header2().with_content(vec![DocSpan::new_text("Heading")]),
            Paragraph::new_checklist().with_checklist_items(vec![
                ChecklistItem::new(false).with_content(vec![DocSpan::new_text("Item 1")]),
            ]),
        ]);

        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Move to the heading
        let h2_pointer = CursorPointer {
            paragraph_path: ParagraphPath::new_root(0),
            span_path: SpanPath::new(vec![0]),
            offset: 0,
            segment_kind: SegmentKind::Text,
        };
        assert!(display.move_to_pointer(&h2_pointer));

        // Try to move down WITHOUT rendering (visual_positions will be empty)
        // This should fall back to logical cursor movement
        display.move_cursor_vertical(1);

        let cursor_after_move = display.cursor_pointer();
        let mut expected_path = ParagraphPath::new_root(1);
        expected_path.push_checklist_item(vec![0]);

        assert_eq!(
            cursor_after_move.paragraph_path, expected_path,
            "Cursor should have used logical movement fallback to reach checklist item"
        );
    }

    #[test]
    fn vertical_movement_from_text_to_quote_with_earlier_column() {
        let doc = ftml! {
            p { "Regular text here" }
            quote { p { b { "Note:" } " Quote text here" } }
        };
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));
        display.editor.ensure_cursor_selectable();

        assert_eq!(display.get_content_pos(), Some((0, 0)));
        assert_eq!(display.get_pos(), Some((0, 0)));

        // Move down - target line will be 1 (blank line), should skip to line 2
        display.move_cursor_vertical(1);
        assert_eq!(display.get_content_pos(), Some((2, 0)));
        assert_eq!(display.get_pos(), Some((2, 2)));
    }

    #[test]
    fn vertical_movement_into_nested_inline_styles_is_consistent() {
        use tdoc::{Document, InlineStyle, Paragraph, Span as DocSpan};

        // Create a document with nested inline styles
        // Line 1: "Plain text on first line."
        // Line 2: "Text with **_nested_** styles here."
        // Line 3: "Another plain line below."
        let italic_span = DocSpan::new_styled(InlineStyle::Italic).with_text("nested");
        let bold_span = DocSpan::new_styled(InlineStyle::Bold).with_children(vec![italic_span]);

        let para1 = Paragraph::new_text()
            .with_content(vec![DocSpan::new_text("Plain text on first line.")]);
        let para2 = Paragraph::new_text().with_content(vec![
            DocSpan::new_text("Text with "),
            bold_span,
            DocSpan::new_text(" styles here."),
        ]);
        let para3 = Paragraph::new_text()
            .with_content(vec![DocSpan::new_text("Another plain line below.")]);

        let doc = Document::new().with_paragraphs(vec![para1, para2, para3]);
        let mut display = EditorDisplay::new(DocumentEditor::new(doc));

        // Render to populate paragraph_lines with positions
        let _ = display.render_document_with_positions(80, 0, None);

        // Start from paragraph 1 (line 0), move to the 'T' in "Text"
        for _ in 0..10 {
            display.editor.move_right();
        }

        // Move down to line 2 - this should land at the same column in "Text with ..."
        display.move_cursor_vertical(1);
        let after_first_move = display.editor.cursor_pointer();

        // Move down to line 3, then back up to line 2
        display.move_cursor_vertical(1);
        display.move_cursor_vertical(-1);
        let after_second_move = display.editor.cursor_pointer();

        // The cursor should land at the same position both times
        assert_eq!(
            after_first_move, after_second_move,
            "Cursor should land at the same position when moving vertically to a line with nested styles. \
            First move: {:?}, Second move: {:?}",
            after_first_move, after_second_move
        );
    }

    #[test]
    fn backspace_from_beginning_merges_with_previous_paragraph() {
        let doc = ftml! {
            p { "First paragraph" }
            p { "Next paragraph" }
        };

        let mut editor = DocumentEditor::new(doc);
        editor.ensure_cursor_selectable();
        let mut display = EditorDisplay::new(editor);

        display.move_cursor_vertical(1);
        assert_eq!(display.get_content_pos(), Some((2, 0)));

        assert!(
            display.backspace(),
            "Backspace should successfully merge paragraphs"
        );

        assert_eq!(display.get_txt(), "First paragraphNext paragraph\n");
        assert_eq!(display.get_content_pos(), Some((0, 15)));
    }

    #[test]
    fn backspace_from_beginning_of_multi_entry_list_merges_with_previous_paragraph() {
        let doc = ftml! {
            p { "First paragraph" }
            ul {
                li { p { "Next paragraph" } }
                li { p { "Another paragraph" } }
            }
        };

        let mut editor = DocumentEditor::new(doc);
        editor.ensure_cursor_selectable();
        let mut display = EditorDisplay::new(editor);

        display.move_cursor_vertical(1);
        assert_eq!(display.get_content_pos(), Some((2, 0)));

        assert!(
            display.backspace(),
            "Backspace should successfully merge paragraphs"
        );

        assert_eq!(
            display.get_txt(),
            "First paragraphNext paragraph\n\n• Another paragraph\n"
        );
        assert_eq!(display.get_content_pos(), Some((0, 15)));
    }

    #[test]
    fn backspace_from_beginning_of_list_merges_with_previous_paragraph() {
        let doc = ftml! {
            p { "First paragraph" }
            ul { li { p { "Next paragraph" } } }
        };

        let mut editor = DocumentEditor::new(doc);
        editor.ensure_cursor_selectable();
        let mut display = EditorDisplay::new(editor);

        display.move_cursor_vertical(1);
        assert_eq!(display.get_content_pos(), Some((2, 0)));

        assert!(
            display.backspace(),
            "Backspace should successfully merge paragraphs"
        );

        assert_eq!(display.get_txt(), "First paragraphNext paragraph\n");
        assert_eq!(display.get_content_pos(), Some((0, 15)));
    }

    #[test]
    fn backspace_from_beginning_merges_with_empty_paragraph() {
        let doc = ftml! {
            p { }
            p { "Next paragraph" }
        };

        let mut editor = DocumentEditor::new(doc);
        editor.ensure_cursor_selectable();
        let mut display = EditorDisplay::new(editor);

        display.move_cursor_vertical(1);
        assert_eq!(display.get_content_pos(), Some((2, 0)));

        // Press Backspace - should remove empty paragraph and stay at beginning
        assert!(
            display.backspace(),
            "Backspace should successfully remove empty paragraph"
        );

        assert_eq!(display.get_txt(), "Next paragraph\n");
        assert_eq!(display.get_content_pos(), Some((0, 0)));
    }

    #[test]
    fn test_breaking_at_the_beginning_of_bold_text_works() {
        let doc = ftml! {
            p {
                "First paragraph"
                b { "This"} " will become the second paragraph"
            }
        };
        let mut editor = DocumentEditor::new(doc);
        editor.ensure_cursor_selectable();
        let mut display = EditorDisplay::new(editor);

        // Length of "First paragraph"
        for _ in 0..15 {
            display.editor.move_right();
        }

        assert_eq!(
            display.get_txt(),
            "First paragraphThis will become the second paragraph\n"
        );
        assert_eq!(display.get_content_pos(), Some((0, 15)));

        display.insert_paragraph_break();
        assert_eq!(
            display.get_txt(),
            "First paragraph\n\nThis will become the second paragraph\n"
        );
        assert_eq!(display.get_content_pos(), Some((2, 0)));
    }

    #[test]
    fn test_delete_joins_two_text_paragraphs() {
        let doc = ftml! {
            p { "First paragraph" }
            p { "Second paragraph" }
        };
        let mut editor = DocumentEditor::new(doc);
        editor.ensure_cursor_selectable();
        let mut display = EditorDisplay::new(editor);

        // Move to the end of the first paragraph
        for _ in 0..15 {
            // Length of "First paragraph"
            display.editor.move_right();
        }

        // Clear layout_dirty to test if delete sets it
        display.layout_dirty = false;
        display.last_modified_paragraphs.clear();

        eprintln!("Before delete:");
        eprintln!(
            "  Paragraph count: {}",
            display.editor.document().paragraphs.len()
        );
        eprintln!("  Cursor: {:?}", display.editor.cursor_pointer());
        eprintln!("  layout_dirty: {}", display.layout_dirty);

        // Delete should merge the paragraphs
        let result = display.delete();
        assert!(result, "Delete should successfully merge paragraphs");

        eprintln!("\nAfter delete:");
        eprintln!(
            "  Paragraph count: {}",
            display.editor.document().paragraphs.len()
        );
        eprintln!("  Cursor: {:?}", display.editor.cursor_pointer());
        eprintln!("  layout_dirty: {}", display.layout_dirty);
        eprintln!(
            "  last_modified_paragraphs: {:?}",
            display.last_modified_paragraphs
        );

        assert_eq!(
            display.editor.document().paragraphs.len(),
            1,
            "Should have 1 paragraph after merge"
        );
        assert!(
            display.layout_dirty,
            "layout_dirty should be true after paragraph merge"
        );
    }
}
