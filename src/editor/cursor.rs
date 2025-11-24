use super::content::{next_word_boundary, previous_word_boundary, skip_leading_whitespace};
use super::inspect::{
    breadcrumbs_for_pointer, checklist_item_ref, collect_segments, paragraph_path_is_prefix,
    paragraph_ref, span_path_is_prefix, span_ref, span_ref_from_item,
};
use super::{
    CursorPointer, DocumentEditor, ParagraphPath, PointerKey, SegmentKind, SegmentRef,
    select_text_in_paragraph,
};
use tdoc::{Paragraph, Span};

impl DocumentEditor {
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

    pub(crate) fn span_text_for_pointer<'a>(&'a self, pointer: &CursorPointer) -> Option<&'a str> {
        if let Some(item) = checklist_item_ref(&self.document, &pointer.paragraph_path) {
            let span = span_ref_from_item(item, &pointer.span_path)?;
            return Some(span.text.as_str());
        }

        let paragraph = paragraph_ref(&self.document, &pointer.paragraph_path)?;
        let span = span_ref(paragraph, &pointer.span_path)?;
        Some(span.text.as_str())
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

    pub(crate) fn previous_paragraph_path(&self) -> Option<ParagraphPath> {
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

    pub(crate) fn next_paragraph_path(&self) -> Option<ParagraphPath> {
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

    pub(crate) fn move_to_paragraph_path(
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

    pub(crate) fn fallback_move_to_text(
        &mut self,
        pointer: &CursorPointer,
        prefer_trailing: bool,
    ) -> bool {
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

    pub(crate) fn pointer_key(&self, pointer: &CursorPointer) -> Option<PointerKey> {
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

    pub(crate) fn shift_to_previous_segment(&mut self) -> bool {
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

    pub(crate) fn shift_to_next_segment(&mut self) -> bool {
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

    pub(crate) fn skip_forward_reveal_segments(&mut self) {
        while matches!(
            self.current_segment_kind(),
            Some(SegmentKind::RevealStart(_) | SegmentKind::RevealEnd(_))
        ) {
            if !self.shift_to_next_segment() {
                break;
            }
        }
    }

    pub(crate) fn current_segment_len(&self) -> usize {
        self.segments
            .get(self.cursor_segment)
            .map(|segment| segment.len)
            .unwrap_or(0)
    }

    pub(crate) fn find_previous_text_segment_in_paragraph(
        &self,
        start_idx: usize,
    ) -> Option<usize> {
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

    pub(crate) fn current_segment_kind(&self) -> Option<SegmentKind> {
        self.segments
            .get(self.cursor_segment)
            .map(|segment| segment.kind)
    }

    pub(crate) fn next_segment_kind(&self) -> Option<SegmentKind> {
        self.segments
            .get(self.cursor_segment + 1)
            .map(|segment| segment.kind)
    }

    pub(crate) fn normalize_cursor_after_forward_move(&mut self) {
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

    pub(crate) fn normalize_cursor_after_backward_move(&mut self, crossed_boundary: bool) {
        let pending_boundary = crossed_boundary;
        let Some(segment) = self.segments.get(self.cursor_segment) else {
            return;
        };
        match segment.kind {
            SegmentKind::RevealStart(_) | SegmentKind::RevealEnd(_) => {
                if self.cursor.offset > 0 {
                    self.cursor.offset = 0;
                }
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
            }
        }
    }

    pub(crate) fn current_span_text(&self) -> Option<&str> {
        let segment = self.segments.get(self.cursor_segment)?;
        if segment.kind != SegmentKind::Text {
            return None;
        }
        if let Some(item) = checklist_item_ref(&self.document, &self.cursor.paragraph_path) {
            let span = span_ref_from_item(item, &self.cursor.span_path)?;
            return Some(span.text.as_str());
        }
        let paragraph = paragraph_ref(&self.document, &self.cursor.paragraph_path)?;
        let span = span_ref(paragraph, &self.cursor.span_path)?;
        Some(span.text.as_str())
    }

    pub(crate) fn segment_text(&self, segment: &SegmentRef) -> Option<&str> {
        if segment.kind != SegmentKind::Text {
            return None;
        }
        if let Some(item) = checklist_item_ref(&self.document, &segment.paragraph_path) {
            let span = span_ref_from_item(item, &segment.span_path)?;
            return Some(span.text.as_str());
        }
        let paragraph = paragraph_ref(&self.document, &segment.paragraph_path)?;
        let span = span_ref(paragraph, &segment.span_path)?;
        Some(span.text.as_str())
    }

    pub(crate) fn previous_word_position(&self) -> Option<(usize, CursorPointer)> {
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

    pub(crate) fn next_word_position(&self) -> Option<(usize, CursorPointer)> {
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

    pub(crate) fn count_backward_steps(
        &self,
        target_segment: usize,
        target_offset: usize,
    ) -> usize {
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

    pub(crate) fn count_forward_steps(&self, target_segment: usize, target_offset: usize) -> usize {
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

    pub(crate) fn rebuild_segments(&mut self) {
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

    /// Incrementally update segments for a single paragraph without rebuilding the entire tree.
    /// This is much faster than rebuild_segments() for localized changes.
    pub(crate) fn update_segments_for_paragraph(&mut self, root_path: &ParagraphPath) {
        // Find the range of segments belonging to this paragraph tree
        let (start_idx, end_idx) = self.find_paragraph_segment_range(root_path);

        // Collect new segments for just this paragraph tree
        let new_segments = super::inspect::collect_segments_for_paragraph_tree(
            &self.document,
            root_path,
            self.reveal_codes,
        );

        // Replace the old segment range with new segments
        self.segments.splice(start_idx..end_idx, new_segments);

        // If segments are now empty, ensure we have a placeholder
        if self.segments.is_empty() {
            self.ensure_placeholder_segment();
            self.segments = collect_segments(&self.document, self.reveal_codes);
        }

        if self.segments.is_empty() {
            self.cursor = CursorPointer::default();
            self.cursor_segment = 0;
            return;
        }

        // Re-sync cursor position
        self.sync_cursor_segment();
        self.clamp_cursor_offset();
    }

    /// Find the range [start, end) of segments belonging to a paragraph path and all its descendants.
    fn find_paragraph_segment_range(&self, root_path: &ParagraphPath) -> (usize, usize) {
        use super::inspect::paragraph_path_is_prefix;

        let start = self
            .segments
            .iter()
            .position(|seg| paragraph_path_is_prefix(root_path, &seg.paragraph_path))
            .unwrap_or(self.segments.len());

        let end = self.segments[start..]
            .iter()
            .position(|seg| !paragraph_path_is_prefix(root_path, &seg.paragraph_path))
            .map(|offset| start + offset)
            .unwrap_or(self.segments.len());

        (start, end)
    }

    pub(crate) fn ensure_placeholder_segment(&mut self) {
        if self.document.paragraphs.is_empty() {
            self.document
                .paragraphs
                .push(Paragraph::new_text().with_content(vec![Span::new_text("")]));
        } else if let Some(first) = self.document.paragraphs.get_mut(0)
            && first.paragraph_type().is_leaf()
            && first.content().is_empty()
        {
            first.content_mut().push(Span::new_text(""));
        }
    }

    pub(crate) fn sync_cursor_segment(&mut self) {
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

    pub(crate) fn clamp_cursor_offset(&mut self) {
        let len = self.current_segment_len();
        if self.cursor.offset > len {
            self.cursor.offset = len;
        }
    }

    pub(crate) fn nearest_text_pointer_for(
        &self,
        pointer: &CursorPointer,
    ) -> Option<CursorPointer> {
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

    pub(crate) fn find_text_pointer_forward(&self, start_index: usize) -> Option<CursorPointer> {
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

    pub(crate) fn find_text_pointer_backward(&self, start_index: usize) -> Option<CursorPointer> {
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
}
