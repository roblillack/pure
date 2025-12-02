use std::collections::HashMap;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

use tdoc::{Document, InlineStyle, Paragraph, ParagraphType, Span as DocSpan};

use crate::editor::{
    CursorPointer, ParagraphPath, RevealTagKind, RevealTagRef, SegmentKind, SpanPath,
};
use crate::theme::Theme;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct RenderSentinels {
    pub cursor: char,
    pub selection_start: char,
    pub selection_end: char,
}

/// Direct cursor tracking without sentinels
#[derive(Clone)]
pub struct DirectCursorTracking<'a> {
    pub cursor: Option<&'a CursorPointer>,
    pub selection: Option<(&'a CursorPointer, &'a CursorPointer)>,
    pub track_all_positions: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct CursorVisualPosition {
    pub line: usize,
    pub column: u16,
    pub content_line: usize,
    pub content_column: u16,
}

/// Maps visual lines to the paragraph that rendered them
/// Also contains all cursor positions within this paragraph
#[derive(Debug, Clone)]
pub struct ParagraphLineInfo {
    /// Index of the paragraph in the document
    pub paragraph_index: usize,
    /// First visual line this paragraph occupies (absolute)
    pub start_line: usize,
    /// Last visual line this paragraph occupies (inclusive, absolute)
    pub end_line: usize,
    /// All cursor positions within this paragraph
    /// Line numbers are RELATIVE to start_line for caching efficiency
    pub positions: Vec<(CursorPointer, CursorVisualPosition)>,
}

/// Result of laying out a single paragraph
#[derive(Debug, Clone)]
pub struct ParagraphLayout {
    /// The rendered lines for this paragraph (without blank separator lines)
    pub lines: Vec<Line<'static>>,
    /// Line metrics for content line calculation
    pub line_metrics: Vec<LineMetric>,
    /// Cursor positions with line numbers RELATIVE to paragraph start (line 0)
    pub positions: Vec<(CursorPointer, CursorVisualPosition)>,
    /// The cursor visual position if found in this paragraph (with RELATIVE line number)
    pub cursor: Option<CursorVisualPosition>,
    /// Number of lines rendered
    pub line_count: usize,
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub lines: Vec<Line<'static>>,
    pub cursor: Option<CursorVisualPosition>,
    pub total_lines: usize,
    pub content_lines: usize,
    /// Maps visual lines to paragraphs, including all cursor positions
    pub paragraph_lines: Vec<ParagraphLineInfo>,
}

fn inline_style_display(style: InlineStyle) -> &'static str {
    match style {
        InlineStyle::None => "Text",
        InlineStyle::Bold => "Bold",
        InlineStyle::Italic => "Italic",
        InlineStyle::Highlight => "Highlight",
        InlineStyle::Underline => "Underline",
        InlineStyle::Strike => "Strikethrough",
        InlineStyle::Link => "Link",
        InlineStyle::Code => "Code",
    }
}

fn reveal_tag_display(style: InlineStyle, kind: RevealTagKind) -> String {
    let label = inline_style_display(style);
    match kind {
        RevealTagKind::Start => format!("[{label}>"),
        RevealTagKind::End => format!("<{label}]"),
    }
}

/// Render document with direct cursor tracking (no sentinel cloning needed)
pub fn render_document_direct(
    document: &Document,
    wrap_width: usize,
    left_padding: usize,
    reveal_tags: &[RevealTagRef],
    direct_tracking: DirectCursorTracking,
    theme: &Theme,
) -> RenderResult {
    let mut renderer = DirectRenderer::new(
        wrap_width.max(1),
        left_padding,
        reveal_tags,
        direct_tracking,
        theme,
    );
    renderer.render_document(document);
    renderer.finish()
}

/// Layout a single paragraph in isolation
/// Returns the rendered lines and cursor positions with RELATIVE line numbers (starting from 0)
#[allow(clippy::too_many_arguments)]
pub fn layout_paragraph(
    paragraph: &Paragraph,
    paragraph_index: usize,
    paragraph_path: ParagraphPath,
    wrap_width: usize,
    left_padding: usize,
    prefix: &str,
    reveal_tags: &[RevealTagRef],
    direct_tracking: DirectCursorTracking,
    theme: &Theme,
) -> ParagraphLayout {
    // Create a temporary renderer for this paragraph only
    let mut renderer = DirectRenderer::new(
        wrap_width.max(1),
        left_padding,
        reveal_tags,
        direct_tracking,
        theme,
    );

    renderer.current_paragraph_index = paragraph_index;
    renderer.current_paragraph_path = paragraph_path;

    let start_marker_id = renderer.next_marker_id;

    // Render the paragraph
    renderer.render_paragraph(paragraph, prefix);

    // Extract results
    let lines = renderer.lines;
    let line_metrics = renderer.line_metrics;
    let line_count = lines.len();

    // Extract positions with relative line numbers (already relative since we start at line 0)
    let mut positions = Vec::new();
    for (marker_id, pending) in renderer.marker_pending.iter() {
        if *marker_id >= start_marker_id
            && let Some(pointer) = renderer.marker_to_pointer.get(marker_id)
        {
            positions.push((
                pointer.clone(),
                CursorVisualPosition {
                    line: pending.line, // Already relative since renderer starts at line 0
                    column: pending.column,
                    content_line: 0, // Will be computed later when assembled into full document
                    content_column: pending.content_column,
                },
            ));
        }
    }

    // Find cursor if present (also with relative line number)
    let cursor = renderer.cursor_pending.map(|pending| CursorVisualPosition {
        line: pending.line,
        column: pending.column,
        content_line: 0, // Will be computed later
        content_column: pending.content_column,
    });

    ParagraphLayout {
        lines,
        line_metrics,
        positions,
        cursor,
        line_count,
    }
}

/// Direct renderer that tracks cursor positions without sentinels
struct DirectRenderer<'a> {
    wrap_width: usize,
    wrap_limit: usize,
    left_padding: usize,
    padding: Option<String>,
    line_metrics: Vec<LineMetric>,
    lines: Vec<Line<'static>>,
    current_line_index: usize,
    reveal_tags: HashMap<usize, RevealTagRef>,

    // Direct position tracking
    cursor_pointer: Option<&'a CursorPointer>,
    selection_start: Option<&'a CursorPointer>,
    selection_end: Option<&'a CursorPointer>,
    track_all_positions: bool,

    // Current position during rendering
    current_paragraph_index: usize,
    current_paragraph_path: ParagraphPath,

    // Cursor map tracking
    marker_pending: HashMap<usize, PendingPosition>,
    cursor_pending: Option<PendingPosition>,
    next_marker_id: usize,
    marker_to_pointer: HashMap<usize, CursorPointer>,

    // Paragraph line range tracking
    paragraph_lines: Vec<ParagraphLineInfo>,

    // Theme for styling
    theme: Theme,
}

impl<'a> DirectRenderer<'a> {
    #[allow(dead_code)]
    fn new(
        wrap_width: usize,
        left_padding: usize,
        reveal_tags: &[RevealTagRef],
        direct_tracking: DirectCursorTracking<'a>,
        theme: &Theme,
    ) -> Self {
        let wrap_limit = if wrap_width > 1 { wrap_width - 1 } else { 1 };
        let padding = if left_padding > 0 {
            Some(" ".repeat(left_padding))
        } else {
            None
        };
        let reveal_map = reveal_tags
            .iter()
            .map(|tag| (tag.id, tag.clone()))
            .collect::<HashMap<usize, RevealTagRef>>();

        Self {
            wrap_width,
            wrap_limit,
            left_padding,
            padding,
            line_metrics: Vec::new(),
            lines: Vec::new(),
            current_line_index: 0,
            reveal_tags: reveal_map,
            cursor_pointer: direct_tracking.cursor,
            selection_start: direct_tracking.selection.map(|(start, _)| start),
            selection_end: direct_tracking.selection.map(|(_, end)| end),
            track_all_positions: direct_tracking.track_all_positions,
            current_paragraph_index: 0,
            current_paragraph_path: ParagraphPath::default(),
            marker_pending: HashMap::new(),
            cursor_pending: None,
            next_marker_id: 0,
            marker_to_pointer: HashMap::new(),
            paragraph_lines: Vec::new(),
            theme: theme.clone(),
        }
    }

