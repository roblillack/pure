const MARKER_POINTER_PREFIX: &str = "1337;M";
const MARKER_REVEAL_PREFIX: &str = "1337;R";
use std::cmp::Ordering;
use tdoc::{ChecklistItem, Document, InlineStyle, Paragraph, ParagraphType, Span};

use content::{insert_char_at, prune_and_merge_spans, remove_char_at};

pub mod content;
pub mod cursor;
pub mod inspect;
mod structure;
mod styles;

pub(crate) use styles::inline_style_label;

use inspect::{checklist_item_ref, paragraph_ref, span_ref, span_ref_from_item};
use structure::{
    IndentTargetKind, ParentRelation, append_paragraph_as_checklist_child,
    append_paragraph_to_entry, append_paragraph_to_list, append_paragraph_to_quote,
    break_list_entry_for_non_list_target, checklist_item_mut, convert_paragraph_into_list,
    determine_parent_scope, ensure_checklist_item_has_content, ensure_document_initialized,
    ensure_list_entry_has_paragraph, entry_has_multiple_paragraphs, extract_checklist_item_context,
    extract_entry_context, find_container_indent_target, find_indent_target,
    find_list_ancestor_path, indent_checklist_item_into_item, indent_list_entry_into_entry,
    indent_list_entry_into_foreign_list, indent_paragraph_within_entry,
    insert_paragraph_after_parent, is_list_type, is_single_paragraph_entry,
    list_entry_append_target, merge_adjacent_lists, paragraph_is_empty, paragraph_mut,
    parent_paragraph_path, promote_list_entry_to_parent, promote_single_child_into_parent,
    remove_paragraph_by_path, span_mut, span_mut_from_item, split_paragraph_break,
    take_checklist_item_at, take_list_entry, take_paragraph_at, unindent_checklist_item,
    update_existing_list_type, update_paragraph_type,
};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
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
    pub fn new_root(idx: usize) -> Self {
        Self {
            steps: vec![PathStep::Root(idx)],
        }
    }

    fn from_steps(steps: Vec<PathStep>) -> Self {
        Self { steps }
    }

    pub fn push_child(&mut self, idx: usize) {
        self.steps.push(PathStep::Child(idx));
    }

    pub fn push_entry(&mut self, entry_index: usize, paragraph_index: usize) {
        self.steps.push(PathStep::Entry {
            entry_index,
            paragraph_index,
        });
    }

    pub fn push_checklist_item(&mut self, indices: Vec<usize>) {
        self.steps.push(PathStep::ChecklistItem { indices });
    }

    pub fn pop(&mut self) {
        if self.steps.len() > 1 {
            self.steps.pop();
        }
    }

    fn steps(&self) -> &[PathStep] {
        &self.steps
    }

    #[allow(dead_code)]
    pub fn numeric_steps(&self) -> Vec<usize> {
        let mut nums = Vec::new();
        for step in &self.steps {
            match step {
                PathStep::Root(idx) => nums.push(*idx),
                PathStep::Child(idx) => nums.push(*idx),
                PathStep::Entry {
                    entry_index,
                    paragraph_index,
                } => {
                    nums.push(*paragraph_index);
                    nums.push(*entry_index);
                }
                PathStep::ChecklistItem { indices } => {
                    for idx in indices {
                        nums.push(*idx);
                    }
                }
            }
        }
        nums
    }

    fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn root_index(&self) -> Option<usize> {
        if let Some(PathStep::Root(idx)) = self.steps.first() {
            Some(*idx)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SpanPath {
    pub indices: Vec<usize>,
}

impl SpanPath {
    pub fn new(indices: Vec<usize>) -> Self {
        Self { indices }
    }

    pub fn push(&mut self, idx: usize) {
        self.indices.push(idx);
    }

    pub fn pop(&mut self) {
        self.indices.pop();
    }

    pub fn indices(&self) -> &[usize] {
        &self.indices
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
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
pub(crate) struct PointerKey {
    pub(crate) segment_index: usize,
    pub(crate) offset: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct MarkerRef {
    pub id: usize,
    pub pointer: CursorPointer,
}

#[derive(Clone)]
#[allow(dead_code)]
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

#[derive(Debug)]
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
        #[derive(Clone)]
        struct SpanAssembly {
            paragraph_path: ParagraphPath,
            span_path: SpanPath,
            original_text: String,
            start: Option<String>,
            text: Option<String>,
            end: Option<String>,
            from_checklist: bool,
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
            let assembly = if let Some(existing) = assemblies.iter_mut().find(|entry| {
                entry.paragraph_path == segment.paragraph_path
                    && entry.span_path == segment.span_path
            }) {
                existing
            } else {
                if let Some(item) = checklist_item_ref(&self.document, &segment.paragraph_path) {
                    let Some(span) = span_ref_from_item(item, &segment.span_path) else {
                        continue;
                    };
                    assemblies.push(SpanAssembly {
                        paragraph_path: segment.paragraph_path.clone(),
                        span_path: segment.span_path.clone(),
                        original_text: span.text.clone(),
                        start: None,
                        text: None,
                        end: None,
                        from_checklist: true,
                    });
                } else {
                    let Some(paragraph) = paragraph_ref(&self.document, &segment.paragraph_path)
                    else {
                        continue;
                    };
                    let Some(span) = span_ref(paragraph, &segment.span_path) else {
                        continue;
                    };
                    assemblies.push(SpanAssembly {
                        paragraph_path: segment.paragraph_path.clone(),
                        span_path: segment.span_path.clone(),
                        original_text: span.text.clone(),
                        start: None,
                        text: None,
                        end: None,
                        from_checklist: false,
                    });
                }
                assemblies.last_mut().unwrap()
            };

            match segment.kind {
                SegmentKind::Text => {
                    let original_chars: Vec<char> = assembly.original_text.chars().collect();
                    let len = original_chars.len();
                    let mut rebuilt = String::new();

                    #[allow(clippy::needless_range_loop)]
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
            if assembly.from_checklist {
                let Some(item) = checklist_item_mut(&mut clone, &assembly.paragraph_path) else {
                    continue;
                };
                let Some(span) = span_mut_from_item(item, &assembly.span_path) else {
                    continue;
                };
                span.text = combined;
            } else {
                let Some(paragraph) = paragraph_mut(&mut clone, &assembly.paragraph_path) else {
                    continue;
                };
                let Some(span) = span_mut(paragraph, &assembly.span_path) else {
                    continue;
                };
                span.text = combined;
            }
        }

        (clone, markers, reveal_tags, inserted_cursor)
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
        if find_indent_target(&self.document, &self.cursor.paragraph_path).is_some() {
            return true;
        }
        if let Some(ctx) = extract_entry_context(&self.cursor.paragraph_path) {
            return find_container_indent_target(&self.document, &ctx.list_path).is_some();
        }
        false
    }

    pub fn can_indent_less(&self) -> bool {
        if let Some(ctx) = extract_checklist_item_context(&self.cursor.paragraph_path) {
            return ctx.indices.len() > 1;
        }
        self.cursor.paragraph_path.steps().len() > 1
    }

    pub fn can_change_paragraph_type(&self) -> bool {
        if let Some(ctx) = extract_checklist_item_context(&self.cursor.paragraph_path) {
            return ctx.indices.len() <= 1;
        }
        true
    }

    pub fn indent_current_paragraph(&mut self) -> bool {
        let mut target = find_indent_target(&self.document, &self.cursor.paragraph_path);
        if target.is_none()
            && let Some(ctx) = extract_entry_context(&self.cursor.paragraph_path)
        {
            target = find_container_indent_target(&self.document, &ctx.list_path);
        }
        let Some(target) = target else {
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
                if !self.move_to_pointer(&new_pointer)
                    && !self.fallback_move_to_text(&new_pointer, false)
                {
                    self.ensure_cursor_selectable();
                }
                return true;
            }
        }

        if matches!(target.kind, IndentTargetKind::ChecklistItem) {
            if let Some(new_pointer) =
                indent_checklist_item_into_item(&mut self.document, &pointer, &target.path)
            {
                self.rebuild_segments();
                if !self.move_to_pointer(&new_pointer)
                    && !self.fallback_move_to_text(&new_pointer, false)
                {
                    self.ensure_cursor_selectable();
                }
                return true;
            }

            let Some(paragraph) = take_paragraph_at(&mut self.document, &pointer.paragraph_path)
            else {
                return false;
            };
            let Some(paragraph_path) =
                append_paragraph_as_checklist_child(&mut self.document, &target.path, paragraph)
            else {
                return false;
            };
            let mut new_pointer = pointer;
            new_pointer.paragraph_path = paragraph_path;
            self.rebuild_segments();
            if !self.move_to_pointer(&new_pointer)
                && !self.fallback_move_to_text(&new_pointer, false)
            {
                self.ensure_cursor_selectable();
            }
            return true;
        }

        if matches!(target.kind, IndentTargetKind::List)
            && let Some(source_ctx) = extract_entry_context(&pointer.paragraph_path)
            && !entry_has_multiple_paragraphs(&self.document, &source_ctx)
            && let Some(new_pointer) = indent_list_entry_into_foreign_list(
                &mut self.document,
                &pointer,
                &source_ctx,
                &target.path,
            )
        {
            self.rebuild_segments();
            if !self.move_to_pointer(&new_pointer)
                && !self.fallback_move_to_text(&new_pointer, false)
            {
                self.ensure_cursor_selectable();
            }
            return true;
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
            IndentTargetKind::ChecklistItem => unreachable!(),
        };
        let Some(paragraph_path) = new_path else {
            return false;
        };
        let mut new_pointer = pointer;
        new_pointer.paragraph_path = paragraph_path;
        self.rebuild_segments();
        if !self.move_to_pointer(&new_pointer) && !self.fallback_move_to_text(&new_pointer, false) {
            self.ensure_cursor_selectable();
        }
        true
    }

    pub fn unindent_current_paragraph(&mut self) -> bool {
        if self.cursor.paragraph_path.steps().len() <= 1 {
            return false;
        }
        let pointer = self.cursor_stable_pointer();

        if let Some(new_pointer) = unindent_checklist_item(&mut self.document, &pointer) {
            self.rebuild_segments();
            if !self.move_to_pointer(&new_pointer)
                && !self.fallback_move_to_text(&new_pointer, false)
            {
                self.ensure_cursor_selectable();
            }
            return true;
        }

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
        if !self.move_to_pointer(&new_pointer) && !self.fallback_move_to_text(&new_pointer, false) {
            self.ensure_cursor_selectable();
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
            if !self.move_to_pointer(&new_pointer)
                && !self.fallback_move_to_text(&new_pointer, false)
            {
                self.ensure_cursor_selectable();
            }
            true
        } else {
            if let Some(ctx) = extract_entry_context(&pointer.paragraph_path)
                && let Some(new_pointer) =
                    promote_list_entry_to_parent(&mut self.document, pointer, &ctx, paragraph_index)
            {
                self.rebuild_segments();
                if !self.move_to_pointer(&new_pointer)
                    && !self.fallback_move_to_text(&new_pointer, false)
                {
                    self.ensure_cursor_selectable();
                }
                return true;
            }
            let Some(mut new_pointer) = break_list_entry_for_non_list_target(
                &mut self.document,
                &pointer.paragraph_path,
                paragraph_type,
            ) else {
                return false;
            };
            new_pointer.offset = pointer.offset;
            self.rebuild_segments();
            if !self.move_to_pointer(&new_pointer)
                && !self.fallback_move_to_text(&new_pointer, false)
            {
                self.ensure_cursor_selectable();
            }
            true
        }
    }

    pub fn set_paragraph_type(&mut self, target: ParagraphType) -> bool {
        let current_pointer = self.cursor.clone();

        let mut in_checklist_context = false;
        if let Some(ctx) = extract_checklist_item_context(&current_pointer.paragraph_path) {
            if ctx.indices.len() > 1 {
                return false;
            }
            in_checklist_context = true;
        }
        let mut replacement_pointer = None;

        let mut operation_path = current_pointer.paragraph_path.clone();
        let mut pointer_hint = None;
        let mut post_merge_pointer = None;
        let mut handled_directly = false;
        let treat_as_singular_entry =
            is_single_paragraph_entry(&self.document, &current_pointer.paragraph_path);

        if !in_checklist_context
            && let Some(scope) = determine_parent_scope(&self.document, &operation_path)
        {
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

        if in_checklist_context && is_list_type(target) {
            if let Some(pointer) = break_list_entry_for_non_list_target(
                &mut self.document,
                &current_pointer.paragraph_path,
                target,
            ) {
                operation_path = pointer.paragraph_path.clone();
                pointer_hint = Some(pointer);
                handled_directly = true;
            } else {
                return false;
            }
        }

        if !is_list_type(target)
            && treat_as_singular_entry
            && let Some(pointer) =
                break_list_entry_for_non_list_target(&mut self.document, &operation_path, target)
        {
            operation_path = pointer.paragraph_path.clone();
            pointer_hint = Some(pointer);
            handled_directly = true;
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

        if !self.move_to_pointer(&desired) && !self.fallback_move_to_text(&desired, false) {
            self.ensure_cursor_selectable();
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
        let pointer = self.cursor.clone();

        // Check if we're trying to insert into an empty structure and populate it if needed
        // This must be done before prepare_cursor_for_text_insertion because that function
        // expects a valid text segment to exist
        let mut needs_rebuild = false;

        // Check for empty list entries
        if ensure_list_entry_has_paragraph(&mut self.document, &pointer.paragraph_path) {
            needs_rebuild = true;
        }

        // Check for empty checklist items
        if ensure_checklist_item_has_content(&mut self.document, &pointer.paragraph_path) {
            needs_rebuild = true;
        }

        if needs_rebuild {
            // Content was added, so we need to rebuild segments
            self.rebuild_segments();
        }

        if !self.prepare_cursor_for_text_insertion() {
            return false;
        }
        let pointer = self.cursor.clone();
        if !pointer.is_valid() {
            return false;
        }

        if insert_char_at(&mut self.document, &pointer, self.cursor.offset, ch) {
            self.cursor.offset += 1;
            // Incremental update: only rebuild segments for the modified paragraph
            self.update_segments_for_paragraph(&pointer.paragraph_path);
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
                    && let Some(segment) = self.segments.get(idx).cloned()
                {
                    self.cursor_segment = idx;
                    self.cursor.update_from_segment(&segment);
                    self.cursor.offset = segment.len;
                    return self.cursor.is_valid();
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
        if self.current_paragraph_is_empty()
            && self.remove_current_paragraph(RemovalDirection::Backward)
        {
            return true;
        }
        if self.cursor.offset == 0 {
            if self.try_merge_checklist_item_with_previous_paragraph() {
                return true;
            }
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
        if let Some(segment) = self.segments.get(self.cursor_segment)
            && segment.kind != SegmentKind::Text
        {
            if let Some(target_pointer) = self.remove_reveal_tag_segment(self.cursor_segment) {
                self.rebuild_segments();
                if !self.move_to_pointer(&target_pointer)
                    && !self.fallback_move_to_text(&target_pointer, false)
                {
                    self.ensure_cursor_selectable();
                }
                return true;
            } else {
                return false;
            }
        }
        let pointer = self.cursor.clone();
        if remove_char_at(&mut self.document, &pointer, self.cursor.offset) {
            // Incremental update: only rebuild segments for the modified paragraph
            self.update_segments_for_paragraph(&pointer.paragraph_path);
            true
        } else {
            false
        }
    }

    fn try_merge_checklist_item_with_previous_paragraph(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        if !matches!(self.cursor.segment_kind, SegmentKind::Text) {
            return false;
        }
        let current_segment = match self.segments.get(self.cursor_segment) {
            Some(segment) => segment.clone(),
            None => return false,
        };
        if !matches!(
            current_segment.paragraph_path.steps().last(),
            Some(PathStep::ChecklistItem { .. })
        ) {
            return false;
        }

        // Ensure we're at the start of the checklist item (ignoring zero-length and reveal segments)
        let mut idx = self.cursor_segment;
        while idx > 0 {
            idx -= 1;
            let segment = &self.segments[idx];
            if segment.paragraph_path != current_segment.paragraph_path {
                break;
            }
            if matches!(segment.kind, SegmentKind::Text) && segment.len > 0 {
                return false;
            }
        }

        let Some(ctx) = extract_checklist_item_context(&current_segment.paragraph_path) else {
            return false;
        };

        let previous_item_path = ctx.indices.last().and_then(|last| {
            if *last == 0 {
                return None;
            }
            let mut prev_indices = ctx.indices.clone();
            if let Some(last_mut) = prev_indices.last_mut() {
                *last_mut -= 1;
            }
            let mut steps = ctx.checklist_path.steps().to_vec();
            steps.push(PathStep::ChecklistItem {
                indices: prev_indices,
            });
            Some(ParagraphPath::from_steps(steps))
        });

        let target_path = if let Some(prev_path) = previous_item_path {
            prev_path
        } else {
            let mut search_idx = self.cursor_segment;
            let mut found_path: Option<ParagraphPath> = None;
            while search_idx > 0 {
                search_idx -= 1;
                let segment = &self.segments[search_idx];
                if !matches!(segment.kind, SegmentKind::Text) {
                    continue;
                }
                if segment.paragraph_path == current_segment.paragraph_path {
                    continue;
                }
                found_path = Some(segment.paragraph_path.clone());
                break;
            }
            match found_path {
                Some(path) => path,
                None => return false,
            }
        };
        let target_char_count: usize = self
            .segments
            .iter()
            .filter(|segment| {
                matches!(segment.kind, SegmentKind::Text) && segment.paragraph_path == target_path
            })
            .map(|segment| segment.len)
            .sum();

        enum MergeTargetKind {
            Paragraph,
            ChecklistItem,
        }

        let target_kind = if matches!(
            target_path.steps().last(),
            Some(PathStep::ChecklistItem { .. })
        ) {
            MergeTargetKind::ChecklistItem
        } else {
            if paragraph_ref(&self.document, &target_path).is_none() {
                return false;
            }
            MergeTargetKind::Paragraph
        };

        let Some(item) = take_checklist_item_at(&mut self.document, &ctx) else {
            return false;
        };

        // If we removed the only item in a top-level checklist, remove the checklist paragraph itself.
        if ctx.indices.len() == 1 {
            let mut remove_checklist = false;
            if let Some(paragraph) = paragraph_mut(&mut self.document, &ctx.checklist_path)
                && let Paragraph::Checklist { items } = paragraph
                && items.is_empty()
            {
                remove_checklist = true;
            }
            if remove_checklist {
                remove_paragraph_by_path(&mut self.document, &ctx.checklist_path);
            }
        }

        let ChecklistItem {
            checked: _,
            mut content,
            children,
        } = item;

        match target_kind {
            MergeTargetKind::Paragraph => {
                let maybe_paragraph = paragraph_mut(&mut self.document, &target_path);
                let Some(target_paragraph) = maybe_paragraph else {
                    self.rebuild_segments();
                    self.ensure_cursor_selectable();
                    return true;
                };
                let spans = target_paragraph.content_mut();
                if spans.is_empty() {
                    spans.push(Span::new_text(""));
                }
                if !content.is_empty() {
                    spans.append(&mut content);
                }
                prune_and_merge_spans(spans);

                if !children.is_empty() {
                    let child_paragraph = Paragraph::new_checklist().with_checklist_items(children);
                    let _ = insert_paragraph_after_parent(
                        &mut self.document,
                        &target_path,
                        child_paragraph,
                    );
                }
            }
            MergeTargetKind::ChecklistItem => {
                let Some(target_item) = checklist_item_mut(&mut self.document, &target_path) else {
                    return false;
                };
                if target_item.content.is_empty() {
                    target_item.content.push(Span::new_text(""));
                }
                if !content.is_empty() {
                    target_item.content.append(&mut content);
                }
                prune_and_merge_spans(&mut target_item.content);

                if !children.is_empty() {
                    target_item.children.extend(children);
                }
            }
        }

        self.rebuild_segments();

        if let Some(pointer) =
            self.pointer_at_paragraph_char_offset(&target_path, target_char_count)
        {
            if !self.move_to_pointer(&pointer) && !self.fallback_move_to_text(&pointer, false) {
                self.ensure_cursor_selectable();
            }
        } else {
            self.ensure_cursor_selectable();
        }

        true
    }

    fn pointer_at_paragraph_char_offset(
        &self,
        path: &ParagraphPath,
        mut char_offset: usize,
    ) -> Option<CursorPointer> {
        for segment in &self.segments {
            if segment.paragraph_path == *path && matches!(segment.kind, SegmentKind::Text) {
                if char_offset <= segment.len {
                    return Some(CursorPointer {
                        paragraph_path: segment.paragraph_path.clone(),
                        span_path: segment.span_path.clone(),
                        offset: char_offset,
                        segment_kind: SegmentKind::Text,
                    });
                }
                char_offset = char_offset.saturating_sub(segment.len);
            }
        }
        None
    }

    fn try_merge_with_next_paragraph(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        if !matches!(self.cursor.segment_kind, SegmentKind::Text) {
            return false;
        }

        // Get current segment
        let current_segment = match self.segments.get(self.cursor_segment) {
            Some(segment) => segment.clone(),
            None => return false,
        };

        // Find the next text segment in a different paragraph
        let mut next_segment_idx = self.cursor_segment + 1;
        while next_segment_idx < self.segments.len() {
            let segment = &self.segments[next_segment_idx];
            if matches!(segment.kind, SegmentKind::Text)
                && segment.paragraph_path != current_segment.paragraph_path
            {
                break;
            }
            next_segment_idx += 1;
        }

        if next_segment_idx >= self.segments.len() {
            return false;
        }

        let next_segment = self.segments[next_segment_idx].clone();

        // Calculate current paragraph's character count for cursor positioning
        let current_char_count: usize = self
            .segments
            .iter()
            .filter(|segment| {
                matches!(segment.kind, SegmentKind::Text)
                    && segment.paragraph_path == current_segment.paragraph_path
            })
            .map(|segment| segment.len)
            .sum();

        // Handle list entry merging
        if matches!(
            next_segment.paragraph_path.steps().last(),
            Some(PathStep::Entry { .. })
        ) {
            let Some(ctx) = extract_entry_context(&next_segment.paragraph_path) else {
                return false;
            };

            // Only merge single-paragraph entries
            if ctx.paragraph_index != 0 {
                return false;
            }

            let Some((entry, list_became_empty)) = take_list_entry(&mut self.document, &ctx) else {
                return false;
            };

            // Remove the list if it's now empty
            if list_became_empty {
                remove_paragraph_by_path(&mut self.document, &ctx.list_path);
            }

            // Get the first (and should be only) paragraph from the entry
            if entry.is_empty() {
                self.rebuild_segments();
                if let Some(pointer) =
                    self.pointer_at_paragraph_char_offset(&current_segment.paragraph_path, current_char_count)
                {
                    if !self.move_to_pointer(&pointer) && !self.fallback_move_to_text(&pointer, false) {
                        self.ensure_cursor_selectable();
                    }
                } else {
                    self.ensure_cursor_selectable();
                }
                return true;
            }

            let first_paragraph = entry.into_iter().next().unwrap();
            let mut entry_content = match first_paragraph {
                Paragraph::Text { content }
                | Paragraph::Header1 { content }
                | Paragraph::Header2 { content }
                | Paragraph::Header3 { content }
                | Paragraph::CodeBlock { content } => content,
                _ => {
                    // Can't merge complex paragraphs
                    self.rebuild_segments();
                    self.ensure_cursor_selectable();
                    return false;
                }
            };

            // Determine target based on current paragraph type
            let is_current_checklist = matches!(
                current_segment.paragraph_path.steps().last(),
                Some(PathStep::ChecklistItem { .. })
            );

            if is_current_checklist {
                // Merge into current checklist item
                let Some(current_item) = checklist_item_mut(&mut self.document, &current_segment.paragraph_path) else {
                    return false;
                };
                if current_item.content.is_empty() {
                    current_item.content.push(Span::new_text(""));
                }
                if !entry_content.is_empty() {
                    current_item.content.append(&mut entry_content);
                }
                prune_and_merge_spans(&mut current_item.content);
            } else {
                // Merge into current regular paragraph
                let Some(current_paragraph) = paragraph_mut(&mut self.document, &current_segment.paragraph_path) else {
                    return false;
                };
                let spans = current_paragraph.content_mut();
                if spans.is_empty() {
                    spans.push(Span::new_text(""));
                }
                if !entry_content.is_empty() {
                    spans.append(&mut entry_content);
                }
                prune_and_merge_spans(spans);
            }

            self.rebuild_segments();

            // Position cursor at the junction point
            if let Some(pointer) =
                self.pointer_at_paragraph_char_offset(&current_segment.paragraph_path, current_char_count)
            {
                if !self.move_to_pointer(&pointer) && !self.fallback_move_to_text(&pointer, false) {
                    self.ensure_cursor_selectable();
                }
            } else {
                self.ensure_cursor_selectable();
            }

            return true;
        }

        // Handle checklist item merging
        if matches!(
            next_segment.paragraph_path.steps().last(),
            Some(PathStep::ChecklistItem { .. })
        ) {
            // Take the next checklist item
            let Some(ctx) = extract_checklist_item_context(&next_segment.paragraph_path) else {
                return false;
            };

            let Some(next_item) = take_checklist_item_at(&mut self.document, &ctx) else {
                return false;
            };

            // If we removed the only item in a top-level checklist, remove the checklist paragraph
            if ctx.indices.len() == 1 {
                let mut remove_checklist = false;
                if let Some(paragraph) = paragraph_mut(&mut self.document, &ctx.checklist_path)
                    && let Paragraph::Checklist { items } = paragraph
                    && items.is_empty()
                {
                    remove_checklist = true;
                }
                if remove_checklist {
                    remove_paragraph_by_path(&mut self.document, &ctx.checklist_path);
                }
            }

            let ChecklistItem {
                checked: _,
                mut content,
                children,
            } = next_item;

            // Determine if current paragraph is a checklist item
            let is_current_checklist = matches!(
                current_segment.paragraph_path.steps().last(),
                Some(PathStep::ChecklistItem { .. })
            );

            if is_current_checklist {
                // Merge into current checklist item
                let Some(current_item) = checklist_item_mut(&mut self.document, &current_segment.paragraph_path) else {
                    return false;
                };
                if current_item.content.is_empty() {
                    current_item.content.push(Span::new_text(""));
                }
                if !content.is_empty() {
                    current_item.content.append(&mut content);
                }
                prune_and_merge_spans(&mut current_item.content);

                if !children.is_empty() {
                    current_item.children.extend(children);
                }
            } else {
                // Merge into current regular paragraph
                let Some(current_paragraph) = paragraph_mut(&mut self.document, &current_segment.paragraph_path) else {
                    return false;
                };
                let spans = current_paragraph.content_mut();
                if spans.is_empty() {
                    spans.push(Span::new_text(""));
                }
                if !content.is_empty() {
                    spans.append(&mut content);
                }
                prune_and_merge_spans(spans);

                if !children.is_empty() {
                    let child_paragraph = Paragraph::new_checklist().with_checklist_items(children);
                    let _ = insert_paragraph_after_parent(
                        &mut self.document,
                        &current_segment.paragraph_path,
                        child_paragraph,
                    );
                }
            }
        } else {
            // Merge regular paragraph or quote child
            // Check if next paragraph is a quote child so we can clean up empty quote
            let quote_parent_path = if matches!(
                next_segment.paragraph_path.steps().last(),
                Some(PathStep::Child(_))
            ) {
                // Extract the parent quote path
                let steps = next_segment.paragraph_path.steps();
                if steps.len() > 1 {
                    Some(ParagraphPath::from_steps(steps[..steps.len() - 1].to_vec()))
                } else {
                    None
                }
            } else {
                None
            };

            let Some(next_paragraph) = take_paragraph_at(&mut self.document, &next_segment.paragraph_path) else {
                return false;
            };

            // If we took a quote child, check if the quote is now empty and remove it
            if let Some(ref parent_path) = quote_parent_path {
                let mut remove_quote = false;
                if let Some(quote) = paragraph_mut(&mut self.document, parent_path)
                    && let Paragraph::Quote { children } = quote
                    && children.is_empty()
                {
                    remove_quote = true;
                }
                if remove_quote {
                    remove_paragraph_by_path(&mut self.document, parent_path);
                }
            }

            let mut next_content = match next_paragraph {
                Paragraph::Text { content }
                | Paragraph::Header1 { content }
                | Paragraph::Header2 { content }
                | Paragraph::Header3 { content }
                | Paragraph::CodeBlock { content } => content,
                Paragraph::Checklist { .. } | Paragraph::Quote { .. } => {
                    // These should have been handled by the checklist case above
                    return false;
                }
                Paragraph::OrderedList { .. } | Paragraph::UnorderedList { .. } => {
                    // Lists shouldn't be merged like this
                    return false;
                }
            };

            // Determine target based on current paragraph type
            let is_current_checklist = matches!(
                current_segment.paragraph_path.steps().last(),
                Some(PathStep::ChecklistItem { .. })
            );

            if is_current_checklist {
                // Merge into current checklist item
                let Some(current_item) = checklist_item_mut(&mut self.document, &current_segment.paragraph_path) else {
                    return false;
                };
                if current_item.content.is_empty() {
                    current_item.content.push(Span::new_text(""));
                }
                if !next_content.is_empty() {
                    current_item.content.append(&mut next_content);
                }
                prune_and_merge_spans(&mut current_item.content);
            } else {
                // Merge into current regular paragraph
                let Some(current_paragraph) = paragraph_mut(&mut self.document, &current_segment.paragraph_path) else {
                    return false;
                };
                let spans = current_paragraph.content_mut();
                if spans.is_empty() {
                    spans.push(Span::new_text(""));
                }
                if !next_content.is_empty() {
                    spans.append(&mut next_content);
                }
                prune_and_merge_spans(spans);
            }
        }

        self.rebuild_segments();

        // Position cursor at the junction point
        if let Some(pointer) =
            self.pointer_at_paragraph_char_offset(&current_segment.paragraph_path, current_char_count)
        {
            if !self.move_to_pointer(&pointer) && !self.fallback_move_to_text(&pointer, false) {
                self.ensure_cursor_selectable();
            }
        } else {
            self.ensure_cursor_selectable();
        }

        true
    }

    pub fn delete(&mut self) -> bool {
        if self.segments.is_empty() {
            return false;
        }
        if self.current_paragraph_is_empty()
            && self.remove_current_paragraph(RemovalDirection::Forward)
        {
            return true;
        }
        let current_len = self.current_segment_len();
        if self.cursor.offset < current_len {
            if self.cursor.segment_kind != SegmentKind::Text {
                if let Some(target_pointer) = self.remove_reveal_tag_segment(self.cursor_segment) {
                    self.rebuild_segments();
                    if !self.move_to_pointer(&target_pointer)
                        && !self.fallback_move_to_text(&target_pointer, false)
                    {
                        self.ensure_cursor_selectable();
                    }
                    return true;
                } else {
                    return false;
                }
            }
            let pointer = self.cursor.clone();
            if remove_char_at(&mut self.document, &pointer, self.cursor.offset) {
                // Incremental update: only rebuild segments for the modified paragraph
                self.update_segments_for_paragraph(&pointer.paragraph_path);
                return true;
            }
            return false;
        }

        // Check if we're at the end of the current segment
        // and need to merge with the next paragraph
        if self.cursor_segment + 1 >= self.segments.len() {
            return false;
        }

        let next_segment = &self.segments[self.cursor_segment + 1];
        if next_segment.kind != SegmentKind::Text {
            if let Some(target_pointer) = self.remove_reveal_tag_segment(self.cursor_segment + 1) {
                self.rebuild_segments();
                if !self.move_to_pointer(&target_pointer)
                    && !self.fallback_move_to_text(&target_pointer, false)
                {
                    self.ensure_cursor_selectable();
                }
                return true;
            } else {
                return false;
            }
        }

        // Check if the next segment is in a different paragraph
        let current_para = &self.segments[self.cursor_segment].paragraph_path;
        let next_para = &next_segment.paragraph_path;

        if current_para != next_para {
            // Different paragraphs: merge them
            return self.try_merge_with_next_paragraph();
        }

        // Same paragraph: just delete the next character
        let pointer = CursorPointer {
            paragraph_path: next_segment.paragraph_path.clone(),
            span_path: next_segment.span_path.clone(),
            offset: 0,
            segment_kind: next_segment.kind,
        };
        if remove_char_at(&mut self.document, &pointer, 0) {
            // Same paragraph: use incremental update
            self.update_segments_for_paragraph(&pointer.paragraph_path);
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
        let paragraph = paragraph_mut(&mut self.document, &segment.paragraph_path)?;
        let span = span_mut(paragraph, &segment.span_path)?;
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

        // Get the root index of the paragraph being removed (if it's a root paragraph)
        let removed_root_index = current_path.root_index();

        if !remove_paragraph_by_path(&mut self.document, &current_path) {
            return false;
        }

        self.rebuild_segments();

        if self.segments.is_empty() {
            return true;
        }

        // Adjust target pointer if needed: if we removed a root paragraph at index N,
        // all root paragraphs at index > N are now at index - 1
        let adjusted_target_pointer = if let Some(mut pointer) = target_pointer {
            if let (Some(target_idx), Some(removed_idx)) = (pointer.paragraph_path.root_index(), removed_root_index) {
                if target_idx > removed_idx {
                    // Create a new path with decremented root index
                    let steps = pointer.paragraph_path.steps();
                    if let Some(PathStep::Root(idx)) = steps.first() {
                        let mut new_steps = steps.to_vec();
                        new_steps[0] = PathStep::Root(idx - 1);
                        pointer.paragraph_path = ParagraphPath::from_steps(new_steps);
                    }
                }
            }
            Some(pointer)
        } else {
            None
        };

        if let Some(pointer) = adjusted_target_pointer
            && self.move_to_pointer(&pointer)
        {
            return true;
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
