use std::collections::HashMap;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

use tdoc::{Document, InlineStyle, Paragraph, ParagraphType, Span as DocSpan};

use crate::editor::{CursorPointer, MarkerRef, RevealTagKind, RevealTagRef, SegmentKind};

#[derive(Clone, Copy)]
pub struct RenderSentinels {
    pub cursor: char,
    pub selection_start: char,
    pub selection_end: char,
}

#[derive(Clone, Copy, Debug)]
pub struct CursorVisualPosition {
    pub line: usize,
    pub column: u16,
    pub content_line: usize,
    pub content_column: u16,
}

#[derive(Debug)]
pub struct RenderResult {
    pub lines: Vec<Line<'static>>,
    pub cursor: Option<CursorVisualPosition>,
    pub total_lines: usize,
    pub cursor_map: Vec<(CursorPointer, CursorVisualPosition)>,
}

struct FragmentContext<'a> {
    sentinels: RenderSentinels,
    marker_map: &'a HashMap<usize, CursorPointer>,
    reveal_tags: &'a HashMap<usize, RevealTagRef>,
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

fn reveal_pointer_hints(pointer: &CursorPointer) -> Option<(usize, usize)> {
    match pointer.segment_kind {
        SegmentKind::RevealStart(style) => {
            let width = visible_width(&reveal_tag_display(style, RevealTagKind::Start));
            match pointer.offset {
                0 => Some((0, 1)),
                1 => Some((width, 1)),
                _ => None,
            }
        }
        SegmentKind::RevealEnd(style) => {
            let width = visible_width(&reveal_tag_display(style, RevealTagKind::End));
            match pointer.offset {
                0 => Some((0, 1)),
                1 => Some((width, 1)),
                _ => None,
            }
        }
        SegmentKind::Text => None,
    }
}

pub fn render_document(
    document: &Document,
    wrap_width: usize,
    left_padding: usize,
    markers: &[MarkerRef],
    reveal_tags: &[RevealTagRef],
    sentinels: RenderSentinels,
) -> RenderResult {
    let mut renderer = Renderer::new(
        wrap_width.max(1),
        left_padding,
        sentinels,
        markers,
        reveal_tags,
    );
    renderer.render_document(document);
    renderer.finish()
}

struct Renderer<'a> {
    wrap_width: usize,
    wrap_limit: usize,
    left_padding: usize,
    padding: Option<String>,
    sentinels: RenderSentinels,
    line_metrics: Vec<LineMetric>,
    marker_pending: HashMap<usize, PendingPosition>,
    cursor_pending: Option<PendingPosition>,
    lines: Vec<Line<'static>>,
    current_line_index: usize,
    markers: &'a [MarkerRef],
    marker_map: HashMap<usize, CursorPointer>,
    reveal_tags: HashMap<usize, RevealTagRef>,
}

impl<'a> Renderer<'a> {
    fn new(
        wrap_width: usize,
        left_padding: usize,
        sentinels: RenderSentinels,
        markers: &'a [MarkerRef],
        reveal_tags: &[RevealTagRef],
    ) -> Self {
        let wrap_limit = if wrap_width > 1 { wrap_width - 1 } else { 1 };
        let padding = if left_padding > 0 {
            Some(" ".repeat(left_padding))
        } else {
            None
        };
        let marker_map = markers
            .iter()
            .map(|marker| (marker.id, marker.pointer.clone()))
            .collect::<HashMap<usize, CursorPointer>>();
        let reveal_map = reveal_tags
            .iter()
            .map(|tag| (tag.id, tag.clone()))
            .collect::<HashMap<usize, RevealTagRef>>();
        Self {
            wrap_width,
            wrap_limit,
            left_padding,
            padding,
            sentinels,
            line_metrics: Vec::new(),
            marker_pending: HashMap::new(),
            cursor_pending: None,
            lines: Vec::new(),
            current_line_index: 0,
            markers,
            marker_map,
            reveal_tags: reveal_map,
        }
    }

