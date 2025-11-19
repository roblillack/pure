use super::{
    CursorPointer, ParagraphPath, PathStep, SegmentKind, SegmentRef, SpanPath, inline_style_label,
};
use tdoc::{ChecklistItem, Document, InlineStyle, Paragraph, ParagraphType, Span};

pub fn collect_segments(document: &Document, reveal_codes: bool) -> Vec<SegmentRef> {
    let mut result = Vec::new();
    for (idx, paragraph) in document.paragraphs.iter().enumerate() {
        let mut path = ParagraphPath::new_root(idx);
        collect_paragraph_segments(paragraph, &mut path, reveal_codes, &mut result);
    }
    result
}

/// Collect segments for a single paragraph subtree (including all descendants).
/// This is used for incremental updates when only one paragraph changes.
pub fn collect_segments_for_paragraph_tree(
    document: &Document,
    root_path: &ParagraphPath,
    reveal_codes: bool,
) -> Vec<SegmentRef> {
    let mut result = Vec::new();
    if let Some(paragraph) = paragraph_ref(document, root_path) {
        let mut path = root_path.clone();
        collect_paragraph_segments(paragraph, &mut path, reveal_codes, &mut result);
    }
    result
}

pub fn breadcrumbs_for_pointer(
    document: &Document,
    pointer: &CursorPointer,
) -> Option<Vec<String>> {
    if pointer.paragraph_path.is_empty() {
        return None;
    }
    let (mut labels, target) = collect_paragraph_labels(document, &pointer.paragraph_path)?;
    let inline_labels = match target {
        LabelTarget::Paragraph(paragraph) => collect_inline_labels(paragraph, &pointer.span_path)?,
        LabelTarget::ChecklistItem(item) => {
            collect_inline_labels_from_item(item, &pointer.span_path)?
        }
    };
    labels.extend(inline_labels);
    Some(labels)
}

enum LabelTarget<'a> {
    Paragraph(&'a Paragraph),
    ChecklistItem(&'a ChecklistItem),
}

fn collect_paragraph_labels<'a>(
    document: &'a Document,
    path: &ParagraphPath,
) -> Option<(Vec<String>, LabelTarget<'a>)> {
    let mut labels = Vec::new();
    let mut current: Option<&'a Paragraph> = None;
    let mut current_item: Option<&'a ChecklistItem> = None;
    let mut traversed = Vec::new();

    for step in path.steps() {
        traversed.push(step.clone());
        let paragraph = match *step {
            PathStep::Root(idx) => document.paragraphs.get(idx)?,
            PathStep::Child(idx) => {
                let parent = current?;
                parent.children().get(idx)?
            }
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => {
                let parent = current?;
                let entry = parent.entries().get(entry_index)?;
                entry.get(paragraph_index)?
            }
            PathStep::ChecklistItem { ref indices } => {
                if indices.len() > 1 {
                    for _ in 1..indices.len() {
                        labels.push("Checklist".to_string());
                    }
                }
                if labels.is_empty() {
                    labels.push("Checklist".to_string());
                }
                let current_path = ParagraphPath::from_steps(traversed.clone());
                current_item = checklist_item_ref(document, &current_path);
                continue;
            }
            PathStep::Root(_) => return None,
        };
        let current_path = ParagraphPath::from_steps(traversed.clone());
        let hide_label = text_effective_relation(document, &current_path).is_some();
        if !hide_label {
            labels.push(paragraph.paragraph_type().to_string());
        }
        current = Some(paragraph);
        current_item = None;
    }

    if let Some(item) = current_item {
        Some((labels, LabelTarget::ChecklistItem(item)))
    } else {
        let paragraph = current?;
        Some((labels, LabelTarget::Paragraph(paragraph)))
    }
}

fn collect_inline_labels(paragraph: &Paragraph, span_path: &SpanPath) -> Option<Vec<String>> {
    let mut labels = Vec::new();
    if span_path.is_empty() {
        return Some(labels);
    }

    let mut spans = paragraph.content();
    for &idx in span_path.indices() {
        let span = spans.get(idx)?;
        if let Some(label) = inline_style_label(span.style) {
            labels.push(label.to_string());
        }
        spans = &span.children;
    }

    Some(labels)
}

