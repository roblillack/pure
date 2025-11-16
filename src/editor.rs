const MARKER_POINTER_PREFIX: &str = "1337;M";
const MARKER_REVEAL_PREFIX: &str = "1337;R";
use std::{cmp::Ordering, mem};
use tdoc::{ChecklistItem, Document, InlineStyle, Paragraph, ParagraphType, Span};

use content::{
    apply_style_to_span_path,
    checklist_item_is_empty,
    insert_char_at,
    prune_and_merge_spans,
    remove_char_at,
    span_is_empty as content_span_is_empty,
    split_spans,
};

mod inspect;
mod cursor;
mod content;

use inspect::{checklist_item_ref, paragraph_ref};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphPath {
    steps: Vec<PathStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PathStep {
    Root(usize),
    Child(usize),
    Entry {
        entry_index: usize,
        paragraph_index: usize,
    },
    ChecklistItem {
        indices: Vec<usize>,
    },
}

impl ParagraphPath {
    fn new_root(idx: usize) -> Self {
        Self {
            steps: vec![PathStep::Root(idx)],
        }
    }

    fn from_steps(steps: Vec<PathStep>) -> Self {
        Self { steps }
    }

    fn push_child(&mut self, idx: usize) {
        self.steps.push(PathStep::Child(idx));
    }

    fn push_entry(&mut self, entry_index: usize, paragraph_index: usize) {
        self.steps.push(PathStep::Entry {
            entry_index,
            paragraph_index,
        });
    }

    fn push_checklist_item(&mut self, indices: Vec<usize>) {
        self.steps.push(PathStep::ChecklistItem { indices });
    }

    fn pop(&mut self) {
        if self.steps.len() > 1 {
            self.steps.pop();
        }
    }

    fn steps(&self) -> &[PathStep] {
        &self.steps
    }

    fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl Default for ParagraphPath {
    fn default() -> Self {
        Self { steps: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpanPath {
    indices: Vec<usize>,
}

impl SpanPath {
    fn new(indices: Vec<usize>) -> Self {
        Self { indices }
    }

    fn push(&mut self, idx: usize) {
        self.indices.push(idx);
    }

    fn pop(&mut self) {
        self.indices.pop();
    }

    fn indices(&self) -> &[usize] {
        &self.indices
    }

    fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

impl Default for SpanPath {
    fn default() -> Self {
        Self {
            indices: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorPointer {
    pub paragraph_path: ParagraphPath,
    pub span_path: SpanPath,
    pub offset: usize,
    pub segment_kind: SegmentKind,
}

impl CursorPointer {
    fn update_from_segment(&mut self, segment: &SegmentRef) {
        self.paragraph_path = segment.paragraph_path.clone();
        self.span_path = segment.span_path.clone();
        self.segment_kind = segment.kind;
    }

    fn is_valid(&self) -> bool {
        !self.paragraph_path.is_empty() && !self.span_path.is_empty()
    }
}

impl Default for CursorPointer {
    fn default() -> Self {
        Self {
            paragraph_path: ParagraphPath::default(),
            span_path: SpanPath::default(),
            offset: 0,
            segment_kind: SegmentKind::Text,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SegmentRef {
    pub paragraph_path: ParagraphPath,
    pub span_path: SpanPath,
    pub len: usize,
    pub kind: SegmentKind,
}

impl SegmentRef {
    fn matches(&self, cursor: &CursorPointer) -> bool {
        self.paragraph_path == cursor.paragraph_path
            && self.span_path == cursor.span_path
            && self.kind == cursor.segment_kind
    }

    fn matches_pointer(&self, pointer: &CursorPointer) -> bool {
        self.paragraph_path == pointer.paragraph_path
            && self.span_path == pointer.span_path
            && self.kind == pointer.segment_kind
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PointerKey {
    segment_index: usize,
    offset: usize,
}

#[derive(Clone)]
pub struct MarkerRef {
    pub id: usize,
    pub pointer: CursorPointer,
}

#[derive(Clone)]
pub struct RevealTagRef {
    pub id: usize,
    pub style: InlineStyle,
    pub kind: RevealTagKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealTagKind {
    Start,
    End,
}

#[derive(Clone, Copy)]
enum RemovalDirection {
    Backward,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Text,
    RevealStart(InlineStyle),
    RevealEnd(InlineStyle),
}

pub struct DocumentEditor {
    document: Document,
    segments: Vec<SegmentRef>,
    cursor: CursorPointer,
    cursor_segment: usize,
    reveal_codes: bool,
}

impl DocumentEditor {
    pub fn new(mut document: Document) -> Self {
        ensure_document_initialized(&mut document);
        let mut editor = Self {
            document,
            segments: Vec::new(),
            cursor: CursorPointer::default(),
            cursor_segment: 0,
            reveal_codes: false,
        };
        editor.rebuild_segments();
        editor.ensure_cursor_selectable();
        editor
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn current_checklist_item_state(&self) -> Option<bool> {
        let item = checklist_item_ref(&self.document, &self.cursor.paragraph_path)?;
        Some(item.checked)
    }

    pub fn set_current_checklist_item_checked(&mut self, checked: bool) -> bool {
        let Some(item) = checklist_item_mut(&mut self.document, &self.cursor.paragraph_path) else {
            return false;
        };
        let previous = item.checked;
        item.checked = checked;
        previous != checked
    }

    pub fn can_indent_more(&self) -> bool {
        find_indent_target(&self.document, &self.cursor.paragraph_path).is_some()
    }

    pub fn can_indent_less(&self) -> bool {
        self.cursor.paragraph_path.steps().len() > 1
    }

    pub fn indent_current_paragraph(&mut self) -> bool {
        let Some(target) = find_indent_target(&self.document, &self.cursor.paragraph_path) else {
            return false;
        };
        let pointer = self.cursor_stable_pointer();

        if let (Some(ctx), IndentTargetKind::ListEntry { entry_index }) =
            (extract_entry_context(&pointer.paragraph_path), target.kind)
        {
            let handled = if entry_has_multiple_paragraphs(&self.document, &ctx) {
                indent_paragraph_within_entry(&mut self.document, &pointer, &ctx)
            } else {
                indent_list_entry_into_entry(&mut self.document, &pointer, &ctx, entry_index)
            };

            if let Some(new_pointer) = handled {
                self.rebuild_segments();
                if !self.move_to_pointer(&new_pointer) {
                    if !self.fallback_move_to_text(&new_pointer, false) {
                        self.ensure_cursor_selectable();
                    }
                }
                return true;
            }
        }

        let Some(paragraph) = take_paragraph_at(&mut self.document, &pointer.paragraph_path) else {
            return false;
        };
        let new_path = match target.kind {
            IndentTargetKind::Quote => {
                append_paragraph_to_quote(&mut self.document, &target.path, paragraph)
            }
            IndentTargetKind::List => {
                let pointer_in_list_entry =
                    extract_entry_context(&pointer.paragraph_path).is_some();
                if !pointer_in_list_entry {
                    if let Some(entry_index) =
                        list_entry_append_target(&self.document, &target.path)
                    {
                        append_paragraph_to_entry(
                            &mut self.document,
                            &target.path,
                            entry_index,
                            paragraph,
                        )
                    } else {
                        append_paragraph_to_list(&mut self.document, &target.path, paragraph)
                    }
                } else {
                    append_paragraph_to_list(&mut self.document, &target.path, paragraph)
                }
            }
            IndentTargetKind::ListEntry { entry_index } => {
                append_paragraph_to_entry(&mut self.document, &target.path, entry_index, paragraph)
            }
        };
        let Some(paragraph_path) = new_path else {
            return false;
        };
        let mut new_pointer = pointer;
        new_pointer.paragraph_path = paragraph_path;
        self.rebuild_segments();
        if !self.move_to_pointer(&new_pointer) {
            if !self.fallback_move_to_text(&new_pointer, false) {
                self.ensure_cursor_selectable();
            }
        }
        true
    }

    pub fn unindent_current_paragraph(&mut self) -> bool {
        if self.cursor.paragraph_path.steps().len() <= 1 {
            return false;
        }
        let pointer = self.cursor_stable_pointer();
        if matches!(
            pointer.paragraph_path.steps().last(),
            Some(PathStep::Entry { .. })
        ) {
            return self.unindent_list_entry(&pointer);
        }
        let Some(parent_path) = parent_paragraph_path(&pointer.paragraph_path) else {
            return false;
        };
        let Some(paragraph) = take_paragraph_at(&mut self.document, &pointer.paragraph_path) else {
            return false;
        };
        let Some(paragraph_path) =
            insert_paragraph_after_parent(&mut self.document, &parent_path, paragraph)
        else {
            return false;
        };
        let mut new_pointer = pointer;
        new_pointer.paragraph_path = paragraph_path;
        self.rebuild_segments();
        if !self.move_to_pointer(&new_pointer) {
            if !self.fallback_move_to_text(&new_pointer, false) {
                self.ensure_cursor_selectable();
            }
        }
        true
    }

    fn unindent_list_entry(&mut self, pointer: &CursorPointer) -> bool {
        let paragraph_type = paragraph_ref(&self.document, &pointer.paragraph_path)
            .map(|p| p.paragraph_type)
            .unwrap_or(ParagraphType::Text);
        let steps = pointer.paragraph_path.steps();
        let (last_step, prefix) = match steps.split_last() {
            Some(value) => value,
            None => return false,
        };
        let PathStep::Entry {
            entry_index: _,
            paragraph_index,
        } = *last_step
        else {
            return false;
        };

        if paragraph_index > 0 {
            let list_path = ParagraphPath::from_steps(prefix.to_vec());
            let Some(paragraph) = take_paragraph_at(&mut self.document, &pointer.paragraph_path)
            else {
                return false;
            };
            let Some(paragraph_path) =
                insert_paragraph_after_parent(&mut self.document, &list_path, paragraph)
            else {
                return false;
            };
            let mut new_pointer = pointer.clone();
            new_pointer.paragraph_path = paragraph_path;
            new_pointer.offset = pointer.offset;
            self.rebuild_segments();
            if !self.move_to_pointer(&new_pointer) {
                if !self.fallback_move_to_text(&new_pointer, false) {
                    self.ensure_cursor_selectable();
                }
            }
            true
        } else {
            let Some(mut new_pointer) = break_list_entry_for_non_list_target(
                &mut self.document,
                &pointer.paragraph_path,
                paragraph_type,
            ) else {
                return false;
            };
            new_pointer.offset = pointer.offset;
            self.rebuild_segments();
            if !self.move_to_pointer(&new_pointer) {
                if !self.fallback_move_to_text(&new_pointer, false) {
                    self.ensure_cursor_selectable();
                }
            }
            true
        }
    }

    pub fn set_paragraph_type(&mut self, target: ParagraphType) -> bool {
        let current_pointer = self.cursor.clone();
        let mut replacement_pointer = None;

        let mut operation_path = current_pointer.paragraph_path.clone();
        let mut pointer_hint = None;
        let mut post_merge_pointer = None;
        let mut handled_directly = false;
        let treat_as_singular_entry =
            is_single_paragraph_entry(&self.document, &current_pointer.paragraph_path);


        if let Some(scope) = determine_parent_scope(&self.document, &operation_path) {
            operation_path = scope.parent_path.clone();
            let needs_promotion = match scope.relation {
                ParentRelation::Child(_) => true,
                ParentRelation::Entry { .. } => !is_list_type(target),
            };

            if needs_promotion {
                if !promote_single_child_into_parent(&mut self.document, &scope) {
                    return false;
                }
                pointer_hint = Some(CursorPointer {
                    paragraph_path: operation_path.clone(),
                    span_path: current_pointer.span_path.clone(),
                    offset: current_pointer.offset,
                    segment_kind: current_pointer.segment_kind,
                });
            }
        }

        if !is_list_type(target) && treat_as_singular_entry {
            if let Some(pointer) =
                break_list_entry_for_non_list_target(&mut self.document, &operation_path, target)
            {
                operation_path = pointer.paragraph_path.clone();
                pointer_hint = Some(pointer);
                handled_directly = true;
            }
        }

        if !handled_directly && is_list_type(target) {
            if let Some(list_path) = find_list_ancestor_path(&self.document, &operation_path) {
                if !update_existing_list_type(&mut self.document, &list_path, target) {
                    return false;
                }
                operation_path = list_path;
            } else {
                match convert_paragraph_into_list(&mut self.document, &operation_path, target) {
                    Some(pointer) => replacement_pointer = Some(pointer),
                    None => return false,
                }
            }
        } else if !handled_directly
            && !update_paragraph_type(&mut self.document, &operation_path, target)
        {
            return false;
        }

        if !handled_directly && is_list_type(target) {
            let pointer_for_merge = replacement_pointer
                .clone()
                .or_else(|| pointer_hint.clone())
                .or_else(|| Some(current_pointer.clone()));

            if let Some(pointer) = pointer_for_merge {
                if let Some(ctx) = extract_entry_context(&pointer.paragraph_path) {
                    if let Some((merged_list_path, merged_entry_idx)) =
                        merge_adjacent_lists(&mut self.document, &ctx.list_path, ctx.entry_index)
                    {
                        let mut steps = merged_list_path.steps().to_vec();
                        // Reconstruct the path step based on the target list type
                        if target == ParagraphType::Checklist {
                            steps.push(PathStep::ChecklistItem {
                                indices: vec![merged_entry_idx],
                            });
                        } else {
                            steps.push(PathStep::Entry {
                                entry_index: merged_entry_idx,
                                paragraph_index: ctx.paragraph_index,
                            });
                        }
                        steps.extend(ctx.tail_steps.iter().cloned());
                        let new_paragraph_path = ParagraphPath::from_steps(steps);
                        let new_pointer = CursorPointer {
                            paragraph_path: new_paragraph_path,
                            span_path: pointer.span_path.clone(),
                            offset: pointer.offset,
                            segment_kind: pointer.segment_kind,
                        };
                        if replacement_pointer.is_some() {
                            replacement_pointer = Some(new_pointer.clone());
                        }
                        if pointer_hint.is_some() {
                            pointer_hint = Some(new_pointer.clone());
                        }
                        post_merge_pointer = Some(new_pointer);
                    }
                } else {
                    merge_adjacent_lists(&mut self.document, &operation_path, 0);
                }
            } else {
                merge_adjacent_lists(&mut self.document, &operation_path, 0);
            }
        }

        self.rebuild_segments();

        let desired = if let Some(pointer) = replacement_pointer {
            pointer
        } else if let Some(pointer) = post_merge_pointer {
            pointer
        } else if let Some(pointer) = pointer_hint {
            pointer
        } else {
            current_pointer
        };

        if !self.move_to_pointer(&desired) {
            if !self.fallback_move_to_text(&desired, false) {
                self.ensure_cursor_selectable();
            }
        }

        true
    }

    pub fn insert_char(&mut self, ch: char) -> bool {
        if !self.prepare_cursor_for_text_insertion() {
            return false;
        }
        let pointer = self.cursor.clone();
        if !pointer.is_valid() {
            return false;
        }
        if insert_char_at(&mut self.document, &pointer, self.cursor.offset, ch) {
            self.cursor.offset += 1;
            self.rebuild_segments();
            true
        } else {
            false
        }
    }

    fn prepare_cursor_for_text_insertion(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        match self.cursor.segment_kind {
            SegmentKind::Text => self.cursor.is_valid(),
            SegmentKind::RevealStart(_) => {
                if let Some(idx) = self.find_previous_text_segment_in_paragraph(self.cursor_segment)
                {
                    if let Some(segment) = self.segments.get(idx).cloned() {
                        self.cursor_segment = idx;
                        self.cursor.update_from_segment(&segment);
                        self.cursor.offset = segment.len;
                        return self.cursor.is_valid();
                    }
                }
                let pointer = self.cursor.clone();
                if !self.fallback_move_to_text(&pointer, false) {
                    return false;
                }
                self.cursor.offset = 0;
                self.cursor.segment_kind = SegmentKind::Text;
                self.cursor.is_valid()
            }
            SegmentKind::RevealEnd(_) => {
                let pointer = self.cursor.clone();
                if !self.fallback_move_to_text(&pointer, true) {
                    return false;
                }
                self.cursor.offset = self.current_segment_len();
                self.cursor.segment_kind = SegmentKind::Text;
                self.cursor.is_valid()
            }
        }
    }

    pub fn backspace(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        if self.current_paragraph_is_empty() {
            if self.remove_current_paragraph(RemovalDirection::Backward) {
                return true;
            }
        }
        if self.cursor.offset == 0 {
            if !self.shift_to_previous_segment() {
                return false;
            }
            let current_len = self.current_segment_len();
            if current_len == 0 {
                return false;
            }
            self.cursor.offset = current_len.saturating_sub(1);
        } else {
            self.cursor.offset -= 1;
        }
        if let Some(segment) = self.segments.get(self.cursor_segment) {
            if segment.kind != SegmentKind::Text {
                if let Some(target_pointer) = self.remove_reveal_tag_segment(self.cursor_segment) {
                    self.rebuild_segments();
                    if !self.move_to_pointer(&target_pointer) {
                        if !self.fallback_move_to_text(&target_pointer, false) {
                            self.ensure_cursor_selectable();
                        }
                    }
                    return true;
                } else {
                    return false;
                }
            }
        }
        let pointer = self.cursor.clone();
        if remove_char_at(&mut self.document, &pointer, self.cursor.offset) {
            self.rebuild_segments();
            true
        } else {
            false
        }
    }

    pub fn delete(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        if self.current_paragraph_is_empty() {
            if self.remove_current_paragraph(RemovalDirection::Forward) {
                return true;
            }
        }
        let current_len = self.current_segment_len();
        if self.cursor.offset < current_len {
            if self.cursor.segment_kind != SegmentKind::Text {
                if let Some(target_pointer) = self.remove_reveal_tag_segment(self.cursor_segment) {
                    self.rebuild_segments();
                    if !self.move_to_pointer(&target_pointer) {
                        if !self.fallback_move_to_text(&target_pointer, false) {
                            self.ensure_cursor_selectable();
                        }
                    }
                    return true;
                } else {
                    return false;
                }
            }
            let pointer = self.cursor.clone();
            if remove_char_at(&mut self.document, &pointer, self.cursor.offset) {
                self.rebuild_segments();
                return true;
            }
            return false;
        }

        if self.cursor_segment + 1 >= self.segments.len() {
            return false;
        }

        let next_segment = &self.segments[self.cursor_segment + 1];
        if next_segment.kind != SegmentKind::Text {
            if let Some(target_pointer) = self.remove_reveal_tag_segment(self.cursor_segment + 1) {
                self.rebuild_segments();
                if !self.move_to_pointer(&target_pointer) {
                    if !self.fallback_move_to_text(&target_pointer, false) {
                        self.ensure_cursor_selectable();
                    }
                }
                return true;
            } else {
                return false;
            }
        }
        let pointer = CursorPointer {
            paragraph_path: next_segment.paragraph_path.clone(),
            span_path: next_segment.span_path.clone(),
            offset: 0,
            segment_kind: next_segment.kind,
        };
        if remove_char_at(&mut self.document, &pointer, 0) {
            self.rebuild_segments();
            true
        } else {
            false
        }
    }

    pub fn delete_word_backward(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }

        let Some((target_segment, target_pointer)) = self.previous_word_position() else {
            return false;
        };

        let target_offset = target_pointer.offset;
        let steps = self.count_backward_steps(target_segment, target_offset);
        if steps == 0 {
            return false;
        }

        let mut removed = false;
        let mut remaining = steps;

        while remaining > 0 {
            if !self.backspace() {
                break;
            }
            removed = true;
            remaining -= 1;
            if self.cursor.paragraph_path == target_pointer.paragraph_path
                && self.cursor.span_path == target_pointer.span_path
                && self.cursor.segment_kind == target_pointer.segment_kind
                && self.cursor.offset == target_pointer.offset
            {
                break;
            }
        }

        if removed
            && !(self.cursor.paragraph_path == target_pointer.paragraph_path
                && self.cursor.span_path == target_pointer.span_path
                && self.cursor.segment_kind == target_pointer.segment_kind
                && self.cursor.offset == target_pointer.offset)
        {
            let _ = self.move_to_pointer(&target_pointer);
        }

        removed
    }

    pub fn delete_word_forward(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }

        let start_pointer = self.cursor_pointer();

        let Some((target_segment, target_pointer)) = self.next_word_position() else {
            return false;
        };

        let target_offset = target_pointer.offset;
        let steps = self.count_forward_steps(target_segment, target_offset);
        if steps == 0 {
            return false;
        }

        if !self.move_to_pointer(&target_pointer) {
            self.cursor = target_pointer.clone();
            if self.segments.is_empty() {
                self.cursor_segment = 0;
            } else {
                self.cursor_segment = target_segment.min(self.segments.len() - 1);
            }
            self.clamp_cursor_offset();
        }

        let mut removed = false;
        let mut remaining = steps;

        while remaining > 0 {
            if !self.backspace() {
                break;
            }
            removed = true;
            remaining -= 1;
            if self.cursor.paragraph_path == start_pointer.paragraph_path
                && self.cursor.span_path == start_pointer.span_path
                && self.cursor.segment_kind == start_pointer.segment_kind
                && self.cursor.offset == start_pointer.offset
            {
                break;
            }
        }

        if removed
            && !(self.cursor.paragraph_path == start_pointer.paragraph_path
                && self.cursor.span_path == start_pointer.span_path
                && self.cursor.segment_kind == start_pointer.segment_kind
                && self.cursor.offset == start_pointer.offset)
        {
            let _ = self.move_to_pointer(&start_pointer);
        }

        removed
    }

    fn remove_reveal_tag_segment(&mut self, segment_index: usize) -> Option<CursorPointer> {
        let segment = self.segments.get(segment_index)?.clone();
        let style = match segment.kind {
            SegmentKind::RevealStart(style) | SegmentKind::RevealEnd(style) => style,
            SegmentKind::Text => return None,
        };
        let Some(paragraph) = paragraph_mut(&mut self.document, &segment.paragraph_path) else {
            return None;
        };
        let Some(span) = span_mut(paragraph, &segment.span_path) else {
            return None;
        };
        if span.style != style {
            // Style mismatch indicates the structure changed; treat as no-op.
            return None;
        }
        span.style = InlineStyle::None;
        span.link_target = None;
        let span_len = span.text.chars().count();
        prune_and_merge_spans(paragraph.content_mut());
        let offset = match segment.kind {
            SegmentKind::RevealStart(_) => 0,
            SegmentKind::RevealEnd(_) => span_len,
            SegmentKind::Text => span_len,
        };
        Some(CursorPointer {
            paragraph_path: segment.paragraph_path,
            span_path: segment.span_path,
            offset,
            segment_kind: SegmentKind::Text,
        })
    }

    fn current_paragraph_is_empty(&self) -> bool {
        paragraph_ref(&self.document, &self.cursor.paragraph_path)
            .map(paragraph_is_empty)
            .unwrap_or(false)
    }

    fn current_paragraph_segment_range(&self) -> Option<(usize, usize)> {
        if self.segments.is_empty() {
            return None;
        }
        let segment = self.segments.get(self.cursor_segment)?;
        let target_path = &segment.paragraph_path;

        let mut start = self.cursor_segment;
        while start > 0 && self.segments[start - 1].paragraph_path == *target_path {
            start -= 1;
        }

        let mut end = self.cursor_segment + 1;
        while end < self.segments.len() && self.segments[end].paragraph_path == *target_path {
            end += 1;
        }

        Some((start, end))
    }

    fn remove_current_paragraph(&mut self, direction: RemovalDirection) -> bool {
        let (start_idx, end_idx) = match self.current_paragraph_segment_range() {
            Some(range) => range,
            None => return false,
        };

        let current_path = self.cursor.paragraph_path.clone();

        let target_pointer = match direction {
            RemovalDirection::Backward => {
                if start_idx > 0 {
                    let prev = &self.segments[start_idx - 1];
                    Some(CursorPointer {
                        paragraph_path: prev.paragraph_path.clone(),
                        span_path: prev.span_path.clone(),
                        offset: prev.len,
                        segment_kind: prev.kind,
                    })
                } else if end_idx < self.segments.len() {
                    let next = &self.segments[end_idx];
                    Some(CursorPointer {
                        paragraph_path: next.paragraph_path.clone(),
                        span_path: next.span_path.clone(),
                        offset: 0,
                        segment_kind: next.kind,
                    })
                } else {
                    None
                }
            }
            RemovalDirection::Forward => {
                if end_idx < self.segments.len() {
                    let next = &self.segments[end_idx];
                    Some(CursorPointer {
                        paragraph_path: next.paragraph_path.clone(),
                        span_path: next.span_path.clone(),
                        offset: 0,
                        segment_kind: next.kind,
                    })
                } else if start_idx > 0 {
                    let prev = &self.segments[start_idx - 1];
                    Some(CursorPointer {
                        paragraph_path: prev.paragraph_path.clone(),
                        span_path: prev.span_path.clone(),
                        offset: prev.len,
                        segment_kind: prev.kind,
                    })
                } else {
                    None
                }
            }
        };

        if !remove_paragraph_by_path(&mut self.document, &current_path) {
            return false;
        }

        self.rebuild_segments();

        if self.segments.is_empty() {
            return true;
        }

        if let Some(pointer) = target_pointer {
            if self.move_to_pointer(&pointer) {
                return true;
            }
        }

        if let Some(segment) = self.segments.first().cloned() {
            self.cursor.update_from_segment(&segment);
            self.cursor.offset = segment.len.min(self.cursor.offset);
            self.cursor_segment = 0;
        }

        true
    }

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

        if matches!(self.compare_pointers(&start, &end), Some(Ordering::Greater)) {
            mem::swap(&mut start, &mut end);
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
            if len == 0 {
                continue;
            }
            if segment.kind != SegmentKind::Text {
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
            &mut paragraph.content,
            segment.span_path.indices(),
            start,
            end,
            style,
        )
    }

    pub fn compare_pointers(&self, a: &CursorPointer, b: &CursorPointer) -> Option<Ordering> {
        let key_a = self.pointer_key(a)?;
        let key_b = self.pointer_key(b)?;
        Some(key_a.cmp(&key_b))
    }
}

fn ensure_document_initialized(document: &mut Document) {
    if document.paragraphs.is_empty() {
        document
            .paragraphs
            .push(Paragraph::new_text().with_content(vec![Span::new_text("")]));
    }
}

fn paragraph_from_checklist_item(item: ChecklistItem) -> Paragraph {
    Paragraph::new_text().with_content(item.content)
}

fn select_text_in_paragraph(
    segments: &[SegmentRef],
    paragraph_path: &ParagraphPath,
    prefer_trailing: bool,
) -> Option<(usize, SegmentRef)> {
    let mut result: Option<(usize, SegmentRef)> = None;
    for (index, segment) in segments.iter().enumerate() {
        if segment.paragraph_path != *paragraph_path {
            continue;
        }
        if segment.kind != SegmentKind::Text {
            continue;
        }
        if !prefer_trailing {
            return Some((index, segment.clone()));
        }
        result = Some((index, segment.clone()));
    }
    result
}


fn is_single_paragraph_entry(document: &Document, path: &ParagraphPath) -> bool {
    let steps = path.steps();
    let (last_step, prefix) = match steps.split_last() {
        Some(result) => result,
        None => return false,
    };

    match last_step {
        PathStep::Entry { entry_index, .. } => {
            let parent = match paragraph_ref(document, &ParagraphPath::from_steps(prefix.to_vec())) {
                Some(paragraph) => paragraph,
                None => return false,
            };
            match parent {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    entries
                        .get(*entry_index)
                        .map(|entry| entry.len() == 1)
                        .unwrap_or(false)
                }
                _ => false,
            }
        }
        PathStep::ChecklistItem { .. } => {
            // Checklist items are always treated as single entries
            true
        }
        _ => false,
    }
}

#[derive(Clone)]
struct ParentScope {
    parent_path: ParagraphPath,
    relation: ParentRelation,
}

#[derive(Clone, Copy)]
enum ParentRelation {
    Child(usize),
    Entry {
        entry_index: usize,
        paragraph_index: usize,
    },
}

fn determine_parent_scope(document: &Document, path: &ParagraphPath) -> Option<ParentScope> {
    let steps = path.steps();
    if steps.len() <= 1 {
        return None;
    }

    let (last, prefix) = steps.split_last()?;
    let parent_path = ParagraphPath::from_steps(prefix.to_vec());
    let parent = paragraph_ref(document, &parent_path)?;

    match last {
        PathStep::Child(idx) => match parent {
            Paragraph::Quote { children } if children.len() == 1 && *idx < children.len() => {
                Some(ParentScope {
                    parent_path,
                    relation: ParentRelation::Child(*idx),
                })
            }
            _ => None,
        },
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => match parent {
            Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries }
                if *entry_index < entries.len() =>
            {
                let entry = entries.get(*entry_index)?;
                if entries.len() == 1 && entry.len() == 1 && *paragraph_index < entry.len() {
                    Some(ParentScope {
                        parent_path,
                        relation: ParentRelation::Entry {
                            entry_index: *entry_index,
                            paragraph_index: *paragraph_index,
                        },
                    })
                } else {
                    None
                }
            }
            _ => None,
        },
        PathStep::ChecklistItem { indices } => {
            let item_index = *indices.first()?;
            match parent {
                Paragraph::Checklist { items } if items.len() == 1 && item_index < items.len() => {
                    Some(ParentScope {
                        parent_path,
                        relation: ParentRelation::Entry {
                            entry_index: item_index,
                            paragraph_index: 0,
                        },
                    })
                }
                _ => None,
            }
        }
        PathStep::Root(_) => None,
    }
}

fn promote_single_child_into_parent(document: &mut Document, scope: &ParentScope) -> bool {
    let Some(parent) = paragraph_mut(document, &scope.parent_path) else {
        return false;
    };

    let is_checklist = parent.paragraph_type == ParagraphType::Checklist;

    let child = match scope.relation {
        ParentRelation::Child(idx) => match parent {
            Paragraph::Quote { children } => {
                if idx >= children.len() {
                    return false;
                }
                children.remove(idx)
            }
            _ => return false,
        },
        ParentRelation::Entry {
            entry_index,
            paragraph_index,
        } => {
            if is_checklist {
                let Paragraph::Checklist { items } = parent else {
                    return false;
                };
                if entry_index >= items.len() {
                    return false;
                }
                let item = items.remove(entry_index);
                paragraph_from_checklist_item(item)
            } else {
                match parent {
                    Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                        if entry_index >= entries.len() {
                            return false;
                        }
                        let mut entry = entries.remove(entry_index);
                        if paragraph_index >= entry.len() {
                            return false;
                        }
                        let extracted = entry.remove(paragraph_index);
                        if !entry.is_empty() {
                            entries.insert(entry_index, entry);
                        }
                        extracted
                    }
                    _ => return false,
                }
            }
        }
    };

    *parent = child;
    true
}

fn apply_paragraph_type_in_place(paragraph: &mut Paragraph, target: ParagraphType) {
    if target == ParagraphType::Quote {
        paragraph.paragraph_type = ParagraphType::Quote;

        let mut children = Vec::new();
        if !paragraph.content.is_empty() {
            let mut text_child = Paragraph::new_text();
            text_child.content = mem::take(&mut paragraph.content);
            if text_child.content.is_empty() {
                text_child.content.push(Span::new_text(""));
            }
            children.push(text_child);
        }

        if !paragraph.children.is_empty() {
            children.append(&mut paragraph.children);
        }

        if !paragraph.entries.is_empty() {
            for entry in paragraph.entries.drain(..) {
                children.extend(entry);
            }
        }

        if children.is_empty() {
            children.push(empty_text_paragraph());
        }

        paragraph.children = children;
        paragraph.entries.clear();
        paragraph.content.clear();
        return;
    }

    paragraph.paragraph_type = target;

    if target.is_leaf() {
        paragraph.children.clear();
        paragraph.entries.clear();
    }

    if paragraph.content.is_empty() {
        paragraph.content.push(Span::new_text(""));
    }
}

fn break_list_entry_for_non_list_target(
    document: &mut Document,
    entry_path: &ParagraphPath,
    target: ParagraphType,
) -> Option<CursorPointer> {
    let steps = entry_path.steps();
    let (last_step, prefix) = steps.split_last()?;

    // Handle both Entry and ChecklistItem path steps
    let (entry_index, paragraph_index, is_checklist_item) = match last_step {
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => (*entry_index, *paragraph_index, false),
        PathStep::ChecklistItem { indices } => (*indices.first()?, 0, true),
        _ => return None,
    };

    let list_path = ParagraphPath::from_steps(prefix.to_vec());
    let Some(list_paragraph) = paragraph_ref(document, &list_path) else {
        return None;
    };
    if !is_list_type(list_paragraph.paragraph_type) {
        return None;
    }

    let (list_type, entries_after, checklist_items_after, extracted_paragraphs, target_offset, has_prefix_entries) = {
        let list = paragraph_mut(document, &list_path)?;

        if is_checklist_item {
            // Handle checklist items
            if entry_index >= list.checklist_items.len() {
                return None;
            }
            let items_after: Vec<ChecklistItem> = list.checklist_items.split_off(entry_index + 1);
            let selected_item = list.checklist_items.remove(entry_index);

            // Convert checklist item to a paragraph
            let mut paragraph = Paragraph::new_text();
            paragraph.content = selected_item.content;
            apply_paragraph_type_in_place(&mut paragraph, target);
            if paragraph.paragraph_type.is_leaf() && paragraph.content.is_empty() {
                paragraph.content.push(Span::new_text(""));
            }

            let has_prefix = !list.checklist_items.is_empty();

            (list.paragraph_type, vec![], items_after, vec![paragraph], 0, has_prefix)
        } else {
            // Handle regular list entries
            if entry_index >= list.entries.len() {
                return None;
            }
            if list.entries[entry_index].len() > 1 {
                return None;
            }
            let entries_after = list.entries.split_off(entry_index + 1);
            let selected_entry = list.entries.remove(entry_index);
            if paragraph_index >= selected_entry.len() {
                return None;
            }

            let mut extracted = Vec::new();
            for (idx, mut paragraph) in selected_entry.into_iter().enumerate() {
                if idx == paragraph_index {
                    apply_paragraph_type_in_place(&mut paragraph, target);
                }
                if paragraph.paragraph_type.is_leaf() && paragraph.content.is_empty() {
                    paragraph.content.push(Span::new_text(""));
                }
                extracted.push(paragraph);
            }
            if extracted.is_empty() {
                return None;
            }
            let target_offset = paragraph_index.min(extracted.len().saturating_sub(1));
            let has_prefix_entries = !list.entries.is_empty();
            (
                list.paragraph_type,
                entries_after,
                vec![], // No checklist items for regular lists
                extracted,
                target_offset,
                has_prefix_entries,
            )
        }
    };

    let extract_count = extracted_paragraphs.len();
    let list_steps = list_path.steps();
    let (list_last_step, list_prefix) = list_steps.split_last()?;

    match *list_last_step {
        PathStep::Root(idx) if list_prefix.is_empty() => {
            if has_prefix_entries {
                let insertion_index = idx + 1;
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    document
                        .paragraphs
                        .insert(insertion_index + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    if list_type == ParagraphType::Checklist {
                        tail.checklist_items = checklist_items_after;
                    } else {
                        tail.entries = entries_after;
                    }
                    document
                        .paragraphs
                        .insert(insertion_index + extract_count, tail);
                }
                let new_path = ParagraphPath::from_steps(vec![PathStep::Root(
                    insertion_index + target_offset,
                )]);
                Some(CursorPointer {
                    paragraph_path: new_path,
                    span_path: SpanPath::new(vec![0]),
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                })
            } else {
                document.paragraphs.remove(idx);
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    document.paragraphs.insert(idx + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    if list_type == ParagraphType::Checklist {
                        tail.checklist_items = checklist_items_after;
                    } else {
                        tail.entries = entries_after;
                    }
                    document.paragraphs.insert(idx + extract_count, tail);
                }
                let new_path = ParagraphPath::from_steps(vec![PathStep::Root(idx + target_offset)]);
                Some(CursorPointer {
                    paragraph_path: new_path,
                    span_path: SpanPath::new(vec![0]),
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                })
            }
        }
        PathStep::Child(child_idx) => {
            let parent_path = ParagraphPath::from_steps(list_prefix.to_vec());
            let parent = paragraph_mut(document, &parent_path)?;
            if has_prefix_entries {
                let insertion_index = child_idx + 1;
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    parent.children.insert(insertion_index + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    if list_type == ParagraphType::Checklist {
                        tail.checklist_items = checklist_items_after;
                    } else {
                        tail.entries = entries_after;
                    }
                    parent
                        .children
                        .insert(insertion_index + extract_count, tail);
                }
                let mut new_steps = list_prefix.to_vec();
                new_steps.push(PathStep::Child(child_idx + 1 + target_offset));
                Some(CursorPointer {
                    paragraph_path: ParagraphPath::from_steps(new_steps),
                    span_path: SpanPath::new(vec![0]),
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                })
            } else {
                parent.children.remove(child_idx);
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    parent.children.insert(child_idx + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    if list_type == ParagraphType::Checklist {
                        tail.checklist_items = checklist_items_after;
                    } else {
                        tail.entries = entries_after;
                    }
                    parent.children.insert(child_idx + extract_count, tail);
                }
                let mut new_steps = list_prefix.to_vec();
                new_steps.push(PathStep::Child(child_idx + target_offset));
                Some(CursorPointer {
                    paragraph_path: ParagraphPath::from_steps(new_steps),
                    span_path: SpanPath::new(vec![0]),
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                })
            }
        }
        PathStep::Entry {
            entry_index,
            paragraph_index: list_child_idx,
        } => {
            let parent_path = ParagraphPath::from_steps(list_prefix.to_vec());
            let parent = paragraph_mut(document, &parent_path)?;
            if entry_index >= parent.entries.len() {
                return None;
            }
            let entry = &mut parent.entries[entry_index];
            if has_prefix_entries {
                let insertion_index = list_child_idx + 1;
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    entry.insert(insertion_index + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    if list_type == ParagraphType::Checklist {
                        tail.checklist_items = checklist_items_after;
                    } else {
                        tail.entries = entries_after;
                    }
                    entry.insert(insertion_index + extract_count, tail);
                }
                let mut new_steps = list_prefix.to_vec();
                new_steps.push(PathStep::Entry {
                    entry_index,
                    paragraph_index: list_child_idx + 1 + target_offset,
                });
                Some(CursorPointer {
                    paragraph_path: ParagraphPath::from_steps(new_steps),
                    span_path: SpanPath::new(vec![0]),
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                })
            } else {
                if list_child_idx >= entry.len() {
                    return None;
                }
                entry.remove(list_child_idx);
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    entry.insert(list_child_idx + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    if list_type == ParagraphType::Checklist {
                        tail.checklist_items = checklist_items_after;
                    } else {
                        tail.entries = entries_after;
                    }
                    entry.insert(list_child_idx + extract_count, tail);
                }
                let mut new_steps = list_prefix.to_vec();
                new_steps.push(PathStep::Entry {
                    entry_index,
                    paragraph_index: list_child_idx + target_offset,
                });
                Some(CursorPointer {
                    paragraph_path: ParagraphPath::from_steps(new_steps),
                    span_path: SpanPath::new(vec![0]),
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                })
            }
        }
        _ => None,
    }
}

#[derive(Clone)]
struct EntryContext {
    list_path: ParagraphPath,
    entry_index: usize,
    paragraph_index: usize,
    tail_steps: Vec<PathStep>,
}

fn extract_entry_context(path: &ParagraphPath) -> Option<EntryContext> {
    let steps = path.steps();
    for idx in (0..steps.len()).rev() {
        match &steps[idx] {
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => {
                let list_path = ParagraphPath::from_steps(steps[..idx].to_vec());
                let tail_steps = steps[idx + 1..].to_vec();
                return Some(EntryContext {
                    list_path,
                    entry_index: *entry_index,
                    paragraph_index: *paragraph_index,
                    tail_steps,
                });
            }
            PathStep::ChecklistItem { indices } => {
                // For checklist items, treat the first index as the entry_index
                // and use 0 as paragraph_index since checklist items are not wrapped in entries
                let list_path = ParagraphPath::from_steps(steps[..idx].to_vec());
                let tail_steps = steps[idx + 1..].to_vec();
                return Some(EntryContext {
                    list_path,
                    entry_index: *indices.first().unwrap_or(&0),
                    paragraph_index: 0,
                    tail_steps,
                });
            }
            _ => {}
        }
    }
    None
}

fn merge_adjacent_lists(
    document: &mut Document,
    list_path: &ParagraphPath,
    entry_index: usize,
) -> Option<(ParagraphPath, usize)> {
    let list_type = paragraph_ref(document, list_path)?.paragraph_type;

    let steps = list_path.steps();
    let (last_step, prefix) = steps.split_last()?;

    match *last_step {
        PathStep::Root(idx) if prefix.is_empty() => {
            let (new_idx, new_entry_idx) =
                merge_adjacent_lists_in_vec(&mut document.paragraphs, idx, entry_index, list_type)?;
            let new_path = ParagraphPath::from_steps(vec![PathStep::Root(new_idx)]);
            Some((new_path, new_entry_idx))
        }
        PathStep::Child(child_idx) => {
            let parent_path = ParagraphPath::from_steps(prefix.to_vec());
            let parent = paragraph_mut(document, &parent_path)?;
            let (new_idx, new_entry_idx) = merge_adjacent_lists_in_vec(
                &mut parent.children,
                child_idx,
                entry_index,
                list_type,
            )?;
            let mut steps = prefix.to_vec();
            steps.push(PathStep::Child(new_idx));
            Some((ParagraphPath::from_steps(steps), new_entry_idx))
        }
        PathStep::Entry {
            entry_index: parent_entry_index,
            paragraph_index,
        } => {
            let parent_path = ParagraphPath::from_steps(prefix.to_vec());
            let parent = paragraph_mut(document, &parent_path)?;
            if parent_entry_index >= parent.entries.len() {
                return None;
            }
            let entry = &mut parent.entries[parent_entry_index];
            let (new_idx, new_entry_idx) =
                merge_adjacent_lists_in_vec(entry, paragraph_index, entry_index, list_type)?;
            let mut steps = prefix.to_vec();
            steps.push(PathStep::Entry {
                entry_index: parent_entry_index,
                paragraph_index: new_idx,
            });
            Some((ParagraphPath::from_steps(steps), new_entry_idx))
        }
        _ => None,
    }
}

fn merge_adjacent_lists_in_vec(
    paragraphs: &mut Vec<Paragraph>,
    index: usize,
    entry_index: usize,
    list_type: ParagraphType,
) -> Option<(usize, usize)> {
    if index >= paragraphs.len() {
        return None;
    }

    let mut list_index = index;
    let mut target_entry_index = entry_index;

    if list_index > 0 && paragraphs[list_index - 1].paragraph_type == list_type {
        let current = paragraphs.remove(list_index);
        let previous = &mut paragraphs[list_index - 1];

        if list_type == ParagraphType::Checklist {
            let previous_entry_count = previous.checklist_items.len();
            target_entry_index += previous_entry_count;
            previous.checklist_items.extend(current.checklist_items);
        } else {
            let previous_entry_count = previous.entries.len();
            target_entry_index += previous_entry_count;
            previous.entries.extend(current.entries);
        }
        list_index -= 1;
    }

    if list_index + 1 < paragraphs.len() && paragraphs[list_index + 1].paragraph_type == list_type {
        let next = paragraphs.remove(list_index + 1);
        let current = &mut paragraphs[list_index];

        if list_type == ParagraphType::Checklist {
            current.checklist_items.extend(next.checklist_items);
        } else {
            current.entries.extend(next.entries);
        }
    }

    Some((list_index, target_entry_index))
}

fn is_list_type(kind: ParagraphType) -> bool {
    matches!(
        kind,
        ParagraphType::OrderedList | ParagraphType::UnorderedList | ParagraphType::Checklist
    )
}

fn find_list_ancestor_path(document: &Document, path: &ParagraphPath) -> Option<ParagraphPath> {
    let mut steps = path.steps().to_vec();
    while !steps.is_empty() {
        let candidate = ParagraphPath::from_steps(steps.clone());
        if let Some(paragraph) = paragraph_ref(document, &candidate) {
            if is_list_type(paragraph.paragraph_type) {
                return Some(candidate);
            }
        }
        steps.pop();
    }
    None
}

fn update_existing_list_type(
    document: &mut Document,
    path: &ParagraphPath,
    target: ParagraphType,
) -> bool {
    let Some(paragraph) = paragraph_mut(document, path) else {
        return false;
    };

    paragraph.paragraph_type = target;
    paragraph.content.clear();

    match target {
        ParagraphType::Checklist => {
            // Convert entries to checklist items
            if paragraph.checklist_items.is_empty() && !paragraph.entries.is_empty() {
                let mut items = Vec::new();
                for entry in &paragraph.entries {
                    if let Some(first) = entry.first() {
                        let item = ChecklistItem::new(false).with_content(first.content.clone());
                        items.push(item);
                    }
                }
                paragraph.checklist_items = items;
                paragraph.entries.clear();
            }
            if paragraph.checklist_items.is_empty() {
                paragraph.checklist_items.push(ChecklistItem::new(false).with_content(vec![Span::new_text("")]));
            }
        }
        ParagraphType::OrderedList | ParagraphType::UnorderedList => {
            // Convert checklist items to entries
            if paragraph.entries.is_empty() && !paragraph.checklist_items.is_empty() {
                let mut entries = Vec::new();
                for item in &paragraph.checklist_items {
                    let para = Paragraph::new_text().with_content(item.content.clone());
                    entries.push(vec![para]);
                }
                paragraph.entries = entries;
                paragraph.checklist_items.clear();
            }
            normalize_entries_for_standard_list(&mut paragraph.entries);
            if paragraph.entries.is_empty() {
                paragraph.entries.push(vec![empty_text_paragraph()]);
            }
        }
        _ => {}
    }

    true
}

fn convert_paragraph_into_list(
    document: &mut Document,
    path: &ParagraphPath,
    target: ParagraphType,
) -> Option<CursorPointer> {
    let paragraph = paragraph_mut(document, path)?;

    let mut content = mem::take(&mut paragraph.content);
    if content.is_empty() {
        content.push(Span::new_text(""));
    }

    paragraph.paragraph_type = target;

    match target {
        ParagraphType::Checklist => {
            let item = ChecklistItem::new(false).with_content(content);
            paragraph.checklist_items = vec![item];
            paragraph.entries.clear();
            paragraph.children.clear();

            let mut steps = path.steps().to_vec();
            steps.push(PathStep::ChecklistItem { indices: vec![0] });
            let new_path = ParagraphPath::from_steps(steps);
            let span_path = SpanPath::new(vec![0]);

            Some(CursorPointer {
                paragraph_path: new_path,
                span_path,
                offset: 0,
                segment_kind: SegmentKind::Text,
            })
        }
        ParagraphType::OrderedList | ParagraphType::UnorderedList => {
            let mut head = Paragraph::new_text();
            head.content = content;
            if head.content.is_empty() {
                head.content.push(Span::new_text(""));
            }

            let children = mem::take(&mut paragraph.children);
            let mut entry = vec![head];
            if !children.is_empty() {
                entry.extend(children);
            }

            paragraph.entries = vec![entry];
            paragraph.checklist_items.clear();

            let mut steps = path.steps().to_vec();
            steps.push(PathStep::Entry {
                entry_index: 0,
                paragraph_index: 0,
            });
            let new_path = ParagraphPath::from_steps(steps);
            let span_path = SpanPath::new(vec![0]);

            Some(CursorPointer {
                paragraph_path: new_path,
                span_path,
                offset: 0,
                segment_kind: SegmentKind::Text,
            })
        }
        _ => None,
    }
}

fn update_paragraph_type(
    document: &mut Document,
    path: &ParagraphPath,
    target: ParagraphType,
) -> bool {
    let Some(paragraph) = paragraph_mut(document, path) else {
        return false;
    };

    apply_paragraph_type_in_place(paragraph, target);
    true
}

fn normalize_entries_for_standard_list(entries: &mut Vec<Vec<Paragraph>>) {
    if entries.is_empty() {
        entries.push(vec![empty_text_paragraph()]);
        return;
    }

    for entry in entries.iter_mut() {
        if entry.is_empty() {
            entry.push(empty_text_paragraph());
            continue;
        }

        if entry[0].content.is_empty() {
            entry[0].content.push(Span::new_text(""));
        }
    }
}

fn empty_text_paragraph() -> Paragraph {
    Paragraph::new_text().with_content(vec![Span::new_text("")])
}

fn inline_style_label(style: InlineStyle) -> Option<&'static str> {
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

fn split_paragraph_break(
    document: &mut Document,
    pointer: &CursorPointer,
    prefer_entry_sibling: bool,
) -> Option<CursorPointer> {
    let steps_vec: Vec<PathStep> = pointer.paragraph_path.steps().to_vec();
    let (last_step, prefix) = steps_vec.split_last()?;

    let span_indices = pointer.span_path.indices().to_vec();

    let mut right_spans = {
        // Check if we're in a checklist item
        let is_checklist_item = steps_vec.iter()
            .any(|step| matches!(step, PathStep::ChecklistItem { .. }));

        if is_checklist_item {
            let item = checklist_item_mut(document, &pointer.paragraph_path)?;
            let split = split_spans(&mut item.content, &span_indices, pointer.offset);
            if item.content.is_empty() {
                item.content.push(Span::new_text(""));
            }
            split
        } else {
            let paragraph = paragraph_mut(document, &pointer.paragraph_path)?;
            let split = {
                let spans = paragraph.content_mut();
                let split = split_spans(spans, &span_indices, pointer.offset);
                if spans.is_empty() {
                    spans.push(Span::new_text(""));
                }
                split
            };
            split
        }
    };

    if right_spans.is_empty() {
        right_spans.push(Span::new_text(""));
    }

    match last_step {
        PathStep::Root(idx) if prefix.is_empty() => {
            let insert_idx = (*idx + 1).min(document.paragraphs.len());
            let mut new_paragraph = Paragraph::new_text();
            new_paragraph.content = right_spans;
            document.paragraphs.insert(insert_idx, new_paragraph);

            let new_path = ParagraphPath::from_steps(vec![PathStep::Root(insert_idx)]);
            let span_path = SpanPath::new(vec![0]);
            Some(CursorPointer {
                paragraph_path: new_path,
                span_path,
                offset: 0,
                segment_kind: SegmentKind::Text,
            })
        }
        PathStep::Child(child_idx) => {
            let parent_path = ParagraphPath::from_steps(prefix.to_vec());
            let parent = paragraph_mut(document, &parent_path)?;
            let insert_idx = (*child_idx + 1).min(parent.children.len());
            let mut new_paragraph = Paragraph::new_text();
            new_paragraph.content = right_spans;
            parent.children.insert(insert_idx, new_paragraph);

            let mut new_steps = prefix.to_vec();
            new_steps.push(PathStep::Child(insert_idx));
            let new_path = ParagraphPath::from_steps(new_steps);
            let span_path = SpanPath::new(vec![0]);
            Some(CursorPointer {
                paragraph_path: new_path,
                span_path,
                offset: 0,
                segment_kind: SegmentKind::Text,
            })
        }
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => {
            let parent_path = ParagraphPath::from_steps(prefix.to_vec());
            let parent = paragraph_mut(document, &parent_path)?;

            if prefer_entry_sibling
                && matches!(
                    parent.paragraph_type,
                    ParagraphType::OrderedList | ParagraphType::UnorderedList
                )
            {
                let entry = parent.entries.get_mut(*entry_index)?;
                let insert_idx = (*paragraph_index + 1).min(entry.len());
                let mut new_paragraph = Paragraph::new_text();
                new_paragraph.content = right_spans;
                entry.insert(insert_idx, new_paragraph);

                let mut new_steps = prefix.to_vec();
                new_steps.push(PathStep::Entry {
                    entry_index: *entry_index,
                    paragraph_index: insert_idx,
                });
                let new_path = ParagraphPath::from_steps(new_steps);
                let span_path = SpanPath::new(vec![0]);
                return Some(CursorPointer {
                    paragraph_path: new_path,
                    span_path,
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                });
            }

            let insert_idx = (*entry_index + 1).min(parent.entries.len());

            let new_entry = {
                let entry = parent.entries.get_mut(*entry_index)?;
                if *paragraph_index >= entry.len() {
                    return None;
                }

                let mut trailing = entry.split_off(*paragraph_index + 1);

                // Checklists use PathStep::ChecklistItem, not PathStep::Entry
                // So this should only handle OrderedList and UnorderedList
                let mut head = Paragraph::new_text();
                head.content = right_spans;
                if head.content.is_empty() {
                    head.content.push(Span::new_text(""));
                }

                if paragraph_is_empty(&entry[*paragraph_index]) && entry.len() > 1 {
                    entry.remove(*paragraph_index);
                } else if entry[*paragraph_index].content.is_empty() {
                    entry[*paragraph_index].content.push(Span::new_text(""));
                }

                let mut assembled = vec![head];
                assembled.append(&mut trailing);
                assembled
            };

            parent.entries.insert(insert_idx, new_entry);

            let mut new_steps = prefix.to_vec();
            new_steps.push(PathStep::Entry {
                entry_index: insert_idx,
                paragraph_index: 0,
            });
            let new_path = ParagraphPath::from_steps(new_steps);
            let span_path = SpanPath::new(vec![0]);
            Some(CursorPointer {
                paragraph_path: new_path,
                span_path,
                offset: 0,
                segment_kind: SegmentKind::Text,
            })
        }
        PathStep::ChecklistItem { indices } => {
            let item_index = *indices.first()?;
            let parent_path = ParagraphPath::from_steps(prefix.to_vec());
            let parent = paragraph_mut(document, &parent_path)?;

            if parent.paragraph_type != ParagraphType::Checklist {
                return None;
            }

            if item_index >= parent.checklist_items.len() {
                return None;
            }

            // Ensure the current item has content
            if parent.checklist_items[item_index].content.is_empty() {
                parent.checklist_items[item_index].content.push(Span::new_text(""));
            }

            // Insert a new checklist item after the current one
            let insert_idx = (item_index + 1).min(parent.checklist_items.len());

            let new_item = ChecklistItem::new(false).with_content(right_spans);
            parent.checklist_items.insert(insert_idx, new_item);

            let mut new_steps = prefix.to_vec();
            new_steps.push(PathStep::ChecklistItem {
                indices: vec![insert_idx],
            });
            let new_path = ParagraphPath::from_steps(new_steps);
            let span_path = SpanPath::new(vec![0]);
            Some(CursorPointer {
                paragraph_path: new_path,
                span_path,
                offset: 0,
                segment_kind: SegmentKind::Text,
            })
        }
        _ => None,
    }
}

fn parent_paragraph_path(path: &ParagraphPath) -> Option<ParagraphPath> {
    let steps = path.steps();
    if steps.len() <= 1 {
        return None;
    }
    Some(ParagraphPath::from_steps(steps[..steps.len() - 1].to_vec()))
}

fn previous_sibling_path(path: &ParagraphPath) -> Option<ParagraphPath> {
    let steps = path.steps();
    if steps.is_empty() {
        return None;
    }
    let (last_step, prefix) = steps.split_last()?;
    match *last_step {
        PathStep::Root(idx) if prefix.is_empty() && idx > 0 => {
            Some(ParagraphPath::from_steps(vec![PathStep::Root(idx - 1)]))
        }
        PathStep::Child(idx) if idx > 0 => {
            let mut new_steps = prefix.to_vec();
            new_steps.push(PathStep::Child(idx - 1));
            Some(ParagraphPath::from_steps(new_steps))
        }
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } if paragraph_index > 0 => {
            let mut new_steps = prefix.to_vec();
            new_steps.push(PathStep::Entry {
                entry_index,
                paragraph_index: paragraph_index - 1,
            });
            Some(ParagraphPath::from_steps(new_steps))
        }
        PathStep::Entry {
            entry_index,
            paragraph_index: 0,
        } if entry_index > 0 => {
            let mut new_steps = prefix.to_vec();
            new_steps.push(PathStep::Entry {
                entry_index: entry_index - 1,
                paragraph_index: 0,
            });
            Some(ParagraphPath::from_steps(new_steps))
        }
        _ => None,
    }
}

#[derive(Clone)]
struct IndentTarget {
    path: ParagraphPath,
    kind: IndentTargetKind,
}

#[derive(Clone, Copy)]
enum IndentTargetKind {
    Quote,
    List,
    ListEntry { entry_index: usize },
}

fn find_indent_target(document: &Document, path: &ParagraphPath) -> Option<IndentTarget> {
    let prev_path = previous_sibling_path(path)?;
    let target = determine_indent_target(document, &prev_path)?;
    if let IndentTargetKind::ListEntry { entry_index } = target.kind {
        if let Some(ctx) = extract_entry_context(path) {
            if ctx.list_path == target.path && ctx.entry_index == entry_index {
                return None;
            }
        }
    }
    Some(target)
}

fn determine_indent_target(document: &Document, path: &ParagraphPath) -> Option<IndentTarget> {
    let paragraph = paragraph_ref(document, path)?;
    if paragraph.paragraph_type == ParagraphType::Quote {
        return Some(IndentTarget {
            path: path.clone(),
            kind: IndentTargetKind::Quote,
        });
    }
    if is_list_type(paragraph.paragraph_type) {
        return Some(IndentTarget {
            path: path.clone(),
            kind: IndentTargetKind::List,
        });
    }
    if let Some(ctx) = extract_entry_context(path) {
        return Some(IndentTarget {
            path: ctx.list_path.clone(),
            kind: IndentTargetKind::ListEntry {
                entry_index: ctx.entry_index,
            },
        });
    }
    None
}

fn append_paragraph_to_quote(
    document: &mut Document,
    path: &ParagraphPath,
    paragraph: Paragraph,
) -> Option<ParagraphPath> {
    let quote = paragraph_mut(document, path)?;
    let child_index = quote.children.len();
    quote.children.push(paragraph);
    let mut steps = path.steps().to_vec();
    steps.push(PathStep::Child(child_index));
    Some(ParagraphPath::from_steps(steps))
}

fn append_paragraph_to_list(
    document: &mut Document,
    path: &ParagraphPath,
    paragraph: Paragraph,
) -> Option<ParagraphPath> {
    let list = paragraph_mut(document, path)?;
    let entry_index = list.entries.len();
    let (entry, paragraph_index) = match list.paragraph_type {
        ParagraphType::Checklist => convert_paragraph_to_checklist_entry(paragraph),
        _ => (vec![paragraph], 0),
    };
    list.entries.push(entry);
    let mut steps = path.steps().to_vec();
    steps.push(PathStep::Entry {
        entry_index,
        paragraph_index,
    });
    Some(ParagraphPath::from_steps(steps))
}

fn list_entry_append_target(document: &Document, path: &ParagraphPath) -> Option<usize> {
    let list = paragraph_ref(document, path)?;
    if list.entries.is_empty() {
        return None;
    }
    let entry_index = list.entries.len() - 1;
    let entry = list.entries.get(entry_index)?;
    let last_paragraph = entry.last()?;
    if matches!(
        last_paragraph.paragraph_type,
        ParagraphType::Quote
            | ParagraphType::OrderedList
            | ParagraphType::UnorderedList
            | ParagraphType::Checklist
    ) {
        return None;
    }
    Some(entry_index)
}

fn append_paragraph_to_entry(
    document: &mut Document,
    list_path: &ParagraphPath,
    entry_index: usize,
    paragraph: Paragraph,
) -> Option<ParagraphPath> {
    let list = paragraph_mut(document, list_path)?;
    if entry_index >= list.entries.len() {
        return None;
    }
    let entry = &mut list.entries[entry_index];
    entry.push(paragraph);
    let paragraph_index = entry.len() - 1;
    let mut steps = list_path.steps().to_vec();
    steps.push(PathStep::Entry {
        entry_index,
        paragraph_index,
    });
    Some(ParagraphPath::from_steps(steps))
}

fn entry_has_multiple_paragraphs(document: &Document, ctx: &EntryContext) -> bool {
    paragraph_ref(document, &ctx.list_path)
        .and_then(|list| list.entries.get(ctx.entry_index))
        .map(|entry| entry.len() > 1)
        .unwrap_or(false)
}

fn ensure_nested_list(entry: &mut Vec<Paragraph>, list_type: ParagraphType) -> usize {
    if let Some(idx) = entry.iter().position(|p| p.paragraph_type == list_type) {
        idx
    } else {
        entry.push(Paragraph::new(list_type));
        entry.len() - 1
    }
}

fn indent_paragraph_within_entry(
    document: &mut Document,
    pointer: &CursorPointer,
    ctx: &EntryContext,
) -> Option<CursorPointer> {
    let list_type = paragraph_ref(document, &ctx.list_path)
        .map(|p| p.paragraph_type)
        .filter(|kind| is_list_type(*kind))?;

    let paragraph = {
        let list = paragraph_mut(document, &ctx.list_path)?;
        if ctx.entry_index >= list.entries.len() {
            return None;
        }
        let entry = &mut list.entries[ctx.entry_index];
        if ctx.paragraph_index >= entry.len() || entry.len() <= 1 {
            return None;
        }
        entry.remove(ctx.paragraph_index)
    };

    let mut nested_list = Paragraph::new(list_type);
    nested_list.content.clear();
    // checklist_item_checked field removed - checklists use checklist_items now

    let (nested_entry, nested_paragraph_index) = match list_type {
        ParagraphType::Checklist => convert_paragraph_to_checklist_entry(paragraph),
        ParagraphType::OrderedList | ParagraphType::UnorderedList => (vec![paragraph], 0),
        _ => return None,
    };

    nested_list.entries.push(nested_entry);

    {
        let list = paragraph_mut(document, &ctx.list_path)?;
        let entry = list.entries.get_mut(ctx.entry_index)?;
        entry.insert(ctx.paragraph_index, nested_list);
    }

    let mut steps = ctx.list_path.steps().to_vec();
    steps.push(PathStep::Entry {
        entry_index: ctx.entry_index,
        paragraph_index: ctx.paragraph_index,
    });
    steps.push(PathStep::Entry {
        entry_index: 0,
        paragraph_index: nested_paragraph_index,
    });
    steps.extend(ctx.tail_steps.iter().cloned());

    Some(CursorPointer {
        paragraph_path: ParagraphPath::from_steps(steps),
        span_path: pointer.span_path.clone(),
        offset: pointer.offset,
        segment_kind: pointer.segment_kind,
    })
}

fn indent_list_entry_into_entry(
    document: &mut Document,
    pointer: &CursorPointer,
    ctx: &EntryContext,
    target_entry_index: usize,
) -> Option<CursorPointer> {
    if target_entry_index >= ctx.entry_index {
        return None;
    }

    let list_type = paragraph_ref(document, &ctx.list_path)
        .map(|p| p.paragraph_type)
        .filter(|kind| is_list_type(*kind))?;

    let entry = {
        let list = paragraph_mut(document, &ctx.list_path)?;
        if ctx.entry_index >= list.entries.len() {
            return None;
        }
        list.entries.remove(ctx.entry_index)
    };

    if entry.is_empty() {
        return None;
    }

    let entry_len = entry.len();

    let paragraph_path = {
        let list = paragraph_mut(document, &ctx.list_path)?;
        if target_entry_index >= list.entries.len() {
            return None;
        }
        let target_entry = &mut list.entries[target_entry_index];

        let has_matching_nested_list = target_entry
            .iter()
            .any(|paragraph| paragraph.paragraph_type == list_type);
        let should_use_nested_list = has_matching_nested_list || target_entry.len() == 1;

        if should_use_nested_list {
            let nested_index = ensure_nested_list(target_entry, list_type);
            let nested_list = target_entry.get_mut(nested_index)?;
            nested_list.content.clear();
            // checklist_item_checked field removed - checklists use checklist_items now
            let new_entry_index = nested_list.entries.len();
            nested_list.entries.push(entry);

            let mut steps = ctx.list_path.steps().to_vec();
            steps.push(PathStep::Entry {
                entry_index: target_entry_index,
                paragraph_index: nested_index,
            });
            steps.push(PathStep::Entry {
                entry_index: new_entry_index,
                paragraph_index: ctx.paragraph_index.min(entry_len.saturating_sub(1)),
            });
            ParagraphPath::from_steps(steps)
        } else {
            let insert_index = target_entry.len();
            let relative_index = ctx.paragraph_index.min(entry_len.saturating_sub(1));
            let new_index = insert_index + relative_index;
            target_entry.extend(entry);

            let mut steps = ctx.list_path.steps().to_vec();
            steps.push(PathStep::Entry {
                entry_index: target_entry_index,
                paragraph_index: new_index,
            });
            ParagraphPath::from_steps(steps)
        }
    };

    Some(CursorPointer {
        paragraph_path,
        span_path: pointer.span_path.clone(),
        offset: pointer.offset,
        segment_kind: pointer.segment_kind,
    })
}

fn convert_paragraph_to_checklist_entry(paragraph: Paragraph) -> (Vec<Paragraph>, usize) {
    // Note: This function is using the old checklist API and needs refactoring
    // to work with the new checklist_items API. For now, just return the paragraph
    // wrapped in a vec to make it compile.
    let mut paragraph = paragraph;
    let mut content = mem::take(&mut paragraph.content);
    if content.is_empty() {
        content.push(Span::new_text(""));
    }
    let mut item = Paragraph::new_text();
    item.content = content;
    let mut entry = vec![item];
    if !paragraph.children.is_empty() || !paragraph.entries.is_empty() {
        entry.push(paragraph);
    }
    (entry, 0)
}

fn take_paragraph_at(document: &mut Document, path: &ParagraphPath) -> Option<Paragraph> {
    let mut steps = path.steps().to_vec();
    let last = steps.pop()?;
    match last {
        PathStep::Root(idx) => {
            if !steps.is_empty() {
                return None;
            }
            if idx < document.paragraphs.len() {
                Some(document.paragraphs.remove(idx))
            } else {
                None
            }
        }
        PathStep::Child(idx) => {
            let parent_path = ParagraphPath::from_steps(steps);
            let parent = paragraph_mut(document, &parent_path)?;
            if idx < parent.children.len() {
                Some(parent.children.remove(idx))
            } else {
                None
            }
        }
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => {
            let parent_path = ParagraphPath::from_steps(steps);
            let parent = paragraph_mut(document, &parent_path)?;
            if entry_index >= parent.entries.len() {
                return None;
            }
            let entry = &mut parent.entries[entry_index];
            if paragraph_index >= entry.len() {
                return None;
            }
            let removed = entry.remove(paragraph_index);
            if entry.is_empty() {
                parent.entries.remove(entry_index);
            }
            // ensure_entries_have_checklist_items removed - checklists use checklist_items now
            if is_list_type(parent.paragraph_type) && parent.entries.is_empty() {
                parent.entries.push(vec![empty_text_paragraph()]);
            }
            Some(removed)
        }
        PathStep::ChecklistItem { .. } => {
            // TODO: Implement checklist item removal and return as paragraph
            None
        }
    }
}

fn insert_paragraph_after_parent(
    document: &mut Document,
    parent_path: &ParagraphPath,
    paragraph: Paragraph,
) -> Option<ParagraphPath> {
    let steps = parent_path.steps();
    let (last_step, prefix) = steps.split_last()?;
    match *last_step {
        PathStep::Root(idx) if prefix.is_empty() => {
            let insert_idx = (idx + 1).min(document.paragraphs.len());
            document.paragraphs.insert(insert_idx, paragraph);
            Some(ParagraphPath::from_steps(vec![PathStep::Root(insert_idx)]))
        }
        PathStep::Child(child_idx) => {
            let host_path = ParagraphPath::from_steps(prefix.to_vec());
            let host = paragraph_mut(document, &host_path)?;
            let insert_idx = (child_idx + 1).min(host.children.len());
            host.children.insert(insert_idx, paragraph);
            let mut new_steps = prefix.to_vec();
            new_steps.push(PathStep::Child(insert_idx));
            Some(ParagraphPath::from_steps(new_steps))
        }
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => {
            let host_path = ParagraphPath::from_steps(prefix.to_vec());
            let host = paragraph_mut(document, &host_path)?;
            if entry_index >= host.entries.len() {
                return None;
            }
            let entry = &mut host.entries[entry_index];
            let insert_idx = (paragraph_index + 1).min(entry.len());
            entry.insert(insert_idx, paragraph);
            let mut new_steps = prefix.to_vec();
            new_steps.push(PathStep::Entry {
                entry_index,
                paragraph_index: insert_idx,
            });
            Some(ParagraphPath::from_steps(new_steps))
        }
        _ => None,
    }
}

fn remove_paragraph_by_path(document: &mut Document, path: &ParagraphPath) -> bool {
    let mut steps = path.steps().to_vec();
    let last = match steps.pop() {
        Some(step) => step,
        None => return false,
    };

    match last {
        PathStep::Root(idx) => {
            if !steps.is_empty() {
                return false;
            }
            if idx < document.paragraphs.len() {
                document.paragraphs.remove(idx);
                true
            } else {
                false
            }
        }
        PathStep::Child(idx) => {
            let parent_path = ParagraphPath::from_steps(steps);
            if let Some(parent) = paragraph_mut(document, &parent_path) {
                if idx < parent.children.len() {
                    parent.children.remove(idx);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        }
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => {
            let parent_path = ParagraphPath::from_steps(steps);
            let Some(parent) = paragraph_mut(document, &parent_path) else {
                return false;
            };
            if entry_index >= parent.entries.len() {
                return false;
            }
            let entry = &mut parent.entries[entry_index];
            if paragraph_index >= entry.len() {
                return false;
            }
            entry.remove(paragraph_index);
            if entry.is_empty() {
                parent.entries.remove(entry_index);
            }
            true
        }
        PathStep::ChecklistItem { .. } => {
            // TODO: Implement checklist item removal
            false
        }
    }
}

pub(crate) fn paragraph_mut<'a>(
    document: &'a mut Document,
    path: &ParagraphPath,
) -> Option<&'a mut Paragraph> {
    let mut iter = path.steps().iter();
    let first = iter.next()?;
    let mut paragraph = match first {
        PathStep::Root(idx) => document.paragraphs.get_mut(*idx)?,
        _ => return None,
    };
    for step in iter {
        paragraph = match step {
            PathStep::Child(idx) => match paragraph {
                Paragraph::Quote { children } => children.get_mut(*idx)?,
                _ => return None,
            },
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => match paragraph {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    let entry = entries.get_mut(*entry_index)?;
                    entry.get_mut(*paragraph_index)?
                }
                _ => return None,
            },
            PathStep::ChecklistItem { .. } => return None,
            PathStep::Root(_) => return None,
        };
    }
    Some(paragraph)
}

pub(crate) fn checklist_item_mut<'a>(document: &'a mut Document, path: &ParagraphPath) -> Option<&'a mut ChecklistItem> {
    let steps = path.steps();
    let (checklist_step_idx, checklist_step) = steps
        .iter()
        .enumerate()
        .find(|(_, s)| matches!(s, PathStep::ChecklistItem { .. }))?;

    let PathStep::ChecklistItem { indices } = checklist_step else {
        return None;
    };

    let paragraph_path = ParagraphPath::from_steps(steps[..checklist_step_idx].to_vec());
    let paragraph = paragraph_mut(document, &paragraph_path)?;
    let items = match paragraph {
        Paragraph::Checklist { items } => items,
        _ => return None,
    };

    let mut item: &mut ChecklistItem = items.get_mut(*indices.first()?)?;
    for &idx in &indices[1..] {
        item = item.children.get_mut(idx)?;
    }
    Some(item)
}

pub(crate) fn span_mut<'a>(paragraph: &'a mut Paragraph, path: &SpanPath) -> Option<&'a mut Span> {
    let mut iter = path.indices().iter();
    let first = iter.next()?;
    let mut span = paragraph.content_mut().get_mut(*first)?;
    for idx in iter {
        span = span.children.get_mut(*idx)?;
    }
    Some(span)
}

pub(crate) fn span_mut_from_item<'a>(item: &'a mut ChecklistItem, path: &SpanPath) -> Option<&'a mut Span> {
    let mut iter = path.indices().iter();
    let first = iter.next()?;
    let mut span = item.content.get_mut(*first)?;
    for idx in iter {
        span = span.children.get_mut(*idx)?;
    }
    Some(span)
}


fn paragraph_is_empty(paragraph: &Paragraph) -> bool {
    let content_empty = if paragraph.paragraph_type().is_leaf() {
        paragraph
            .content()
            .iter()
            .all(content_span_is_empty)
    } else {
        true
    };

    let children_empty = match paragraph {
        Paragraph::Quote { children } => children.iter().all(paragraph_is_empty),
        _ => true,
    };

    let entries_empty = match paragraph {
        Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries
            .iter()
            .all(|entry| entry.iter().all(paragraph_is_empty)),
        _ => true,
    };

    let checklist_empty = match paragraph {
        Paragraph::Checklist { items } => items.iter().all(checklist_item_is_empty),
        _ => true,
    };

    content_empty && children_empty && entries_empty && checklist_empty
}

#[cfg(test)]
#[path = "editor_tests.rs"]
mod editor_tests;

#[cfg(test)]
#[path = "cursor_tests.rs"]
mod cursor_tests;

#[cfg(test)]
#[path = "content_tests.rs"]
mod content_tests;