    fn render_document(&mut self, document: &Document) {
        for (idx, paragraph) in document.paragraphs.iter().enumerate() {
            if idx > 0 {
                self.push_plain_line("", false);
            }
            self.render_paragraph(paragraph, "");
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
        let ctx = FragmentContext {
            sentinels: self.sentinels,
            marker_map: &self.marker_map,
            reveal_tags: &self.reveal_tags,
        };
        for span in paragraph.content() {
            collect_fragments(span, Style::default(), &ctx, &mut fragments);
        }
        let fragments = trim_layout_fragments(fragments);
        let lines = wrap_fragments(
            &fragments,
            first_prefix,
            continuation_prefix,
            self.wrap_limit,
        );
        self.consume_lines(lines);
    }

    fn render_header(&mut self, paragraph: &Paragraph, prefix: &str, level: HeaderLevel) {
        let mut fragments = Vec::new();
        let ctx = FragmentContext {
            sentinels: self.sentinels,
            marker_map: &self.marker_map,
            reveal_tags: &self.reveal_tags,
        };
        for span in paragraph.content() {
            collect_fragments(span, Style::default(), &ctx, &mut fragments);
        }
        let fragments = trim_layout_fragments(fragments);
        let mut lines = wrap_fragments(&fragments, prefix, prefix, self.wrap_limit);

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

        self.consume_lines(lines);

        if matches!(level, HeaderLevel::Two | HeaderLevel::Three) {
            let width = self.lines.last().map(|line| line_width(line)).unwrap_or(0);
            let underline_char = match level {
                HeaderLevel::Two => '=',
                HeaderLevel::Three => '-',
                HeaderLevel::One => '=',
            };
            let underline_width = width.saturating_sub(self.left_padding);
            let underline = underline_string(underline_width, underline_char);
            self.push_plain_line(&underline, false);
        }
    }

    fn render_code_block(&mut self, paragraph: &Paragraph, prefix: &str) {
        let fence = self.code_block_fence(prefix);
        self.push_plain_line(&fence, false);

        let mut fragments = Vec::new();
        let ctx = FragmentContext {
            sentinels: self.sentinels,
            marker_map: &self.marker_map,
            reveal_tags: &self.reveal_tags,
        };
        for span in paragraph.content() {
            collect_fragments(span, Style::default(), &ctx, &mut fragments);
        }
        let lines = wrap_fragments(&fragments, prefix, prefix, usize::MAX / 4);
        self.consume_lines(lines);

        self.push_plain_line(&fence, false);
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
            self.render_paragraph(child, &quote_prefix);
        }
    }

    fn render_unordered_list(&mut self, paragraph: &Paragraph, prefix: &str) {
        for (idx, entry) in paragraph.entries().iter().enumerate() {
            if idx > 0 {
                self.push_plain_line("", false);
            }
            let marker = "• ";
            let first_prefix = format!("{}{}", prefix, marker);
            let continuation_prefix = format!("{}{}", prefix, " ".repeat(marker.chars().count()));
            self.render_list_entry(entry, &first_prefix, &continuation_prefix);
        }
    }

    fn render_ordered_list(&mut self, paragraph: &Paragraph, prefix: &str) {
        for (idx, entry) in paragraph.entries().iter().enumerate() {
            if idx > 0 {
                self.push_plain_line("", false);
            }
            let number_label = format!("{}. ", idx + 1);
            let first_prefix = format!("{}{}", prefix, number_label);
            let continuation_spaces = " ".repeat(
                first_prefix
                    .chars()
                    .count()
                    .saturating_sub(prefix.chars().count()),
            );
            let continuation_prefix = format!("{}{}", prefix, continuation_spaces);
            self.render_list_entry(entry, &first_prefix, &continuation_prefix);
        }
    }

    fn render_checklist(&mut self, paragraph: &Paragraph, prefix: &str) {
        for (idx, item) in paragraph.checklist_items().iter().enumerate() {
            if idx > 0 {
                self.push_plain_line("", false);
            }
            self.render_checklist_item_struct(item, prefix);
        }
    }

