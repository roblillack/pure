use super::{CursorPointer, checklist_item_mut, paragraph_mut, span_mut, span_mut_from_item};
use tdoc::{ChecklistItem, Document, InlineStyle, Span};

pub(crate) fn insert_char_at(
    document: &mut Document,
    pointer: &CursorPointer,
    offset: usize,
    ch: char,
) -> bool {
    if let Some(item) = checklist_item_mut(document, &pointer.paragraph_path) {
        let Some(span) = span_mut_from_item(item, &pointer.span_path) else {
            return false;
        };
        let char_len = span.text.chars().count();
        let clamped_offset = offset.min(char_len);
        let byte_idx = char_to_byte_idx(&span.text, clamped_offset);
        span.text.insert(byte_idx, ch);
        return true;
    }

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

pub(crate) fn remove_char_at(
    document: &mut Document,
    pointer: &CursorPointer,
    offset: usize,
) -> bool {
    if let Some(item) = checklist_item_mut(document, &pointer.paragraph_path) {
        let Some(span) = span_mut_from_item(item, &pointer.span_path) else {
            return false;
        };
        return remove_char_from_text(&mut span.text, offset);
    }

    let Some(paragraph) = paragraph_mut(document, &pointer.paragraph_path) else {
        return false;
    };
    let Some(span) = span_mut(paragraph, &pointer.span_path) else {
        return false;
    };
    remove_char_from_text(&mut span.text, offset)
}

fn remove_char_from_text(text: &mut String, offset: usize) -> bool {
    let char_len = text.chars().count();
    if offset >= char_len {
        return false;
    }
    let start = char_to_byte_idx(text, offset);
    let end = char_to_byte_idx(text, offset + 1);
    if start >= end || end > text.len() {
        return false;
    }
    text.drain(start..end);
    true
}

pub fn char_to_byte_idx(text: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    for (count, (byte_idx, _)) in text.char_indices().enumerate() {
        if count == char_idx {
            return byte_idx;
        }
    }
    text.len()
}

pub(crate) fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub fn previous_word_boundary(text: &str, offset: usize) -> usize {
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

/// Find the start of the word at the given offset (for word selection).
pub fn word_start_boundary(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut idx = offset.min(chars.len());
    if idx == 0 {
        return 0;
    }

    // If we're on whitespace, skip backward to find the previous word
    if idx < chars.len() && chars[idx].is_whitespace() {
        // Skip backward through whitespace
        while idx > 0 && chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        if idx == 0 {
            return 0;
        }
    }

    // Check what character we're on/before
    let check_idx = if idx < chars.len() { idx } else { idx - 1 };

    // If we're on/in a word character, move to the start of this word
    if check_idx < chars.len() && is_word_char(chars[check_idx]) {
        while idx > 0 && is_word_char(chars[idx - 1]) {
            idx -= 1;
        }
        return idx;
    }

    // If we're on punctuation or other non-word, non-whitespace character
    if check_idx < chars.len()
        && !chars[check_idx].is_whitespace()
        && !is_word_char(chars[check_idx])
    {
        while idx > 0 && !chars[idx - 1].is_whitespace() && !is_word_char(chars[idx - 1]) {
            idx -= 1;
        }
        return idx;
    }

    idx
}

pub fn next_word_boundary(text: &str, offset: usize) -> usize {
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

/// Find the end of the word at the given offset (for word selection).
pub fn word_end_boundary(text: &str, offset: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut idx = offset.min(len);
    if idx >= len {
        return len;
    }

    // If we're on whitespace, skip it to find the next word
    if chars[idx].is_whitespace() {
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        if idx >= len {
            return len;
        }
    }

    // If we're on a word character, move to the end of the word
    if is_word_char(chars[idx]) {
        while idx < len && is_word_char(chars[idx]) {
            idx += 1;
        }
        return idx;
    }

    // If we're on punctuation or other non-word, non-whitespace character
    while idx < len && !chars[idx].is_whitespace() && !is_word_char(chars[idx]) {
        idx += 1;
    }
    idx
}

pub(crate) fn skip_leading_whitespace(text: &str) -> usize {
    text.chars().take_while(|ch| ch.is_whitespace()).count()
}

pub(crate) fn apply_style_to_span_path(
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

pub(crate) fn prune_and_merge_spans(spans: &mut Vec<Span>) {
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

pub(crate) fn split_spans(spans: &mut Vec<Span>, path: &[usize], offset: usize) -> Vec<Span> {
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

pub(crate) fn span_is_empty(span: &Span) -> bool {
    span.text.is_empty() && span.children.iter().all(span_is_empty)
}

pub(crate) fn checklist_item_is_empty(item: &ChecklistItem) -> bool {
    item.content.iter().all(span_is_empty) && item.children.iter().all(checklist_item_is_empty)
}