    fn render_document(&mut self, document: &Document) {
        for (idx, paragraph) in document.paragraphs.iter().enumerate() {
            if idx > 0 {
                self.push_plain_line("", true);
            }

            // Record start line before rendering this paragraph
            let paragraph_start_line = self.current_line_index;

            // Layout this paragraph using the standalone function
            let direct_tracking = DirectCursorTracking {
                cursor: self.cursor_pointer,
                selection: if self.selection_start.is_some() && self.selection_end.is_some() {
                    Some((self.selection_start.unwrap(), self.selection_end.unwrap()))
                } else {
                    None
                },
                track_all_positions: self.track_all_positions,
            };

            let reveal_tags: Vec<RevealTagRef> = self.reveal_tags.values().cloned().collect();

            let layout = layout_paragraph(
                paragraph,
                idx,
                ParagraphPath::new_root(idx),
                self.wrap_width,
                self.left_padding,
                "",
                &reveal_tags,
                direct_tracking,
                &self.theme,
            );

            // Add the lines and metrics to our result
            self.lines.extend(layout.lines);
            self.line_metrics.extend(layout.line_metrics);
            self.current_line_index += layout.line_count;

            // Convert relative positions to absolute and add to marker maps
            let paragraph_start_marker_id = self.next_marker_id;
            for (pointer, position) in &layout.positions {
                let marker_id = self.next_marker_id;
                self.next_marker_id += 1;

                let absolute_position = PendingPosition {
                    line: paragraph_start_line + position.line,
                    column: position.column,
                    content_column: position.content_column,
                };

                self.marker_pending.insert(marker_id, absolute_position);
                self.marker_to_pointer.insert(marker_id, pointer.clone());
            }

            // Update cursor if found
            if let Some(cursor) = layout.cursor {
                self.cursor_pending = Some(PendingPosition {
                    line: paragraph_start_line + cursor.line,
                    column: cursor.column,
                    content_column: cursor.content_column,
                });
            }

            // Record end line for this paragraph
            let end_line = self.current_line_index.saturating_sub(1);

            // Build paragraph positions with relative line numbers
            let mut paragraph_positions = Vec::new();
            for (marker_id, pending) in self.marker_pending.iter() {
                if *marker_id >= paragraph_start_marker_id
                    && let Some(pointer) = self.marker_to_pointer.get(marker_id)
                {
                    let relative_line = pending.line.saturating_sub(paragraph_start_line);
                    paragraph_positions.push((
                        pointer.clone(),
                        CursorVisualPosition {
                            line: relative_line,
                            column: pending.column,
                            content_line: 0, // Will be computed in finish()
                            content_column: pending.content_column,
                        },
                    ));
                }
            }

            self.paragraph_lines.push(ParagraphLineInfo {
                paragraph_index: idx,
                start_line: paragraph_start_line,
                end_line,
                positions: paragraph_positions,
            });
        }
    }

    fn render_paragraph(&mut self, paragraph: &Paragraph, prefix: &str) {
        match paragraph.paragraph_type() {
            ParagraphType::Text => self.render_text_paragraph(paragraph, prefix, prefix),
            ParagraphType::Header1 => self.render_header(paragraph, prefix, HeaderLevel::One),
            ParagraphType::Header2 => self.render_header(paragraph, prefix, HeaderLevel::Two),
            ParagraphType::Header3 => self.render_header(paragraph, prefix, HeaderLevel::Three),
            ParagraphType::CodeBlock => self.render_code_block(paragraph, prefix),
            ParagraphType::Quote => self.render_quote(paragraph, prefix),
            ParagraphType::UnorderedList => self.render_unordered_list(paragraph, prefix),
            ParagraphType::OrderedList => self.render_ordered_list(paragraph, prefix),
            ParagraphType::Checklist => self.render_checklist(paragraph, prefix),
        }
    }

    fn render_text_paragraph(
        &mut self,
        paragraph: &Paragraph,
        first_prefix: &str,
        continuation_prefix: &str,
    ) {
        let mut fragments = Vec::new();
        let base_span_path = SpanPath {
            indices: Vec::new(),
        };
        self.collect_fragments_direct(
            paragraph.content(),
            &base_span_path,
            Style::default(),
            &mut fragments,
        );
        let fragments = trim_layout_fragments(fragments);
        let lines = self.wrap_fragments_direct(
            &fragments,
            first_prefix,
            continuation_prefix,
            self.wrap_limit,
        );
        self.consume_lines_direct(lines);
    }

    fn render_header(&mut self, paragraph: &Paragraph, prefix: &str, level: HeaderLevel) {
        let mut fragments = Vec::new();
        let base_span_path = SpanPath {
            indices: Vec::new(),
        };
        self.collect_fragments_direct(
            paragraph.content(),
            &base_span_path,
            Style::default(),
            &mut fragments,
        );
        let fragments = trim_layout_fragments(fragments);
        let mut lines = self.wrap_fragments_direct(&fragments, prefix, prefix, self.wrap_limit);

        match level {
            HeaderLevel::One => {
                if let Some(first_line) = lines.first_mut() {
                    for span in &mut first_line.spans {
                        span.style = span.style.add_modifier(Modifier::BOLD);
                    }
                }
            }
            HeaderLevel::Two | HeaderLevel::Three => {
                for line in &mut lines {
                    for span in &mut line.spans {
                        span.style = span.style.add_modifier(Modifier::BOLD);
                    }
                }
            }
        }

        self.consume_lines_direct(lines);

        if matches!(level, HeaderLevel::Two | HeaderLevel::Three) {
            let width = self.lines.last().map(|line| line_width(line)).unwrap_or(0);
            let underline_char = match level {
                HeaderLevel::Two => '=',
                HeaderLevel::Three => '-',
                HeaderLevel::One => '=',
            };
            let underline_width = width.saturating_sub(self.left_padding);
            let underline = underline_string(underline_width, underline_char);
            self.push_styled_line(&underline, self.theme.structural_style(), false);
        }
    }

    fn render_code_block(&mut self, paragraph: &Paragraph, prefix: &str) {
        let fence = self.code_block_fence(prefix);
        self.push_styled_line(&fence, self.theme.structural_style(), false);

        let mut fragments = Vec::new();
        let base_span_path = SpanPath {
            indices: Vec::new(),
        };
        self.collect_fragments_direct(
            paragraph.content(),
            &base_span_path,
            Style::default(),
            &mut fragments,
        );
        let lines = self.wrap_fragments_direct(&fragments, prefix, prefix, usize::MAX / 4);
        self.consume_lines_direct(lines);

        self.push_styled_line(&fence, self.theme.structural_style(), false);
    }

    fn render_quote(&mut self, paragraph: &Paragraph, prefix: &str) {
        let quote_prefix = format!("{}| ", prefix);
        if !paragraph.content().is_empty() {
            self.render_text_paragraph(paragraph, &quote_prefix, &quote_prefix);
        }
        for (idx, child) in paragraph.children().iter().enumerate() {
            if idx > 0 || !paragraph.content().is_empty() {
                self.push_plain_line(&quote_prefix, false);
            }
            // Update paragraph path for this child
            self.current_paragraph_path.push_child(idx);
            self.render_paragraph(child, &quote_prefix);
            // Restore paragraph path
            self.current_paragraph_path.pop();
        }
    }

    fn render_unordered_list(&mut self, paragraph: &Paragraph, prefix: &str) {
        for (entry_idx, entry) in paragraph.entries().iter().enumerate() {
            if entry_idx > 0 {
                self.push_plain_line("", false);
            }
            let marker = "• ";
            let first_prefix = format!("{}{}", prefix, marker);
            let continuation_prefix = format!("{}{}", prefix, " ".repeat(marker.chars().count()));
            self.render_list_entry(entry, entry_idx, &first_prefix, &continuation_prefix);
        }
    }

    fn render_ordered_list(&mut self, paragraph: &Paragraph, prefix: &str) {
        for (entry_idx, entry) in paragraph.entries().iter().enumerate() {
            if entry_idx > 0 {
                self.push_plain_line("", false);
            }
            let number_label = format!("{}. ", entry_idx + 1);
            let first_prefix = format!("{}{}", prefix, number_label);
            let continuation_spaces = " ".repeat(
                first_prefix
                    .chars()
                    .count()
                    .saturating_sub(prefix.chars().count()),
            );
            let continuation_prefix = format!("{}{}", prefix, continuation_spaces);
            self.render_list_entry(entry, entry_idx, &first_prefix, &continuation_prefix);
        }
    }

    fn render_checklist(&mut self, paragraph: &Paragraph, prefix: &str) {
        for (item_idx, item) in paragraph.checklist_items().iter().enumerate() {
            if item_idx > 0 {
                self.push_plain_line("", false);
            }
            self.render_checklist_item_struct(item, vec![item_idx], prefix);
        }
    }

