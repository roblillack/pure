const MARKER_PREFIX: &str = "1337;M";
use std::mem;
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
}

impl CursorPointer {
    fn update_from_segment(&mut self, segment: &SegmentRef) {
        self.paragraph_path = segment.paragraph_path.clone();
        self.span_path = segment.span_path.clone();
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
        }
    }
}

#[derive(Clone, Debug)]
pub struct SegmentRef {
    pub paragraph_path: ParagraphPath,
    pub span_path: SpanPath,
    pub len: usize,
}

impl SegmentRef {
    fn matches(&self, cursor: &CursorPointer) -> bool {
        self.paragraph_path == cursor.paragraph_path && self.span_path == cursor.span_path
    }

    fn matches_pointer(&self, pointer: &CursorPointer) -> bool {
        self.paragraph_path == pointer.paragraph_path && self.span_path == pointer.span_path
    }
}

#[derive(Clone)]
pub struct MarkerRef {
    pub id: usize,
    pub pointer: CursorPointer,
}

#[derive(Clone, Copy)]
enum RemovalDirection {
    Backward,
    Forward,
}

pub struct DocumentEditor {
    document: Document,
    segments: Vec<SegmentRef>,
    cursor: CursorPointer,
    cursor_segment: usize,
}

impl DocumentEditor {
    pub fn new(mut document: Document) -> Self {
        ensure_document_initialized(&mut document);
        let mut editor = Self {
            document,
            segments: Vec::new(),
            cursor: CursorPointer::default(),
            cursor_segment: 0,
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

    pub fn cursor_pointer(&self) -> CursorPointer {
        self.cursor.clone()
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
        if self.cursor.offset > 0 {
            self.cursor.offset -= 1;
            return true;
        }
        self.shift_to_previous_segment()
    }

    pub fn move_right(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        let current_len = self.current_segment_len();
        if self.cursor.offset < current_len {
            self.cursor.offset += 1;
            return true;
        }
        self.shift_to_next_segment()
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
            self.ensure_cursor_selectable();
        }

        true
    }

    pub fn insert_char(&mut self, ch: char) -> bool {
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
        let pointer = CursorPointer {
            paragraph_path: next_segment.paragraph_path.clone(),
            span_path: next_segment.span_path.clone(),
            offset: 0,
        };
        if remove_char_at(&mut self.document, &pointer, 0) {
            self.rebuild_segments();
            true
        } else {
            false
        }
    }

    pub fn insert_paragraph_break(&mut self) -> bool {
        let pointer = self.cursor.clone();
        if !pointer.is_valid() {
            return false;
        }
        let Some(new_pointer) = split_paragraph_break(&mut self.document, &pointer) else {
            return false;
        };
        self.rebuild_segments();
        if !self.move_to_pointer(&new_pointer) {
            self.cursor = new_pointer;
        }
        true
    }

    pub fn clone_with_markers(&self, sentinel: char) -> (Document, Vec<MarkerRef>, bool) {
        let mut clone = self.document.clone();
        let mut markers = Vec::new();
        let mut inserted_cursor = false;

        for segment in &self.segments {
            let Some(paragraph) = paragraph_mut(&mut clone, &segment.paragraph_path) else {
                continue;
            };
            let Some(span) = span_mut(paragraph, &segment.span_path) else {
                continue;
            };

            let original: Vec<char> = span.text.chars().collect();
            let mut rebuilt = String::new();

            for offset in 0..=original.len() {
                let id = markers.len();
                rebuilt.push_str(&format!("\x1b]{}{}\x1b\\", MARKER_PREFIX, id));
                markers.push(MarkerRef {
                    id,
                    pointer: CursorPointer {
                        paragraph_path: segment.paragraph_path.clone(),
                        span_path: segment.span_path.clone(),
                        offset,
                    },
                });

                if segment.matches(&self.cursor) && offset == self.cursor.offset {
                    rebuilt.push(sentinel);
                    inserted_cursor = true;
                }

                if offset < original.len() {
                    rebuilt.push(original[offset]);
                }
            }

            span.text = rebuilt;
        }

        (clone, markers, inserted_cursor)
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

    fn current_segment_len(&self) -> usize {
        self.segments
            .get(self.cursor_segment)
            .map(|segment| segment.len)
            .unwrap_or(0)
    }

    fn rebuild_segments(&mut self) {
        self.segments = collect_segments(&self.document);
        if self.segments.is_empty() {
            self.ensure_placeholder_segment();
            self.segments = collect_segments(&self.document);
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
                    })
                } else if end_idx < self.segments.len() {
                    let next = &self.segments[end_idx];
                    Some(CursorPointer {
                        paragraph_path: next.paragraph_path.clone(),
                        span_path: next.span_path.clone(),
                        offset: 0,
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
                    })
                } else if start_idx > 0 {
                    let prev = &self.segments[start_idx - 1];
                    Some(CursorPointer {
                        paragraph_path: prev.paragraph_path.clone(),
                        span_path: prev.span_path.clone(),
                        offset: prev.len,
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

fn collect_segments(document: &Document) -> Vec<SegmentRef> {
    let mut result = Vec::new();
    for (idx, paragraph) in document.paragraphs.iter().enumerate() {
        let mut path = ParagraphPath::new_root(idx);
        collect_paragraph_segments(paragraph, &mut path, &mut result);
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
        let paragraph = match *step {
            PathStep::Root(idx) => document.paragraphs.get(idx)?,
            PathStep::Child(idx) => current?.children.get(idx)?,
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => {
                let entry = current?.entries.get(entry_index)?;
                entry.get(paragraph_index)?
            }
        };
        labels.push(paragraph.paragraph_type.to_string());
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
    paragraph.paragraph_type = target;
    paragraph.checklist_item_checked = None;

    if target.is_leaf() {
        paragraph.children.clear();
        paragraph.entries.clear();
    } else if target == ParagraphType::Quote {
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
            if paragraph.content.is_empty() {
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
    segments: &mut Vec<SegmentRef>,
) {
    collect_span_segments(paragraph, path, segments);
    for (child_index, child) in paragraph.children.iter().enumerate() {
        path.push_child(child_index);
        collect_paragraph_segments(child, path, segments);
        path.pop();
    }
    for (entry_index, entry) in paragraph.entries.iter().enumerate() {
        for (child_index, child) in entry.iter().enumerate() {
            path.push_entry(entry_index, child_index);
            collect_paragraph_segments(child, path, segments);
            path.pop();
        }
    }
}

fn collect_span_segments(
    paragraph: &Paragraph,
    path: &ParagraphPath,
    segments: &mut Vec<SegmentRef>,
) {
    for (index, span) in paragraph.content.iter().enumerate() {
        let mut span_path = SpanPath::new(vec![index]);
        collect_span_rec(span, path, &mut span_path, segments);
    }
}

fn collect_span_rec(
    span: &Span,
    paragraph_path: &ParagraphPath,
    span_path: &mut SpanPath,
    segments: &mut Vec<SegmentRef>,
) {
    let len = span.text.chars().count();
    if span.children.is_empty() || !span.text.is_empty() {
        segments.push(SegmentRef {
            paragraph_path: paragraph_path.clone(),
            span_path: span_path.clone(),
            len,
        });
    } else if len == 0 && span.children.is_empty() {
        segments.push(SegmentRef {
            paragraph_path: paragraph_path.clone(),
            span_path: span_path.clone(),
            len: 0,
        });
    }

    for (child_index, child) in span.children.iter().enumerate() {
        span_path.push(child_index);
        collect_span_rec(child, paragraph_path, span_path, segments);
        span_path.pop();
    }
}

fn split_paragraph_break(
    document: &mut Document,
    pointer: &CursorPointer,
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
            })
        }
        PathStep::Entry {
            entry_index,
            paragraph_index: _,
        } => {
            let parent_path = ParagraphPath::from_steps(prefix.to_vec());
            let parent = paragraph_mut(document, &parent_path)?;
            let insert_idx = (entry_index + 1).min(parent.entries.len());

            let new_entry = match parent.paragraph_type {
                ParagraphType::Checklist => {
                    let mut item = Paragraph::new_checklist_item(false);
                    item.content = right_spans;
                    vec![item]
                }
                _ => {
                    let mut text_paragraph = Paragraph::new_text();
                    text_paragraph.content = right_spans;
                    vec![text_paragraph]
                }
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
    use super::*;

    fn pointer_to_root_span(root_index: usize) -> CursorPointer {
        CursorPointer {
            paragraph_path: ParagraphPath::new_root(root_index),
            span_path: SpanPath::new(vec![0]),
            offset: 0,
        }
    }

    fn pointer_to_child_span(root_index: usize, child_index: usize) -> CursorPointer {
        let mut path = ParagraphPath::new_root(root_index);
        path.push_child(child_index);
        CursorPointer {
            paragraph_path: path,
            span_path: SpanPath::new(vec![0]),
            offset: 0,
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
