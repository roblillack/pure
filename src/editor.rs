const MARKER_POINTER_PREFIX: &str = "1337;M";
const MARKER_REVEAL_PREFIX: &str = "1337;R";
use std::{cmp::Ordering, mem};
use tdoc::{Document, InlineStyle, Paragraph, ParagraphType, Span};

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

    pub fn ensure_cursor_selectable(&mut self) {
        if self.segments.is_empty() {
            self.ensure_placeholder_segment();
        }
        if let Some(first) = self.segments.first() {
            self.cursor = CursorPointer {
                paragraph_path: first.paragraph_path.clone(),
                span_path: first.span_path.clone(),
                offset: self.cursor.offset.min(first.len),
                segment_kind: first.kind,
            };
            self.cursor_segment = 0;
        } else {
            self.cursor = CursorPointer::default();
            self.cursor_segment = 0;
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn current_checklist_item_state(&self) -> Option<bool> {
        let paragraph = paragraph_ref(&self.document, &self.cursor.paragraph_path)?;
        if paragraph.paragraph_type == ParagraphType::ChecklistItem {
            Some(paragraph.checklist_item_checked.unwrap_or(false))
        } else {
            None
        }
    }

    pub fn set_current_checklist_item_checked(&mut self, checked: bool) -> bool {
        let Some(paragraph) = paragraph_mut(&mut self.document, &self.cursor.paragraph_path) else {
            return false;
        };
        if paragraph.paragraph_type != ParagraphType::ChecklistItem {
            return false;
        }
        let previous = paragraph.checklist_item_checked;
        paragraph.checklist_item_checked = Some(checked);
        match previous {
            Some(value) => value != checked,
            None => true,
        }
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

    pub fn cursor_pointer(&self) -> CursorPointer {
        self.cursor.clone()
    }

    pub fn cursor_stable_pointer(&self) -> CursorPointer {
        self.stable_pointer(&self.cursor)
    }

    pub fn stable_pointer(&self, pointer: &CursorPointer) -> CursorPointer {
        if self.segments.is_empty() || pointer.segment_kind == SegmentKind::Text {
            return pointer.clone();
        }
        self.nearest_text_pointer_for(pointer)
            .unwrap_or_else(|| pointer.clone())
    }

    pub fn word_boundaries_at(
        &self,
        pointer: &CursorPointer,
    ) -> Option<(CursorPointer, CursorPointer)> {
        let segment = self
            .segments
            .iter()
            .find(|segment| segment.matches_pointer(pointer))?;
        if segment.kind != SegmentKind::Text {
            return None;
        }
        let text = self.span_text_for_pointer(pointer)?;
        if text.is_empty() {
            return None;
        }
        let len = text.chars().count();
        let offset = pointer.offset.min(len);
        let mut start = pointer.clone();
        start.offset = previous_word_boundary(text, offset);
        let mut end = pointer.clone();
        end.offset = next_word_boundary(text, offset);
        Some((start, end))
    }

    fn span_text_for_pointer<'a>(&'a self, pointer: &CursorPointer) -> Option<&'a str> {
        let paragraph = paragraph_ref(&self.document, &pointer.paragraph_path)?;
        let span = span_ref(paragraph, &pointer.span_path)?;
        Some(span.text.as_str())
    }

    pub fn reveal_codes(&self) -> bool {
        self.reveal_codes
    }

    pub fn set_reveal_codes(&mut self, enabled: bool) {
        if self.reveal_codes != enabled {
            self.reveal_codes = enabled;
            self.rebuild_segments();
            self.ensure_cursor_selectable();
        }
    }

    pub fn cursor_breadcrumbs(&self) -> Option<Vec<String>> {
        breadcrumbs_for_pointer(&self.document, &self.cursor)
    }

    pub fn move_to_pointer(&mut self, pointer: &CursorPointer) -> bool {
        if let Some(index) = self
            .segments
            .iter()
            .position(|segment| segment.matches_pointer(pointer))
        {
            let mut new_pointer = pointer.clone();
            let len = self.segments[index].len;
            if new_pointer.offset > len {
                new_pointer.offset = len;
            }
            self.cursor = new_pointer;
            self.cursor_segment = index;
            true
        } else {
            false
        }
    }

    pub fn move_left(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        let mut crossed_boundary = false;
        if self.cursor.offset > 0 {
            self.cursor.offset -= 1;
        } else if self.shift_to_previous_segment() {
            crossed_boundary = true;
        } else {
            return false;
        }
        self.normalize_cursor_after_backward_move(crossed_boundary);
        true
    }

    pub fn move_right(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        if self.cursor.offset < self.current_segment_len() {
            self.cursor.offset += 1;
        } else {
            if !self.shift_to_next_segment() {
                return false;
            }
            self.skip_forward_reveal_segments();
        }
        self.normalize_cursor_after_forward_move();
        true
    }

    pub fn move_up(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        let preferred_offset = self.cursor.offset;
        let Some(target_path) = self.previous_paragraph_path() else {
            return false;
        };
        self.move_to_paragraph_path(&target_path, true, preferred_offset)
    }

    pub fn move_down(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        let preferred_offset = self.cursor.offset;
        let Some(target_path) = self.next_paragraph_path() else {
            return false;
        };
        self.move_to_paragraph_path(&target_path, false, preferred_offset)
    }

    fn previous_paragraph_path(&self) -> Option<ParagraphPath> {
        if self.segments.is_empty() {
            return None;
        }
        let current_path = &self.cursor.paragraph_path;
        let mut idx = self.cursor_segment;
        while idx > 0 {
            idx -= 1;
            let segment = &self.segments[idx];
            if segment.paragraph_path != *current_path {
                return Some(segment.paragraph_path.clone());
            }
        }
        None
    }

    fn next_paragraph_path(&self) -> Option<ParagraphPath> {
        if self.segments.is_empty() {
            return None;
        }
        let current_path = &self.cursor.paragraph_path;
        let mut idx = self.cursor_segment + 1;
        while idx < self.segments.len() {
            let segment = &self.segments[idx];
            if segment.paragraph_path != *current_path {
                return Some(segment.paragraph_path.clone());
            }
            idx += 1;
        }
        None
    }

    fn move_to_paragraph_path(
        &mut self,
        paragraph_path: &ParagraphPath,
        prefer_trailing: bool,
        preferred_offset: usize,
    ) -> bool {
        if let Some((index, segment)) =
            select_text_in_paragraph(&self.segments, paragraph_path, prefer_trailing)
        {
            self.cursor_segment = index;
            self.cursor = CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: preferred_offset.min(segment.len),
                segment_kind: SegmentKind::Text,
            };
            self.clamp_cursor_offset();
            return true;
        }

        if let Some((index, segment)) = self
            .segments
            .iter()
            .enumerate()
            .find(|(_, segment)| segment.paragraph_path == *paragraph_path)
        {
            self.cursor_segment = index;
            self.cursor = CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: preferred_offset.min(segment.len),
                segment_kind: segment.kind,
            };
            self.clamp_cursor_offset();
            return true;
        }

        false
    }

    pub fn move_word_left(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }

        if let Some(text) = self.current_span_text() {
            let current_offset = self.cursor.offset.min(text.chars().count());
            let new_offset = previous_word_boundary(text, current_offset);
            if new_offset < current_offset {
                self.cursor.offset = new_offset;
                return true;
            }
        }

        while self.shift_to_previous_segment() {
            let len = self.current_segment_len();
            self.cursor.offset = len;
            if len == 0 {
                continue;
            }
            if let Some(text) = self.current_span_text() {
                let new_offset = previous_word_boundary(text, len);
                self.cursor.offset = new_offset;
            }
            return true;
        }

        false
    }

    pub fn move_word_right(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }

        if let Some(text) = self.current_span_text() {
            let len = text.chars().count();
            let current_offset = self.cursor.offset.min(len);
            let new_offset = next_word_boundary(text, current_offset);
            if new_offset > current_offset {
                self.cursor.offset = new_offset;
                return true;
            }
        }

        while self.shift_to_next_segment() {
            let len = self.current_segment_len();
            self.cursor.offset = 0;
            if len == 0 {
                continue;
            }
            if let Some(text) = self.current_span_text() {
                let new_offset = skip_leading_whitespace(text).min(len);
                self.cursor.offset = new_offset;
            }
            return true;
        }

        false
    }

    pub fn move_to_segment_start(&mut self) {
        self.cursor.offset = 0;
    }

    pub fn move_to_segment_end(&mut self) {
        self.cursor.offset = self.current_segment_len();
    }

    pub fn set_paragraph_type(&mut self, target: ParagraphType) -> bool {
        let current_pointer = self.cursor.clone();
        let mut replacement_pointer = None;

        let mut operation_path = current_pointer.paragraph_path.clone();
        let mut pointer_hint = None;
        let mut post_merge_pointer = None;
        let mut handled_directly = false;

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

        if !is_list_type(target) {
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
                        steps.push(PathStep::Entry {
                            entry_index: merged_entry_idx,
                            paragraph_index: ctx.paragraph_index,
                        });
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
        prune_and_merge_spans(&mut paragraph.content);
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

    fn fallback_move_to_text(&mut self, pointer: &CursorPointer, prefer_trailing: bool) -> bool {
        if let Some((index, segment)) = self.segments.iter().enumerate().find(|(_, segment)| {
            segment.paragraph_path == pointer.paragraph_path
                && segment.span_path == pointer.span_path
                && segment.kind == SegmentKind::Text
        }) {
            self.cursor_segment = index;
            self.cursor = CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: pointer.offset.min(segment.len),
                segment_kind: SegmentKind::Text,
            };
            self.clamp_cursor_offset();
            return true;
        }

        let mut descendant_match: Option<(usize, SegmentRef)> = None;
        for (index, segment) in self.segments.iter().enumerate() {
            if segment.paragraph_path != pointer.paragraph_path {
                continue;
            }
            if segment.kind != SegmentKind::Text {
                continue;
            }
            if !span_path_is_prefix(pointer.span_path.indices(), segment.span_path.indices()) {
                continue;
            }
            descendant_match = match descendant_match {
                None => Some((index, segment.clone())),
                Some((current_index, current_segment)) => {
                    if prefer_trailing {
                        Some((index, segment.clone()))
                    } else {
                        Some((current_index, current_segment))
                    }
                }
            };
            if !prefer_trailing {
                break;
            }
        }

        if let Some((index, segment)) = descendant_match {
            self.cursor_segment = index;
            self.cursor = CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: pointer.offset.min(segment.len),
                segment_kind: SegmentKind::Text,
            };
            self.clamp_cursor_offset();
            return true;
        }

        let mut nested_paragraph_match: Option<(usize, SegmentRef)> = None;
        for (index, segment) in self.segments.iter().enumerate() {
            if segment.kind != SegmentKind::Text {
                continue;
            }
            if segment.paragraph_path == pointer.paragraph_path {
                continue;
            }
            if !paragraph_path_is_prefix(&pointer.paragraph_path, &segment.paragraph_path) {
                continue;
            }
            nested_paragraph_match = match nested_paragraph_match {
                None => Some((index, segment.clone())),
                Some((current_index, current_segment)) => {
                    if prefer_trailing {
                        Some((index, segment.clone()))
                    } else {
                        Some((current_index, current_segment))
                    }
                }
            };
            if !prefer_trailing {
                break;
            }
        }

        if let Some((index, segment)) = nested_paragraph_match {
            self.cursor_segment = index;
            self.cursor = CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: pointer.offset.min(segment.len),
                segment_kind: SegmentKind::Text,
            };
            self.clamp_cursor_offset();
            return true;
        }

        if let Some((index, segment)) =
            select_text_in_paragraph(&self.segments, &pointer.paragraph_path, prefer_trailing)
        {
            self.cursor_segment = index;
            self.cursor = CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: pointer.offset.min(segment.len),
                segment_kind: SegmentKind::Text,
            };
            self.clamp_cursor_offset();
            return true;
        }

        false
    }

    fn insert_paragraph_break_internal(&mut self, prefer_entry_sibling: bool) -> bool {
        let pointer = self.cursor.clone();
        if !pointer.is_valid() {
            return false;
        }
        let prefer_entry_sibling = if prefer_entry_sibling {
            match paragraph_ref(&self.document, &pointer.paragraph_path) {
                Some(paragraph) if paragraph.paragraph_type == ParagraphType::ChecklistItem => {
                    false
                }
                Some(_) => true,
                None => false,
            }
        } else {
            false
        };
        let Some(new_pointer) =
            split_paragraph_break(&mut self.document, &pointer, prefer_entry_sibling)
        else {
            return false;
        };
        self.rebuild_segments();
        if !self.move_to_pointer(&new_pointer) {
            self.cursor = new_pointer;
        }
        true
    }

    pub fn insert_paragraph_break(&mut self) -> bool {
        self.insert_paragraph_break_internal(false)
    }

    pub fn insert_paragraph_break_as_sibling(&mut self) -> bool {
        self.insert_paragraph_break_internal(true)
    }

    pub fn clone_with_markers(
        &self,
        cursor_sentinel: char,
        selection: Option<(CursorPointer, CursorPointer)>,
        selection_start_sentinel: char,
        selection_end_sentinel: char,
    ) -> (Document, Vec<MarkerRef>, Vec<RevealTagRef>, bool) {
        #[derive(Clone)]
        struct SpanAssembly {
            paragraph_path: ParagraphPath,
            span_path: SpanPath,
            original_text: String,
            start: Option<String>,
            text: Option<String>,
            end: Option<String>,
        }

        let mut clone = self.document.clone();
        let mut markers = Vec::new();
        let mut reveal_tags = Vec::new();
        let mut inserted_cursor = false;

        let selection_bounds = selection.and_then(|(start_ptr, end_ptr)| {
            let start_key = self.pointer_key(&start_ptr)?;
            let end_key = self.pointer_key(&end_ptr)?;
            if start_key <= end_key {
                Some((start_key, end_key))
            } else {
                Some((end_key, start_key))
            }
        });

        let mut assemblies: Vec<SpanAssembly> = Vec::new();

        for (segment_index, segment) in self.segments.iter().enumerate() {
            let Some(paragraph) = paragraph_ref(&self.document, &segment.paragraph_path) else {
                continue;
            };
            let Some(span) = span_ref(paragraph, &segment.span_path) else {
                continue;
            };

            let assembly = if let Some(existing) = assemblies.iter_mut().find(|entry| {
                entry.paragraph_path == segment.paragraph_path
                    && entry.span_path == segment.span_path
            }) {
                existing
            } else {
                assemblies.push(SpanAssembly {
                    paragraph_path: segment.paragraph_path.clone(),
                    span_path: segment.span_path.clone(),
                    original_text: span.text.clone(),
                    start: None,
                    text: None,
                    end: None,
                });
                assemblies.last_mut().unwrap()
            };

            match segment.kind {
                SegmentKind::Text => {
                    let original_chars: Vec<char> = assembly.original_text.chars().collect();
                    let len = original_chars.len();
                    let mut rebuilt = String::new();

                    for offset in 0..=len {
                        let id = markers.len();
                        rebuilt.push_str(&format!("\x1b]{}{}\x1b\\", MARKER_POINTER_PREFIX, id));
                        markers.push(MarkerRef {
                            id,
                            pointer: CursorPointer {
                                paragraph_path: segment.paragraph_path.clone(),
                                span_path: segment.span_path.clone(),
                                offset,
                                segment_kind: SegmentKind::Text,
                            },
                        });

                        if let Some((start_key, end_key)) = selection_bounds {
                            let current_key = PointerKey {
                                segment_index,
                                offset,
                            };
                            if current_key == start_key {
                                rebuilt.push(selection_start_sentinel);
                            }
                            if current_key == end_key {
                                rebuilt.push(selection_end_sentinel);
                            }
                        }

                        if segment.matches(&self.cursor) && offset == self.cursor.offset {
                            rebuilt.push(cursor_sentinel);
                            inserted_cursor = true;
                        }

                        if offset < len {
                            rebuilt.push(original_chars[offset]);
                        }
                    }

                    assembly.text = Some(rebuilt);
                }
                SegmentKind::RevealStart(style) => {
                    let len = segment.len.max(1);
                    let mut rebuilt = String::new();
                    for offset in 0..=len {
                        let id = markers.len();
                        rebuilt.push_str(&format!("\x1b]{}{}\x1b\\", MARKER_POINTER_PREFIX, id));
                        markers.push(MarkerRef {
                            id,
                            pointer: CursorPointer {
                                paragraph_path: segment.paragraph_path.clone(),
                                span_path: segment.span_path.clone(),
                                offset,
                                segment_kind: SegmentKind::RevealStart(style),
                            },
                        });

                        if let Some((start_key, end_key)) = selection_bounds {
                            let current_key = PointerKey {
                                segment_index,
                                offset,
                            };
                            if current_key == start_key {
                                rebuilt.push(selection_start_sentinel);
                            }
                            if current_key == end_key {
                                rebuilt.push(selection_end_sentinel);
                            }
                        }

                        if segment.matches(&self.cursor) && offset == self.cursor.offset {
                            rebuilt.push(cursor_sentinel);
                            inserted_cursor = true;
                        }

                        if offset < len {
                            let tag_id = reveal_tags.len();
                            reveal_tags.push(RevealTagRef {
                                id: tag_id,
                                style,
                                kind: RevealTagKind::Start,
                            });
                            rebuilt.push_str(&format!(
                                "\x1b]{}{}\x1b\\",
                                MARKER_REVEAL_PREFIX, tag_id
                            ));
                        }
                    }
                    assembly.start = Some(rebuilt);
                }
                SegmentKind::RevealEnd(style) => {
                    let len = segment.len.max(1);
                    let mut rebuilt = String::new();
                    for offset in 0..=len {
                        let id = markers.len();
                        rebuilt.push_str(&format!("\x1b]{}{}\x1b\\", MARKER_POINTER_PREFIX, id));
                        markers.push(MarkerRef {
                            id,
                            pointer: CursorPointer {
                                paragraph_path: segment.paragraph_path.clone(),
                                span_path: segment.span_path.clone(),
                                offset,
                                segment_kind: SegmentKind::RevealEnd(style),
                            },
                        });

                        if let Some((start_key, end_key)) = selection_bounds {
                            let current_key = PointerKey {
                                segment_index,
                                offset,
                            };
                            if current_key == start_key {
                                rebuilt.push(selection_start_sentinel);
                            }
                            if current_key == end_key {
                                rebuilt.push(selection_end_sentinel);
                            }
                        }

                        if segment.matches(&self.cursor) && offset == self.cursor.offset {
                            rebuilt.push(cursor_sentinel);
                            inserted_cursor = true;
                        }

                        if offset < len {
                            let tag_id = reveal_tags.len();
                            reveal_tags.push(RevealTagRef {
                                id: tag_id,
                                style,
                                kind: RevealTagKind::End,
                            });
                            rebuilt.push_str(&format!(
                                "\x1b]{}{}\x1b\\",
                                MARKER_REVEAL_PREFIX, tag_id
                            ));
                        }
                    }
                    assembly.end = Some(rebuilt);
                }
            }
        }

        for assembly in assemblies.into_iter() {
            let Some(paragraph) = paragraph_mut(&mut clone, &assembly.paragraph_path) else {
                continue;
            };
            let Some(span) = span_mut(paragraph, &assembly.span_path) else {
                continue;
            };
            let mut combined = String::new();
            if let Some(start) = assembly.start {
                combined.push_str(&start);
            }
            if let Some(text) = assembly.text {
                combined.push_str(&text);
            } else {
                combined.push_str(&assembly.original_text);
            }
            if let Some(end) = assembly.end {
                combined.push_str(&end);
            }
            span.text = combined;
        }

        (clone, markers, reveal_tags, inserted_cursor)
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
                    prune_and_merge_spans(&mut paragraph.content);
                }
            }
            self.rebuild_segments();
        }

        changed
    }

    fn pointer_key(&self, pointer: &CursorPointer) -> Option<PointerKey> {
        for (index, segment) in self.segments.iter().enumerate() {
            if segment.matches_pointer(pointer) {
                let offset = pointer.offset.min(segment.len);
                return Some(PointerKey {
                    segment_index: index,
                    offset,
                });
            }
        }
        None
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

    fn shift_to_previous_segment(&mut self) -> bool {
        if self.cursor_segment == 0 || self.segments.is_empty() {
            return false;
        }
        self.cursor_segment -= 1;
        if let Some(segment) = self.segments.get(self.cursor_segment).cloned() {
            self.cursor.update_from_segment(&segment);
            self.cursor.offset = segment.len;
            true
        } else {
            false
        }
    }

    fn shift_to_next_segment(&mut self) -> bool {
        if self.cursor_segment + 1 >= self.segments.len() {
            return false;
        }
        self.cursor_segment += 1;
        if let Some(segment) = self.segments.get(self.cursor_segment).cloned() {
            self.cursor.update_from_segment(&segment);
            self.cursor.offset = 0;
            true
        } else {
            false
        }
    }

    fn skip_forward_reveal_segments(&mut self) {
        while matches!(
            self.current_segment_kind(),
            Some(SegmentKind::RevealStart(_) | SegmentKind::RevealEnd(_))
        ) {
            if !self.shift_to_next_segment() {
                break;
            }
        }
    }

    fn current_segment_len(&self) -> usize {
        self.segments
            .get(self.cursor_segment)
            .map(|segment| segment.len)
            .unwrap_or(0)
    }

    fn find_previous_text_segment_in_paragraph(&self, start_idx: usize) -> Option<usize> {
        if self.segments.is_empty() || start_idx == 0 {
            return None;
        }
        let paragraph_path = self.segments.get(start_idx)?.paragraph_path.clone();
        let mut idx = start_idx;
        while idx > 0 {
            idx -= 1;
            let segment = &self.segments[idx];
            if segment.paragraph_path != paragraph_path {
                continue;
            }
            if matches!(segment.kind, SegmentKind::Text) {
                return Some(idx);
            }
        }
        None
    }

    fn current_segment_kind(&self) -> Option<SegmentKind> {
        self.segments
            .get(self.cursor_segment)
            .map(|segment| segment.kind)
    }

    fn next_segment_kind(&self) -> Option<SegmentKind> {
        self.segments
            .get(self.cursor_segment + 1)
            .map(|segment| segment.kind)
    }

    fn normalize_cursor_after_forward_move(&mut self) {
        loop {
            let current_len = self.current_segment_len();
            if self.cursor.offset < current_len {
                break;
            }
            if matches!(
                self.current_segment_kind(),
                Some(SegmentKind::RevealStart(_) | SegmentKind::RevealEnd(_))
            ) {
                if !self.shift_to_next_segment() {
                    break;
                }
                continue;
            }
            match self.next_segment_kind() {
                Some(SegmentKind::RevealStart(_) | SegmentKind::RevealEnd(_)) => {
                    if !self.shift_to_next_segment() {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    fn normalize_cursor_after_backward_move(&mut self, crossed_boundary: bool) {
        let pending_boundary = crossed_boundary;
        loop {
            let Some(segment) = self.segments.get(self.cursor_segment) else {
                return;
            };
            match segment.kind {
                SegmentKind::RevealStart(_) | SegmentKind::RevealEnd(_) => {
                    if self.cursor.offset > 0 {
                        self.cursor.offset = 0;
                    }
                    return;
                }
                SegmentKind::Text => {
                    if pending_boundary {
                        if segment.len == 0 {
                            return;
                        }
                        if self.cursor.offset >= segment.len {
                            self.cursor.offset = segment.len.saturating_sub(1);
                        }
                    }
                    return;
                }
            }
        }
    }

    fn current_span_text(&self) -> Option<&str> {
        let segment = self.segments.get(self.cursor_segment)?;
        if segment.kind != SegmentKind::Text {
            return None;
        }
        let paragraph = paragraph_ref(&self.document, &self.cursor.paragraph_path)?;
        let span = span_ref(paragraph, &self.cursor.span_path)?;
        Some(span.text.as_str())
    }

    fn segment_text(&self, segment: &SegmentRef) -> Option<&str> {
        if segment.kind != SegmentKind::Text {
            return None;
        }
        let paragraph = paragraph_ref(&self.document, &segment.paragraph_path)?;
        let span = span_ref(paragraph, &segment.span_path)?;
        Some(span.text.as_str())
    }

    fn previous_word_position(&self) -> Option<(usize, CursorPointer)> {
        if self.segments.is_empty() {
            return None;
        }

        if let Some(text) = self.current_span_text() {
            let len = text.chars().count();
            let current_offset = self.cursor.offset.min(len);
            let new_offset = previous_word_boundary(text, current_offset);
            if new_offset < current_offset {
                let mut pointer = self.cursor.clone();
                pointer.offset = new_offset;
                return Some((self.cursor_segment, pointer));
            }
        }

        let mut idx = self.cursor_segment;
        while idx > 0 {
            idx -= 1;
            let segment = &self.segments[idx];
            let len = segment.len;
            if len == 0 {
                continue;
            }
            let Some(text) = self.segment_text(segment) else {
                continue;
            };
            let new_offset = previous_word_boundary(text, len);
            let pointer = CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: new_offset.min(len),
                segment_kind: segment.kind,
            };
            return Some((idx, pointer));
        }

        None
    }

    fn next_word_position(&self) -> Option<(usize, CursorPointer)> {
        if self.segments.is_empty() {
            return None;
        }

        if let Some(text) = self.current_span_text() {
            let len = text.chars().count();
            let current_offset = self.cursor.offset.min(len);
            let new_offset = next_word_boundary(text, current_offset);
            if new_offset > current_offset {
                let mut pointer = self.cursor.clone();
                pointer.offset = new_offset.min(len);
                return Some((self.cursor_segment, pointer));
            }
        }

        let mut idx = self.cursor_segment + 1;
        while idx < self.segments.len() {
            let segment = &self.segments[idx];
            let len = segment.len;
            if len == 0 {
                idx += 1;
                continue;
            }
            let Some(text) = self.segment_text(segment) else {
                idx += 1;
                continue;
            };
            let new_offset = next_word_boundary(text, 0).min(len);
            let pointer = CursorPointer {
                paragraph_path: segment.paragraph_path.clone(),
                span_path: segment.span_path.clone(),
                offset: new_offset,
                segment_kind: segment.kind,
            };
            return Some((idx, pointer));
        }

        None
    }

    fn count_backward_steps(&self, target_segment: usize, target_offset: usize) -> usize {
        if self.segments.is_empty() {
            return 0;
        }

        if self.cursor_segment == target_segment {
            let len = self.current_segment_len();
            let clamped_target = target_offset.min(len);
            return self.cursor.offset.min(len).saturating_sub(clamped_target);
        }

        if self.cursor_segment < target_segment {
            return 0;
        }

        let mut count = self.cursor.offset;
        let mut idx = self.cursor_segment;

        while idx > target_segment {
            idx -= 1;
            let segment = &self.segments[idx];
            if idx == target_segment {
                let len = segment.len;
                let clamped_target = target_offset.min(len);
                count += len.saturating_sub(clamped_target);
            } else {
                count += segment.len;
            }
        }

        count
    }

    fn count_forward_steps(&self, target_segment: usize, target_offset: usize) -> usize {
        if self.segments.is_empty() {
            return 0;
        }

        if self.cursor_segment == target_segment {
            let len = self.current_segment_len();
            let clamped_target = target_offset.min(len);
            return clamped_target.saturating_sub(self.cursor.offset.min(len));
        }

        if self.cursor_segment > target_segment {
            return 0;
        }

        let mut count = {
            let len = self.current_segment_len();
            len.saturating_sub(self.cursor.offset.min(len))
        };

        let mut idx = self.cursor_segment + 1;
        while idx < target_segment {
            count += self.segments[idx].len;
            idx += 1;
        }

        if let Some(segment) = self.segments.get(target_segment) {
            count += target_offset.min(segment.len);
        }

        count
    }

    fn rebuild_segments(&mut self) {
        self.segments = collect_segments(&self.document, self.reveal_codes);
        if self.segments.is_empty() {
            self.ensure_placeholder_segment();
            self.segments = collect_segments(&self.document, self.reveal_codes);
        }
        if self.segments.is_empty() {
            self.cursor = CursorPointer::default();
            self.cursor_segment = 0;
            return;
        }
        self.sync_cursor_segment();
        self.clamp_cursor_offset();
    }

    fn ensure_placeholder_segment(&mut self) {
        if self.document.paragraphs.is_empty() {
            self.document
                .paragraphs
                .push(Paragraph::new_text().with_content(vec![Span::new_text("")]));
        } else if let Some(first) = self.document.paragraphs.get_mut(0) {
            if first.content.is_empty() {
                first.content.push(Span::new_text(""));
            }
        }
    }

    fn sync_cursor_segment(&mut self) {
        if let Some(index) = self
            .segments
            .iter()
            .position(|segment| segment.matches(&self.cursor))
        {
            self.cursor_segment = index;
        } else if let Some(first) = self.segments.first() {
            self.cursor_segment = 0;
            self.cursor.update_from_segment(first);
        }
    }

    fn clamp_cursor_offset(&mut self) {
        let len = self.current_segment_len();
        if self.cursor.offset > len {
            self.cursor.offset = len;
        }
    }

    fn nearest_text_pointer_for(&self, pointer: &CursorPointer) -> Option<CursorPointer> {
        let index = self
            .segments
            .iter()
            .position(|segment| segment.matches_pointer(pointer))?;
        match pointer.segment_kind {
            SegmentKind::RevealStart(_) => self
                .find_text_pointer_forward(index + 1)
                .or_else(|| self.find_text_pointer_backward(index)),
            SegmentKind::RevealEnd(_) => self
                .find_text_pointer_backward(index)
                .or_else(|| self.find_text_pointer_forward(index + 1)),
            SegmentKind::Text => None,
        }
    }

    fn find_text_pointer_forward(&self, start_index: usize) -> Option<CursorPointer> {
        if start_index >= self.segments.len() {
            return None;
        }
        let mut idx = start_index;
        while idx < self.segments.len() {
            let segment = &self.segments[idx];
            if matches!(segment.kind, SegmentKind::Text) {
                return Some(CursorPointer {
                    paragraph_path: segment.paragraph_path.clone(),
                    span_path: segment.span_path.clone(),
                    offset: 0,
                    segment_kind: SegmentKind::Text,
                });
            }
            idx += 1;
        }
        None
    }

    fn find_text_pointer_backward(&self, start_index: usize) -> Option<CursorPointer> {
        if self.segments.is_empty() {
            return None;
        }
        let mut idx = start_index.min(self.segments.len());
        while idx > 0 {
            idx -= 1;
            let segment = &self.segments[idx];
            if matches!(segment.kind, SegmentKind::Text) {
                return Some(CursorPointer {
                    paragraph_path: segment.paragraph_path.clone(),
                    span_path: segment.span_path.clone(),
                    offset: segment.len,
                    segment_kind: SegmentKind::Text,
                });
            }
        }
        None
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
}

fn ensure_document_initialized(document: &mut Document) {
    if document.paragraphs.is_empty() {
        document
            .paragraphs
            .push(Paragraph::new_text().with_content(vec![Span::new_text("")]));
    }
}

fn span_path_is_prefix(prefix: &[usize], target: &[usize]) -> bool {
    prefix.len() <= target.len() && target.starts_with(prefix)
}

fn paragraph_path_is_prefix(prefix: &ParagraphPath, target: &ParagraphPath) -> bool {
    let prefix_steps = prefix.steps();
    let target_steps = target.steps();
    prefix_steps.len() <= target_steps.len() && target_steps.starts_with(prefix_steps)
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

fn collect_segments(document: &Document, reveal_codes: bool) -> Vec<SegmentRef> {
    let mut result = Vec::new();
    for (idx, paragraph) in document.paragraphs.iter().enumerate() {
        let mut path = ParagraphPath::new_root(idx);
        collect_paragraph_segments(paragraph, &mut path, reveal_codes, &mut result);
    }
    result
}

fn breadcrumbs_for_pointer(document: &Document, pointer: &CursorPointer) -> Option<Vec<String>> {
    if pointer.paragraph_path.is_empty() {
        return None;
    }
    let (mut labels, paragraph) = collect_paragraph_labels(document, &pointer.paragraph_path)?;
    let inline_labels = collect_inline_labels(paragraph, &pointer.span_path)?;
    labels.extend(inline_labels);
    Some(labels)
}

fn collect_paragraph_labels<'a>(
    document: &'a Document,
    path: &ParagraphPath,
) -> Option<(Vec<String>, &'a Paragraph)> {
    let mut labels = Vec::new();
    let mut current: Option<&'a Paragraph> = None;

    for step in path.steps() {
        let (paragraph, container_len) = match *step {
            PathStep::Root(idx) => (
                document.paragraphs.get(idx)?,
                document.paragraphs.len(),
            ),
            PathStep::Child(idx) => {
                let parent = current?;
                (parent.children.get(idx)?, parent.children.len())
            }
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => {
                let parent = current?;
                let entry = parent.entries.get(entry_index)?;
                (entry.get(paragraph_index)?, entry.len())
            }
        };
        let has_parent = current.is_some();
        let has_siblings = has_parent && container_len > 1;
        if paragraph.paragraph_type != ParagraphType::Text || !has_parent || has_siblings {
            labels.push(paragraph.paragraph_type.to_string());
        }
        current = Some(paragraph);
    }

    let paragraph = current?;
    Some((labels, paragraph))
}

fn collect_inline_labels(paragraph: &Paragraph, span_path: &SpanPath) -> Option<Vec<String>> {
    let mut labels = Vec::new();
    if span_path.is_empty() {
        return Some(labels);
    }

    let mut spans = &paragraph.content;
    for &idx in span_path.indices() {
        let span = spans.get(idx)?;
        if let Some(label) = inline_style_label(span.style) {
            labels.push(label.to_string());
        }
        spans = &span.children;
    }

    Some(labels)
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

    match *last {
        PathStep::Child(idx) => {
            if parent.children.len() == 1 && idx < parent.children.len() {
                Some(ParentScope {
                    parent_path,
                    relation: ParentRelation::Child(idx),
                })
            } else {
                None
            }
        }
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => {
            if entry_index >= parent.entries.len() {
                return None;
            }
            let entry = parent.entries.get(entry_index)?;
            if parent.entries.len() == 1 && entry.len() == 1 && paragraph_index < entry.len() {
                Some(ParentScope {
                    parent_path,
                    relation: ParentRelation::Entry {
                        entry_index,
                        paragraph_index,
                    },
                })
            } else {
                None
            }
        }
        PathStep::Root(_) => None,
    }
}

fn promote_single_child_into_parent(document: &mut Document, scope: &ParentScope) -> bool {
    let Some(parent) = paragraph_mut(document, &scope.parent_path) else {
        return false;
    };

    let child = match scope.relation {
        ParentRelation::Child(idx) => {
            if idx >= parent.children.len() {
                return false;
            }
            parent.children.remove(idx)
        }
        ParentRelation::Entry {
            entry_index,
            paragraph_index,
        } => {
            if entry_index >= parent.entries.len() {
                return false;
            }
            let mut entry = parent.entries.remove(entry_index);
            if paragraph_index >= entry.len() {
                return false;
            }
            let child = entry.remove(paragraph_index);
            if !entry.is_empty() {
                parent.entries.insert(entry_index, entry);
            }
            child
        }
    };

    parent.content = child.content;
    parent.children = child.children;
    parent.entries = child.entries;
    parent.checklist_item_checked = child.checklist_item_checked;
    true
}

fn apply_paragraph_type_in_place(paragraph: &mut Paragraph, target: ParagraphType) {
    if target == ParagraphType::Quote {
        paragraph.paragraph_type = ParagraphType::Quote;
        paragraph.checklist_item_checked = None;

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
    paragraph.checklist_item_checked = None;

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
    let (entry_index, paragraph_index) = match *last_step {
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => (entry_index, paragraph_index),
        _ => return None,
    };

    let list_path = ParagraphPath::from_steps(prefix.to_vec());
    let Some(list_paragraph) = paragraph_ref(document, &list_path) else {
        return None;
    };
    if !is_list_type(list_paragraph.paragraph_type) {
        return None;
    }

    let (list_type, entries_after, extracted_paragraphs, target_offset, has_prefix_entries) = {
        let list = paragraph_mut(document, &list_path)?;
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
            extracted,
            target_offset,
            has_prefix_entries,
        )
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
                if !entries_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    tail.entries = entries_after;
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
                if !entries_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    tail.entries = entries_after;
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
                if !entries_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    tail.entries = entries_after;
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
                if !entries_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    tail.entries = entries_after;
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
                if !entries_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    tail.entries = entries_after;
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
                if !entries_after.is_empty() {
                    let mut tail = Paragraph::new(list_type);
                    tail.entries = entries_after;
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
        if let PathStep::Entry {
            entry_index,
            paragraph_index,
        } = steps[idx]
        {
            let list_path = ParagraphPath::from_steps(steps[..idx].to_vec());
            let tail_steps = steps[idx + 1..].to_vec();
            return Some(EntryContext {
                list_path,
                entry_index,
                paragraph_index,
                tail_steps,
            });
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
        let previous_entry_count = paragraphs[list_index - 1].entries.len();
        let current = paragraphs.remove(list_index);
        let previous = &mut paragraphs[list_index - 1];
        target_entry_index += previous_entry_count;
        previous.entries.extend(current.entries);
        list_index -= 1;
    }

    if list_index + 1 < paragraphs.len() && paragraphs[list_index + 1].paragraph_type == list_type {
        let next = paragraphs.remove(list_index + 1);
        let current = &mut paragraphs[list_index];
        current.entries.extend(next.entries);
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
    paragraph.checklist_item_checked = None;

    match target {
        ParagraphType::Checklist => ensure_entries_have_checklist_items(&mut paragraph.entries),
        ParagraphType::OrderedList | ParagraphType::UnorderedList => {
            normalize_entries_for_standard_list(&mut paragraph.entries)
        }
        _ => {}
    }

    if paragraph.entries.is_empty() {
        let entry = match target {
            ParagraphType::Checklist => vec![empty_checklist_item()],
            ParagraphType::OrderedList | ParagraphType::UnorderedList => {
                vec![empty_text_paragraph()]
            }
            _ => Vec::new(),
        };
        if !entry.is_empty() {
            paragraph.entries.push(entry);
        }
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
    let mut entry = Vec::new();

    match target {
        ParagraphType::Checklist => {
            let mut head = Paragraph::new_checklist_item(false);
            head.content = content;
            if head.content.is_empty() {
                head.content.push(Span::new_text(""));
            }
            entry.push(head);
        }
        ParagraphType::OrderedList | ParagraphType::UnorderedList => {
            let mut head = Paragraph::new_text();
            head.content = content;
            if head.content.is_empty() {
                head.content.push(Span::new_text(""));
            }
            entry.push(head);
        }
        _ => return None,
    }

    let children = mem::take(&mut paragraph.children);
    if !children.is_empty() {
        entry.extend(children);
    }

    paragraph.paragraph_type = target;
    paragraph.entries = vec![entry];
    paragraph.checklist_item_checked = None;

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

fn ensure_entries_have_checklist_items(entries: &mut Vec<Vec<Paragraph>>) {
    if entries.is_empty() {
        entries.push(vec![empty_checklist_item()]);
        return;
    }

    for entry in entries.iter_mut() {
        if entry.is_empty() {
            entry.push(empty_checklist_item());
            continue;
        }

        if entry[0].paragraph_type == ParagraphType::ChecklistItem {
            let first = &mut entry[0];
            if first.content.is_empty() {
                first.content.push(Span::new_text(""));
            }
            if first.checklist_item_checked.is_none() {
                first.checklist_item_checked = Some(false);
            }
            first.children.clear();
            first.entries.clear();
            continue;
        }

        let mut head = entry.remove(0);
        let content = mem::take(&mut head.content);
        let mut item = Paragraph::new_checklist_item(head.checklist_item_checked.unwrap_or(false));
        if content.is_empty() {
            item.content.push(Span::new_text(""));
        } else {
            item.content = content;
        }
        entry.insert(0, item);

        if !head.children.is_empty() || !head.entries.is_empty() {
            entry.insert(1, head);
        }
    }
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

        if entry[0].paragraph_type == ParagraphType::ChecklistItem {
            let first = &mut entry[0];
            first.paragraph_type = ParagraphType::Text;
            first.checklist_item_checked = None;
        }

        if entry[0].content.is_empty() {
            entry[0].content.push(Span::new_text(""));
        }
    }
}

fn empty_text_paragraph() -> Paragraph {
    Paragraph::new_text().with_content(vec![Span::new_text("")])
}

fn empty_checklist_item() -> Paragraph {
    Paragraph::new_checklist_item(false).with_content(vec![Span::new_text("")])
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

fn collect_paragraph_segments(
    paragraph: &Paragraph,
    path: &mut ParagraphPath,
    reveal_codes: bool,
    segments: &mut Vec<SegmentRef>,
) {
    collect_span_segments(paragraph, path, reveal_codes, segments);
    for (child_index, child) in paragraph.children.iter().enumerate() {
        path.push_child(child_index);
        collect_paragraph_segments(child, path, reveal_codes, segments);
        path.pop();
    }
    for (entry_index, entry) in paragraph.entries.iter().enumerate() {
        for (child_index, child) in entry.iter().enumerate() {
            path.push_entry(entry_index, child_index);
            collect_paragraph_segments(child, path, reveal_codes, segments);
            path.pop();
        }
    }
}

fn collect_span_segments(
    paragraph: &Paragraph,
    path: &ParagraphPath,
    reveal_codes: bool,
    segments: &mut Vec<SegmentRef>,
) {
    for (index, span) in paragraph.content.iter().enumerate() {
        let mut span_path = SpanPath::new(vec![index]);
        collect_span_rec(span, path, &mut span_path, reveal_codes, segments);
    }
}

fn collect_span_rec(
    span: &Span,
    paragraph_path: &ParagraphPath,
    span_path: &mut SpanPath,
    reveal_codes: bool,
    segments: &mut Vec<SegmentRef>,
) {
    let len = span.text.chars().count();
    if reveal_codes && span.style != InlineStyle::None {
        segments.push(SegmentRef {
            paragraph_path: paragraph_path.clone(),
            span_path: span_path.clone(),
            len: 1,
            kind: SegmentKind::RevealStart(span.style),
        });
    }

    if span.children.is_empty() || !span.text.is_empty() {
        segments.push(SegmentRef {
            paragraph_path: paragraph_path.clone(),
            span_path: span_path.clone(),
            len,
            kind: SegmentKind::Text,
        });
    } else if len == 0 && span.children.is_empty() {
        segments.push(SegmentRef {
            paragraph_path: paragraph_path.clone(),
            span_path: span_path.clone(),
            len: 0,
            kind: SegmentKind::Text,
        });
    }

    for (child_index, child) in span.children.iter().enumerate() {
        span_path.push(child_index);
        collect_span_rec(child, paragraph_path, span_path, reveal_codes, segments);
        span_path.pop();
    }

    if reveal_codes && span.style != InlineStyle::None {
        segments.push(SegmentRef {
            paragraph_path: paragraph_path.clone(),
            span_path: span_path.clone(),
            len: 1,
            kind: SegmentKind::RevealEnd(span.style),
        });
    }
}

fn apply_style_to_span_path(
    spans: &mut Vec<Span>,
    path: &[usize],
    start: usize,
    end: usize,
    style: InlineStyle,
) -> bool {
    if path.is_empty() {
        return false;
    }
    let idx = path[0];
    if idx >= spans.len() {
        return false;
    }
    if path.len() == 1 {
        apply_style_to_leaf_span(spans, idx, start, end, style)
    } else {
        let span = &mut spans[idx];
        apply_style_to_span_path(&mut span.children, &path[1..], start, end, style)
    }
}

fn apply_style_to_leaf_span(
    spans: &mut Vec<Span>,
    idx: usize,
    start: usize,
    end: usize,
    style: InlineStyle,
) -> bool {
    if idx >= spans.len() {
        return false;
    }
    let original = spans[idx].clone();
    let len = original.text.chars().count();
    if len == 0 {
        return false;
    }
    let clamped_end = end.min(len);
    let clamped_start = start.min(clamped_end);
    if clamped_start >= clamped_end {
        return false;
    }

    let (before_end, right_text) = split_text(&original.text, clamped_end);
    let (left_text, mid_text) = split_text(&before_end, clamped_start);

    if mid_text.is_empty() {
        return false;
    }

    let mut replacements = Vec::new();

    if !left_text.is_empty() {
        let mut left_span = original.clone();
        left_span.text = left_text;
        left_span.children.clear();
        replacements.push(left_span);
    }

    let mut mid_span = original.clone();
    mid_span.text = mid_text;
    mid_span.children.clear();
    mid_span.style = style;
    if mid_span.style != InlineStyle::Link {
        mid_span.link_target = None;
    }
    replacements.push(mid_span);

    if !right_text.is_empty() {
        let mut right_span = original.clone();
        right_span.text = right_text;
        right_span.children.clear();
        replacements.push(right_span);
    }

    spans.remove(idx);
    for (offset, span) in replacements.into_iter().enumerate() {
        spans.insert(idx + offset, span);
    }

    true
}

fn prune_and_merge_spans(spans: &mut Vec<Span>) {
    let mut idx = 0;
    while idx < spans.len() {
        prune_and_merge_spans(&mut spans[idx].children);
        if spans[idx].text.is_empty() && spans[idx].children.is_empty() {
            spans.remove(idx);
        } else {
            idx += 1;
        }
    }

    let mut i = 0;
    while i + 1 < spans.len() {
        if can_merge_spans(&spans[i], &spans[i + 1]) {
            let right = spans.remove(i + 1);
            spans[i].text.push_str(&right.text);
        } else {
            i += 1;
        }
    }
}

fn can_merge_spans(left: &Span, right: &Span) -> bool {
    left.style == right.style
        && left.link_target == right.link_target
        && left.children.is_empty()
        && right.children.is_empty()
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
        let paragraph = paragraph_mut(document, &pointer.paragraph_path)?;
        let split = split_spans(&mut paragraph.content, &span_indices, pointer.offset);
        if paragraph.content.is_empty() {
            paragraph.content.push(Span::new_text(""));
        }
        split
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

                let mut head = match parent.paragraph_type {
                    ParagraphType::Checklist => Paragraph::new_checklist_item(false),
                    _ => Paragraph::new_text(),
                };
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
        _ => None,
    }
}

fn split_spans(spans: &mut Vec<Span>, path: &[usize], offset: usize) -> Vec<Span> {
    if path.is_empty() {
        return Vec::new();
    }

    let idx = path[0];
    if idx >= spans.len() {
        return Vec::new();
    }

    let mut trailing = if idx + 1 < spans.len() {
        spans.split_off(idx + 1)
    } else {
        Vec::new()
    };

    let span = &mut spans[idx];
    let original_span = span.clone();

    let new_span_opt = if path.len() == 1 {
        let (left_text, right_text) = split_text(&original_span.text, offset);
        span.text = left_text;
        if right_text.is_empty() && original_span.children.is_empty() {
            None
        } else {
            let mut new_span = original_span;
            new_span.text = right_text;
            if new_span.is_content_empty() {
                None
            } else {
                Some(new_span)
            }
        }
    } else {
        let child_tail = split_spans(&mut span.children, &path[1..], offset);
        if child_tail.is_empty() {
            None
        } else {
            let mut new_span = original_span;
            new_span.children = child_tail;
            new_span.text.clear();
            if new_span.is_content_empty() {
                None
            } else {
                Some(new_span)
            }
        }
    };

    if let Some(new_span) = new_span_opt {
        trailing.insert(0, new_span);
    }

    trailing
}

fn split_text(text: &str, offset: usize) -> (String, String) {
    let byte_idx = char_to_byte_idx(text, offset);
    let left = text[..byte_idx].to_string();
    let right = text[byte_idx..].to_string();
    (left, right)
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
    nested_list.checklist_item_checked = None;

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
            nested_list.checklist_item_checked = None;
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
    if paragraph.paragraph_type == ParagraphType::ChecklistItem {
        let mut item = paragraph;
        if item.content.is_empty() {
            item.content.push(Span::new_text(""));
        }
        if item.checklist_item_checked.is_none() {
            item.checklist_item_checked = Some(false);
        }
        return (vec![item], 0);
    }

    let mut paragraph = paragraph;
    let mut content = mem::take(&mut paragraph.content);
    if content.is_empty() {
        content.push(Span::new_text(""));
    }
    let mut item = Paragraph::new_checklist_item(paragraph.checklist_item_checked.unwrap_or(false));
    item.content = content;
    let mut entry = vec![item];
    paragraph.checklist_item_checked = None;
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
            if parent.paragraph_type == ParagraphType::Checklist {
                ensure_entries_have_checklist_items(&mut parent.entries);
            } else if is_list_type(parent.paragraph_type) && parent.entries.is_empty() {
                parent.entries.push(vec![empty_text_paragraph()]);
            }
            Some(removed)
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
    }
}

fn paragraph_ref<'a>(document: &'a Document, path: &ParagraphPath) -> Option<&'a Paragraph> {
    let mut iter = path.steps().iter();
    let first = iter.next()?;
    let mut paragraph = match first {
        PathStep::Root(idx) => document.paragraphs.get(*idx)?,
        _ => return None,
    };
    for step in iter {
        paragraph = match step {
            PathStep::Child(idx) => paragraph.children.get(*idx)?,
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => {
                let entry = paragraph.entries.get(*entry_index)?;
                entry.get(*paragraph_index)?
            }
            PathStep::Root(_) => return None,
        };
    }
    Some(paragraph)
}

fn paragraph_mut<'a>(
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
            PathStep::Child(idx) => paragraph.children.get_mut(*idx)?,
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => {
                let entry = paragraph.entries.get_mut(*entry_index)?;
                entry.get_mut(*paragraph_index)?
            }
            PathStep::Root(_) => return None,
        };
    }
    Some(paragraph)
}

fn span_ref<'a>(paragraph: &'a Paragraph, path: &SpanPath) -> Option<&'a Span> {
    let mut iter = path.indices().iter();
    let first = iter.next()?;
    let mut span = paragraph.content.get(*first)?;
    for idx in iter {
        span = span.children.get(*idx)?;
    }
    Some(span)
}

fn span_mut<'a>(paragraph: &'a mut Paragraph, path: &SpanPath) -> Option<&'a mut Span> {
    let mut iter = path.indices().iter();
    let first = iter.next()?;
    let mut span = paragraph.content.get_mut(*first)?;
    for idx in iter {
        span = span.children.get_mut(*idx)?;
    }
    Some(span)
}