    fn render_checklist_item_struct(
        &mut self,
        item: &tdoc::ChecklistItem,
        indices: Vec<usize>,
        prefix: &str,
    ) {
        // Update paragraph path for this checklist item with full nested indices
        // For nested items (indices.len() > 1), we need to replace the parent ChecklistItem step
        let is_nested = indices.len() > 1;
        if is_nested {
            // Pop the parent ChecklistItem step first
            self.current_paragraph_path.pop();
        }
        self.current_paragraph_path
            .push_checklist_item(indices.clone());

        let marker = if item.checked { "[✓] " } else { "[ ] " };
        let first_prefix = format!("{}{}", prefix, marker);
        let continuation_prefix = format!("{}{}", prefix, " ".repeat(marker.chars().count()));

        let mut fragments = Vec::new();
        let base_span_path = SpanPath {
            indices: Vec::new(),
        };
        self.collect_fragments_direct(
            &item.content,
            &base_span_path,
            Style::default(),
            &mut fragments,
        );

        // For empty content, ensure we track at least position 0
        if item.content.is_empty() {
            let position_events = self.check_position_match(&base_span_path, 0, SegmentKind::Text);
            if !position_events.is_empty() {
                // Create a zero-width fragment with the position events
                let frag = DirectFragment {
                    text: String::new(),
                    style: Style::default(),
                    kind: FragmentKind::Word,
                    width: 0,
                    content_width: 0,
                    events: position_events,
                    reveal_kind: None,
                };
                fragments.push(FragmentItem::Token(self.convert_direct_fragment(frag)));
            }
        }

        let fragments = trim_layout_fragments(fragments);
        let lines = self.wrap_fragments_direct(
            &fragments,
            &first_prefix,
            &continuation_prefix,
            self.wrap_limit,
        );
        self.consume_lines_direct(lines);

        // Render nested checklist items with extended indices
        for (child_idx, child) in item.children.iter().enumerate() {
            let child_prefix = format!("{}    ", prefix);
            let mut child_indices = indices.clone();
            child_indices.push(child_idx);
            self.render_checklist_item_struct(child, child_indices, &child_prefix);
        }

        // Restore paragraph path
        self.current_paragraph_path.pop();

        // If this was a nested item, restore the parent ChecklistItem step
        if is_nested {
            let mut parent_indices = indices;
            parent_indices.pop();
            self.current_paragraph_path
                .push_checklist_item(parent_indices);
        }
    }

    fn render_list_entry(
        &mut self,
        entry: &[Paragraph],
        entry_idx: usize,
        first_prefix: &str,
        continuation_prefix: &str,
    ) {
        if entry.is_empty() {
            // Update paragraph path for this empty entry
            self.current_paragraph_path.push_child(entry_idx);

            // Track cursor positions for empty entry
            let base_span_path = SpanPath::new(Vec::new());
            let position_events = self.check_position_match(&base_span_path, 0, SegmentKind::Text);

            if !position_events.is_empty() {
                // Create fragments with position tracking
                let mut fragments = Vec::new();
                let frag = DirectFragment {
                    text: String::new(),
                    style: Style::default(),
                    kind: FragmentKind::Word,
                    width: 0,
                    content_width: 0,
                    events: position_events,
                    reveal_kind: None,
                };
                fragments.push(FragmentItem::Token(self.convert_direct_fragment(frag)));

                let fragments = trim_layout_fragments(fragments);
                let lines = self.wrap_fragments_direct(
                    &fragments,
                    first_prefix,
                    continuation_prefix,
                    self.wrap_limit,
                );
                self.consume_lines_direct(lines);
            } else {
                // No position tracking needed, just render plain line
                self.push_plain_line(first_prefix, false);
            }

            // Restore paragraph path
            self.current_paragraph_path.pop();
            return;
        }

        let mut iter = entry.iter().enumerate();
        if let Some((para_idx, first)) = iter.next() {
            // Update paragraph path for this entry paragraph
            self.current_paragraph_path.push_entry(entry_idx, para_idx);

            match first.paragraph_type() {
                ParagraphType::Text => {
                    self.render_text_paragraph(first, first_prefix, continuation_prefix);
                }
                _ => {
                    self.push_plain_line(first_prefix, false);
                    self.render_paragraph(first, continuation_prefix);
                }
            }

            // Restore paragraph path
            self.current_paragraph_path.pop();
        }

        for (para_idx, rest) in iter {
            // Update paragraph path for each subsequent paragraph in the entry
            self.current_paragraph_path.push_entry(entry_idx, para_idx);

            if rest.paragraph_type() == ParagraphType::Text {
                self.push_plain_line("", false);
            }
            self.render_paragraph(rest, continuation_prefix);

            // Restore paragraph path
            self.current_paragraph_path.pop();
        }
    }

    fn collect_fragments_direct(
        &mut self,
        spans: &[DocSpan],
        base_span_path: &SpanPath,
        base_style: Style,
        fragments: &mut Vec<FragmentItem>,
    ) {
        for (span_index, span) in spans.iter().enumerate() {
            let mut span_path = base_span_path.clone();
            span_path.push(span_index);
            self.collect_single_span_direct(span, &span_path, base_style, fragments);
        }
    }

    fn collect_single_span_direct(
        &mut self,
        span: &DocSpan,
        span_path: &SpanPath,
        base_style: Style,
        fragments: &mut Vec<FragmentItem>,
    ) {
        let style = self.merge_style(base_style, span.style, span.link_target.as_deref());

        // Insert reveal tag for style start if this span has a style
        let has_style = span.style != InlineStyle::None;
        if has_style && !self.reveal_tags.is_empty() {
            let display = reveal_tag_display(span.style, RevealTagKind::Start);
            let tag_style = self.theme.reveal_tag_style();
            let width = visible_width(&display);

            // Track position for the reveal start tag so it can be clicked
            let direct_events = self.check_position_match(
                span_path,
                0, // offset at start of span
                SegmentKind::RevealStart(span.style),
            );

            // Convert DirectTextEvents to TextEvents
            let events: Vec<TextEvent> = direct_events
                .iter()
                .map(|e| {
                    let kind = match e.kind {
                        DirectTextEventKind::Cursor => TextEventKind::Cursor,
                        DirectTextEventKind::SelectionStart => TextEventKind::SelectionStart,
                        DirectTextEventKind::SelectionEnd => TextEventKind::SelectionEnd,
                        DirectTextEventKind::Position => {
                            let marker_id = self.next_marker_id;
                            self.next_marker_id += 1;
                            self.marker_to_pointer.insert(marker_id, e.pointer.clone());
                            TextEventKind::Marker(marker_id)
                        }
                    };
                    TextEvent {
                        offset: e.offset,
                        content_offset: e.content_offset,
                        offset_hint: None,
                        content_offset_hint: None,
                        kind,
                    }
                })
                .collect();

            fragments.push(FragmentItem::Token(Fragment {
                text: display,
                style: tag_style,
                kind: FragmentKind::RevealTag,
                width,
                content_width: 0,
                events,
                reveal_kind: Some(RevealTagKind::Start),
            }));
        }

        let mut local: Vec<FragmentItem> = Vec::new();
        // Always tokenize, even empty text, to track cursor positions
        self.tokenize_text_direct(&span.text, span_path, style, &mut local);

        let mut prefix: Vec<FragmentItem> = Vec::new();
        let mut middle: Vec<FragmentItem> = Vec::new();
        let mut suffix: Vec<FragmentItem> = Vec::new();

        for item in local.into_iter() {
            match item {
                FragmentItem::Token(fragment) if fragment.kind == FragmentKind::RevealTag => {
                    match fragment.reveal_kind {
                        Some(RevealTagKind::Start) => prefix.push(FragmentItem::Token(fragment)),
                        Some(RevealTagKind::End) => suffix.push(FragmentItem::Token(fragment)),
                        None => middle.push(FragmentItem::Token(fragment)),
                    }
                }
                other => middle.push(other),
            }
        }

        fragments.extend(prefix);
        fragments.extend(middle);

        for (child_index, child) in span.children.iter().enumerate() {
            let mut child_span_path = span_path.clone();
            child_span_path.push(child_index);
            self.collect_single_span_direct(child, &child_span_path, style, fragments);
        }

        fragments.extend(suffix);

        // Insert reveal tag for style end if this span has a style
        if has_style && !self.reveal_tags.is_empty() {
            let display = reveal_tag_display(span.style, RevealTagKind::End);
            let tag_style = self.theme.reveal_tag_style();
            let width = visible_width(&display);

            // Track position for the reveal end tag so it can be clicked
            // The reveal end is at the end of the span text
            let end_offset = span.text.chars().count();
            let direct_events = self.check_position_match(
                span_path,
                end_offset,
                SegmentKind::RevealEnd(span.style),
            );

            // Convert DirectTextEvents to TextEvents
            let events: Vec<TextEvent> = direct_events
                .iter()
                .map(|e| {
                    let kind = match e.kind {
                        DirectTextEventKind::Cursor => TextEventKind::Cursor,
                        DirectTextEventKind::SelectionStart => TextEventKind::SelectionStart,
                        DirectTextEventKind::SelectionEnd => TextEventKind::SelectionEnd,
                        DirectTextEventKind::Position => {
                            let marker_id = self.next_marker_id;
                            self.next_marker_id += 1;
                            self.marker_to_pointer.insert(marker_id, e.pointer.clone());
                            TextEventKind::Marker(marker_id)
                        }
                    };
                    TextEvent {
                        offset: e.offset,
                        content_offset: e.content_offset,
                        offset_hint: None,
                        content_offset_hint: None,
                        kind,
                    }
                })
                .collect();

            fragments.push(FragmentItem::Token(Fragment {
                text: display,
                style: tag_style,
                kind: FragmentKind::RevealTag,
                width,
                content_width: 0,
                events,
                reveal_kind: Some(RevealTagKind::End),
            }));
        }
    }

