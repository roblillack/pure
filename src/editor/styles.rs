use super::content::{apply_style_to_content_range, prune_and_merge_spans};
use super::inspect::{checklist_item_ref, paragraph_ref};
use super::{
    CursorPointer, DocumentEditor, ParagraphPath, SegmentKind, SegmentRef, SpanPath,
    checklist_item_mut, paragraph_mut,
};
use tdoc::{InlineStyle, Span};

/// The portion of one content root (paragraph or checklist item) covered by
/// a selection, delimited by leaf span positions.
struct StyledRange {
    paragraph_path: ParagraphPath,
    is_checklist_item: bool,
    start: (SpanPath, usize),
    end: (SpanPath, usize),
}

impl DocumentEditor {
    pub fn apply_inline_style_to_selection(
        &mut self,
        selection: &(CursorPointer, CursorPointer),
        style: InlineStyle,
    ) -> bool {
        if self.segments.is_empty() {
            return false;
        }

        let mut start = selection.0.clone();
        let mut end = selection.1.clone();

        if matches!(
            self.compare_pointers(&start, &end),
            Some(std::cmp::Ordering::Greater)
        ) {
            std::mem::swap(&mut start, &mut end);
        }

        let start_key = match self.pointer_key(&start) {
            Some(key) => key,
            None => return false,
        };

        let end_key = match self.pointer_key(&end) {
            Some(key) => key,
            None => return false,
        };

        if start_key > end_key {
            return false;
        }

        // Refuse to restyle a selection that touches a read-only paragraph (a
        // table): its content is immutable.
        for segment_index in start_key.segment_index..=end_key.segment_index {
            if let Some(segment) = self.segments.get(segment_index)
                && self.is_readonly_paragraph(&segment.paragraph_path)
            {
                return false;
            }
        }

        // Collect the selected text ranges grouped by content root. While at
        // it, detect when the whole selection already carries the requested
        // style (or is plain already, when clearing) so re-applying it does
        // not pile up redundant spans.
        let mut ranges: Vec<StyledRange> = Vec::new();
        let mut already_styled = true;
        for segment_index in start_key.segment_index..=end_key.segment_index {
            let Some(segment) = self.segments.get(segment_index) else {
                continue;
            };
            if segment.kind != SegmentKind::Text {
                continue;
            }
            let seg_start = if segment_index == start_key.segment_index {
                start_key.offset.min(segment.len)
            } else {
                0
            };
            let seg_end = if segment_index == end_key.segment_index {
                end_key.offset.min(segment.len)
            } else {
                segment.len
            };
            if seg_start >= seg_end {
                continue;
            }

            if !self.segment_already_styled(segment, style) {
                already_styled = false;
            }

            match ranges.last_mut() {
                Some(range) if range.paragraph_path == segment.paragraph_path => {
                    range.end = (segment.span_path.clone(), seg_end);
                }
                _ => ranges.push(StyledRange {
                    paragraph_path: segment.paragraph_path.clone(),
                    is_checklist_item: checklist_item_ref(&self.document, &segment.paragraph_path)
                        .is_some(),
                    start: (segment.span_path.clone(), seg_start),
                    end: (segment.span_path.clone(), seg_end),
                }),
            }
        }

        if ranges.is_empty() || already_styled {
            return false;
        }

        // Applying a style splits and merges spans, invalidating span paths.
        // Remember the cursor as a character offset so it can be restored at
        // the same document position afterwards.
        let cursor_position = self
            .paragraph_char_offset_of_pointer(&self.cursor)
            .map(|char_offset| (self.cursor.paragraph_path.clone(), char_offset));

        let mut changed = false;
        let mut touched_paragraphs: Vec<ParagraphPath> = Vec::new();
        let mut touched_checklists: Vec<ParagraphPath> = Vec::new();

        for range in ranges.iter().rev() {
            if range.is_checklist_item {
                let Some(item) = checklist_item_mut(&mut self.document, &range.paragraph_path)
                else {
                    continue;
                };
                if apply_style_to_content_range(
                    &mut item.content,
                    range.start.0.indices(),
                    range.start.1,
                    range.end.0.indices(),
                    range.end.1,
                    style,
                ) {
                    changed = true;
                    touched_checklists.push(range.paragraph_path.clone());
                }
            } else {
                let Some(paragraph) = paragraph_mut(&mut self.document, &range.paragraph_path)
                else {
                    continue;
                };
                if apply_style_to_content_range(
                    paragraph.content_mut(),
                    range.start.0.indices(),
                    range.start.1,
                    range.end.0.indices(),
                    range.end.1,
                    style,
                ) {
                    changed = true;
                    touched_paragraphs.push(range.paragraph_path.clone());
                }
            }
        }

        if changed {
            // Collect all unique root paths that need updating
            let mut unique_paths = Vec::new();

            for path in touched_paragraphs {
                if let Some(paragraph) = paragraph_mut(&mut self.document, &path) {
                    prune_and_merge_spans(paragraph.content_mut());
                }
                // Get the root path for this paragraph (first step in path)
                if let Some(first_step) = path.steps().first() {
                    let root_path = super::ParagraphPath::from_steps(vec![first_step.clone()]);
                    if !unique_paths.iter().any(|p| p == &root_path) {
                        unique_paths.push(root_path);
                    }
                }
            }
            for path in touched_checklists {
                if let Some(item) = checklist_item_mut(&mut self.document, &path) {
                    prune_and_merge_spans(&mut item.content);
                }
                // Get the root path for this checklist (first step in path)
                if let Some(first_step) = path.steps().first() {
                    let root_path = super::ParagraphPath::from_steps(vec![first_step.clone()]);
                    if !unique_paths.iter().any(|p| p == &root_path) {
                        unique_paths.push(root_path);
                    }
                }
            }

            // Incrementally update only the affected root paragraphs
            if unique_paths.len() == 1 {
                // Single paragraph affected: use incremental update
                self.update_segments_for_paragraph(&unique_paths[0]);
            } else if unique_paths.len() > 1 {
                // Multiple root paragraphs affected: fall back to full rebuild for simplicity
                // (Could be optimized further to update each root incrementally)
                self.rebuild_segments();
            }

            if let Some((paragraph_path, char_offset)) = cursor_position {
                self.move_to_paragraph_char_offset(&paragraph_path, char_offset);
            }
        }

        changed
    }