    fn render_checklist_item_struct(&mut self, item: &tdoc::ChecklistItem, prefix: &str) {
        let marker = if item.checked { "[✓] " } else { "[ ] " };
        let first_prefix = format!("{}{}", prefix, marker);
        let continuation_prefix = format!("{}{}", prefix, " ".repeat(marker.chars().count()));

        let mut fragments = Vec::new();
        let ctx = FragmentContext {
            sentinels: self.sentinels,
            marker_map: &self.marker_map,
            reveal_tags: &self.reveal_tags,
        };
        for span in &item.content {
            collect_fragments(span, Style::default(), &ctx, &mut fragments);
        }
        let fragments = trim_layout_fragments(fragments);
        let lines = wrap_fragments(
            &fragments,
            &first_prefix,
            &continuation_prefix,
            self.wrap_limit,
        );
        self.consume_lines(lines);

        // Render nested checklist items
        for child in &item.children {
            let child_prefix = format!("{}    ", prefix);
            self.render_checklist_item_struct(child, &child_prefix);
        }
    }

    // Deprecated: render_checklist_item for old Paragraph-based checklist API
    // This function is no longer used since checklists now use ChecklistItem structs
    #[allow(dead_code)]
    fn render_checklist_item(&mut self, paragraph: &Paragraph, prefix: &str) {
        // This is a deprecated function kept for compatibility
        // Checklists now use render_checklist_item_struct
        self.render_text_paragraph(paragraph, prefix, prefix);
    }