fn collect_inline_labels_from_item(
    item: &ChecklistItem,
    span_path: &SpanPath,
) -> Option<Vec<String>> {
    let mut labels = Vec::new();
    if span_path.is_empty() {
        return Some(labels);
    }

    let mut spans = &item.content;
    for &idx in span_path.indices() {
        let span = spans.get(idx)?;
        if let Some(label) = inline_style_label(span.style) {
            labels.push(label.to_string());
        }
        spans = &span.children;
    }

    Some(labels)
}

#[derive(Clone, Copy)]
enum TextEffectiveRelation {
    ParentChild,
    Entry,
}

fn text_effective_relation(
    document: &Document,
    path: &ParagraphPath,
) -> Option<TextEffectiveRelation> {
    let paragraph = paragraph_ref(document, path)?;
    if paragraph.paragraph_type() != ParagraphType::Text {
        return None;
    }
    let steps = path.steps();
    if steps.len() <= 1 {
        return None;
    }
    let (last_step, prefix) = steps.split_last()?;
    let parent = paragraph_ref(document, &ParagraphPath::from_steps(prefix.to_vec()))?;
    match *last_step {
        PathStep::Child(_) => parent
            .children()
            .len()
            .eq(&1)
            .then_some(TextEffectiveRelation::ParentChild),
        PathStep::Entry { entry_index, .. } => parent
            .entries()
            .get(entry_index)
            .and_then(|entry| (entry.len() == 1).then_some(TextEffectiveRelation::Entry)),
        PathStep::Root(_) | PathStep::ChecklistItem { .. } => None,
    }
}

pub fn paragraph_path_is_prefix(prefix: &ParagraphPath, target: &ParagraphPath) -> bool {
    let prefix_steps = prefix.steps();
    let target_steps = target.steps();
    prefix_steps.len() <= target_steps.len() && target_steps.starts_with(prefix_steps)
}

pub fn span_path_is_prefix(prefix: &[usize], target: &[usize]) -> bool {
    prefix.len() <= target.len() && target.starts_with(prefix)
}

pub fn paragraph_ref<'a>(document: &'a Document, path: &ParagraphPath) -> Option<&'a Paragraph> {
    let mut iter = path.steps().iter();
    let first = iter.next()?;
    let mut paragraph = match first {
        PathStep::Root(idx) => document.paragraphs.get(*idx)?,
        _ => return None,
    };
    for step in iter {
        paragraph = match step {
            PathStep::Child(idx) => match paragraph {
                Paragraph::Quote { children } => children.get(*idx)?,
                _ => return None,
            },
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => match paragraph {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    let entry = entries.get(*entry_index)?;
                    entry.get(*paragraph_index)?
                }
                _ => return None,
            },
            PathStep::ChecklistItem { .. } => return None,
            PathStep::Root(_) => return None,
        };
    }
    Some(paragraph)
}

pub fn checklist_item_ref<'a>(
    document: &'a Document,
    path: &ParagraphPath,
) -> Option<&'a ChecklistItem> {
    let steps = path.steps();
    let (checklist_step_idx, checklist_step) = steps
        .iter()
        .enumerate()
        .find(|(_, s)| matches!(s, PathStep::ChecklistItem { .. }))?;

    let PathStep::ChecklistItem { indices } = checklist_step else {
        return None;
    };

    let paragraph_path = ParagraphPath::from_steps(steps[..checklist_step_idx].to_vec());
    let paragraph = paragraph_ref(document, &paragraph_path)?;

    let mut item: &ChecklistItem = paragraph.checklist_items().get(*indices.first()?)?;
    for &idx in &indices[1..] {
        item = item.children.get(idx)?;
    }
    Some(item)
}

pub fn span_ref<'a>(paragraph: &'a Paragraph, path: &SpanPath) -> Option<&'a Span> {
    let mut iter = path.indices().iter();
    let first = iter.next()?;
    let mut spans = paragraph.content();
    let mut span = spans.get(*first)?;
    for idx in iter {
        span = span.children.get(*idx)?;
    }
    Some(span)
}