    /// Returns whether the segment's chain of span styles — its leaf and all
    /// ancestors — already provides `style`. For `InlineStyle::None` this
    /// checks that the chain is entirely unstyled (nothing to clear).
    fn segment_already_styled(&self, segment: &SegmentRef, style: InlineStyle) -> bool {
        let spans: &[Span] =
            if let Some(item) = checklist_item_ref(&self.document, &segment.paragraph_path) {
                &item.content
            } else if let Some(paragraph) = paragraph_ref(&self.document, &segment.paragraph_path) {
                paragraph.content()
            } else {
                return false;
            };

        let mut current = spans;
        let mut chain_styles = Vec::new();
        for &idx in segment.span_path.indices() {
            let Some(span) = current.get(idx) else {
                return false;
            };
            chain_styles.push(span.style);
            current = &span.children;
        }

        if style == InlineStyle::None {
            chain_styles
                .iter()
                .all(|&chain_style| chain_style == InlineStyle::None)
        } else {
            chain_styles.contains(&style)
        }
    }
}

pub(crate) fn inline_style_label(style: InlineStyle) -> Option<&'static str> {
    match style {
        InlineStyle::None => None,
        InlineStyle::Bold => Some("Bold"),
        InlineStyle::Italic => Some("Italic"),
        InlineStyle::Highlight => Some("Highlight"),
        InlineStyle::Underline => Some("Underline"),
        InlineStyle::Strike => Some("Strikethrough"),
        InlineStyle::Link => Some("Link"),
        InlineStyle::Code => Some("Code"),
    }
}