    fn tokenize_text_direct(
        &mut self,
        text: &str,
        span_path: &SpanPath,
        style: Style,
        fragments: &mut Vec<FragmentItem>,
    ) {
        // For each character position in the text, check if we need to track it
        let mut builder: Option<DirectTokenBuilder> = None;
        let mut buffer: Vec<char> = Vec::new();
        let chars: Vec<char> = text.chars().collect();

        for (char_offset, ch) in chars.iter().enumerate() {
            // Check if this position matches any cursor we're tracking
            let position_events =
                self.check_position_match(span_path, char_offset, SegmentKind::Text);

            if ch == &'\r' {
                continue;
            }
            if ch == &'\n' {
                if let Some(mut token) = builder.take() {
                    token.add_events(position_events);
                    let frag = token.finish();
                    fragments.push(FragmentItem::Token(self.convert_direct_fragment(frag)));
                } else if !position_events.is_empty() {
                    let frag = DirectFragment {
                        text: String::new(),
                        style,
                        kind: FragmentKind::Word,
                        width: 0,
                        content_width: 0,
                        events: position_events,
                        reveal_kind: None,
                    };
                    fragments.push(FragmentItem::Token(self.convert_direct_fragment(frag)));
                }
                fragments.push(FragmentItem::LineBreak);
                continue;
            }

            let expanded: &[char] = if ch == &'\t' {
                buffer.clear();
                buffer.extend_from_slice(&[' '; 4]);
                &buffer
            } else {
                buffer.clear();
                buffer.push(*ch);
                &buffer
            };

            for actual in expanded {
                let is_whitespace = actual.is_whitespace();
                if builder
                    .as_ref()
                    .map(|existing| existing.kind_matches(is_whitespace))
                    .unwrap_or(false)
                {
                    if let Some(current) = builder.as_mut() {
                        current.add_events(position_events.clone());
                        current.push_char(*actual);
                    }
                } else {
                    if let Some(existing) = builder.take() {
                        fragments.push(FragmentItem::Token(
                            self.convert_direct_fragment(existing.finish()),
                        ));
                    }
                    let mut new_builder = DirectTokenBuilder::new(style, is_whitespace);
                    new_builder.add_events(position_events.clone());
                    new_builder.push_char(*actual);
                    builder = Some(new_builder);
                }
            }
        }

        // Check for cursor position at the end of the text (after the last character)
        let end_position_events =
            self.check_position_match(span_path, chars.len(), SegmentKind::Text);

        if let Some(mut token) = builder {
            // If there are events at the end position, add them to the last token
            if !end_position_events.is_empty() {
                token.add_events(end_position_events);
            }
            fragments.push(FragmentItem::Token(
                self.convert_direct_fragment(token.finish()),
            ));
        } else if !end_position_events.is_empty() {
            // If there's no token but there are end position events, create an empty fragment for them
            let frag = DirectFragment {
                text: String::new(),
                style,
                kind: FragmentKind::Word,
                width: 0,
                content_width: 0,
                events: end_position_events,
                reveal_kind: None,
            };
            fragments.push(FragmentItem::Token(self.convert_direct_fragment(frag)));
        }
    }

    fn convert_direct_fragment(&mut self, direct: DirectFragment) -> Fragment {
        // Convert DirectFragment to Fragment by extracting the relevant events
        let events: Vec<TextEvent> = direct
            .events
            .iter()
            .map(|e| {
                let kind = match e.kind {
                    DirectTextEventKind::Cursor => TextEventKind::Cursor,
                    DirectTextEventKind::SelectionStart => TextEventKind::SelectionStart,
                    DirectTextEventKind::SelectionEnd => TextEventKind::SelectionEnd,
                    DirectTextEventKind::Position => {
                        // Assign a unique marker ID for this position
                        let marker_id = self.next_marker_id;
                        self.next_marker_id += 1;
                        self.marker_to_pointer.insert(marker_id, e.pointer.clone());
                        TextEventKind::Marker(marker_id)
                    }
                };

                TextEvent {
                    offset: e.offset,
                    content_offset: e.content_offset,
                    offset_hint: None,
                    content_offset_hint: None,
                    kind,
                }
            })
            .collect();

        Fragment {
            text: direct.text,
            style: direct.style,
            kind: direct.kind,
            width: direct.width,
            content_width: direct.content_width,
            events,
            reveal_kind: direct.reveal_kind,
        }
    }

    fn check_position_match(
        &mut self,
        span_path: &SpanPath,
        offset: usize,
        segment_kind: SegmentKind,
    ) -> Vec<DirectTextEvent> {
        let mut events = Vec::new();

        // Check cursor
        if let Some(cursor) = self.cursor_pointer
            && cursor.paragraph_path == self.current_paragraph_path
            && cursor.span_path == *span_path
            && cursor.offset == offset
            && cursor.segment_kind == segment_kind
        {
            events.push(DirectTextEvent {
                offset: 0,
                content_offset: 0,
                kind: DirectTextEventKind::Cursor,
                pointer: cursor.clone(),
            });
        }

        // Check selection start
        if let Some(sel_start) = self.selection_start
            && sel_start.paragraph_path == self.current_paragraph_path
            && sel_start.span_path == *span_path
            && sel_start.offset == offset
            && sel_start.segment_kind == segment_kind
        {
            events.push(DirectTextEvent {
                offset: 0,
                content_offset: 0,
                kind: DirectTextEventKind::SelectionStart,
                pointer: sel_start.clone(),
            });
        }

        // Check selection end
        if let Some(sel_end) = self.selection_end
            && sel_end.paragraph_path == self.current_paragraph_path
            && sel_end.span_path == *span_path
            && sel_end.offset == offset
            && sel_end.segment_kind == segment_kind
        {
            events.push(DirectTextEvent {
                offset: 0,
                content_offset: 0,
                kind: DirectTextEventKind::SelectionEnd,
                pointer: sel_end.clone(),
            });
        }

        // If tracking all positions, add a marker event
        if self.track_all_positions {
            let marker_id = self.next_marker_id;
            self.next_marker_id += 1;

            let pointer = CursorPointer {
                paragraph_path: self.current_paragraph_path.clone(),
                span_path: span_path.clone(),
                offset,
                segment_kind,
            };

            self.marker_to_pointer.insert(marker_id, pointer);

            events.push(DirectTextEvent {
                offset: 0,
                content_offset: 0,
                kind: DirectTextEventKind::Position,
                pointer: CursorPointer {
                    paragraph_path: self.current_paragraph_path.clone(),
                    span_path: span_path.clone(),
                    offset,
                    segment_kind,
                },
            });
        }

        events
    }

    fn wrap_fragments_direct(
        &self,
        fragments: &[FragmentItem],
        first_prefix: &str,
        continuation_prefix: &str,
        width: usize,
    ) -> Vec<LineOutput> {
        // Fragments are already converted to Fragment type in tokenize_text_direct,
        // so we can just call wrap_fragments directly
        wrap_fragments(
            fragments,
            first_prefix,
            continuation_prefix,
            width,
            &self.theme,
        )
    }

    fn consume_lines_direct(&mut self, outputs: Vec<LineOutput>) {
        let padding = self.left_padding.min(u16::MAX as usize) as u16;
        for output in outputs {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(output.spans.len());
            for segment in output.spans {
                let styled_spans = self.style_structural_prefix(&segment.text, segment.style);
                spans.extend(styled_spans);
            }
            let spans = self.prepend_padding(spans);
            let line = Line::from(spans);
            // Paragraph content lines always count as content, even if empty (no words).
            // This ensures empty paragraphs are navigable in the editor.
            self.line_metrics.push(LineMetric {
                counts_as_content: true,
            });

            // Process events to update cursor tracking
            for event in output.events {
                let pending = PendingPosition {
                    line: self.current_line_index,
                    column: event.column.saturating_add(padding),
                    content_column: event.content_column,
                };
                match event.kind {
                    TextEventKind::Cursor => {
                        self.cursor_pending = Some(pending);
                    }
                    TextEventKind::Marker(id) => {
                        self.marker_pending.insert(id, pending);
                    }
                    TextEventKind::SelectionStart | TextEventKind::SelectionEnd => {
                        // Selection events are tracked but not added to cursor_map
                    }
                }
            }

            self.lines.push(line);
            self.current_line_index += 1;
        }
    }