pub fn span_ref_from_item<'a>(item: &'a ChecklistItem, path: &SpanPath) -> Option<&'a Span> {
    let mut iter = path.indices().iter();
    let first = iter.next()?;
    let mut span = item.content.get(*first)?;
    for idx in iter {
        span = span.children.get(*idx)?;
    }
    Some(span)
}

fn collect_paragraph_segments(
    paragraph: &Paragraph,
    path: &mut ParagraphPath,
    reveal_codes: bool,
    segments: &mut Vec<SegmentRef>,
) {
    collect_span_segments(paragraph, path, reveal_codes, segments);
    for (child_index, child) in paragraph.children().iter().enumerate() {
        path.push_child(child_index);
        collect_paragraph_segments(child, path, reveal_codes, segments);
        path.pop();
    }
    for (entry_index, entry) in paragraph.entries().iter().enumerate() {
        for (child_index, child) in entry.iter().enumerate() {
            path.push_entry(entry_index, child_index);
            collect_paragraph_segments(child, path, reveal_codes, segments);
            path.pop();
        }
    }
    if paragraph.paragraph_type() == ParagraphType::Checklist {
        for (item_index, item) in paragraph.checklist_items().iter().enumerate() {
            collect_checklist_item_segments(item, path, &[item_index], reveal_codes, segments);
        }
    }
}

fn collect_checklist_item_segments(
    item: &ChecklistItem,
    path: &mut ParagraphPath,
    indices: &[usize],
    reveal_codes: bool,
    segments: &mut Vec<SegmentRef>,
) {
    path.push_checklist_item(indices.to_vec());
    collect_span_segments_from_item(item, path, reveal_codes, segments);
    path.pop();

    for (child_index, child) in item.children.iter().enumerate() {
        let mut child_indices = indices.to_vec();
        child_indices.push(child_index);
        collect_checklist_item_segments(child, path, &child_indices, reveal_codes, segments);
    }
}

fn collect_span_segments(
    paragraph: &Paragraph,
    path: &ParagraphPath,
    reveal_codes: bool,
    segments: &mut Vec<SegmentRef>,
) {
    for (index, span) in paragraph.content().iter().enumerate() {
        let mut span_path = SpanPath::new(vec![index]);
        collect_span_rec(span, path, &mut span_path, reveal_codes, segments);
    }
}

fn collect_span_segments_from_item(
    item: &ChecklistItem,
    path: &ParagraphPath,
    reveal_codes: bool,
    segments: &mut Vec<SegmentRef>,
) {
    for (index, span) in item.content.iter().enumerate() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{CursorPointer, SegmentKind};
    use tdoc::{Document, Paragraph, Span};

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

    fn pointer_to_checklist_item_span(root_index: usize, indices: Vec<usize>) -> CursorPointer {
        let mut path = ParagraphPath::new_root(root_index);
        path.push_checklist_item(indices);
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
        assert_eq!(
            breadcrumbs,
            vec!["Unordered List".to_string(), "Text".to_string()]
        );
    }

    #[test]
    fn breadcrumbs_include_checklist_items() {
        let nested = ChecklistItem::new(false).with_content(vec![Span::new_text("Nested")]);
        let parent = ChecklistItem::new(false)
            .with_content(vec![Span::new_text("Parent")])
            .with_children(vec![nested.clone()]);
        let checklist = Paragraph::new_checklist().with_checklist_items(vec![parent]);
        let document = Document::new().with_paragraphs(vec![checklist]);

        let top_pointer = pointer_to_checklist_item_span(0, vec![0]);
        let breadcrumbs = breadcrumbs_for_pointer(&document, &top_pointer).unwrap();
        assert_eq!(breadcrumbs, vec!["Checklist".to_string()]);

        let nested_pointer = pointer_to_checklist_item_span(0, vec![0, 0]);
        let nested_breadcrumbs = breadcrumbs_for_pointer(&document, &nested_pointer).unwrap();
        assert_eq!(
            nested_breadcrumbs,
            vec!["Checklist".to_string(), "Checklist".to_string()]
        );
    }
}
