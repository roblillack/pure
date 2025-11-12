const MARKER_PREFIX: &str = "1337;M";
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