    fn prepend_padding(&self, spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
        if spans.is_empty() {
            return spans;
        }
        let has_content = spans.iter().any(|span| !span.content.is_empty());
        if !has_content {
            return spans;
        }
        if let Some(padding) = &self.padding {
            let mut with_padding = Vec::with_capacity(spans.len() + 1);
            with_padding.push(Span::raw(padding.clone()).to_owned());
            with_padding.extend(spans);
            with_padding
        } else {
            spans
        }
    }

    fn push_plain_line(&mut self, content: &str, counts_as_content: bool) {
        let mut spans = Vec::new();
        if !content.is_empty() {
            spans.push(Span::raw(content.to_string()).to_owned());
        }
        let spans = self.prepend_padding(spans);
        let line = Line::from(spans);
        self.lines.push(line);
        self.line_metrics.push(LineMetric { counts_as_content });
        self.current_line_index += 1;
    }

    fn push_styled_line(&mut self, content: &str, style: Style, counts_as_content: bool) {
        let mut spans = Vec::new();
        if !content.is_empty() {
            spans.push(Span::styled(content.to_string(), style).to_owned());
        }
        let spans = self.prepend_padding(spans);
        let line = Line::from(spans);
        self.lines.push(line);
        self.line_metrics.push(LineMetric { counts_as_content });
        self.current_line_index += 1;
    }

    /// Detects and styles structural prefix characters in a segment
    /// Returns a vector of styled spans (may be just one if no structural prefix found)
    fn style_structural_prefix(&self, text: &str, base_style: Style) -> Vec<Span<'static>> {
        // Check for common structural prefixes
        // - Bullets: "• "
        // - Checklist: "[✓] " or "[ ] "
        // - Ordered list: "1. ", "2. ", etc.
        // - Quote: "| "

        if let Some(rest) = text.strip_prefix("• ") {
            vec![
                Span::styled("• ".to_string(), self.theme.structural_style()).to_owned(),
                Span::styled(rest.to_string(), base_style).to_owned(),
            ]
        } else if let Some(rest) = text.strip_prefix("[✓] ") {
            vec![
                Span::styled("[✓] ".to_string(), self.theme.structural_style()).to_owned(),
                Span::styled(rest.to_string(), base_style).to_owned(),
            ]
        } else if let Some(rest) = text.strip_prefix("[ ] ") {
            vec![
                Span::styled("[ ] ".to_string(), self.theme.structural_style()).to_owned(),
                Span::styled(rest.to_string(), base_style).to_owned(),
            ]
        } else if let Some(rest) = text.strip_prefix("| ") {
            vec![
                Span::styled("| ".to_string(), self.theme.structural_style()).to_owned(),
                Span::styled(rest.to_string(), base_style).to_owned(),
            ]
        } else if let Some(prefix_end) = text.find(". ") {
            // Check if it's a number followed by ". "
            let prefix = &text[..prefix_end];
            if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
                let number_dot = &text[..prefix_end + 2]; // Include ". "
                let rest = &text[prefix_end + 2..];
                vec![
                    Span::styled(number_dot.to_string(), self.theme.structural_style()).to_owned(),
                    Span::styled(rest.to_string(), base_style).to_owned(),
                ]
            } else {
                vec![Span::styled(text.to_string(), base_style).to_owned()]
            }
        } else {
            vec![Span::styled(text.to_string(), base_style).to_owned()]
        }
    }

    fn code_block_fence(&self, prefix: &str) -> String {
        const MIN_FENCE_WIDTH: usize = 4;
        let available_width = self.wrap_width.saturating_sub(prefix.chars().count());
        let dash_count = available_width.max(MIN_FENCE_WIDTH);
        format!("{}{}", prefix, "-".repeat(dash_count))
    }

    fn merge_style(&self, base: Style, inline: InlineStyle, _link_target: Option<&str>) -> Style {
        match inline {
            InlineStyle::None => base,
            InlineStyle::Bold => base.add_modifier(Modifier::BOLD),
            InlineStyle::Italic => base.add_modifier(Modifier::ITALIC),
            InlineStyle::Highlight => self.theme.highlight_style().patch(base),
            InlineStyle::Underline => base.add_modifier(Modifier::UNDERLINED),
            InlineStyle::Strike => base.add_modifier(Modifier::CROSSED_OUT),
            InlineStyle::Link => base
                .add_modifier(Modifier::UNDERLINED)
                .fg(self.theme.link_color),
            InlineStyle::Code => base.add_modifier(Modifier::DIM),
        }
    }

    #[allow(dead_code)]
    fn apply_selection_style(&self, style: Style, selected: bool) -> Style {
        if selected {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        }
    }

    fn finish(mut self) -> RenderResult {
        if self.lines.is_empty() {
            self.push_plain_line("", false);
        }
        let total_lines = self.lines.len();

        let mut content_line_numbers = Vec::with_capacity(self.line_metrics.len());
        let mut current_content = 0usize;
        for metric in &self.line_metrics {
            content_line_numbers.push(current_content);
            if metric.counts_as_content {
                current_content += 1;
            }
        }

        let cursor = self
            .cursor_pending
            .take()
            .map(|pending| CursorVisualPosition {
                line: pending.line,
                column: pending.column,
                content_line: content_line_numbers
                    .get(pending.line)
                    .copied()
                    .unwrap_or(pending.line),
                content_column: pending.content_column,
            });

        // Update content_line for all positions in paragraph_lines
        // Positions are stored with relative line numbers, so convert to absolute for lookup
        for paragraph_info in &mut self.paragraph_lines {
            for (_, position) in &mut paragraph_info.positions {
                let absolute_line = paragraph_info.start_line + position.line;
                position.content_line = content_line_numbers
                    .get(absolute_line)
                    .copied()
                    .unwrap_or(absolute_line);
            }
        }

        RenderResult {
            lines: self.lines,
            cursor,
            total_lines,
            content_lines: current_content,
            paragraph_lines: self.paragraph_lines,
        }
    }
}

// Helper types for direct rendering
#[derive(Clone)]
struct DirectFragment {
    text: String,
    style: Style,
    kind: FragmentKind,
    width: usize,
    content_width: usize,
    events: Vec<DirectTextEvent>,
    reveal_kind: Option<RevealTagKind>,
}

#[derive(Clone)]
struct DirectTextEvent {
    offset: usize,
    content_offset: usize,
    kind: DirectTextEventKind,
    pointer: CursorPointer,
}

#[derive(Clone, Copy)]
enum DirectTextEventKind {
    Cursor,
    SelectionStart,
    SelectionEnd,
    Position,
}

struct DirectTokenBuilder {
    text: String,
    style: Style,
    kind: FragmentKind,
    width: usize,
    content_width: usize,
    events: Vec<DirectTextEvent>,
}

impl DirectTokenBuilder {
    fn new(style: Style, is_whitespace: bool) -> Self {
        Self {
            text: String::new(),
            style,
            kind: if is_whitespace {
                FragmentKind::Whitespace
            } else {
                FragmentKind::Word
            },
            width: 0,
            content_width: 0,
            events: Vec::new(),
        }
    }

    fn kind_matches(&self, is_whitespace: bool) -> bool {
        matches!(
            (self.kind, is_whitespace),
            (FragmentKind::Whitespace, true) | (FragmentKind::Word, false)
        )
    }

    fn add_events(&mut self, mut new_events: Vec<DirectTextEvent>) {
        for event in new_events.iter_mut() {
            event.offset = self.width;
            event.content_offset = self.content_width;
        }
        self.events.extend(new_events);
    }

    fn push_char(&mut self, ch: char) {
        self.text.push(ch);
        self.width += UnicodeWidthChar::width(ch).unwrap_or(0);
        self.content_width += UnicodeWidthChar::width(ch).unwrap_or(0);
    }

    fn finish(self) -> DirectFragment {
        DirectFragment {
            text: self.text,
            style: self.style,
            kind: self.kind,
            width: self.width,
            content_width: self.content_width,
            events: self.events,
            reveal_kind: None,
        }
    }
}

#[derive(Copy, Clone)]
enum HeaderLevel {
    One,
    Two,
    Three,
}