    fn render_list_entry(
        &mut self,
        entry: &[Paragraph],
        first_prefix: &str,
        continuation_prefix: &str,
    ) {
        if entry.is_empty() {
            self.push_plain_line(first_prefix, false);
            return;
        }

        let mut iter = entry.iter();
        if let Some(first) = iter.next() {
            match first.paragraph_type() {
                ParagraphType::Text => {
                    self.render_text_paragraph(first, first_prefix, continuation_prefix);
                }
                _ => {
                    self.push_plain_line(first_prefix, false);
                    self.render_paragraph(first, continuation_prefix);
                }
            }
        }

        for rest in iter {
            if rest.paragraph_type() == ParagraphType::Text {
                self.push_plain_line("", false);
            }
            self.render_paragraph(rest, continuation_prefix);
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

    fn code_block_fence(&self, prefix: &str) -> String {
        const MIN_FENCE_WIDTH: usize = 4;
        let available_width = self.wrap_width.saturating_sub(prefix.chars().count());
        let dash_count = available_width.max(MIN_FENCE_WIDTH);
        format!("{}{}", prefix, "-".repeat(dash_count))
    }

    fn consume_lines(&mut self, outputs: Vec<LineOutput>) {
        let padding = self.left_padding.min(u16::MAX as usize) as u16;
        for output in outputs {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(output.spans.len());
            for segment in output.spans {
                spans.push(Span::styled(segment.text.clone(), segment.style).to_owned());
            }
            let spans = self.prepend_padding(spans);
            let line = Line::from(spans);
            self.line_metrics.push(LineMetric {
                counts_as_content: output.has_word,
            });
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
                    TextEventKind::SelectionStart | TextEventKind::SelectionEnd => {}
                }
            }
            self.lines.push(line);
            self.current_line_index += 1;
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

        let mut cursor_map = Vec::new();
        for marker in self.markers {
            if let Some(pending) = self.marker_pending.remove(&marker.id) {
                let content_line = content_line_numbers
                    .get(pending.line)
                    .copied()
                    .unwrap_or(pending.line);
                let position = CursorVisualPosition {
                    line: pending.line,
                    column: pending.column,
                    content_line,
                    content_column: pending.content_column,
                };
                cursor_map.push((marker.pointer.clone(), position));
            }
        }

        adjust_reveal_content_columns(&mut cursor_map);

        RenderResult {
            lines: self.lines,
            cursor,
            total_lines,
            cursor_map,
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
    has_word: bool,
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

fn collect_fragments(
    span: &DocSpan,
    base_style: Style,
    ctx: &FragmentContext<'_>,
    fragments: &mut Vec<FragmentItem>,
) {
    let style = merge_style(base_style, span.style, span.link_target.as_deref());

    let mut local: Vec<FragmentItem> = Vec::new();
    if !span.text.is_empty() {
        tokenize_text(&span.text, style, ctx, &mut local);
    }

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

    fragments.extend(prefix.into_iter());
    fragments.extend(middle.into_iter());

    for child in &span.children {
        collect_fragments(child, style, ctx, fragments);
    }

    fragments.extend(suffix.into_iter());
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
        FragmentItem::LineBreak => true,
        FragmentItem::Token(fragment) => {
            fragment.kind == FragmentKind::Whitespace
                && fragment.events.is_empty()
                && fragment.text.chars().all(|ch| ch.is_whitespace())
        }
    }
}

fn merge_style(base: Style, inline: InlineStyle, _link_target: Option<&str>) -> Style {
    match inline {
        InlineStyle::None => base,
        InlineStyle::Bold => base.add_modifier(Modifier::BOLD),
        InlineStyle::Italic => base.add_modifier(Modifier::ITALIC),
        InlineStyle::Highlight => base.add_modifier(Modifier::REVERSED),
        InlineStyle::Underline => base.add_modifier(Modifier::UNDERLINED),
        InlineStyle::Strike => base.add_modifier(Modifier::CROSSED_OUT),
        InlineStyle::Link => base.add_modifier(Modifier::UNDERLINED).fg(Color::Blue),
        InlineStyle::Code => base.add_modifier(Modifier::DIM),
    }
}

fn apply_selection_style(style: Style, selected: bool) -> Style {
    if selected {
        style.add_modifier(Modifier::REVERSED)
    } else {
        style
    }
}

fn tokenize_text(
    text: &str,
    style: Style,
    ctx: &FragmentContext<'_>,
    fragments: &mut Vec<FragmentItem>,
) {
    let mut builder: Option<TokenBuilder> = None;
    let mut pending_events: Vec<TextEvent> = Vec::new();
    let mut buffer: Vec<char> = Vec::new();
    let bytes = text.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if let Some((marker, next_idx)) = parse_marker(bytes, idx) {
            match marker {
                ParsedMarker::Pointer(id) => {
                    let mut event = TextEvent {
                        offset: 0,
                        content_offset: 0,
                        offset_hint: None,
                        content_offset_hint: None,
                        kind: TextEventKind::Marker(id),
                    };
                    if let Some(pointer) = ctx.marker_map.get(&id) {
                        if let Some((offset_hint, content_hint)) = reveal_pointer_hints(pointer) {
                            event.offset_hint = Some(offset_hint);
                            event.content_offset_hint = Some(content_hint);
                        }
                    }
                    pending_events.push(event);
                }
                ParsedMarker::Reveal(id) => {
                    if let Some(mut token) = builder.take() {
                        token.add_events(&mut pending_events);
                        fragments.push(FragmentItem::Token(token.finish()));
                    }
                    if let Some(tag) = ctx.reveal_tags.get(&id) {
                        let display = reveal_tag_display(tag.style, tag.kind);
                        let style = Style::default().fg(Color::Yellow).bg(Color::Blue);
                        let width = visible_width(&display);
                        let mut events = Vec::new();
                        for mut event in pending_events.drain(..) {
                            if let Some(pointer) = match event.kind {
                                TextEventKind::Marker(marker_id) => ctx.marker_map.get(&marker_id),
                                _ => None,
                            } {
                                if event.offset_hint.is_none() {
                                    if let Some((offset_hint, content_hint)) =
                                        reveal_pointer_hints(pointer)
                                    {
                                        event.offset_hint = Some(offset_hint);
                                        event.content_offset_hint = Some(content_hint);
                                    }
                                }
                            }
                            events.push(event);
                        }
                        fragments.push(FragmentItem::Token(Fragment {
                            text: display,
                            style,
                            kind: FragmentKind::RevealTag,
                            width,
                            content_width: 0,
                            events,
                            reveal_kind: Some(tag.kind),
                        }));
                    } else {
                        pending_events.clear();
                    }
                }
            }
            idx = next_idx;
            continue;
        }
        if let Some(ch) = text[idx..].chars().next() {
            idx += ch.len_utf8();
            if ch == ctx.sentinels.cursor {
                pending_events.push(TextEvent {
                    offset: 0,
                    content_offset: 0,
                    offset_hint: None,
                    content_offset_hint: None,
                    kind: TextEventKind::Cursor,
                });
                continue;
            }
            if ch == ctx.sentinels.selection_start {
                pending_events.push(TextEvent {
                    offset: 0,
                    content_offset: 0,
                    offset_hint: None,
                    content_offset_hint: None,
                    kind: TextEventKind::SelectionStart,
                });
                continue;
            }
            if ch == ctx.sentinels.selection_end {
                pending_events.push(TextEvent {
                    offset: 0,
                    content_offset: 0,
                    offset_hint: None,
                    content_offset_hint: None,
                    kind: TextEventKind::SelectionEnd,
                });
                continue;
            }
            if ch == '\r' {
                continue;
            }
            if ch == '\n' {
                if let Some(mut token) = builder.take() {
                    token.add_events(&mut pending_events);
                    fragments.push(FragmentItem::Token(token.finish()));
                } else if !pending_events.is_empty() {
                    fragments.push(FragmentItem::Token(Fragment {
                        text: String::new(),
                        style,
                        kind: FragmentKind::Word,
                        width: 0,
                        content_width: 0,
                        events: pending_events.drain(..).collect(),
                        reveal_kind: None,
                    }));
                }
                fragments.push(FragmentItem::LineBreak);
                continue;
            }

            let expanded: &[char] = if ch == '\t' {
                buffer.clear();
                buffer.extend_from_slice(&[' '; 4]);
                &buffer
            } else {
                buffer.clear();
                buffer.push(ch);
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
                        current.add_events(&mut pending_events);
                        current.push_char(*actual);
                    }
                } else {
                    if let Some(existing) = builder.take() {
                        fragments.push(FragmentItem::Token(existing.finish()));
                    }
                    let mut new_builder = TokenBuilder::new(style, is_whitespace);
                    new_builder.add_events(&mut pending_events);
                    new_builder.push_char(*actual);
                    builder = Some(new_builder);
                }
            }
        } else {
            break;
        }
    }

