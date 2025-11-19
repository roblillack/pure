use super::content::{apply_style_to_span_path, prune_and_merge_spans};
use super::{
    CursorPointer, DocumentEditor, ParagraphPath, SegmentKind, SegmentRef, checklist_item_mut,
    paragraph_mut,
};
use tdoc::InlineStyle;

enum InlineStyleScope {
    None,
    Paragraph,
    Checklist,
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

        let segments_snapshot = self.segments.clone();
        let mut changed = false;
        let mut touched_paragraphs: Vec<ParagraphPath> = Vec::new();
        let mut touched_checklists: Vec<ParagraphPath> = Vec::new();

        for segment_index in (start_key.segment_index..=end_key.segment_index).rev() {
            let Some(segment) = segments_snapshot.get(segment_index) else {
                continue;
            };
            let len = segment.len;
            if len == 0 || segment.kind != SegmentKind::Text {
                continue;
            }

            let seg_start = if segment_index == start_key.segment_index {
                start_key.offset.min(len)
            } else {
                0
            };
            let seg_end = if segment_index == end_key.segment_index {
                end_key.offset.min(len)
            } else {
                len
            };

            if seg_start >= seg_end {
                continue;
            }

            match self.apply_inline_style_to_segment(segment, seg_start, seg_end, style) {
                InlineStyleScope::None => {}
                InlineStyleScope::Paragraph => {
                    changed = true;
                    if !touched_paragraphs
                        .iter()
                        .any(|path| *path == segment.paragraph_path)
                    {
                        touched_paragraphs.push(segment.paragraph_path.clone());
                    }
                }
                InlineStyleScope::Checklist => {
                    changed = true;
                    if !touched_checklists
                        .iter()
                        .any(|path| *path == segment.paragraph_path)
                    {
                        touched_checklists.push(segment.paragraph_path.clone());
                    }
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
        }

        changed
    }

    fn apply_inline_style_to_segment(
        &mut self,
        segment: &SegmentRef,
        start: usize,
        end: usize,
        style: InlineStyle,
    ) -> InlineStyleScope {
        if let Some(item) = checklist_item_mut(&mut self.document, &segment.paragraph_path) {
            if apply_style_to_span_path(
                &mut item.content,
                segment.span_path.indices(),
                start,
                end,
                style,
            ) {
                return InlineStyleScope::Checklist;
            }
            return InlineStyleScope::None;
        }

        let Some(paragraph) = paragraph_mut(&mut self.document, &segment.paragraph_path) else {
            return InlineStyleScope::None;
        };
        if apply_style_to_span_path(
            paragraph.content_mut(),
            segment.span_path.indices(),
            start,
            end,
            style,
        ) {
            InlineStyleScope::Paragraph
        } else {
            InlineStyleScope::None
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