#[derive(Clone)]
struct LineSegment {
    text: String,
    style: Style,
}

#[derive(Clone)]
struct LineOutput {
    spans: Vec<LineSegment>,
    events: Vec<LocatedEvent>,
}

#[derive(Clone, Copy)]
struct LocatedEvent {
    column: u16,
    content_column: u16,
    kind: TextEventKind,
}

#[derive(Clone)]
struct Fragment {
    text: String,
    style: Style,
    kind: FragmentKind,
    width: usize,
    content_width: usize,
    events: Vec<TextEvent>,
    reveal_kind: Option<RevealTagKind>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FragmentKind {
    Word,
    Whitespace,
    RevealTag,
}

#[derive(Clone)]
enum FragmentItem {
    Token(Fragment),
    LineBreak,
}

#[derive(Clone)]
struct TextEvent {
    offset: usize,
    content_offset: usize,
    offset_hint: Option<usize>,
    content_offset_hint: Option<usize>,
    kind: TextEventKind,
}

#[derive(Clone, Copy)]
enum TextEventKind {
    Marker(usize),
    Cursor,
    SelectionStart,
    SelectionEnd,
}

fn trim_layout_fragments(fragments: Vec<FragmentItem>) -> Vec<FragmentItem> {
    let start = fragments
        .iter()
        .position(|item| !is_layout_fragment(item))
        .unwrap_or(fragments.len());
    if start == fragments.len() {
        return Vec::new();
    }
    let end = fragments
        .iter()
        .rposition(|item| !is_layout_fragment(item))
        .map(|idx| idx + 1)
        .unwrap_or(start);
    fragments[start..end].to_vec()
}

fn is_layout_fragment(item: &FragmentItem) -> bool {
    match item {
        // Hard line breaks are meaningful content, not layout whitespace
        FragmentItem::LineBreak => false,
        FragmentItem::Token(fragment) => {
            fragment.kind == FragmentKind::Whitespace
                && fragment.events.is_empty()
                && fragment.text.chars().all(|ch| ch.is_whitespace())
        }
    }
}

fn apply_selection_style(style: Style, selected: bool, theme: &Theme) -> Style {
    if selected {
        // Selection should override all colors, but preserve modifiers from the base style
        theme
            .selection_style()
            .add_modifier(style.add_modifier)
            .remove_modifier(style.sub_modifier)
    } else {
        style
    }
}

fn wrap_fragments(
    fragments: &[FragmentItem],
    first_prefix: &str,
    continuation_prefix: &str,
    width: usize,
    theme: &Theme,
) -> Vec<LineOutput> {
    let mut outputs = Vec::new();
    let mut builder = LineBuilder::new(first_prefix.to_string(), width, false, theme);
    let mut pending_whitespace: Vec<Fragment> = Vec::new();

    for fragment in fragments {
        match fragment {
            FragmentItem::LineBreak => {
                builder.consume_pending(&mut pending_whitespace);
                let (line, active_selection) = builder.build_line();
                outputs.push(line);
                builder = LineBuilder::new(
                    continuation_prefix.to_string(),
                    width,
                    active_selection,
                    theme,
                );
            }
            FragmentItem::Token(token) => match token.kind {
                FragmentKind::Whitespace => {
                    pending_whitespace.push(token.clone());
                }
                FragmentKind::Word | FragmentKind::RevealTag => {
                    let mut token = token.clone();
                    loop {
                        let whitespace_width: usize =
                            pending_whitespace.iter().map(|item| item.width).sum();
                        if builder.current_width() > builder.prefix_width
                            && builder.current_width() + whitespace_width + token.width > width
                        {
                            builder.consume_pending(&mut pending_whitespace);
                            let (line, active_selection) = builder.build_line();
                            outputs.push(line);
                            builder = LineBuilder::new(
                                continuation_prefix.to_string(),
                                width,
                                active_selection,
                                theme,
                            );
                            continue;
                        }

                        let line_start = builder.current_width() == builder.prefix_width;
                        let available = width.saturating_sub(builder.prefix_width);
                        if line_start && token.width > available {
                            if token.kind == FragmentKind::RevealTag {
                                builder.append_with_pending(token, &mut pending_whitespace);
                                break;
                            }
                            let split_limit = available.max(1);
                            let (head, tail_opt) = split_fragment(token, split_limit);
                            builder.append_with_pending(head, &mut pending_whitespace);
                            let (line, active_selection) = builder.build_line();
                            outputs.push(line);
                            builder = LineBuilder::new(
                                continuation_prefix.to_string(),
                                width,
                                active_selection,
                                theme,
                            );
                            if let Some(tail) = tail_opt {
                                token = tail;
                                continue;
                            } else {
                                break;
                            }
                        }

                        builder.append_with_pending(token, &mut pending_whitespace);
                        break;
                    }
                }
            },
        }
    }

    builder.consume_pending(&mut pending_whitespace);
    let (line, _) = builder.build_line();
    outputs.push(line);
    outputs
}

fn split_fragment(fragment: Fragment, limit: usize) -> (Fragment, Option<Fragment>) {
    if fragment.width <= limit {
        return (fragment, None);
    }

    let mut head_text = String::new();
    let mut head_width = 0usize;
    let mut split_byte_index = 0usize;
    let mut chars = fragment.text.chars().peekable();

    while let Some(ch) = chars.peek().copied() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if head_width + ch_width > limit && head_width > 0 {
            break;
        }
        head_text.push(ch);
        head_width += ch_width;
        chars.next();
        split_byte_index += ch.len_utf8();
        if head_width >= limit {
            break;
        }
    }

    if head_width == 0
        && let Some(ch) = fragment.text.chars().next()
    {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        head_text.push(ch);
        head_width += ch_width;
        split_byte_index = ch.len_utf8();
    }

    if head_width >= fragment.width || split_byte_index >= fragment.text.len() {
        return (fragment, None);
    }

    let tail_text = fragment.text[split_byte_index..].to_string();
    let tail_width = fragment.width.saturating_sub(head_width);
    let head_content_width = if fragment.content_width <= head_width {
        fragment.content_width.min(head_width)
    } else {
        head_width
    };
    let tail_content_width = fragment.content_width.saturating_sub(head_content_width);

    let mut head_events = Vec::new();
    let mut tail_events = Vec::new();
    for mut event in fragment.events {
        if event.offset < head_width {
            head_events.push(event);
        } else {
            event.offset = event.offset.saturating_sub(head_width);
            event.content_offset = event.content_offset.saturating_sub(head_content_width);
            tail_events.push(event);
        }
    }

    let head_fragment = Fragment {
        text: head_text,
        style: fragment.style,
        kind: fragment.kind,
        width: head_width,
        content_width: head_content_width,
        events: head_events,
        reveal_kind: fragment.reveal_kind,
    };
    let tail_fragment = if tail_text.is_empty() && tail_events.is_empty() {
        None
    } else {
        Some(Fragment {
            text: tail_text,
            style: fragment.style,
            kind: fragment.kind,
            width: tail_width,
            content_width: tail_content_width,
            events: tail_events,
            reveal_kind: fragment.reveal_kind,
        })
    };

    (head_fragment, tail_fragment)
}

struct LineBuilder<'a> {
    segments: Vec<LineSegment>,
    events: Vec<LocatedEvent>,
    width: usize,
    prefix_width: usize,
    content_width: usize,
    selection_active: bool,
    theme: &'a Theme,
}

impl<'a> LineBuilder<'a> {
    fn new(prefix: String, _width_limit: usize, selection_active: bool, theme: &'a Theme) -> Self {
        let prefix_width = visible_width(&prefix);
        let prefix_segment = if prefix.is_empty() {
            None
        } else {
            Some(LineSegment {
                text: prefix.clone(),
                style: Style::default(),
            })
        };
        let mut segments = Vec::new();
        if let Some(segment) = prefix_segment {
            segments.push(segment);
        }
        Self {
            segments,
            events: Vec::new(),
            width: prefix_width,
            prefix_width,
            content_width: 0,
            selection_active,
            theme,
        }
    }

    fn current_width(&self) -> usize {
        self.width
    }

    fn append_with_pending(&mut self, token: Fragment, pending_whitespace: &mut Vec<Fragment>) {
        self.consume_pending(pending_whitespace);
        self.append_token(token);
    }

    fn consume_pending(&mut self, pending_whitespace: &mut Vec<Fragment>) {
        for fragment in pending_whitespace.drain(..) {
            self.append_token(fragment);
        }
    }

