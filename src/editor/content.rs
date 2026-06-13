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

/// Applies `style` to the character range delimited by two leaf positions of
/// the same content root, stacking it on top of any styles already present.
/// `InlineStyle::None` clears all formatting from the range instead.
pub(crate) fn apply_style_to_content_range(
    spans: &mut Vec<Span>,
    start_path: &[usize],
    start: usize,
    end_path: &[usize],
    end: usize,
    style: InlineStyle,
) -> bool {
    if start_path.is_empty() || end_path.is_empty() {
        return false;
    }
    if start_path == end_path && style != InlineStyle::None {
        return stack_style_in_leaf_span(spans, start_path, start, end, style);
    }

    // Carve the selected range out of the span tree. Splitting at the end
    // first keeps the start path valid for the second split.
    let tail = split_spans(spans, end_path, end);
    let mut mid = split_spans(spans, start_path, start);
    if mid.iter().all(span_is_empty) {
        spans.extend(mid);
        spans.extend(tail);
        return false;
    }

    if style == InlineStyle::None {
        // Clearing strips every style in the range, including any carried by
        // ancestor spans the split cloned into `mid`.
        let mut text = String::new();
        collect_span_text(&mid, &mut text);
        mid = vec![Span::new_text(text)];
    } else if mid
        .iter()
        .all(|span| span.style == InlineStyle::None && span.children.is_empty())
    {
        for span in &mut mid {
            span.style = style;
        }
    } else {
        mid = vec![Span::new_styled(style).with_children(mid)];
    }

    spans.extend(mid);
    spans.extend(tail);
    true
}

/// Replaces the character range delimited by two leaf positions of the same
/// content root with a single span. With a `target` the new span is a
/// [`InlineStyle::Link`] carrying that URL; without one it is plain text,
/// which is how clearing a link's target unlinks it. An empty range (the two
/// positions coincide) inserts the span without removing anything.
pub(crate) fn replace_range_with_link(
    spans: &mut Vec<Span>,
    start_path: &[usize],
    start: usize,
    end_path: &[usize],
    end: usize,
    text: &str,
    target: Option<&str>,
) -> bool {
    if start_path.is_empty() || end_path.is_empty() {
        return false;
    }

    // Carve the selected range out of the span tree. Splitting at the end
    // first keeps the start path valid for the second split. The carved-out
    // middle is discarded: the dialog supplies the replacement text in full.
    let tail = split_spans(spans, end_path, end);
    let _removed = split_spans(spans, start_path, start);

    let new_span = match target {
        Some(target) => {
            let mut span = Span::new_styled(InlineStyle::Link);
            span.text = text.to_string();
            span.link_target = Some(target.to_string());
            span
        }
        None => Span::new_text(text.to_string()),
    };
    if !new_span.is_content_empty() {
        spans.push(new_span);
    }
    spans.extend(tail);
    true
}

/// Applies `style` to part of a single leaf span. Plain leaves split into
/// styled siblings as before; styled leaves keep their own style and gain a
/// nested child span, so the styles stack.
fn stack_style_in_leaf_span(
    spans: &mut Vec<Span>,
    path: &[usize],
    start: usize,
    end: usize,
    style: InlineStyle,
) -> bool {
    let idx = path[0];
    if idx >= spans.len() {
        return false;
    }
    if path.len() > 1 {
        return stack_style_in_leaf_span(&mut spans[idx].children, &path[1..], start, end, style);
    }

    let span = &mut spans[idx];
    let len = span.text.chars().count();
    let clamped_end = end.min(len);
    let clamped_start = start.min(clamped_end);
    if clamped_start >= clamped_end {
        return false;
    }

    let (before_end, right_text) = split_text(&span.text, clamped_end);
    let (left_text, mid_text) = split_text(&before_end, clamped_start);
    if mid_text.is_empty() {
        return false;
    }

    if span.style == InlineStyle::None && span.children.is_empty() && span.link_target.is_none() {
        let mut replacements = Vec::new();
        if !left_text.is_empty() {
            replacements.push(Span::new_text(left_text));
        }
        replacements.push(Span::new_styled(style).with_text(mid_text));
        if !right_text.is_empty() {
            replacements.push(Span::new_text(right_text));
        }
        spans.splice(idx..=idx, replacements);
        return true;
    }

    if span.style == style {
        return false;
    }

    // The leaf already carries a style: keep it on the span and nest the
    // newly styled range as a child, ahead of any existing children (which
    // render after the span's own text).
    span.text = left_text;
    let mut new_children = vec![Span::new_styled(style).with_text(mid_text)];
    if !right_text.is_empty() {
        new_children.push(Span::new_text(right_text));
    }
    new_children.append(&mut span.children);
    span.children = new_children;
    true
}

fn collect_span_text(spans: &[Span], buffer: &mut String) {
    for span in spans {
        buffer.push_str(&span.text);
        collect_span_text(&span.children, buffer);
    }
}

pub(crate) fn prune_and_merge_spans(spans: &mut Vec<Span>) {
    strip_redundant_styles(spans, &mut Vec::new());
    prune_and_merge_spans_rec(spans);
}

/// Resets the style of spans that repeat a style already provided by an
/// ancestor, so stacking the same style twice does not pile up tags. Links
/// are exempt: nested links can have different targets.
fn strip_redundant_styles(spans: &mut [Span], active: &mut Vec<InlineStyle>) {
    for span in spans {
        if span.style != InlineStyle::None
            && span.style != InlineStyle::Link
            && active.contains(&span.style)
        {
            span.style = InlineStyle::None;
        }
        let pushed = span.style != InlineStyle::None;
        if pushed {
            active.push(span.style);
        }
        strip_redundant_styles(&mut span.children, active);
        if pushed {
            active.pop();
        }
    }
}

fn prune_and_merge_spans_rec(spans: &mut Vec<Span>) {
    let mut idx = 0;
    while idx < spans.len() {
        prune_and_merge_spans_rec(&mut spans[idx].children);
        let span = &mut spans[idx];
        // Leading style-less child spans render exactly like the span's own
        // text, so fold them into it.
        while let Some(first) = span.children.first() {
            if first.style != InlineStyle::None
                || first.link_target.is_some()
                || !first.children.is_empty()
            {
                break;
            }
            let first = span.children.remove(0);
            span.text.push_str(&first.text);
        }
        if span.style == InlineStyle::None && !span.children.is_empty() {
            // A style-less wrapper contributes nothing: splice its text and
            // children into this level.
            let children = std::mem::take(&mut span.children);
            let text = std::mem::take(&mut span.text);
            let mut replacements = Vec::new();
            if !text.is_empty() {
                replacements.push(Span::new_text(text));
            }
            replacements.extend(children);
            spans.splice(idx..=idx, replacements);
            continue;
        }
        if span.text.is_empty() && span.children.is_empty() {
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
            spans[i].children = right.children;
        } else {
            i += 1;
        }
    }
}

fn can_merge_spans(left: &Span, right: &Span) -> bool {
    // The left span must be childless: its children would otherwise render
    // between the two texts. The right span's children simply carry over.
    left.style == right.style && left.link_target == right.link_target && left.children.is_empty()
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
                // The children render after the span's own text, so they
                // belong entirely to the new (right) span.
                span.children.clear();
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
