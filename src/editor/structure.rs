use crate::editor::content::{checklist_item_is_empty, span_is_empty as content_span_is_empty, split_spans};
use std::mem;
use tdoc::{ChecklistItem, Document, Paragraph, ParagraphType, Span};

use super::{CursorPointer, ParagraphPath, PathStep, SegmentKind, SpanPath};
use super::inspect::paragraph_ref;

// ============================================================================
// Public helper functions (used across modules)
// ============================================================================

pub(crate) fn ensure_document_initialized(document: &mut Document) {
    if document.paragraphs.is_empty() {
        document
            .paragraphs
            .push(Paragraph::new_text().with_content(vec![Span::new_text("")]));
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
            PathStep::Child(idx) => {
                let Paragraph::Quote { children } = paragraph else {
                    return None;
                };
                children.get_mut(*idx)?
            },
            PathStep::Entry {
                entry_index,
                paragraph_index,
            } => {
                match paragraph {
                    Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                        let entry = entries.get_mut(*entry_index)?;
                        entry.get_mut(*paragraph_index)?
                    }
                    _ => return None,
                }
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

    let Paragraph::Checklist { items } = paragraph else {
        return None;
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

pub(crate) fn paragraph_is_empty(paragraph: &Paragraph) -> bool {
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

// ============================================================================
// Structure mutation helper functions
// ============================================================================

fn paragraph_from_checklist_item(item: ChecklistItem) -> Paragraph {
    Paragraph::new_text().with_content(item.content)
}

fn checklist_item_content_to_paragraph(content: Vec<Span>, target: ParagraphType) -> Paragraph {
    let mut paragraph = Paragraph::new_text();
    *paragraph.content_mut() = content;
    apply_paragraph_type_in_place(&mut paragraph, target);
    if paragraph.paragraph_type().is_leaf() && paragraph.content().is_empty() {
        paragraph.content_mut().push(Span::new_text(""));
    }
    paragraph
}

fn flatten_checklist_items_to_text(items: Vec<ChecklistItem>) -> Vec<Paragraph> {
    let mut result = Vec::new();
    for item in items {
        let mut paragraph = Paragraph::new_text();
        *paragraph.content_mut() = item.content;
        if paragraph.content().is_empty() {
            paragraph.content_mut().push(Span::new_text(""));
        }
        result.extend(vec![paragraph]);
        result.extend(flatten_checklist_items_to_text(item.children));
    }
    result
}

fn ensure_checklist_content(mut content: Vec<Span>) -> Vec<Span> {
    if content.is_empty() {
        content.push(Span::new_text(""));
    }
    content
}

fn paragraphs_to_checklist_items_recursive(paragraphs: Vec<Paragraph>) -> Vec<ChecklistItem> {
    paragraphs
        .into_iter()
        .flat_map(paragraph_to_checklist_items_recursive)
        .collect()
}

fn paragraph_to_checklist_items_recursive(paragraph: Paragraph) -> Vec<ChecklistItem> {
    match paragraph {
        Paragraph::Text { content }
        | Paragraph::Header1 { content }
        | Paragraph::Header2 { content }
        | Paragraph::Header3 { content }
        | Paragraph::CodeBlock { content } => {
            vec![ChecklistItem::new(false).with_content(ensure_checklist_content(content))]
        }
        Paragraph::Checklist { items } => items,
        Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries
            .into_iter()
            .map(entry_to_checklist_item)
            .collect(),
        Paragraph::Quote { children } => paragraphs_to_checklist_items_recursive(children),
    }
}

fn entry_to_checklist_item(entry: Vec<Paragraph>) -> ChecklistItem {
    let mut entry_iter = entry.into_iter();
    let mut head = match entry_iter.next() {
        Some(first_paragraph) => {
            let mut produced = paragraph_to_checklist_items_recursive(first_paragraph);
            if produced.is_empty() {
                ChecklistItem::new(false).with_content(vec![Span::new_text("")])
            } else {
                let mut head = produced.remove(0);
                head.children.extend(produced);
                head
            }
        }
        None => ChecklistItem::new(false).with_content(vec![Span::new_text("")]),
    };

    for paragraph in entry_iter {
        let mut children = paragraph_to_checklist_items_recursive(paragraph);
        head.children.append(&mut children);
    }

    head
}

pub(crate) fn is_single_paragraph_entry(document: &Document, path: &ParagraphPath) -> bool {
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
pub(crate) struct ParentScope {
    pub parent_path: ParagraphPath,
    pub relation: ParentRelation,
}

#[derive(Clone, Copy)]
pub(crate) enum ParentRelation {
    Child(usize),
    Entry {
        entry_index: usize,
        paragraph_index: usize,
    },
}

pub(crate) fn determine_parent_scope(document: &Document, path: &ParagraphPath) -> Option<ParentScope> {
    let steps = path.steps();
    if steps.len() <= 1 {
        return None;
    }

    let (last, prefix) = steps.split_last()?;
    let parent_path = ParagraphPath::from_steps(prefix.to_vec());
    let parent = paragraph_ref(document, &parent_path)?;

    match last {
        PathStep::Child(idx) => {
            let Paragraph::Quote { children } = parent else {
                return None;
            };
            if children.len() == 1 && *idx < children.len() {
                Some(ParentScope {
                    parent_path,
                    relation: ParentRelation::Child(*idx),
                })
            } else {
                None
            }
        },
        PathStep::Entry {
            entry_index,
            paragraph_index,
        } => {
            match parent {
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
            }
        },
        PathStep::ChecklistItem { indices } => {
            let item_index = *indices.first()?;
            let Paragraph::Checklist { items } = parent else {
                return None;
            };
            if items.len() == 1 && item_index < items.len() {
                Some(ParentScope {
                    parent_path,
                    relation: ParentRelation::Entry {
                        entry_index: item_index,
                        paragraph_index: 0,
                    },
                })
            } else {
                None
            }
        }
        PathStep::Root(_) => None,
    }
}

pub(crate) fn promote_single_child_into_parent(document: &mut Document, scope: &ParentScope) -> bool {
    let Some(parent) = paragraph_mut(document, &scope.parent_path) else {
        return false;
    };

    let is_checklist = parent.paragraph_type() == ParagraphType::Checklist;

    let child = match scope.relation {
        ParentRelation::Child(idx) => {
            let Paragraph::Quote { children } = parent else {
                return false;
            };
            if idx >= children.len() {
                return false;
            }
            children.remove(idx)
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

pub(crate) fn apply_paragraph_type_in_place(paragraph: &mut Paragraph, target: ParagraphType) {
    if target == ParagraphType::Quote {
        let mut children = Vec::new();

        // Clone the current content using the enum API
        let current_content = paragraph.content().to_vec();
        if !current_content.is_empty() {
            let mut text_child = Paragraph::new_text();
            *text_child.content_mut() = current_content;
            if text_child.content().is_empty() {
                text_child.content_mut().push(Span::new_text(""));
            }
            children.push(text_child);
        }

        // Move existing children
        match paragraph {
            Paragraph::Quote { children: existing_children } => {
                children.append(existing_children);
            },
            Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                for entry in entries.drain(..) {
                    children.extend(entry);
                }
            },
            _ => {}
        }

        if children.is_empty() {
            children.push(empty_text_paragraph());
        }

        *paragraph = Paragraph::new(ParagraphType::Quote);
        if let Paragraph::Quote { children: new_children } = paragraph {
            *new_children = children;
        }
        return;
    }

    if target.is_leaf() {
        // Convert to a leaf paragraph type, preserving only content
        let content = paragraph.content().to_vec();
        *paragraph = Paragraph::new(target);
        *paragraph.content_mut() = content;
        if paragraph.content().is_empty() {
            paragraph.content_mut().push(Span::new_text(""));
        }
    } else {
        // For non-leaf, non-Quote types, convert existing structure
        *paragraph = match target {
            ParagraphType::OrderedList => {
                let entries = match paragraph {
                    Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => mem::take(entries),
                    Paragraph::Quote { children } => {
                        // Convert Quote children into list entries
                        if children.is_empty() {
                            vec![vec![empty_text_paragraph()]]
                        } else {
                            vec![mem::take(children)]
                        }
                    },
                    _ => vec![vec![empty_text_paragraph()]],
                };
                Paragraph::OrderedList { entries }
            },
            ParagraphType::UnorderedList => {
                let entries = match paragraph {
                    Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => mem::take(entries),
                    Paragraph::Quote { children } => {
                        // Convert Quote children into list entries
                        if children.is_empty() {
                            vec![vec![empty_text_paragraph()]]
                        } else {
                            vec![mem::take(children)]
                        }
                    },
                    _ => vec![vec![empty_text_paragraph()]],
                };
                Paragraph::UnorderedList { entries }
            },
            ParagraphType::Checklist => {
                let items = match paragraph {
                    Paragraph::Checklist { items } => mem::take(items),
                    Paragraph::Quote { children } => {
                        let converted = paragraphs_to_checklist_items_recursive(mem::take(children));
                        if converted.is_empty() {
                            vec![ChecklistItem::new(false).with_content(vec![Span::new_text("")])]
                        } else {
                            converted
                        }
                    },
                    Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                        let converted: Vec<ChecklistItem> =
                            entries.drain(..).map(entry_to_checklist_item).collect();
                        if converted.is_empty() {
                            vec![ChecklistItem::new(false).with_content(vec![Span::new_text("")])]
                        } else {
                            converted
                        }
                    },
                    Paragraph::Text { content }
                    | Paragraph::Header1 { content }
                    | Paragraph::Header2 { content }
                    | Paragraph::Header3 { content }
                    | Paragraph::CodeBlock { content } => vec![ChecklistItem::new(false)
                        .with_content(ensure_checklist_content(mem::take(content)))],
                };
                Paragraph::Checklist { items }
            },
            _ => Paragraph::new(target),
        };

        if paragraph.content().is_empty() && target.is_leaf() {
            paragraph.content_mut().push(Span::new_text(""));
        }
    }
}

pub(crate) fn break_list_entry_for_non_list_target(
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
    if !is_list_type(list_paragraph.paragraph_type()) {
        return None;
    }

    let (list_type, entries_after, checklist_items_after, extracted_paragraphs, target_offset, has_prefix_entries) = {
        let list = paragraph_mut(document, &list_path)?;

        if is_checklist_item {
            // Handle checklist items using the enum API
            let Paragraph::Checklist { items } = list else {
                return None;
            };

            if entry_index >= items.len() {
                return None;
            }
            let items_after: Vec<ChecklistItem> = items.split_off(entry_index + 1);
            let selected_item = items.remove(entry_index);
            let ChecklistItem { content, children, .. } = selected_item;
            let has_prefix = !items.is_empty();

            let mut extracted = Vec::new();

            match target {
                ParagraphType::Quote => {
                    let mut quote_children = Vec::new();
                    let mut text_paragraph = checklist_item_content_to_paragraph(content, ParagraphType::Text);
                    quote_children.push(text_paragraph);
                    if !children.is_empty() {
                        quote_children.push(Paragraph::Checklist { items: children });
                    }
                    extracted.push(Paragraph::Quote { children: quote_children });
                }
                ParagraphType::OrderedList | ParagraphType::UnorderedList => {
                    let mut entry = Vec::new();
                    entry.push(checklist_item_content_to_paragraph(content, ParagraphType::Text));
                    if !children.is_empty() {
                        entry.push(Paragraph::Checklist { items: children });
                    }
                    let paragraph = if target == ParagraphType::OrderedList {
                        Paragraph::OrderedList { entries: vec![entry] }
                    } else {
                        Paragraph::UnorderedList { entries: vec![entry] }
                    };
                    extracted.push(paragraph);
                }
                ParagraphType::Checklist => {
                    extracted.push(Paragraph::Checklist {
                        items: if children.is_empty() {
                            vec![ChecklistItem::new(false).with_content(content)]
                        } else {
                            let mut root = ChecklistItem::new(false).with_content(content);
                            root.children = children;
                            vec![root]
                        },
                    });
                }
                _ => {
                    extracted.push(checklist_item_content_to_paragraph(content, target));
                    if !children.is_empty() {
                        let mut extra = flatten_checklist_items_to_text(children);
                        extracted.append(&mut extra);
                    }
                }
            }

            (ParagraphType::Checklist, vec![], items_after, extracted, 0, has_prefix)
        } else {
            // Handle regular list entries
            let (entries, list_type) = match list {
                Paragraph::OrderedList { entries } => (entries, ParagraphType::OrderedList),
                Paragraph::UnorderedList { entries } => (entries, ParagraphType::UnorderedList),
                _ => return None,
            };

            if entry_index >= entries.len() {
                return None;
            }
            if entries[entry_index].len() > 1 {
                return None;
            }
            let entries_after = entries.split_off(entry_index + 1);
            let selected_entry = entries.remove(entry_index);
            if paragraph_index >= selected_entry.len() {
                return None;
            }

            let mut extracted = Vec::new();
            for (idx, mut paragraph) in selected_entry.into_iter().enumerate() {
                if idx == paragraph_index {
                    apply_paragraph_type_in_place(&mut paragraph, target);
                }
                if paragraph.paragraph_type().is_leaf() && paragraph.content().is_empty() {
                    paragraph.content_mut().push(Span::new_text(""));
                }
                extracted.push(paragraph);
            }
            if extracted.is_empty() {
                return None;
            }
            let target_offset = paragraph_index.min(extracted.len().saturating_sub(1));
            let has_prefix_entries = !entries.is_empty();
            (
                list_type,
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
                    let tail = if list_type == ParagraphType::Checklist {
                        Paragraph::Checklist { items: checklist_items_after }
                    } else if list_type == ParagraphType::OrderedList {
                        Paragraph::OrderedList { entries: entries_after }
                    } else {
                        Paragraph::UnorderedList { entries: entries_after }
                    };
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
                    let tail = if list_type == ParagraphType::Checklist {
                        Paragraph::Checklist { items: checklist_items_after }
                    } else if list_type == ParagraphType::OrderedList {
                        Paragraph::OrderedList { entries: entries_after }
                    } else {
                        Paragraph::UnorderedList { entries: entries_after }
                    };
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
            let Paragraph::Quote { children } = parent else {
                return None;
            };

            if has_prefix_entries {
                let insertion_index = child_idx + 1;
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    children.insert(insertion_index + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let tail = if list_type == ParagraphType::Checklist {
                        Paragraph::Checklist { items: checklist_items_after }
                    } else if list_type == ParagraphType::OrderedList {
                        Paragraph::OrderedList { entries: entries_after }
                    } else {
                        Paragraph::UnorderedList { entries: entries_after }
                    };
                    children.insert(insertion_index + extract_count, tail);
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
                children.remove(child_idx);
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    children.insert(child_idx + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let tail = if list_type == ParagraphType::Checklist {
                        Paragraph::Checklist { items: checklist_items_after }
                    } else if list_type == ParagraphType::OrderedList {
                        Paragraph::OrderedList { entries: entries_after }
                    } else {
                        Paragraph::UnorderedList { entries: entries_after }
                    };
                    children.insert(child_idx + extract_count, tail);
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

            let entry = match parent {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    if entry_index >= entries.len() {
                        return None;
                    }
                    &mut entries[entry_index]
                },
                _ => return None,
            };

            if has_prefix_entries {
                let insertion_index = list_child_idx + 1;
                for (offset, paragraph) in extracted_paragraphs.into_iter().enumerate() {
                    entry.insert(insertion_index + offset, paragraph);
                }
                if !entries_after.is_empty() || !checklist_items_after.is_empty() {
                    let tail = if list_type == ParagraphType::Checklist {
                        Paragraph::Checklist { items: checklist_items_after }
                    } else if list_type == ParagraphType::OrderedList {
                        Paragraph::OrderedList { entries: entries_after }
                    } else {
                        Paragraph::UnorderedList { entries: entries_after }
                    };
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
                    let tail = if list_type == ParagraphType::Checklist {
                        Paragraph::Checklist { items: checklist_items_after }
                    } else if list_type == ParagraphType::OrderedList {
                        Paragraph::OrderedList { entries: entries_after }
                    } else {
                        Paragraph::UnorderedList { entries: entries_after }
                    };
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
pub(crate) struct EntryContext {
    pub list_path: ParagraphPath,
    pub entry_index: usize,
    pub paragraph_index: usize,
    pub tail_steps: Vec<PathStep>,
}

pub(crate) fn extract_entry_context(path: &ParagraphPath) -> Option<EntryContext> {
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

#[derive(Clone)]
pub(crate) struct ChecklistItemContext {
    pub checklist_path: ParagraphPath,
    pub indices: Vec<usize>,
    pub tail_steps: Vec<PathStep>,
}

pub(crate) fn extract_checklist_item_context(path: &ParagraphPath) -> Option<ChecklistItemContext> {
    let steps = path.steps();
    for idx in (0..steps.len()).rev() {
        if let PathStep::ChecklistItem { indices } = &steps[idx] {
            let checklist_path = ParagraphPath::from_steps(steps[..idx].to_vec());
            let tail_steps = steps[idx + 1..].to_vec();
            return Some(ChecklistItemContext {
                checklist_path,
                indices: indices.clone(),
                tail_steps,
            });
        }
    }
    None
}

fn checklist_items_container_mut<'a>(
    document: &'a mut Document,
    checklist_path: &ParagraphPath,
    ancestor_indices: &[usize],
) -> Option<&'a mut Vec<ChecklistItem>> {
    let paragraph = paragraph_mut(document, checklist_path)?;
    let Paragraph::Checklist { items } = paragraph else {
        return None;
    };

    let mut current = items;
    for idx in ancestor_indices {
        let item = current.get_mut(*idx)?;
        current = &mut item.children;
    }
    Some(current)
}

fn take_checklist_item_at(
    document: &mut Document,
    ctx: &ChecklistItemContext,
) -> Option<ChecklistItem> {
    if ctx.indices.is_empty() {
        return None;
    }

    let parent_indices = &ctx.indices[..ctx.indices.len().saturating_sub(1)];
    let container = checklist_items_container_mut(document, &ctx.checklist_path, parent_indices)?;
    let target_idx = *ctx.indices.last()?;
    if target_idx >= container.len() {
        return None;
    }
    Some(container.remove(target_idx))
}

fn insert_checklist_item_after_parent(
    document: &mut Document,
    ctx: &ChecklistItemContext,
    item: ChecklistItem,
) -> Option<ParagraphPath> {
    if ctx.indices.len() <= 1 {
        return None;
    }

    let parent_indices = &ctx.indices[..ctx.indices.len() - 1];
    let (parent_idx, container_indices) = match parent_indices.split_last() {
        Some((last, prefix)) => (*last, prefix),
        None => return None,
    };

    let container = checklist_items_container_mut(document, &ctx.checklist_path, container_indices)?;
    let insert_position = parent_idx + 1;
    if insert_position > container.len() {
        return None;
    }
    container.insert(insert_position, item);

    let mut new_indices = container_indices.to_vec();
    new_indices.push(insert_position);
    let mut steps = ctx.checklist_path.steps().to_vec();
    steps.push(PathStep::ChecklistItem { indices: new_indices });
    steps.extend(ctx.tail_steps.iter().cloned());
    Some(ParagraphPath::from_steps(steps))
}

pub(crate) fn merge_adjacent_lists(
    document: &mut Document,
    list_path: &ParagraphPath,
    entry_index: usize,
) -> Option<(ParagraphPath, usize)> {
    let list_type = paragraph_ref(document, list_path)?.paragraph_type();

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
            let Paragraph::Quote { children } = parent else {
                return None;
            };
            let (new_idx, new_entry_idx) = merge_adjacent_lists_in_vec(
                children,
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

            let entry_vec = match parent {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    if parent_entry_index >= entries.len() {
                        return None;
                    }
                    &mut entries[parent_entry_index]
                },
                _ => return None,
            };

            let (new_idx, new_entry_idx) =
                merge_adjacent_lists_in_vec(entry_vec, paragraph_index, entry_index, list_type)?;
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

    if list_index > 0 && paragraphs[list_index - 1].paragraph_type() == list_type {
        let current = paragraphs.remove(list_index);
        let previous = &mut paragraphs[list_index - 1];

        if list_type == ParagraphType::Checklist {
            let (prev_items, curr_items) = match (previous, current) {
                (Paragraph::Checklist { items: prev_items }, Paragraph::Checklist { items: curr_items }) => {
                    (prev_items, curr_items)
                },
                _ => return None,
            };
            let previous_entry_count = prev_items.len();
            target_entry_index += previous_entry_count;
            prev_items.extend(curr_items);
        } else {
            let (prev_entries, curr_entries) = match (previous, current) {
                (Paragraph::OrderedList { entries: prev }, Paragraph::OrderedList { entries: curr }) |
                (Paragraph::UnorderedList { entries: prev }, Paragraph::UnorderedList { entries: curr }) |
                (Paragraph::OrderedList { entries: prev }, Paragraph::UnorderedList { entries: curr }) |
                (Paragraph::UnorderedList { entries: prev }, Paragraph::OrderedList { entries: curr }) => {
                    (prev, curr)
                },
                _ => return None,
            };
            let previous_entry_count = prev_entries.len();
            target_entry_index += previous_entry_count;
            prev_entries.extend(curr_entries);
        }
        list_index -= 1;
    }

    if list_index + 1 < paragraphs.len() && paragraphs[list_index + 1].paragraph_type() == list_type {
        let next = paragraphs.remove(list_index + 1);
        let current = &mut paragraphs[list_index];

        if list_type == ParagraphType::Checklist {
            match (current, next) {
                (Paragraph::Checklist { items: curr_items }, Paragraph::Checklist { items: next_items }) => {
                    curr_items.extend(next_items);
                },
                _ => return None,
            }
        } else {
            match (current, next) {
                (Paragraph::OrderedList { entries: curr }, Paragraph::OrderedList { entries: next }) |
                (Paragraph::UnorderedList { entries: curr }, Paragraph::UnorderedList { entries: next }) |
                (Paragraph::OrderedList { entries: curr }, Paragraph::UnorderedList { entries: next }) |
                (Paragraph::UnorderedList { entries: curr }, Paragraph::OrderedList { entries: next }) => {
                    curr.extend(next);
                },
                _ => return None,
            }
        }
    }

    Some((list_index, target_entry_index))
}

pub(crate) fn is_list_type(kind: ParagraphType) -> bool {
    matches!(
        kind,
        ParagraphType::OrderedList | ParagraphType::UnorderedList | ParagraphType::Checklist
    )
}

pub(crate) fn find_list_ancestor_path(document: &Document, path: &ParagraphPath) -> Option<ParagraphPath> {
    let mut steps = path.steps().to_vec();
    while !steps.is_empty() {
        let candidate = ParagraphPath::from_steps(steps.clone());
        if let Some(paragraph) = paragraph_ref(document, &candidate) {
            if is_list_type(paragraph.paragraph_type()) {
                return Some(candidate);
            }
        }
        steps.pop();
    }
    None
}

pub(crate) fn update_existing_list_type(
    document: &mut Document,
    path: &ParagraphPath,
    target: ParagraphType,
) -> bool {
    let Some(paragraph) = paragraph_mut(document, path) else {
        return false;
    };

    match target {
        ParagraphType::Checklist => {
            // Convert entries to checklist items
            let entries_to_convert = match paragraph {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    mem::take(entries)
                },
                _ => Vec::new(),
            };

            let mut items: Vec<ChecklistItem> = entries_to_convert
                .into_iter()
                .map(entry_to_checklist_item)
                .collect();

            if items.is_empty() {
                items.push(ChecklistItem::new(false).with_content(vec![Span::new_text("")]));
            }

            *paragraph = Paragraph::Checklist { items };
        }
        ParagraphType::OrderedList | ParagraphType::UnorderedList => {
            // Convert checklist items to entries
            let items_to_convert = match paragraph {
                Paragraph::Checklist { items } => mem::take(items),
                _ => Vec::new(),
            };

            let mut entries = match paragraph {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    mem::take(entries)
                },
                _ => Vec::new(),
            };

            if entries.is_empty() && !items_to_convert.is_empty() {
                for item in &items_to_convert {
                    let para = Paragraph::new_text().with_content(item.content.clone());
                    entries.push(vec![para]);
                }
            }

            normalize_entries_for_standard_list(&mut entries);
            if entries.is_empty() {
                entries.push(vec![empty_text_paragraph()]);
            }

            *paragraph = if target == ParagraphType::OrderedList {
                Paragraph::OrderedList { entries }
            } else {
                Paragraph::UnorderedList { entries }
            };
        }
        _ => {}
    }

    true
}

pub(crate) fn convert_paragraph_into_list(
    document: &mut Document,
    path: &ParagraphPath,
    target: ParagraphType,
) -> Option<CursorPointer> {
    let paragraph = paragraph_mut(document, path)?;

    let content = paragraph.content().to_vec();
    let content = if content.is_empty() {
        vec![Span::new_text("")]
    } else {
        content
    };

    match target {
        ParagraphType::Checklist => {
            let item = ChecklistItem::new(false).with_content(content);
            *paragraph = Paragraph::Checklist { items: vec![item] };

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
            *head.content_mut() = content;
            if head.content().is_empty() {
                head.content_mut().push(Span::new_text(""));
            }

            let children = match paragraph {
                Paragraph::Quote { children } => mem::take(children),
                _ => Vec::new(),
            };

            let mut entry = vec![head];
            if !children.is_empty() {
                entry.extend(children);
            }

            *paragraph = if target == ParagraphType::OrderedList {
                Paragraph::OrderedList { entries: vec![entry] }
            } else {
                Paragraph::UnorderedList { entries: vec![entry] }
            };

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

pub(crate) fn update_paragraph_type(
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

        if entry[0].content().is_empty() {
            entry[0].content_mut().push(Span::new_text(""));
        }
    }
}

fn empty_text_paragraph() -> Paragraph {
    Paragraph::new_text().with_content(vec![Span::new_text("")])
}


pub(crate) fn split_paragraph_break(
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
            let spans = paragraph.content_mut();
            let split = split_spans(spans, &span_indices, pointer.offset);
            if spans.is_empty() {
                spans.push(Span::new_text(""));
            }
            split
        }
    };

    if right_spans.is_empty() {
        right_spans.push(Span::new_text(""));
    }

    match last_step {
        PathStep::Root(idx) if prefix.is_empty() => {
            let insert_idx = (*idx + 1).min(document.paragraphs.len());
            let new_paragraph = Paragraph::new_text().with_content(right_spans);
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
            let Paragraph::Quote { children } = parent else {
                return None;
            };
            let insert_idx = (*child_idx + 1).min(children.len());
            let new_paragraph = Paragraph::new_text().with_content(right_spans);
            children.insert(insert_idx, new_paragraph);

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
                    parent.paragraph_type(),
                    ParagraphType::OrderedList | ParagraphType::UnorderedList
                )
            {
                let entry = match parent {
                    Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                        entries.get_mut(*entry_index)?
                    },
                    _ => return None,
                };
                let insert_idx = (*paragraph_index + 1).min(entry.len());
                let new_paragraph = Paragraph::new_text().with_content(right_spans);
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

            let entries = match parent {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries,
                _ => return None,
            };

            let insert_idx = (*entry_index + 1).min(entries.len());

            let new_entry = {
                let entry = entries.get_mut(*entry_index)?;
                if *paragraph_index >= entry.len() {
                    return None;
                }

                let mut trailing = entry.split_off(*paragraph_index + 1);

                // Checklists use PathStep::ChecklistItem, not PathStep::Entry
                // So this should only handle OrderedList and UnorderedList
                let head = Paragraph::new_text().with_content(right_spans);

                if paragraph_is_empty(&entry[*paragraph_index]) && entry.len() > 1 {
                    entry.remove(*paragraph_index);
                } else if entry[*paragraph_index].content().is_empty() {
                    entry[*paragraph_index].content_mut().push(Span::new_text(""));
                }

                let mut assembled = vec![head];
                assembled.append(&mut trailing);
                assembled
            };

            entries.insert(insert_idx, new_entry);

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

            let Paragraph::Checklist { items } = parent else {
                return None;
            };

            if item_index >= items.len() {
                return None;
            }

            // Ensure the current item has content
            if items[item_index].content.is_empty() {
                items[item_index].content.push(Span::new_text(""));
            }

            // Insert a new checklist item after the current one
            let insert_idx = (item_index + 1).min(items.len());

            let checked_state = items[item_index].checked;
            let new_item = ChecklistItem::new(checked_state).with_content(right_spans);
            items.insert(insert_idx, new_item);

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

pub(crate) fn parent_paragraph_path(path: &ParagraphPath) -> Option<ParagraphPath> {
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
        PathStep::ChecklistItem { ref indices } => {
            if let Some((last, head)) = indices.split_last() {
                if *last > 0 {
                    let mut new_indices = head.to_vec();
                    new_indices.push(*last - 1);
                    let mut new_steps = prefix.to_vec();
                    new_steps.push(PathStep::ChecklistItem { indices: new_indices });
                    return Some(ParagraphPath::from_steps(new_steps));
                }
            }
            None
        }
        _ => None,
    }
}

#[derive(Clone)]
pub(crate) struct IndentTarget {
    pub path: ParagraphPath,
    pub kind: IndentTargetKind,
}

#[derive(Clone, Copy)]
pub(crate) enum IndentTargetKind {
    Quote,
    List,
    ListEntry { entry_index: usize },
    ChecklistItem,
}

pub(crate) fn find_indent_target(document: &Document, path: &ParagraphPath) -> Option<IndentTarget> {
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

pub(crate) fn find_container_indent_target(document: &Document, path: &ParagraphPath) -> Option<IndentTarget> {
    let mut current = path.clone();
    loop {
        if let Some(prev_path) = previous_sibling_path(&current) {
            if let Some(target) = determine_indent_target(document, &prev_path) {
                return Some(target);
            }
        }

        if let Some(parent) = parent_paragraph_path(&current) {
            if matches!(current.steps().last(), Some(PathStep::Entry { .. })) {
                break;
            }
            current = parent;
        } else {
            break;
        }
    }
    None
}

fn determine_indent_target(document: &Document, path: &ParagraphPath) -> Option<IndentTarget> {
    if matches!(path.steps().last(), Some(PathStep::ChecklistItem { .. })) {
        return Some(IndentTarget {
            path: path.clone(),
            kind: IndentTargetKind::ChecklistItem,
        });
    }
    let paragraph = paragraph_ref(document, path)?;
    if paragraph.paragraph_type() == ParagraphType::Checklist {
        let items = paragraph.checklist_items();
        if items.is_empty() {
            return Some(IndentTarget {
                path: path.clone(),
                kind: IndentTargetKind::List,
            });
        }
        let mut steps = path.steps().to_vec();
        steps.push(PathStep::ChecklistItem {
            indices: vec![items.len().saturating_sub(1)],
        });
        return Some(IndentTarget {
            path: ParagraphPath::from_steps(steps),
            kind: IndentTargetKind::ChecklistItem,
        });
    }
    if paragraph.paragraph_type() == ParagraphType::Quote {
        return Some(IndentTarget {
            path: path.clone(),
            kind: IndentTargetKind::Quote,
        });
    }
    if is_list_type(paragraph.paragraph_type()) {
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

pub(crate) fn append_paragraph_to_quote(
    document: &mut Document,
    path: &ParagraphPath,
    paragraph: Paragraph,
) -> Option<ParagraphPath> {
    let quote = paragraph_mut(document, path)?;
    let Paragraph::Quote { children } = quote else {
        return None;
    };
    let child_index = children.len();
    children.push(paragraph);
    let mut steps = path.steps().to_vec();
    steps.push(PathStep::Child(child_index));
    Some(ParagraphPath::from_steps(steps))
}

pub(crate) fn append_paragraph_to_list(
    document: &mut Document,
    path: &ParagraphPath,
    paragraph: Paragraph,
) -> Option<ParagraphPath> {
    let list = paragraph_mut(document, path)?;
    let list_type = list.paragraph_type();

    let (entry, paragraph_index) = match list_type {
        ParagraphType::Checklist => convert_paragraph_to_checklist_entry(paragraph),
        _ => (vec![paragraph], 0),
    };

    let entry_index = match list {
        Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
            let idx = entries.len();
            entries.push(entry);
            idx
        },
        _ => return None,
    };

    let mut steps = path.steps().to_vec();
    steps.push(PathStep::Entry {
        entry_index,
        paragraph_index,
    });
    Some(ParagraphPath::from_steps(steps))
}

pub(crate) fn list_entry_append_target(document: &Document, path: &ParagraphPath) -> Option<usize> {
    let list = paragraph_ref(document, path)?;
    let entries = match list {
        Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries,
        _ => return None,
    };

    if entries.is_empty() {
        return None;
    }
    let entry_index = entries.len() - 1;
    let entry = entries.get(entry_index)?;
    let last_paragraph = entry.last()?;
    if matches!(
        last_paragraph.paragraph_type(),
        ParagraphType::Quote
            | ParagraphType::OrderedList
            | ParagraphType::UnorderedList
            | ParagraphType::Checklist
    ) {
        return None;
    }
    Some(entry_index)
}

pub(crate) fn append_paragraph_to_entry(
    document: &mut Document,
    list_path: &ParagraphPath,
    entry_index: usize,
    paragraph: Paragraph,
) -> Option<ParagraphPath> {
    let list = paragraph_mut(document, list_path)?;
    let entry = match list {
        Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
            if entry_index >= entries.len() {
                return None;
            }
            &mut entries[entry_index]
        },
        _ => return None,
    };
    entry.push(paragraph);
    let paragraph_index = entry.len() - 1;
    let mut steps = list_path.steps().to_vec();
    steps.push(PathStep::Entry {
        entry_index,
        paragraph_index,
    });
    Some(ParagraphPath::from_steps(steps))
}

pub(crate) fn entry_has_multiple_paragraphs(document: &Document, ctx: &EntryContext) -> bool {
    paragraph_ref(document, &ctx.list_path)
        .and_then(|list| {
            match list {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    entries.get(ctx.entry_index)
                },
                _ => None,
            }
        })
        .map(|entry| entry.len() > 1)
        .unwrap_or(false)
}

fn ensure_nested_list(entry: &mut Vec<Paragraph>, list_type: ParagraphType) -> usize {
    if let Some(idx) = entry.iter().position(|p| p.paragraph_type() == list_type) {
        idx
    } else {
        entry.push(Paragraph::new(list_type));
        entry.len() - 1
    }
}

pub(crate) fn indent_paragraph_within_entry(
    document: &mut Document,
    pointer: &CursorPointer,
    ctx: &EntryContext,
) -> Option<CursorPointer> {
    let list_type = paragraph_ref(document, &ctx.list_path)
        .map(|p| p.paragraph_type())
        .filter(|kind| is_list_type(*kind))?;

    let paragraph = {
        let list = paragraph_mut(document, &ctx.list_path)?;
        let entry = match list {
            Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                if ctx.entry_index >= entries.len() {
                    return None;
                }
                &mut entries[ctx.entry_index]
            },
            _ => return None,
        };

        if ctx.paragraph_index >= entry.len() || entry.len() <= 1 {
            return None;
        }
        entry.remove(ctx.paragraph_index)
    };

    let (nested_entry, nested_paragraph_index) = match list_type {
        ParagraphType::Checklist => convert_paragraph_to_checklist_entry(paragraph),
        ParagraphType::OrderedList | ParagraphType::UnorderedList => (vec![paragraph], 0),
        _ => return None,
    };

    let mut nested_list = if list_type == ParagraphType::OrderedList {
        Paragraph::OrderedList { entries: vec![nested_entry] }
    } else if list_type == ParagraphType::UnorderedList {
        Paragraph::UnorderedList { entries: vec![nested_entry] }
    } else {
        // Checklist
        Paragraph::Checklist { items: Vec::new() }
    };

    {
        let list = paragraph_mut(document, &ctx.list_path)?;
        let entry = match list {
            Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                entries.get_mut(ctx.entry_index)?
            },
            _ => return None,
        };
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

pub(crate) fn indent_list_entry_into_entry(
    document: &mut Document,
    pointer: &CursorPointer,
    ctx: &EntryContext,
    target_entry_index: usize,
) -> Option<CursorPointer> {
    if target_entry_index >= ctx.entry_index {
        return None;
    }

    let list_type = paragraph_ref(document, &ctx.list_path)
        .map(|p| p.paragraph_type())
        .filter(|kind| is_list_type(*kind))?;

    let entry = {
        let list = paragraph_mut(document, &ctx.list_path)?;
        let entries = match list {
            Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries,
            _ => return None,
        };

        if ctx.entry_index >= entries.len() {
            return None;
        }
        entries.remove(ctx.entry_index)
    };

    if entry.is_empty() {
        return None;
    }

    let entry_len = entry.len();

    let paragraph_path = {
        let list = paragraph_mut(document, &ctx.list_path)?;
        let target_entry = match list {
            Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                if target_entry_index >= entries.len() {
                    return None;
                }
                &mut entries[target_entry_index]
            },
            _ => return None,
        };

        let has_matching_nested_list = target_entry
            .iter()
            .any(|paragraph| paragraph.paragraph_type() == list_type);
        let should_use_nested_list = has_matching_nested_list || target_entry.len() == 1;

        if should_use_nested_list {
            let nested_index = ensure_nested_list(target_entry, list_type);
            let nested_list = target_entry.get_mut(nested_index)?;

            let new_entry_index = match nested_list {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    let idx = entries.len();
                    entries.push(entry);
                    idx
                },
                _ => return None,
            };

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

fn paragraph_to_checklist_item(paragraph: Paragraph) -> ChecklistItem {
    let mut content = paragraph.content().to_vec();
    if content.is_empty() {
        content.push(Span::new_text(""));
    }

    ChecklistItem::new(false).with_content(content)
}

pub(crate) fn append_paragraph_as_checklist_child(
    document: &mut Document,
    target_path: &ParagraphPath,
    paragraph: Paragraph,
) -> Option<ParagraphPath> {
    let new_item = paragraph_to_checklist_item(paragraph);
    append_checklist_child(document, target_path, new_item)
}

fn append_checklist_child(
    document: &mut Document,
    target_path: &ParagraphPath,
    item: ChecklistItem,
) -> Option<ParagraphPath> {
    let parent_item = checklist_item_mut(document, target_path)?;
    parent_item.children.push(item);
    let new_child_index = parent_item.children.len().saturating_sub(1);

    let mut steps = target_path.steps().to_vec();
    let last = steps.pop()?;
    match last {
        PathStep::ChecklistItem { mut indices } => {
            indices.push(new_child_index);
            steps.push(PathStep::ChecklistItem { indices });
            Some(ParagraphPath::from_steps(steps))
        }
        _ => None,
    }
}

fn remove_nested_list_paragraph(document: &mut Document, parent_ctx: &EntryContext) {
    let Some(list_paragraph) = paragraph_mut(document, &parent_ctx.list_path) else {
        return;
    };
    let entries = match list_paragraph {
        Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries,
        _ => return,
    };
    if parent_ctx.entry_index >= entries.len() {
        return;
    }
    let parent_entry = &mut entries[parent_ctx.entry_index];
    if parent_ctx.paragraph_index < parent_entry.len() {
        parent_entry.remove(parent_ctx.paragraph_index);
    }
    if parent_entry.is_empty() {
        parent_entry.push(empty_text_paragraph());
    }
}

fn take_list_entry(document: &mut Document, ctx: &EntryContext) -> Option<(Vec<Paragraph>, bool)> {
    let list = paragraph_mut(document, &ctx.list_path)?;
    let entries = match list {
        Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries,
        _ => return None,
    };
    if ctx.entry_index >= entries.len() {
        return None;
    }
    let entry = entries.remove(ctx.entry_index);
    let became_empty = entries.is_empty();
    Some((entry, became_empty))
}

pub(crate) fn indent_list_entry_into_foreign_list(
    document: &mut Document,
    pointer: &CursorPointer,
    source_ctx: &EntryContext,
    target_list_path: &ParagraphPath,
) -> Option<CursorPointer> {
    let list_type = paragraph_ref(document, &source_ctx.list_path)?.paragraph_type();
    if !matches!(list_type, ParagraphType::OrderedList | ParagraphType::UnorderedList) {
        return None;
    }

    let (entry, list_empty) = take_list_entry(document, source_ctx)?;
    if list_empty {
        remove_paragraph_by_path(document, &source_ctx.list_path);
    }
    if entry.is_empty() {
        return None;
    }

    let nested_paragraph = if list_type == ParagraphType::OrderedList {
        Paragraph::OrderedList { entries: vec![entry.clone()] }
    } else {
        Paragraph::UnorderedList { entries: vec![entry.clone()] }
    };

    let base_path = if let Some(entry_index) = list_entry_append_target(document, target_list_path) {
        append_paragraph_to_entry(document, target_list_path, entry_index, nested_paragraph)?
    } else {
        append_paragraph_to_list(document, target_list_path, nested_paragraph)?
    };
    let mut steps = base_path.steps().to_vec();
    steps.push(PathStep::Entry {
        entry_index: 0,
        paragraph_index: source_ctx.paragraph_index.min(entry.len().saturating_sub(1)),
    });
    steps.extend(source_ctx.tail_steps.iter().cloned());

    Some(CursorPointer {
        paragraph_path: ParagraphPath::from_steps(steps),
        span_path: pointer.span_path.clone(),
        offset: pointer.offset,
        segment_kind: pointer.segment_kind,
    })
}

pub(crate) fn promote_list_entry_to_parent(
    document: &mut Document,
    pointer: &CursorPointer,
    ctx: &EntryContext,
    paragraph_index: usize,
) -> Option<CursorPointer> {
    let parent_ctx = extract_entry_context(&ctx.list_path)?;

    let (entry, list_became_empty) = take_list_entry(document, ctx)?;

    if entry.is_empty() {
        return None;
    }

    if list_became_empty {
        remove_nested_list_paragraph(document, &parent_ctx);
    }

    let list = paragraph_mut(document, &parent_ctx.list_path)?;
    let parent_entries = match list {
        Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries,
        _ => return None,
    };
    let insert_index = (parent_ctx.entry_index + 1).min(parent_entries.len());
    parent_entries.insert(insert_index, entry);

    let mut steps = parent_ctx.list_path.steps().to_vec();
    steps.push(PathStep::Entry {
        entry_index: insert_index,
        paragraph_index,
    });
    steps.extend(ctx.tail_steps.iter().cloned());

    Some(CursorPointer {
        paragraph_path: ParagraphPath::from_steps(steps),
        span_path: pointer.span_path.clone(),
        offset: pointer.offset,
        segment_kind: pointer.segment_kind,
    })
}

pub(crate) fn indent_checklist_item_into_item(
    document: &mut Document,
    pointer: &CursorPointer,
    target_path: &ParagraphPath,
) -> Option<CursorPointer> {
    let source_ctx = extract_checklist_item_context(&pointer.paragraph_path)?;
    let target_ctx = extract_checklist_item_context(target_path)?;

    if source_ctx.checklist_path != target_ctx.checklist_path {
        return None;
    }

    let item = take_checklist_item_at(document, &source_ctx)?;
    let new_base_path = append_checklist_child(document, target_path, item)?;
    let mut steps = new_base_path.steps().to_vec();
    steps.extend(source_ctx.tail_steps.iter().cloned());

    Some(CursorPointer {
        paragraph_path: ParagraphPath::from_steps(steps),
        span_path: pointer.span_path.clone(),
        offset: pointer.offset,
        segment_kind: pointer.segment_kind,
    })
}

pub(crate) fn unindent_checklist_item(
    document: &mut Document,
    pointer: &CursorPointer,
) -> Option<CursorPointer> {
    let ctx = extract_checklist_item_context(&pointer.paragraph_path)?;
    if ctx.indices.len() <= 1 {
        return None;
    }

    let item = take_checklist_item_at(document, &ctx)?;
    let new_path = insert_checklist_item_after_parent(document, &ctx, item)?;

    Some(CursorPointer {
        paragraph_path: new_path,
        span_path: pointer.span_path.clone(),
        offset: pointer.offset,
        segment_kind: pointer.segment_kind,
    })
}

fn convert_paragraph_to_checklist_entry(paragraph: Paragraph) -> (Vec<Paragraph>, usize) {
    let content = paragraph.content().to_vec();
    let content = if content.is_empty() {
        vec![Span::new_text("")]
    } else {
        content
    };

    let mut item = Paragraph::new_text().with_content(content);
    let mut entry = vec![item];

    // If the original paragraph had children or entries, preserve them
    match paragraph {
        Paragraph::Quote { children } if !children.is_empty() => {
            entry.push(Paragraph::Quote { children });
        },
        Paragraph::OrderedList { entries } if !entries.is_empty() => {
            entry.push(Paragraph::OrderedList { entries });
        },
        Paragraph::UnorderedList { entries } if !entries.is_empty() => {
            entry.push(Paragraph::UnorderedList { entries });
        },
        _ => {}
    }

    (entry, 0)
}

pub(crate) fn take_paragraph_at(document: &mut Document, path: &ParagraphPath) -> Option<Paragraph> {
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
            let Paragraph::Quote { children } = parent else {
                return None;
            };
            if idx < children.len() {
                Some(children.remove(idx))
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

            let is_list = is_list_type(parent.paragraph_type());

            let entries = match parent {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries,
                _ => return None,
            };

            if entry_index >= entries.len() {
                return None;
            }
            let entry = &mut entries[entry_index];
            if paragraph_index >= entry.len() {
                return None;
            }
            let removed = entry.remove(paragraph_index);
            if entry.is_empty() {
                entries.remove(entry_index);
            }

            if is_list && entries.is_empty() {
                entries.push(vec![empty_text_paragraph()]);
            }
            Some(removed)
        }
        PathStep::ChecklistItem { .. } => {
            // TODO: Implement checklist item removal and return as paragraph
            None
        }
    }
}

pub(crate) fn insert_paragraph_after_parent(
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
            let Paragraph::Quote { children } = host else {
                return None;
            };
            let insert_idx = (child_idx + 1).min(children.len());
            children.insert(insert_idx, paragraph);
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

            let entry = match host {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => {
                    if entry_index >= entries.len() {
                        return None;
                    }
                    &mut entries[entry_index]
                },
                _ => return None,
            };

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

pub(crate) fn remove_paragraph_by_path(document: &mut Document, path: &ParagraphPath) -> bool {
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
                let Paragraph::Quote { children } = parent else {
                    return false;
                };
                if idx < children.len() {
                    children.remove(idx);
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

            let entries = match parent {
                Paragraph::OrderedList { entries } | Paragraph::UnorderedList { entries } => entries,
                _ => return false,
            };

            if entry_index >= entries.len() {
                return false;
            }
            let entry = &mut entries[entry_index];
            if paragraph_index >= entry.len() {
                return false;
            }
            entry.remove(paragraph_index);
            if entry.is_empty() {
                entries.remove(entry_index);
            }
            true
        }
        PathStep::ChecklistItem { .. } => {
            // TODO: Implement checklist item removal
            false
        }
    }
}