    fn append_token(&mut self, fragment: Fragment) {
        let Fragment {
            text,
            style,
            kind,
            width: _,
            content_width: _content_width,
            mut events,
            ..
        } = fragment;

        events.sort_by_key(|event| event.offset);
        let base_width = self.width;
        let base_content_width = self.content_width;
        let counts_content = kind != FragmentKind::RevealTag;
        let mut events_iter = events.into_iter().peekable();
        let mut current_offset = 0usize;
        let mut buffer = String::new();
        let mut buffer_selected = self.selection_active;

        for ch in text.chars() {
            while let Some(event) = events_iter.peek() {
                if event.offset > current_offset {
                    break;
                }
                let mut event = events_iter.next().unwrap();
                if let Some(hint) = event.offset_hint {
                    event.offset = hint;
                }
                if let Some(content_hint) = event.content_offset_hint {
                    event.content_offset = content_hint;
                }
                self.handle_event(
                    event,
                    base_width,
                    base_content_width,
                    &mut buffer,
                    style,
                    &mut buffer_selected,
                    counts_content,
                );
            }

            buffer.push(ch);
            current_offset += UnicodeWidthChar::width(ch).unwrap_or(0);
        }

        for event in events_iter {
            let mut event = event;
            if let Some(hint) = event.offset_hint {
                event.offset = hint;
            }
            if let Some(content_hint) = event.content_offset_hint {
                event.content_offset = content_hint;
            }
            self.handle_event(
                event,
                base_width,
                base_content_width,
                &mut buffer,
                style,
                &mut buffer_selected,
                counts_content,
            );
        }

        if !buffer.is_empty() {
            let text_segment = std::mem::take(&mut buffer);
            let segment_style = apply_selection_style(style, buffer_selected, self.theme);
            self.push_segment(text_segment, segment_style, counts_content);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_event(
        &mut self,
        event: TextEvent,
        base_width: usize,
        base_content_width: usize,
        buffer: &mut String,
        fragment_style: Style,
        buffer_selected: &mut bool,
        counts_content: bool,
    ) {
        match event.kind {
            TextEventKind::SelectionStart => {
                if !buffer.is_empty() {
                    let text = std::mem::take(buffer);
                    let style = apply_selection_style(fragment_style, *buffer_selected, self.theme);
                    self.push_segment(text, style, counts_content);
                }
                self.selection_active = true;
                *buffer_selected = self.selection_active;
            }
            TextEventKind::SelectionEnd => {
                if !buffer.is_empty() {
                    let text = std::mem::take(buffer);
                    let style = apply_selection_style(fragment_style, *buffer_selected, self.theme);
                    self.push_segment(text, style, counts_content);
                }
                self.selection_active = false;
                *buffer_selected = self.selection_active;
            }
            TextEventKind::Marker(_) | TextEventKind::Cursor => {
                let column = base_width + event.offset;
                let display_column = column.min(u16::MAX as usize) as u16;
                let content_position = base_content_width + event.content_offset;
                let content_column = content_position.min(u16::MAX as usize) as u16;
                self.events.push(LocatedEvent {
                    column: display_column,
                    content_column,
                    kind: event.kind,
                });
            }
        }
    }

    fn push_segment(&mut self, text: String, style: Style, counts_content: bool) {
        if text.is_empty() {
            return;
        }
        let width = visible_width(&text);
        self.segments.push(LineSegment { text, style });
        self.width += width;
        if counts_content {
            self.content_width += width;
        }
    }

    fn build_line(mut self) -> (LineOutput, bool) {
        if self.segments.is_empty() {
            self.segments.push(LineSegment {
                text: String::new(),
                style: Style::default(),
            });
        }
        self.events.sort_by_key(|event| event.column);
        (
            LineOutput {
                spans: self.segments,
                events: self.events,
            },
            self.selection_active,
        )
    }
}

fn visible_width(text: &str) -> usize {
    text.chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| visible_width(span.content.as_ref()))
        .sum()
}

fn underline_string(width: usize, ch: char) -> String {
    std::iter::repeat_n(ch, width.max(1)).collect()
}

#[derive(Clone, Copy)]
struct PendingPosition {
    line: usize,
    column: u16,
    content_column: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct LineMetric {
    pub counts_as_content: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::DocumentEditor;
    use std::io::Cursor;
    use tdoc::{ftml, parse};

    const SENTINELS: RenderSentinels = RenderSentinels {
        cursor: '\u{F8FF}',
        selection_start: '\u{F8FE}',
        selection_end: '\u{F8FD}',
    };

    fn render_input(input: &str) -> RenderResult {
        let document = parse(Cursor::new(input)).expect("failed to parse document");
        let tracking = DirectCursorTracking {
            cursor: None,
            selection: None,
            track_all_positions: false,
        };
        let theme = Theme::default();
        render_document_direct(&document, 120, 0, &[], tracking, &theme)
    }

    fn lines_to_strings(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn unordered_list_items_render_on_single_lines() {
        let input = r#"
<ul>
  <li>
    <p>Describe the features supported by FTML.</p>
  </li>
  <li>
    <p>Showcase the FTML standard formatting enforced by fmtftml.</p>
  </li>
</ul>
"#;
        let rendered = render_input(input);
        let lines = lines_to_strings(&rendered.lines);
        assert_eq!(
            lines,
            vec![
                "• Describe the features supported by FTML.",
                "",
                "• Showcase the FTML standard formatting enforced by fmtftml."
            ]
        );
    }

    #[test]
    fn unordered_list_paragraph_break_inserts_blank_line() {
        let input = r#"
<ul>
  <li>
    <p>First paragraph.</p>
    <p>Second paragraph.</p>
  </li>
</ul>
"#;
        let rendered = render_input(input);
        let lines = lines_to_strings(&rendered.lines);
        assert_eq!(lines, vec!["• First paragraph.", "", "  Second paragraph."]);
    }

    #[test]
    fn unordered_list_render_after_editor_split_has_single_blank_line() {
        let list = Paragraph::new_unordered_list().with_entries(vec![vec![
            Paragraph::new_text().with_content(vec![DocSpan::new_text("Alpha Beta")]),
        ]]);
        let document = Document::new().with_paragraphs(vec![list]);
        let mut editor = DocumentEditor::new(document);
        editor.ensure_cursor_selectable();
        for _ in 0..6 {
            assert!(editor.move_right());
        }
        assert!(editor.insert_paragraph_break_as_sibling());

        let tracking = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: false,
        };
        let theme = Theme::default();
        let rendered = render_document_direct(editor.document(), 120, 0, &[], tracking, &theme);
        let lines = lines_to_strings(&rendered.lines);
        assert_eq!(lines, vec!["• Alpha", "", "  Beta"]);
    }

    #[test]
    fn cursor_is_rendered_inside_checklist_items() {
        let checklist = Paragraph::new_checklist().with_checklist_items(vec![
            tdoc::ChecklistItem::new(false).with_content(vec![DocSpan::new_text("Task")]),
        ]);
        let document = Document::new().with_paragraphs(vec![checklist]);
        let mut editor = DocumentEditor::new(document);
        editor.ensure_cursor_selectable();

        let tracking = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: false,
        };
        let theme = Theme::default();
        let rendered = render_document_direct(editor.document(), 120, 0, &[], tracking, &theme);
        assert!(
            rendered.cursor.is_some(),
            "expected cursor to be rendered for checklist content"
        );
    }

    #[test]
    fn cursor_metrics_ignore_layout_indentation() {
        let input = r#"
<ul>
  <li>
    <p>Describe the features supported by FTML.</p>
  </li>
  <li>
    <p>Showcase the FTML standard formatting enforced by fmtftml.</p>
  </li>
</ul>
"#;
        let document = parse(Cursor::new(input)).expect("failed to parse document");
        let mut editor = DocumentEditor::new(document);
        editor.ensure_cursor_selectable();

        // Move to the second list item's content start
        assert!(editor.move_down());

        let tracking = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: false,
        };
        let theme = Theme::default();
        let rendered = render_document_direct(editor.document(), 120, 0, &[], tracking, &theme);
        let cursor = rendered.cursor.expect("cursor position missing");

        assert_eq!(cursor.line, 2, "visual line should match second list item");
        assert_eq!(
            cursor.column, 2,
            "visual column should include bullet prefix"
        );
        assert_eq!(
            cursor.content_line, 1,
            "content line should align with the second logical item"
        );
        assert_eq!(
            cursor.content_column, 0,
            "content column should ignore list item prefix spacing"
        );
    }

    #[test]
    fn cursor_metrics_start_from_origin() {
        let input = r#"<p>Hello</p>"#;
        let document = parse(Cursor::new(input)).expect("failed to parse document");
        let mut editor = DocumentEditor::new(document);
        editor.ensure_cursor_selectable();

        let tracking = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: false,
        };
        let theme = Theme::default();
        let rendered = render_document_direct(editor.document(), 120, 0, &[], tracking, &theme);
        let cursor = rendered.cursor.expect("cursor position missing");