    if let Some(mut token) = builder {
        token.add_events(&mut pending_events);
        fragments.push(FragmentItem::Token(token.finish()));
    } else if !pending_events.is_empty() {
        fragments.push(FragmentItem::Token(Fragment {
            text: String::new(),
            style,
            kind: FragmentKind::Word,
            width: 0,
            content_width: 0,
            events: pending_events,
            reveal_kind: None,
        }));
    }
}

enum ParsedMarker {
    Pointer(usize),
    Reveal(usize),
}

fn parse_marker(bytes: &[u8], idx: usize) -> Option<(ParsedMarker, usize)> {
    if bytes[idx] != 0x1B {
        return None;
    }
    let mut cursor = idx + 1;
    if cursor >= bytes.len() || bytes[cursor] != b']' {
        return None;
    }
    cursor += 1;
    if cursor + 6 >= bytes.len() {
        return None;
    }
    if &bytes[cursor..cursor + 5] != b"1337;" {
        return None;
    }
    cursor += 5;
    if cursor >= bytes.len() {
        return None;
    }
    let kind_byte = bytes[cursor];
    cursor += 1;
    let kind = match kind_byte {
        b'M' => ParsedMarker::Pointer,
        b'R' => ParsedMarker::Reveal,
        _ => return None,
    };
    let start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    if cursor >= bytes.len() || bytes[cursor] != 0x1B {
        return None;
    }
    let id: usize = std::str::from_utf8(&bytes[start..cursor])
        .ok()?
        .parse()
        .ok()?;
    cursor += 1;
    if cursor >= bytes.len() || bytes[cursor] != b'\\' {
        return None;
    }
    Some((kind(id), cursor + 1))
}

struct TokenBuilder {
    text: String,
    style: Style,
    kind: FragmentKind,
    width: usize,
    content_width: usize,
    events: Vec<TextEvent>,
}

impl TokenBuilder {
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

    fn add_events(&mut self, pending: &mut Vec<TextEvent>) {
        for mut event in pending.drain(..) {
            let offset = event.offset_hint.unwrap_or(self.width);
            let content_offset = event.content_offset_hint.unwrap_or(self.content_width);
            event.offset = offset;
            event.content_offset = content_offset;
            self.events.push(event);
        }
    }

    fn push_char(&mut self, ch: char) {
        self.text.push(ch);
        self.width += UnicodeWidthChar::width(ch).unwrap_or(0);
        self.content_width += UnicodeWidthChar::width(ch).unwrap_or(0);
    }

