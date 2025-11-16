const MARKER_POINTER_PREFIX: &str = "1337;M";
const MARKER_REVEAL_PREFIX: &str = "1337;R";
use std::{cmp::Ordering, mem};
use tdoc::{ChecklistItem, Document, InlineStyle, Paragraph, ParagraphType, Span};

use content::{
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
mod styles;
mod structure;

pub(crate) use styles::inline_style_label;

use inspect::{checklist_item_ref, paragraph_ref};
use structure::{
    ensure_document_initialized,
    paragraph_mut,
    checklist_item_mut,
    span_mut,
    span_mut_from_item,
    paragraph_is_empty,
    is_single_paragraph_entry,
    ParentScope,
    ParentRelation,
    determine_parent_scope,
    promote_single_child_into_parent,
    apply_paragraph_type_in_place,
    break_list_entry_for_non_list_target,
    EntryContext,
    extract_entry_context,
    merge_adjacent_lists,
    is_list_type,
    find_list_ancestor_path,
    update_existing_list_type,
    convert_paragraph_into_list,
    update_paragraph_type,
    split_paragraph_break,
    parent_paragraph_path,
    IndentTarget,
    IndentTargetKind,
    find_indent_target,
    append_paragraph_to_quote,
    append_paragraph_to_list,
    list_entry_append_target,
    append_paragraph_to_entry,
    entry_has_multiple_paragraphs,
    indent_paragraph_within_entry,
    indent_list_entry_into_entry,
    take_paragraph_at,
    insert_paragraph_after_parent,
    remove_paragraph_by_path,
};

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

    pub fn reveal_codes(&self) -> bool {
        self.reveal_codes
    }

    pub fn set_reveal_codes(&mut self, enabled: bool) {
        self.reveal_codes = enabled;
        self.rebuild_segments();
    }

    pub fn clone_with_markers(
        &self,
        cursor_sentinel: char,
        selection: Option<(CursorPointer, CursorPointer)>,
        selection_start_sentinel: char,
        selection_end_sentinel: char,
    ) -> (Document, Vec<MarkerRef>, Vec<RevealTagRef>, bool) {
        // TODO: Implement proper clone_with_markers functionality
        // This is a stub implementation for now
        (self.document.clone(), Vec::new(), Vec::new(), false)
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
            .map(|p| p.paragraph_type())
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

    pub fn insert_paragraph_break(&mut self) -> bool {
        if !self.cursor.is_valid() {
            return false;
        }
        let pointer = self.cursor.clone();
        if let Some(new_pointer) = split_paragraph_break(&mut self.document, &pointer, false) {
            self.cursor = new_pointer;
            self.rebuild_segments();
            true
        } else {
            false
        }
    }

    pub fn insert_paragraph_break_as_sibling(&mut self) -> bool {
        if !self.cursor.is_valid() {
            return false;
        }
        let pointer = self.cursor.clone();
        if let Some(new_pointer) = split_paragraph_break(&mut self.document, &pointer, true) {
            self.cursor = new_pointer;
            self.rebuild_segments();
            true
        } else {
            false
        }
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


    pub fn compare_pointers(&self, a: &CursorPointer, b: &CursorPointer) -> Option<Ordering> {
        let key_a = self.pointer_key(a)?;
        let key_b = self.pointer_key(b)?;
        Some(key_a.cmp(&key_b))
    }
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

// The rest of the structure mutation functions have been moved to structure.rs

#[cfg(test)]
#[path = "editor_tests.rs"]
mod editor_tests;

#[cfg(test)]
#[path = "editor/cursor_tests.rs"]
mod cursor_tests;

#[cfg(test)]
#[path = "editor/content_tests.rs"]
mod content_tests;

#[cfg(test)]
#[path = "editor/style_tests.rs"]
mod style_tests;