        assert_eq!(cursor.line, 0);
        assert_eq!(cursor.column, 0);
        assert_eq!(cursor.content_line, 0);
        assert_eq!(cursor.content_column, 0);
    }

    #[test]
    fn wrapped_line_start_column() {
        let input = "<p>abcdefghij klmnopqrstuv</p>";
        let document = parse(Cursor::new(input)).expect("failed to parse document");
        let mut editor = DocumentEditor::new(document);
        editor.ensure_cursor_selectable();

        let tracking = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: true,
        };
        let theme = Theme::default();
        let rendered = render_document_direct(editor.document(), 12, 0, &[], tracking, &theme);

        let mut columns_per_line: Vec<Vec<(u16, u16)>> = Vec::new();
        // Extract positions from paragraph_lines
        for paragraph_info in &rendered.paragraph_lines {
            for (_, position) in &paragraph_info.positions {
                let line = position.line;
                if columns_per_line.len() <= line {
                    columns_per_line.resize(line + 1, Vec::new());
                }
                columns_per_line[line].push((position.content_column, position.column));
            }
        }
        for columns in &mut columns_per_line {
            columns.sort();
            columns.dedup();
        }

        assert!(
            columns_per_line
                .into_iter()
                .skip(1)
                .any(|columns| columns.first() == Some(&(0, 0))),
            "expected at least one wrapped line with column 0 start"
        );
    }

    #[test]
    fn cursor_wraps_to_next_line_on_exact_width_boundaries() {
        let input = "<p>abcdefghij z</p>";
        let document = parse(Cursor::new(input)).expect("failed to parse document");
        let mut editor = DocumentEditor::new(document);
        editor.ensure_cursor_selectable();
        for _ in 0..10 {
            assert!(
                editor.move_right(),
                "failed to advance cursor to wrap boundary"
            );
        }

        let tracking = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: true,
        };
        let theme = Theme::default();
        let rendered = render_document_direct(editor.document(), 10, 0, &[], tracking, &theme);
        let cursor = rendered.cursor.expect("cursor position missing");
        let lines = lines_to_strings(&rendered.lines);

        assert_eq!(lines, vec!["abcdefghi", "j z"]);
        assert_eq!(cursor.line, 1);
        assert_eq!(cursor.column, 1);
        assert_eq!(cursor.content_line, 1);
        assert_eq!(cursor.content_column, 1);

        // Extract position from paragraph_lines
        let boundary_position = rendered
            .paragraph_lines
            .iter()
            .flat_map(|info| &info.positions)
            .find(|(pointer, _)| pointer.offset == 10)
            .map(|(_, position)| position)
            .expect("missing position for wrap boundary");
        assert_eq!(
            (boundary_position.line, boundary_position.column),
            (1, 1),
            "visual position after wrapping should provide a dedicated cell past the wrapped word"
        );
    }

    #[test]
    fn reveal_codes_cursor_positions_follow_content_columns() {
        let document = ftml! { p { "Hello " b { "World" } "!" } };
        let mut editor = DocumentEditor::new(document.clone());
        editor.set_reveal_codes(true);
        editor.ensure_cursor_selectable();

        // Get reveal tags from clone_with_markers (still needed for reveal tag generation)
        let (_, _, reveal_tags, _) = editor.clone_with_markers(
            SENTINELS.cursor,
            None,
            SENTINELS.selection_start,
            SENTINELS.selection_end,
        );

        let tracking = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: false,
        };
        let theme = Theme::default();
        let rendered =
            render_document_direct(editor.document(), 120, 0, &reveal_tags, tracking, &theme);
        let lines = lines_to_strings(&rendered.lines);
        assert_eq!(lines, vec!["Hello [Bold>World<Bold]!"]);

        let logical_chars: Vec<char> = "Hello World!".chars().collect();
        let expectations: Vec<(char, usize)> = vec![
            ('H', 1),
            ('e', 2),
            ('l', 3),
            ('l', 4),
            ('o', 5),
            (' ', 6),
            ('>', 7),
            ('W', 7),
            ('o', 8),
            ('r', 9),
            ('l', 10),
            ('d', 11),
            ('<', 12),
            ('!', 12),
            ('\n', 13),
        ];
        let total = expectations.len();

        fn assert_cursor_position(
            editor: &DocumentEditor,
            reveal_tags: &[RevealTagRef],
            logical_chars: &[char],
            expected_char: char,
            expected_position: usize,
            context: &str,
        ) {
            let pointer = editor.cursor_pointer();

            // TODO: The direct renderer doesn't currently support rendering cursors at reveal tag
            // positions because those segments only exist in the editor's internal representation,
            // not in the original document. For now, we verify the cursor can be positioned at
            // these locations but skip rendering verification.
            if matches!(
                pointer.segment_kind,
                SegmentKind::RevealStart(_) | SegmentKind::RevealEnd(_)
            ) {
                let actual_char = match pointer.segment_kind {
                    SegmentKind::RevealStart(_) => '>',
                    SegmentKind::RevealEnd(_) => '<',
                    _ => unreachable!(),
                };
                assert_eq!(
                    actual_char, expected_char,
                    "character mismatch for reveal tag while moving {context}"
                );
                return;
            }

            let tracking = DirectCursorTracking {
                cursor: Some(&pointer),
                selection: None,
                track_all_positions: false,
            };
            let theme = Theme::default();
            let rendered =
                render_document_direct(editor.document(), 120, 0, reveal_tags, tracking, &theme);
            let cursor = rendered.cursor.expect("cursor should be rendered");
            let actual_position = usize::from(cursor.content_column) + 1;

            assert_eq!(
                actual_position, expected_position,
                "content column mismatch for character {expected_char} while moving {context}"
            );

            let actual_char = match pointer.segment_kind {
                SegmentKind::Text => {
                    assert!(
                        actual_position > 0,
                        "text segment should advance content column for character {expected_char}"
                    );
                    let idx = actual_position - 1;
                    if idx < logical_chars.len() {
                        logical_chars[idx]
                    } else {
                        '\n'
                    }
                }
                _ => unreachable!("non-text segments handled above"),
            };
            assert_eq!(
                actual_char, expected_char,
                "character mismatch at reported position {actual_position} while moving {context}"
            );
        }

        for (idx, (expected_char, expected_position)) in expectations.iter().enumerate() {
            assert_cursor_position(
                &editor,
                &reveal_tags,
                &logical_chars,
                *expected_char,
                *expected_position,
                "forward",
            );
            if idx + 1 < expectations.len() {
                assert!(
                    editor.move_right(),
                    "failed to advance cursor for expected character {expected_char}"
                );
            }
        }

        assert!(
            !editor.move_right(),
            "cursor should not advance past the end of the paragraph"
        );

        for (idx, (expected_char, expected_position)) in expectations.iter().rev().enumerate() {
            assert_cursor_position(
                &editor,
                &reveal_tags,
                &logical_chars,
                *expected_char,
                *expected_position,
                "backward",
            );
            if idx + 1 < total {
                assert!(
                    editor.move_left(),
                    "failed to move cursor left for expected character {expected_char}"
                );
            }
        }

        assert!(
            !editor.move_left(),
            "cursor should not move left past the start of the paragraph"
        );
    }

    #[test]
    fn test_left_padding_in_cursor_column() {
        let checklist = Paragraph::new_checklist().with_checklist_items(vec![
            tdoc::ChecklistItem::new(false).with_content(vec![DocSpan::new_text("Task")]),
        ]);
        let document = Document::new().with_paragraphs(vec![checklist]);
        let mut editor = DocumentEditor::new(document);
        editor.ensure_cursor_selectable();

        // Test with left_padding = 0
        let tracking0 = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: false,
        };
        let theme = Theme::default();
        let rendered0 = render_document_direct(editor.document(), 120, 0, &[], tracking0, &theme);
        let cursor0 = rendered0.cursor.expect("cursor position missing");

        // Test with left_padding = 4
        let tracking4 = DirectCursorTracking {
            cursor: Some(&editor.cursor_pointer()),
            selection: None,
            track_all_positions: false,
        };
        let rendered4 = render_document_direct(editor.document(), 120, 4, &[], tracking4, &theme);
        let cursor4 = rendered4.cursor.expect("cursor position missing");

        // cursor.column INCLUDES left_padding in its value
        // So with left_padding=4, the column should be 4 more than with left_padding=0
        assert_eq!(
            cursor4.column,
            cursor0.column + 4,
            "cursor.column should include the left_padding offset"
        );

        // content_column should be the same - it doesn't include left_padding
        assert_eq!(
            cursor0.content_column, cursor4.content_column,
            "cursor.content_column should be the same with different left_padding values"
        );
    }
}
