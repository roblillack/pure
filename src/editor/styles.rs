use super::content::{apply_style_to_span_path, prune_and_merge_spans};
use super::{
    paragraph_mut,
    CursorPointer,
    DocumentEditor,
    ParagraphPath,
    SegmentKind,
    SegmentRef,
};
use tdoc::InlineStyle;

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

        if matches!(self.compare_pointers(&start, &end), Some(std::cmp::Ordering::Greater)) {
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
        let mut touched_paths: Vec<ParagraphPath> = Vec::new();

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

            if self.apply_inline_style_to_segment(segment, seg_start, seg_end, style) {
                changed = true;
                if !touched_paths
                    .iter()
                    .any(|path| *path == segment.paragraph_path)
                {
                    touched_paths.push(segment.paragraph_path.clone());
                }
            }
        }

        if changed {
            for path in touched_paths {
                if let Some(paragraph) = paragraph_mut(&mut self.document, &path) {
                    prune_and_merge_spans(paragraph.content_mut());
                }
            }
            self.rebuild_segments();
        }

        changed
    }

    fn apply_inline_style_to_segment(
        &mut self,
        segment: &SegmentRef,
        start: usize,
        end: usize,
        style: InlineStyle,
    ) -> bool {
        let Some(paragraph) = paragraph_mut(&mut self.document, &segment.paragraph_path) else {
            return false;
        };
        apply_style_to_span_path(
            paragraph.content_mut(),
            segment.span_path.indices(),
            start,
            end,
            style,
        )
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