    fn finish(self) -> Fragment {
        Fragment {
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

fn wrap_fragments(
    fragments: &[FragmentItem],
    first_prefix: &str,
    continuation_prefix: &str,
    width: usize,
) -> Vec<LineOutput> {
    let mut outputs = Vec::new();
    let mut builder = LineBuilder::new(first_prefix.to_string(), width, false);
    let mut pending_whitespace: Vec<Fragment> = Vec::new();

    for fragment in fragments {
        match fragment {
            FragmentItem::LineBreak => {
                builder.consume_pending(&mut pending_whitespace);
                let (line, active_selection) = builder.build_line();
                outputs.push(line);
                builder =
                    LineBuilder::new(continuation_prefix.to_string(), width, active_selection);
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

    if head_width == 0 {
        if let Some(ch) = fragment.text.chars().next() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            head_text.push(ch);
            head_width += ch_width;
            split_byte_index = ch.len_utf8();
        }
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

struct LineBuilder {
    segments: Vec<LineSegment>,
    events: Vec<LocatedEvent>,
    width: usize,
    prefix_width: usize,
    content_width: usize,
    has_word: bool,
    selection_active: bool,
}

impl LineBuilder {
    fn new(prefix: String, _width_limit: usize, selection_active: bool) -> Self {
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
            has_word: false,
            selection_active,
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
            width,
            content_width: _content_width,
            mut events,
            ..
        } = fragment;

        if kind == FragmentKind::Word && width > 0 {
            self.has_word = true;
        }

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

        while let Some(event) = events_iter.next() {
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
            let segment_style = apply_selection_style(style, buffer_selected);
            self.push_segment(text_segment, segment_style, counts_content);
        }
    }

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
                    let style = apply_selection_style(fragment_style, *buffer_selected);
                    self.push_segment(text, style, counts_content);
                }
                self.selection_active = true;
                *buffer_selected = self.selection_active;
            }
            TextEventKind::SelectionEnd => {
                if !buffer.is_empty() {
                    let text = std::mem::take(buffer);
                    let style = apply_selection_style(fragment_style, *buffer_selected);
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
                has_word: self.has_word,
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
    std::iter::repeat(ch).take(width.max(1)).collect()
}

#[derive(Clone, Copy)]
struct PendingPosition {
    line: usize,
    column: u16,
    content_column: u16,
}

struct LineMetric {
    counts_as_content: bool,
}

fn adjust_reveal_content_columns(cursor_map: &mut [(CursorPointer, CursorVisualPosition)]) {
    for idx in 0..cursor_map.len() {
        let is_reveal = matches!(
            cursor_map[idx].0.segment_kind,
            SegmentKind::RevealStart(_) | SegmentKind::RevealEnd(_)
        );
        if !is_reveal {
            continue;
        }
        let content_line = cursor_map[idx].1.content_line;
        if let Some(adjacent_column) =
            find_adjacent_text_content_column(cursor_map, idx, content_line)
        {
            cursor_map[idx].1.content_column = adjacent_column;
        }
    }
}

fn find_adjacent_text_content_column(
    cursor_map: &[(CursorPointer, CursorVisualPosition)],
    idx: usize,
    target_content_line: usize,
) -> Option<u16> {
    find_in_direction(
        cursor_map.iter().enumerate().skip(idx + 1),
        target_content_line,
    )
    .or_else(|| {
        find_in_direction(
            cursor_map[..idx]
                .iter()
                .enumerate()
                .rev()
                .map(|(i, entry)| (i, entry)),
            target_content_line,
        )
    })
}

fn find_in_direction<'a, I>(iter: I, target_content_line: usize) -> Option<u16>
where
    I: Iterator<Item = (usize, &'a (CursorPointer, CursorVisualPosition))>,
{
    for (_, (pointer, position)) in iter {
        if position.content_line != target_content_line {
            continue;
        }
        if matches!(pointer.segment_kind, SegmentKind::Text) {
            return Some(position.content_column);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{CursorPointer, DocumentEditor};
    use std::io::Cursor;
    use tdoc::{ftml, parse};

    const SENTINELS: RenderSentinels = RenderSentinels {
        cursor: '\u{F8FF}',
        selection_start: '\u{F8FE}',
        selection_end: '\u{F8FD}',
    };
    const CURSOR_SENTINEL: char = SENTINELS.cursor;

    fn render_input(input: &str) -> RenderResult {
        let document = parse(Cursor::new(input)).expect("failed to parse document");
        render_document(&document, 120, 0, &[], &[], SENTINELS)
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

        let (doc_with_markers, markers, reveal_tags, _) = editor.clone_with_markers(
            SENTINELS.cursor,
            None,
            SENTINELS.selection_start,
            SENTINELS.selection_end,
        );
        let rendered =
            render_document(&doc_with_markers, 120, 0, &markers, &reveal_tags, SENTINELS);
        let lines = lines_to_strings(&rendered.lines);
        assert_eq!(lines, vec!["• Alpha ", "", "  Beta"]);
    }

    #[test]
    fn cursor_metrics_ignore_layout_indentation() {
        let input = format!(
            r#"
<ul>
  <li>
    <p>Describe the features supported by FTML.</p>
  </li>
  <li>
    <p>{cursor}Showcase the FTML standard formatting enforced by fmtftml.</p>
  </li>
</ul>
"#,
            cursor = CURSOR_SENTINEL,
        );
        let rendered = render_input(&input);
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
        let input = format!(r#"<p>{cursor}Hello</p>"#, cursor = CURSOR_SENTINEL);
        let rendered = render_input(&input);
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
        let (doc_with_markers, markers, reveal_tags, _) = editor.clone_with_markers(
            SENTINELS.cursor,
            None,
            SENTINELS.selection_start,
            SENTINELS.selection_end,
        );
        let rendered = render_document(&doc_with_markers, 12, 0, &markers, &reveal_tags, SENTINELS);

        let mut columns_per_line: Vec<Vec<(u16, u16)>> = Vec::new();
        for (_, position) in rendered.cursor_map {
            let line = position.line;
            if columns_per_line.len() <= line {
                columns_per_line.resize(line + 1, Vec::new());
            }
            columns_per_line[line].push((position.content_column, position.column));
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
        let (doc_with_markers, markers, reveal_tags, inserted_cursor) = editor.clone_with_markers(
            SENTINELS.cursor,
            None,
            SENTINELS.selection_start,
            SENTINELS.selection_end,
        );
        assert!(
            inserted_cursor,
            "cursor sentinel should be inserted at wrap boundary"
        );

        let rendered = render_document(&doc_with_markers, 10, 0, &markers, &reveal_tags, SENTINELS);
        let cursor = rendered.cursor.expect("cursor position missing");
        let lines = lines_to_strings(&rendered.lines);

        assert_eq!(lines, vec!["abcdefghi", "j z"]);
        assert_eq!(cursor.line, 1);
        assert_eq!(cursor.column, 1);
        assert_eq!(cursor.content_line, 1);
        assert_eq!(cursor.content_column, 1);

        let boundary_position = rendered
            .cursor_map
            .iter()
            .find(|(pointer, _)| pointer.offset == 10)
            .map(|(_, position)| position)
            .expect("missing cursor map entry for wrap boundary");
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

        let (doc_with_markers, markers, reveal_tags, _) = editor.clone_with_markers(
            SENTINELS.cursor,
            None,
            SENTINELS.selection_start,
            SENTINELS.selection_end,
        );
        let rendered =
            render_document(&doc_with_markers, 120, 0, &markers, &reveal_tags, SENTINELS);
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

        fn assert_pointer_state(
            pointer: &CursorPointer,
            rendered: &RenderResult,
            logical_chars: &[char],
            expected_char: char,
            expected_position: usize,
            context: &str,
        ) {
            let (_, position) = rendered
                .cursor_map
                .iter()
                .find(|(candidate, _)| candidate == pointer)
                .unwrap_or_else(|| {
                    panic!("cursor map should contain pointer for traversal {context}")
                });
            let actual_position = usize::from(position.content_column) + 1;
            assert_eq!(
                actual_position, expected_position,
                "content column mismatch for character {expected_char} while moving {context}"
            );

            let actual_char = match pointer.segment_kind {
                SegmentKind::RevealStart(_) => '>',
                SegmentKind::RevealEnd(_) => '<',
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
            };
            assert_eq!(
                actual_char, expected_char,
                "character mismatch at reported position {actual_position} while moving {context}"
            );
        }

        for (idx, (expected_char, expected_position)) in expectations.iter().enumerate() {
            let pointer = editor.cursor_pointer();
            assert_pointer_state(
                &pointer,
                &rendered,
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
            let pointer = editor.cursor_pointer();
            assert_pointer_state(
                &pointer,
                &rendered,
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
}