fn insert_char_at(
    document: &mut Document,
    pointer: &CursorPointer,
    offset: usize,
    ch: char,
) -> bool {
    let Some(paragraph) = paragraph_mut(document, &pointer.paragraph_path) else {
        return false;
    };
    let Some(span) = span_mut(paragraph, &pointer.span_path) else {
        return false;
    };
    let char_len = span.text.chars().count();
    let clamped_offset = offset.min(char_len);
    let byte_idx = char_to_byte_idx(&span.text, clamped_offset);
    span.text.insert(byte_idx, ch);
    true
}

fn remove_char_at(document: &mut Document, pointer: &CursorPointer, offset: usize) -> bool {
    let Some(paragraph) = paragraph_mut(document, &pointer.paragraph_path) else {
        return false;
    };
    let Some(span) = span_mut(paragraph, &pointer.span_path) else {
        return false;
    };
    let char_len = span.text.chars().count();
    if offset >= char_len {
        return false;
    }
    let start = char_to_byte_idx(&span.text, offset);
    let end = char_to_byte_idx(&span.text, offset + 1);
    if start >= end || end > span.text.len() {
        return false;
    }
    span.text.drain(start..end);
    true
}

fn char_to_byte_idx(text: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    let mut count = 0;
    for (byte_idx, _) in text.char_indices() {
        if count == char_idx {
            return byte_idx;
        }
        count += 1;
    }
    text.len()
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn previous_word_boundary(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut idx = offset.min(chars.len());
    if idx == 0 {
        return 0;
    }

    while idx > 0 && chars[idx - 1].is_whitespace() {
        idx -= 1;
    }
    if idx == 0 {
        return 0;
    }

    while idx > 0 && is_word_char(chars[idx - 1]) {
        idx -= 1;
    }
    if idx > 0 && !is_word_char(chars[idx - 1]) && !chars[idx - 1].is_whitespace() {
        while idx > 0 && !is_word_char(chars[idx - 1]) && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
    }
    idx
}

fn next_word_boundary(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut idx = offset.min(len);
    if idx >= len {
        return len;
    }

    if chars[idx].is_whitespace() {
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        return idx;
    }

    if is_word_char(chars[idx]) {
        while idx < len && is_word_char(chars[idx]) {
            idx += 1;
        }
        while idx < len && !chars[idx].is_whitespace() && !is_word_char(chars[idx]) {
            idx += 1;
        }
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        return idx;
    }

    while idx < len && !chars[idx].is_whitespace() && !is_word_char(chars[idx]) {
        idx += 1;
    }
    while idx < len && chars[idx].is_whitespace() {
        idx += 1;
    }
    idx
}

fn skip_leading_whitespace(text: &str) -> usize {
    let mut count = 0;
    for ch in text.chars() {
        if ch.is_whitespace() {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn paragraph_is_empty(paragraph: &Paragraph) -> bool {
    let content_empty = paragraph.content.iter().all(span_is_empty);
    let children_empty = paragraph.children.iter().all(paragraph_is_empty);
    let entries_empty = paragraph
        .entries
        .iter()
        .all(|entry| entry.iter().all(paragraph_is_empty));
    content_empty && children_empty && entries_empty
}

fn span_is_empty(span: &Span) -> bool {
    span.text.is_empty() && span.children.iter().all(span_is_empty)
}

#[cfg(test)]
mod tests {
    use tdoc::ftml;

    use super::*;

    fn pointer_to_root_span(root_index: usize) -> CursorPointer {
        CursorPointer {
            paragraph_path: ParagraphPath::new_root(root_index),
            span_path: SpanPath::new(vec![0]),
            offset: 0,
            segment_kind: SegmentKind::Text,
        }
    }

    fn pointer_to_child_span(root_index: usize, child_index: usize) -> CursorPointer {
        let mut path = ParagraphPath::new_root(root_index);
        path.push_child(child_index);
        CursorPointer {
            paragraph_path: path,
            span_path: SpanPath::new(vec![0]),
            offset: 0,
            segment_kind: SegmentKind::Text,
        }
    }

    fn pointer_to_child_entry_span(
        root_index: usize,
        child_index: usize,
        entry_index: usize,
        paragraph_index: usize,
    ) -> CursorPointer {
        let mut path = ParagraphPath::new_root(root_index);
        path.push_child(child_index);
        path.push_entry(entry_index, paragraph_index);
        CursorPointer {
            paragraph_path: path,
            span_path: SpanPath::new(vec![0]),
            offset: 0,
            segment_kind: SegmentKind::Text,
        }
    }

    fn pointer_to_entry_span(
        root_index: usize,
        entry_index: usize,
        paragraph_index: usize,
    ) -> CursorPointer {
        let mut path = ParagraphPath::new_root(root_index);
        path.push_entry(entry_index, paragraph_index);
        CursorPointer {
            paragraph_path: path,
            span_path: SpanPath::new(vec![0]),
            offset: 0,
            segment_kind: SegmentKind::Text,
        }
    }

    fn text_paragraph(text: &str) -> Paragraph {
        Paragraph::new_text().with_content(vec![Span::new_text(text)])
    }

    fn unordered_list(items: &[&str]) -> Paragraph {
        let entries = items
            .iter()
            .map(|text| vec![text_paragraph(text)])
            .collect::<Vec<_>>();
        Paragraph::new_unordered_list().with_entries(entries)
    }

    fn ordered_list(items: &[&str]) -> Paragraph {
        let entries = items
            .iter()
            .map(|text| vec![text_paragraph(text)])
            .collect::<Vec<_>>();
        Paragraph::new_ordered_list().with_entries(entries)
    }

    fn checklist(items: &[&str]) -> Paragraph {
        let entries = items
            .iter()
            .map(|text| {
                vec![Paragraph::new_checklist_item(false).with_content(vec![Span::new_text(*text)])]
            })
            .collect::<Vec<_>>();
        Paragraph::new_checklist().with_entries(entries)
    }

    fn document_with_bold_span() -> Document {
        let mut bold = Span::new_text("World");
        bold.style = InlineStyle::Bold;
        let paragraph = Paragraph::new_text().with_content(vec![
            Span::new_text("Hello "),
            bold,
            Span::new_text("!"),
        ]);
        Document::new().with_paragraphs(vec![paragraph])
    }

    fn document_with_checklist_bold_span() -> Document {
        let mut bold = Span::new_text("World");
        bold.style = InlineStyle::Bold;
        let item = Paragraph::new_checklist_item(false).with_content(vec![
            Span::new_text("Hello "),
            bold,
            Span::new_text("!"),
        ]);
        let checklist = Paragraph::new_checklist().with_entries(vec![vec![item]]);
        Document::new().with_paragraphs(vec![checklist])
    }

    fn document_with_checklist_nested_bold_span() -> Document {
        let mut bold = Span::new_text("");
        bold.style = InlineStyle::Bold;
        bold.children = vec![Span::new_text("World")];
        let item = Paragraph::new_checklist_item(false).with_content(vec![
            Span::new_text("Hello "),
            bold,
            Span::new_text("!"),
        ]);
        let checklist = Paragraph::new_checklist().with_entries(vec![vec![item]]);
        Document::new().with_paragraphs(vec![checklist])
    }

    #[test]
    fn breadcrumbs_include_text_for_top_level_paragraphs() {
        let document = Document::new().with_paragraphs(vec![text_paragraph("Top level")]);
        let pointer = pointer_to_root_span(0);
        let breadcrumbs = breadcrumbs_for_pointer(&document, &pointer).unwrap();
        assert_eq!(breadcrumbs, vec!["Text".to_string()]);
    }

    #[test]
    fn breadcrumbs_skip_text_for_quote_children() {
        let quote = Paragraph::new_quote().with_children(vec![text_paragraph("Nested")]);
        let document = Document::new().with_paragraphs(vec![quote]);
        let pointer = pointer_to_child_span(0, 0);
        let breadcrumbs = breadcrumbs_for_pointer(&document, &pointer).unwrap();
        assert_eq!(breadcrumbs, vec!["Quote".to_string()]);
    }

    #[test]
    fn breadcrumbs_skip_text_for_list_items() {
        let document = Document::new().with_paragraphs(vec![unordered_list(&["Item"])]);
        let pointer = pointer_to_entry_span(0, 0, 0);
        let breadcrumbs = breadcrumbs_for_pointer(&document, &pointer).unwrap();
        assert_eq!(breadcrumbs, vec!["Unordered List".to_string()]);
    }

    #[test]
    fn breadcrumbs_include_text_when_list_entry_has_siblings() {
        let entry = vec![
            text_paragraph("First"),
            Paragraph::new_quote().with_children(vec![text_paragraph("Nested")]),
        ];
        let document = Document::new().with_paragraphs(vec![
            Paragraph::new_unordered_list().with_entries(vec![entry]),
        ]);
        let pointer = pointer_to_entry_span(0, 0, 0);
        let breadcrumbs = breadcrumbs_for_pointer(&document, &pointer).unwrap();
        assert_eq!(breadcrumbs, vec!["Unordered List".to_string(), "Text".to_string()]);
    }

    fn insert_text(editor: &mut DocumentEditor, text: &str) {
        for ch in text.chars() {
            assert!(editor.insert_char(ch), "failed to insert char {ch}");
        }
    }

    #[test]
    fn ctrl_p_in_unordered_list_creates_sibling_paragraph() {
        let list = unordered_list(&["Alpha Beta"]);
        let document = Document::new().with_paragraphs(vec![list]);
        let mut editor = DocumentEditor::new(document);

        let mut pointer = pointer_to_entry_span(0, 0, 0);
        pointer.offset = 5;
        assert!(editor.move_to_pointer(&pointer));

        assert!(editor.insert_paragraph_break_as_sibling());

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let list = &doc.paragraphs[0];
        assert_eq!(list.entries.len(), 1);
        let entry = &list.entries[0];
        assert_eq!(entry.len(), 2);
        assert_eq!(entry[0].content[0].text, "Alpha");
        assert_eq!(entry[1].content[0].text, " Beta");
    }

    #[test]
    fn ctrl_p_in_checklist_behaves_like_enter() {
        let checklist = checklist(&["Task"]);
        let document = Document::new().with_paragraphs(vec![checklist]);
        let mut editor = DocumentEditor::new(document);

        let mut pointer = pointer_to_entry_span(0, 0, 0);
        pointer.offset = 4;
        assert!(editor.move_to_pointer(&pointer));

        assert!(editor.insert_paragraph_break_as_sibling());

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let checklist = &doc.paragraphs[0];
        assert_eq!(checklist.entries.len(), 2);
        assert_eq!(checklist.entries[0].len(), 1);
        assert_eq!(checklist.entries[0][0].content[0].text, "Task");
        assert_eq!(
            checklist.entries[1][0].paragraph_type,
            ParagraphType::ChecklistItem
        );
    }

    #[test]
    fn indent_text_paragraph_following_list() {
        let initial_doc = ftml! {
            ul {
                li { p {"Item 1" }}
                li { p {"Item 2" } }
            }
            p { "Following paragraph" }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.move_down());
        assert!(editor.move_down());
        assert!(editor.can_indent_more());
        assert!(editor.indent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li { p { "Item 1"  } }
                    li {
                        p { "Item 2"  }
                        p { "Following paragraph" }
                    }
                }
            }
        );

        assert!(editor.can_indent_less());
        assert!(editor.unindent_current_paragraph());
        assert_eq!(editor.document().clone(), initial_doc);
    }

    #[test]
    fn unindent_text_paragraph_from_beginning_of_list() {
        let initial_doc = ftml! {
            ul {
                li { p {"Item 1" } }
                li { p {"Item 2" } }
                li { p {"Item 3" } }
            }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.can_indent_less());
        assert!(editor.unindent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                p { "Item 1"  }
                ul {
                    li { p { "Item 2" } }
                    li { p { "Item 3" } }
                }
            }
        );
    }

    #[test]
    fn unindent_text_paragraph_from_middle_of_list() {
        let initial_doc = ftml! {
            ul {
                li { p {"Item 1" } }
                li { p {"Item 2" } }
                li { p {"Item 3" } }
            }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.move_down());
        assert!(editor.can_indent_less());
        assert!(editor.unindent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li { p { "Item 1" } }
                }
                p { "Item 2"  }
                ul {
                    li { p { "Item 3" } }
                }
            }
        );
    }

    #[test]
    fn unindent_text_paragraph_from_end_of_list() {
        let initial_doc = ftml! {
            ul {
                li { p {"Item 1" } }
                li { p {"Item 2" } }
                li { p {"Item 3" } }
            }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.move_down());
        assert!(editor.move_down());
        assert!(editor.can_indent_less());
        assert!(editor.unindent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li { p {"Item 1" } }
                    li { p { "Item 2" } }
                }
                p { "Item 3"  }
            }
        );
    }

    #[test]
    fn indent_list_item() {
        let initial_doc = ftml! {
            quote {
                p { "Paragraph in quote" }
                ul {
                    li { p {"Item 1" } }
                    li { p {"Item 2" } }
                    li {
                        ol {
                            li { p { "Subitem 1" }  }
                            li {
                                p { "Subitem 2" }
                                p { "Subitem 2, paragraph 2" }
                            }

                        }
                    }
                }
            }
            p { "Following paragraph" }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        for _ in 0..6 {
            assert!(editor.move_down());
        }

        // Indent the following paragraph into the quote
        assert!(editor.can_indent_more());
        assert!(editor.indent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                quote {
                    p { "Paragraph in quote" }
                    ul {
                        li { p {"Item 1" } }
                        li { p {"Item 2" } }
                        li {
                            ol {
                                li { p { "Subitem 1" }  }
                                li {
                                    p { "Subitem 2" }
                                    p { "Subitem 2, paragraph 2" }
                                }

                            }
                        }
                    }
                    p { "Following paragraph" }
                }
            }
        );

        // Indent further, into the unordered list this time
        assert!(editor.can_indent_more());
        assert!(editor.indent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                quote {
                    p { "Paragraph in quote" }
                    ul {
                        li { p {"Item 1" } }
                        li { p {"Item 2" } }
                        li {
                            ol {
                                li { p { "Subitem 1" }  }
                                li {
                                    p { "Subitem 2" }
                                    p { "Subitem 2, paragraph 2" }
                                }

                            }
                        }
                        li { p { "Following paragraph" } }
                    }
                }
            }
        );

        // Indent further, into the ordered list this time
        assert!(editor.can_indent_more());
        assert!(editor.indent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                quote {
                    p { "Paragraph in quote" }
                    ul {
                        li { p {"Item 1" } }
                        li { p {"Item 2" } }
                        li {
                            ol {
                                li { p { "Subitem 1" }  }
                                li {
                                    p { "Subitem 2" }
                                    p { "Subitem 2, paragraph 2" }
                                }
                                li { p { "Following paragraph" } }
                            }
                        }
                    }
                }
            }
        );

        // Indent further, into the second list item this time
        assert!(editor.can_indent_more());
        assert!(editor.indent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                quote {
                    p { "Paragraph in quote" }
                    ul {
                        li { p {"Item 1" } }
                        li { p {"Item 2" } }
                        li {
                            ol {
                                li { p { "Subitem 1" }  }
                                li {
                                    p { "Subitem 2" }
                                    p { "Subitem 2, paragraph 2" }
                                    p { "Following paragraph" }
                                }
                            }
                        }
                    }
                }
            }
        );

        assert!(!editor.can_indent_more());
    }

    #[test]
    fn indent_more_from_middle_of_list() {
        let initial_doc = ftml! {
            ul {
                li { p {"Item 1" } }
                li { p {"Item 2" } }
                li { p {"Item 3" } }
            }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.move_down());
        assert!(editor.can_indent_more());
        assert!(editor.indent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li {
                        p { "Item 1" }
                        ul {
                            li { p { "Item 2"  } }
                        }
                    }
                    li { p { "Item 3" } }
                }
            }
        );
    }

    #[test]
    fn convert_paragraph_from_middle_of_list() {
        let initial_doc = ftml! {
            ul {
                li { p {"Item 1" } }
                li { p {"Item 2" } }
                li { p {"Item 3" } }
            }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.move_down());
        assert!(editor.set_paragraph_type(ParagraphType::Quote));
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li { p { "Item 1" } }
                }
                quote {
                    p { "Item 2"  }
                }
                ul {
                    li { p { "Item 3" } }
                }
            }
        );
    }

    #[test]
    fn convert_paragraph_from_list_in_middle_of_list() {
        let initial_doc = ftml! {
            ul {
                li { p {"Item 1" } }
                li {
                    p {"Item 2, paragraph 1" }
                    p {"Item 2, paragraph 2" }
                }
                li { p {"Item 3" } }
            }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.move_down());
        assert!(editor.set_paragraph_type(ParagraphType::Quote));
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li { p {"Item 1" } }
                    li {
                        quote { p {"Item 2, paragraph 1" } }
                        p {"Item 2, paragraph 2" }
                    }
                    li { p {"Item 3" } }
                }
            }
        );

        assert!(editor.set_paragraph_type(ParagraphType::Text));
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li { p {"Item 1" } }
                    li {
                        p {"Item 2, paragraph 1" }
                        p {"Item 2, paragraph 2" }
                    }
                    li { p {"Item 3" } }
                }
            }
        );
    }

    #[test]
    fn convert_paragraph_to_nested_list_in_middle_of_list() {
        let initial_doc = ftml! {
            ul {
                li { p {"Item 1" } }
                li {
                    p {"Item 2, paragraph 1" }
                    p {"Item 2, paragraph 2" }
                }
                li { p {"Item 3" } }
            }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.move_down());
        assert!(editor.indent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li { p {"Item 1" } }
                    li {
                        ul {
                            li { p {"Item 2, paragraph 1" } }
                        }
                        p {"Item 2, paragraph 2" }
                    }
                    li { p {"Item 3" } }
                }
            }
        );

        assert!(editor.unindent_current_paragraph());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ul {
                    li { p {"Item 1" } }
                    li {
                        p {"Item 2, paragraph 1" }
                        p {"Item 2, paragraph 2" }
                    }
                    li { p {"Item 3" } }
                }
            }
        );
    }

    #[test]
    fn split_paragraph_list_in_middle_of_list_item() {
        let initial_doc = ftml! {
            ol {
                li { p {"Item 1" } }
                li {
                    p {"Item 2, paragraph 1" }
                    p {"Item 2, paragraph 2" }
                }
                li { p {"Item 3" } }
            }
        };
        let mut editor = DocumentEditor::new(initial_doc.clone());
        assert!(editor.move_down());
        assert!(editor.move_down());
        assert!(editor.insert_paragraph_break());
        assert_eq!(
            editor.document().clone(),
            ftml! {
                ol {
                    li { p {"Item 1" } }
                    li { p {"Item 2, paragraph 1" } }
                    li { p {"Item 2, paragraph 2" } }
                    li { p {"Item 3" } }
                }
            }
        );
    }

    #[test]
    fn move_word_left_within_span() {
        let document = Document::new().with_paragraphs(vec![text_paragraph("hello world")]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_root_span(0);
        assert!(editor.move_to_pointer(&pointer));
        editor.move_to_segment_end();

        assert!(editor.move_word_left());
        assert_eq!(editor.cursor_pointer().offset, 6);

        assert!(editor.move_word_left());
        assert_eq!(editor.cursor_pointer().offset, 0);
    }

    #[test]
    fn move_word_right_advances_to_next_word() {
        let document = Document::new().with_paragraphs(vec![text_paragraph("foo bar baz")]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_root_span(0);
        assert!(editor.move_to_pointer(&pointer));

        assert!(editor.move_word_right());
        assert_eq!(editor.cursor_pointer().offset, 4);

        assert!(editor.move_word_right());
        assert_eq!(editor.cursor_pointer().offset, 8);
    }

    #[test]
    fn delete_word_backward_removes_previous_word() {
        let document = Document::new().with_paragraphs(vec![text_paragraph("foo bar baz")]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_root_span(0);
        assert!(editor.move_to_pointer(&pointer));

        assert!(editor.move_word_right());
        assert!(editor.move_word_right());

        assert!(editor.delete_word_backward());

        let doc = editor.document();
        assert_eq!(doc.paragraphs[0].content[0].text, "foo baz");
        assert_eq!(editor.cursor_pointer().offset, 4);
    }

    #[test]
    fn delete_word_forward_removes_next_word() {
        let document = Document::new().with_paragraphs(vec![text_paragraph("foo bar baz")]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_root_span(0);
        assert!(editor.move_to_pointer(&pointer));

        assert!(editor.delete_word_forward());

        let doc = editor.document();
        assert_eq!(doc.paragraphs[0].content[0].text, "bar baz");
        assert_eq!(editor.cursor_pointer().offset, 0);
    }

    #[test]
    fn move_word_navigation_crosses_segments() {
        let document =
            Document::new().with_paragraphs(vec![text_paragraph("alpha"), text_paragraph("beta")]);
        let mut editor = DocumentEditor::new(document);

        let first = pointer_to_root_span(0);
        assert!(editor.move_to_pointer(&first));
        editor.move_to_segment_end();

        assert!(editor.move_word_right());
        let pointer = editor.cursor_pointer();
        let expected_second = pointer_to_root_span(1);
        assert_eq!(pointer.paragraph_path, expected_second.paragraph_path);
        assert_eq!(pointer.span_path, expected_second.span_path);
        assert_eq!(pointer.offset, 0);

        assert!(editor.move_word_left());
        let pointer = editor.cursor_pointer();
        let expected_first = pointer_to_root_span(0);
        assert_eq!(pointer.paragraph_path, expected_first.paragraph_path);
        assert_eq!(pointer.span_path, expected_first.span_path);
        assert_eq!(pointer.offset, 0);
    }

    #[test]
    fn apply_inline_style_splits_span() {
        let document = Document::new().with_paragraphs(vec![text_paragraph("hello world")]);
        let mut editor = DocumentEditor::new(document);

        let mut start = pointer_to_root_span(0);
        start.offset = 0;
        let mut end = pointer_to_root_span(0);
        end.offset = 5;

        assert!(
            editor
                .apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Bold)
        );

        let doc = editor.document();
        let paragraph = &doc.paragraphs[0];
        assert_eq!(paragraph.content.len(), 2);
        assert_eq!(paragraph.content[0].text, "hello");
        assert_eq!(paragraph.content[0].style, InlineStyle::Bold);
        assert_eq!(paragraph.content[1].text, " world");
        assert_eq!(paragraph.content[1].style, InlineStyle::None);
    }

    #[test]
    fn insert_char_before_reveal_start_marker_inserts_into_previous_span() {
        let mut editor = DocumentEditor::new(document_with_bold_span());
        editor.set_reveal_codes(true);

        // Move to the reveal end marker within the checklist item
        for _ in 0..6 {
            assert!(editor.move_right());
        }
        insert_text(&mut editor, "dear ");

        let doc = editor.document();
        assert_eq!(doc.paragraphs[0].content[0].text, "Hello dear ");
        assert_eq!(doc.paragraphs[0].content[1].text, "World");
    }

    #[test]
    fn insert_char_before_reveal_start_and_remove_again() {
        let mut editor = DocumentEditor::new(document_with_bold_span());
        editor.set_reveal_codes(true);

        // Move to the reveal end marker within the checklist item
        for _ in 0..6 {
            assert!(editor.move_right());
        }
        insert_text(&mut editor, "cruel ");

        // 1. Ok, let's add some text before the reveal start marker
        let doc = editor.document();
        assert_eq!(doc.paragraphs[0].content[0].text, "Hello cruel ");
        assert_eq!(doc.paragraphs[0].content[1].text, "World");

        // 2. Now remove it again
        for _ in 0..6 {
            assert!(editor.backspace());
        }
        let doc = editor.document();
        assert_eq!(doc.paragraphs[0].content[0].text, "Hello ");
        assert_eq!(doc.paragraphs[0].content[1].text, "World");

        // 3. Now let's move past the reveal start marker and add text after it
        assert!(editor.move_right());
        insert_text(&mut editor, "dear ");
        let doc = editor.document();
        assert_eq!(doc.paragraphs[0].content[0].text, "Hello ");
        assert_eq!(doc.paragraphs[0].content[1].text, "dear World");
    }

    #[test]
    fn insert_char_before_reveal_end_marker_appends_to_span() {
        let mut editor = DocumentEditor::new(document_with_bold_span());
        editor.set_reveal_codes(true);

        // Move to the reveal end marker within the checklist item
        for _ in 0..12 {
            assert!(editor.move_right());
        }
        insert_text(&mut editor, " class people");

        let doc = editor.document();
        assert_eq!(doc.paragraphs[0].content[0].text, "Hello ");
        assert_eq!(doc.paragraphs[0].content[1].text, "World class people");
    }

    #[test]
    fn insert_char_before_reveal_end_marker_in_checklist_appends_to_span() {
        let mut editor = DocumentEditor::new(document_with_checklist_bold_span());
        editor.set_reveal_codes(true);

        // Move to the reveal end marker within the checklist item
        for _ in 0..12 {
            assert!(editor.move_right());
        }
        insert_text(&mut editor, " class people");

        let doc = editor.document();
        let checklist = &doc.paragraphs[0];
        assert_eq!(checklist.entries[0][0].content[0].text, "Hello ");
        assert_eq!(
            checklist.entries[0][0].content[1].text,
            "World class people"
        );
    }

    #[test]
    fn insert_char_on_reveal_end_marker_in_checklist_appends_to_span() {
        let mut editor = DocumentEditor::new(document_with_checklist_bold_span());
        editor.set_reveal_codes(true);

        // Move to the end of the paragraph first
        while editor.move_right() {}
        // Walk backwards until we land on the reveal end marker
        while !matches!(
            editor.cursor_pointer().segment_kind,
            SegmentKind::RevealEnd(_)
        ) {
            assert!(editor.move_left());
        }
        let pointer = editor.cursor_pointer();
        assert_eq!(pointer.span_path.indices(), &[1]);

        insert_text(&mut editor, " dear");

        let doc = editor.document();
        let checklist = &doc.paragraphs[0];
        let item = &checklist.entries[0][0];
        assert_eq!(item.content[0].text, "Hello ");
        assert_eq!(item.content[1].text, "World dear");
    }

    #[test]
    fn insert_char_on_reveal_end_marker_in_checklist_with_nested_bold_span_appends_to_span() {
        let mut editor = DocumentEditor::new(document_with_checklist_nested_bold_span());
        editor.set_reveal_codes(true);

        while editor.move_right() {}
        while !matches!(
            editor.cursor_pointer().segment_kind,
            SegmentKind::RevealEnd(_)
        ) {
            assert!(editor.move_left());
        }

        insert_text(&mut editor, " dear");

        let doc = editor.document();
        let checklist = &doc.paragraphs[0];
        let item = &checklist.entries[0][0];
        assert_eq!(item.content[0].text, "Hello ");
        assert_eq!(item.content[1].children[0].text, "World dear");
    }

    #[test]
    fn apply_inline_style_across_segments() {
        let paragraph = Paragraph::new_text()
            .with_content(vec![Span::new_text("hello "), Span::new_text("world")]);
        let document = Document::new().with_paragraphs(vec![paragraph]);
        let mut editor = DocumentEditor::new(document);

        let mut start = pointer_to_root_span(0);
        start.span_path = SpanPath::new(vec![0]);
        start.offset = 3;

        let mut end = pointer_to_root_span(0);
        end.span_path = SpanPath::new(vec![1]);
        end.offset = 2;

        assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::Underline));

        let doc = editor.document();
        let spans = &doc.paragraphs[0].content;
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "hel");
        assert_eq!(spans[0].style, InlineStyle::None);
        assert_eq!(spans[1].text, "lo wo");
        assert_eq!(spans[1].style, InlineStyle::Underline);
        assert_eq!(spans[2].text, "rld");
        assert_eq!(spans[2].style, InlineStyle::None);
    }

    #[test]
    fn move_down_advances_to_next_paragraph() {
        let document = Document::new().with_paragraphs(vec![
            Paragraph::new_text().with_content(vec![Span::new_text("One")]),
            Paragraph::new_text().with_content(vec![Span::new_text("Two")]),
        ]);
        let mut editor = DocumentEditor::new(document);
        assert!(editor.move_down());
        let pointer = editor.cursor_pointer();
        assert_eq!(
            pointer.paragraph_path.steps(),
            pointer_to_root_span(1).paragraph_path.steps()
        );
        assert!(!editor.move_down());
    }

    #[test]
    fn move_up_moves_to_previous_paragraph() {
        let document = Document::new().with_paragraphs(vec![
            Paragraph::new_text().with_content(vec![Span::new_text("Alpha")]),
            Paragraph::new_text().with_content(vec![Span::new_text("Beta")]),
        ]);
        let mut editor = DocumentEditor::new(document);
        assert!(editor.move_down());
        assert!(editor.move_up());
        let pointer = editor.cursor_pointer();
        assert_eq!(
            pointer.paragraph_path.steps(),
            pointer_to_root_span(0).paragraph_path.steps()
        );
        assert_eq!(pointer.offset, 0);
        assert!(!editor.move_up());
    }

    #[test]
    fn clear_inline_style_resets_to_plain() {
        let document = Document::new().with_paragraphs(vec![text_paragraph("styled text")]);
        let mut editor = DocumentEditor::new(document);

        let mut start = pointer_to_root_span(0);
        start.offset = 0;
        let mut end = pointer_to_root_span(0);
        end.offset = 6;

        assert!(
            editor
                .apply_inline_style_to_selection(&(start.clone(), end.clone()), InlineStyle::Code)
        );
        assert!(editor.apply_inline_style_to_selection(&(start, end), InlineStyle::None));

        let doc = editor.document();
        let spans = &doc.paragraphs[0].content;
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "styled text");
        assert_eq!(spans[0].style, InlineStyle::None);
    }

    #[test]
    fn top_level_paragraph_type_change_updates_current_paragraph() {
        let document =
            Document::new().with_paragraphs(vec![text_paragraph("Alpha"), text_paragraph("Beta")]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_root_span(0);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::Header1));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(doc.paragraphs[0].paragraph_type, ParagraphType::Header1);
        assert_eq!(doc.paragraphs[1].paragraph_type, ParagraphType::Text);
    }

    #[test]
    fn changing_sole_child_promotes_parent_container() {
        let quote = Paragraph::new_quote().with_children(vec![text_paragraph("Nested")]);
        let document = Document::new().with_paragraphs(vec![quote]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_child_span(0, 0);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::Header2));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let paragraph = &doc.paragraphs[0];
        assert_eq!(paragraph.paragraph_type, ParagraphType::Header2);
        assert!(paragraph.children.is_empty());
        assert!(paragraph.entries.is_empty());
        assert_eq!(paragraph.content.len(), 1);
        assert_eq!(paragraph.content[0].text, "Nested");
    }

    #[test]
    fn changing_child_with_siblings_only_updates_that_child() {
        let quote = Paragraph::new_quote()
            .with_children(vec![text_paragraph("First"), text_paragraph("Second")]);
        let document = Document::new().with_paragraphs(vec![quote]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_child_span(0, 0);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::Header3));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let quote = &doc.paragraphs[0];
        assert_eq!(quote.paragraph_type, ParagraphType::Quote);
        assert_eq!(quote.children.len(), 2);
        assert_eq!(quote.children[0].paragraph_type, ParagraphType::Header3);
        assert_eq!(quote.children[1].paragraph_type, ParagraphType::Text);
    }

    #[test]
    fn checklist_item_to_text_promotes_parent_list_when_single_item() {
        let item = Paragraph::new_checklist_item(false).with_content(vec![Span::new_text("Task")]);
        let checklist = Paragraph::new_checklist().with_entries(vec![vec![item]]);
        let document = Document::new().with_paragraphs(vec![checklist]);

        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_entry_span(0, 0, 0);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::Text));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let paragraph = &doc.paragraphs[0];
        assert_eq!(paragraph.paragraph_type, ParagraphType::Text);
        assert!(paragraph.entries.is_empty());
        assert_eq!(paragraph.content.len(), 1);
        assert_eq!(paragraph.content[0].text, "Task");
    }

    #[test]
    fn checklist_item_with_siblings_only_changes_item() {
        let first =
            Paragraph::new_checklist_item(false).with_content(vec![Span::new_text("First")]);
        let second =
            Paragraph::new_checklist_item(false).with_content(vec![Span::new_text("Second")]);
        let checklist =
            Paragraph::new_checklist().with_entries(vec![vec![first], vec![second.clone()]]);
        let document = Document::new().with_paragraphs(vec![checklist]);

        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_entry_span(0, 0, 0);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::Header1));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 2);
        assert_eq!(doc.paragraphs[0].paragraph_type, ParagraphType::Header1);
        assert_eq!(doc.paragraphs[0].content[0].text, "First");

        let checklist = &doc.paragraphs[1];
        assert_eq!(checklist.paragraph_type, ParagraphType::Checklist);
        assert_eq!(checklist.entries.len(), 1);
        assert_eq!(checklist.entries[0][0].content[0].text, "Second");
    }

    #[test]
    fn checklist_item_state_updates_through_editor() {
        let item = Paragraph::new_checklist_item(false).with_content(vec![Span::new_text("Task")]);
        let checklist = Paragraph::new_checklist().with_entries(vec![vec![item]]);
        let document = Document::new().with_paragraphs(vec![checklist]);

        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_entry_span(0, 0, 0);
        assert!(editor.move_to_pointer(&pointer));
        assert_eq!(editor.current_checklist_item_state(), Some(false));

        assert!(editor.set_current_checklist_item_checked(true));
        assert_eq!(editor.current_checklist_item_state(), Some(true));

        assert!(!editor.set_current_checklist_item_checked(true));
        assert_eq!(editor.current_checklist_item_state(), Some(true));
    }

    #[test]
    fn unordered_list_item_conversion_splits_list() {
        let first = text_paragraph("First");
        let second = text_paragraph("Second");
        let third = text_paragraph("Third");
        let list = Paragraph::new_unordered_list().with_entries(vec![
            vec![first],
            vec![second],
            vec![third],
        ]);
        let document = Document::new().with_paragraphs(vec![list]);

        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_entry_span(0, 1, 0);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::Header2));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 3);
        assert_eq!(
            doc.paragraphs[0].paragraph_type,
            ParagraphType::UnorderedList
        );
        assert_eq!(doc.paragraphs[0].entries.len(), 1);
        assert_eq!(doc.paragraphs[0].entries[0][0].content[0].text, "First");

        assert_eq!(doc.paragraphs[1].paragraph_type, ParagraphType::Header2);
        assert_eq!(doc.paragraphs[1].content[0].text, "Second");

        assert_eq!(
            doc.paragraphs[2].paragraph_type,
            ParagraphType::UnorderedList
        );
        assert_eq!(doc.paragraphs[2].entries.len(), 1);
        assert_eq!(doc.paragraphs[2].entries[0][0].content[0].text, "Third");
    }

    #[test]
    fn nested_list_item_conversion_inside_quote() {
        let list = Paragraph::new_unordered_list().with_entries(vec![
            vec![text_paragraph("Alpha")],
            vec![text_paragraph("Beta")],
        ]);
        let quote = Paragraph::new_quote().with_children(vec![list]);
        let document = Document::new().with_paragraphs(vec![quote]);

        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_child_entry_span(0, 0, 1, 0);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::Text));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let quote = &doc.paragraphs[0];
        assert_eq!(quote.children.len(), 2);
        assert_eq!(
            quote.children[0].paragraph_type,
            ParagraphType::UnorderedList
        );
        assert_eq!(quote.children[0].entries.len(), 1);
        assert_eq!(quote.children[0].entries[0][0].content[0].text, "Alpha");

        assert_eq!(quote.children[1].paragraph_type, ParagraphType::Text);
        assert_eq!(quote.children[1].content[0].text, "Beta");
    }

    #[test]
    fn converting_between_lists_merges_all_entries() {
        let document = Document::new().with_paragraphs(vec![
            unordered_list(&["One"]),
            text_paragraph("Two"),
            unordered_list(&["Three"]),
        ]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_root_span(1);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::UnorderedList));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let list = &doc.paragraphs[0];
        assert_eq!(list.paragraph_type, ParagraphType::UnorderedList);
        assert_eq!(list.entries.len(), 3);
        assert_eq!(list.entries[0][0].content[0].text, "One");
        assert_eq!(list.entries[1][0].content[0].text, "Two");
        assert_eq!(list.entries[2][0].content[0].text, "Three");
    }

    #[test]
    fn converting_before_list_merges_forward_only() {
        let document =
            Document::new().with_paragraphs(vec![text_paragraph("Start"), ordered_list(&["Next"])]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_root_span(0);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::OrderedList));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let list = &doc.paragraphs[0];
        assert_eq!(list.paragraph_type, ParagraphType::OrderedList);
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[0][0].content[0].text, "Start");
        assert_eq!(list.entries[1][0].content[0].text, "Next");
    }

    #[test]
    fn converting_to_checklist_merges_with_previous_only() {
        let document =
            Document::new().with_paragraphs(vec![checklist(&["Item 1"]), text_paragraph("Item 2")]);
        let mut editor = DocumentEditor::new(document);
        let pointer = pointer_to_root_span(1);
        assert!(editor.move_to_pointer(&pointer));
        assert!(editor.set_paragraph_type(ParagraphType::Checklist));

        let doc = editor.document();
        assert_eq!(doc.paragraphs.len(), 1);
        let list = &doc.paragraphs[0];
        assert_eq!(list.paragraph_type, ParagraphType::Checklist);
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[0][0].content[0].text, "Item 1");
        assert_eq!(list.entries[1][0].content[0].text, "Item 2");
    }
}
