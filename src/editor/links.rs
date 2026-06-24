//! Locating and editing hyperlink spans.
//!
//! A hyperlink is a [`Span`] with [`InlineStyle::Link`] and a `link_target`
//! URL. [`DocumentEditor::link_at_cursor`] reports the link enclosing the
//! cursor (its visible text, target, and the position range it spans) so the
//! UI can pre-fill an edit dialog. [`DocumentEditor::set_link`] writes the
//! dialog back: it replaces a position range with a single link span, or with
//! plain text when the target is cleared (unlinking).

use super::content::{prune_and_merge_spans, replace_range_with_link};
use super::inspect::{checklist_item_ref, paragraph_ref};
use super::{
    CursorPointer, DocumentEditor, ParagraphPath, SegmentKind, checklist_item_mut, paragraph_mut,
};
use std::cmp::Ordering;
use tdoc::{InlineStyle, Span};

/// The hyperlink enclosing the cursor: its visible text, optional target, and
/// the leaf-position range it covers (suitable for [`DocumentEditor::set_link`]).
pub struct LinkAtCursor {
    pub text: String,
    pub target: Option<String>,
    pub range: (CursorPointer, CursorPointer),
}

impl DocumentEditor {
    /// Returns the hyperlink span enclosing the cursor, if any. The cursor may
    /// sit inside a style nested within the link; the outermost enclosing link
    /// is reported so editing affects the whole visible link.
    pub fn link_at_cursor(&self) -> Option<LinkAtCursor> {
        let pointer = self.cursor_pointer();
        let spans: &[Span] =
            if let Some(item) = checklist_item_ref(&self.document, &pointer.paragraph_path) {
                &item.content
            } else if let Some(paragraph) = paragraph_ref(&self.document, &pointer.paragraph_path) {
                paragraph.content()
            } else {
                return None;
            };

        // Walk the cursor's span path from the root, stopping at the first
        // (outermost) link ancestor — including the leaf span itself.
        let indices = pointer.span_path.indices();
        let mut current = spans;
        let mut link_prefix_len = None;
        for (depth, &idx) in indices.iter().enumerate() {
            let span = current.get(idx)?;
            if span.style == InlineStyle::Link {
                link_prefix_len = Some(depth + 1);
                break;
            }
            current = &span.children;
        }
        let link_path = &indices[..link_prefix_len?];

        // Resolve the link span to read its text and target.
        let mut link_span = spans.get(*link_path.first()?)?;
        for &idx in &link_path[1..] {
            link_span = link_span.children.get(idx)?;
        }
        let mut text = String::new();
        collect_visible_text(link_span, &mut text);
        let target = link_span.link_target.clone();

        // The link covers a contiguous run of text segments whose span path
        // descends from the link span. Its start and end give the range.
        let mut start = None;
        let mut end = None;
        for segment in &self.segments {
            if segment.kind != SegmentKind::Text
                || segment.paragraph_path != pointer.paragraph_path
                || !starts_with(segment.span_path.indices(), link_path)
            {
                continue;
            }
            if start.is_none() {
                start = Some(CursorPointer {
                    paragraph_path: segment.paragraph_path.clone(),
                    span_path: segment.span_path.clone(),
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                });
            }
            end = Some(CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: segment.len,
                segment_kind: SegmentKind::Text,
            });
        }

        Some(LinkAtCursor {
            text,
            target,
            range: (start?, end?),
        })
    }

    /// Replaces a position range within one content root with a hyperlink
    /// whose visible text is `text` and whose URL is `target`. A `None` target
    /// inserts the text unlinked instead (so clearing a link's URL removes the
    /// link). An empty range inserts at that point without deleting anything.
    /// The cursor is left at the end of the inserted text.
    pub fn set_link(
        &mut self,
        range: &(CursorPointer, CursorPointer),
        text: &str,
        target: Option<&str>,
    ) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        if text.is_empty() && target.is_none() {
            return false;
        }

        let mut start = range.0.clone();
        let mut end = range.1.clone();
        match self.compare_pointers(&start, &end) {
            Some(Ordering::Greater) => std::mem::swap(&mut start, &mut end),
            Some(_) => {}
            None => return false,
        }
        // Hyperlinks live within a single paragraph or checklist item.
        if start.paragraph_path != end.paragraph_path {
            return false;
        }
        let path = start.paragraph_path.clone();

        // Read-only paragraphs (tables) cannot have their content rewritten.
        if self.is_readonly_paragraph(&path) {
            return false;
        }

        // The character offset of the range start within its content root is
        // stable across the edit (text before it is untouched); remember it to
        // place the cursor at the end of the new link afterwards.
        let start_char = self.paragraph_char_offset_of_pointer(&start).unwrap_or(0);

        let changed = if let Some(item) = checklist_item_mut(&mut self.document, &path) {
            let changed = replace_range_with_link(
                &mut item.content,
                start.span_path.indices(),
                start.offset,
                end.span_path.indices(),
                end.offset,
                text,
                target,
            );
            if changed {
                prune_and_merge_spans(&mut item.content);
            }
            changed
        } else if let Some(paragraph) = paragraph_mut(&mut self.document, &path) {
            let changed = replace_range_with_link(
                paragraph.content_mut(),
                start.span_path.indices(),
                start.offset,
                end.span_path.indices(),
                end.offset,
                text,
                target,
            );
            if changed {
                prune_and_merge_spans(paragraph.content_mut());
            }
            changed
        } else {
            false
        };

        if changed {
            if let Some(first_step) = path.steps().first() {
                let root = ParagraphPath::from_steps(vec![first_step.clone()]);
                self.update_segments_for_paragraph(&root);
            } else {
                self.rebuild_segments();
            }
            let new_offset = start_char + text.chars().count();
            self.move_to_paragraph_char_offset(&path, new_offset);
        }

        changed
    }
}

fn collect_visible_text(span: &Span, buffer: &mut String) {
    buffer.push_str(&span.text);
    for child in &span.children {
        collect_visible_text(child, buffer);
    }
}

fn starts_with(haystack: &[usize], prefix: &[usize]) -> bool {
    haystack.len() >= prefix.len() && haystack[..prefix.len()] == *prefix
}
