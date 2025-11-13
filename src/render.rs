use std::collections::HashMap;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

use tdoc::{Document, InlineStyle, Paragraph, ParagraphType, Span as DocSpan};

use crate::editor::{CursorPointer, MarkerRef};

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

pub fn render_document(
    document: &Document,
    width: usize,
    markers: &[MarkerRef],
    sentinel: char,
) -> RenderResult {
    let mut renderer = Renderer::new(width.max(1), sentinel, markers);
    renderer.render_document(document);
    renderer.finish()
}

struct Renderer<'a> {
    wrap_width: usize,
    wrap_limit: usize,
    sentinel: char,
    line_metrics: Vec<LineMetric>,
    marker_pending: HashMap<usize, PendingPosition>,
    cursor_pending: Option<PendingPosition>,
    lines: Vec<Line<'static>>,
    current_line_index: usize,
    markers: &'a [MarkerRef],
}

impl<'a> Renderer<'a> {
    fn new(wrap_width: usize, sentinel: char, markers: &'a [MarkerRef]) -> Self {
        let wrap_limit = if wrap_width > 1 { wrap_width - 1 } else { 1 };
        Self {
            wrap_width,
            wrap_limit,
            sentinel,
            line_metrics: Vec::new(),
            marker_pending: HashMap::new(),
            cursor_pending: None,
            lines: Vec::new(),
            current_line_index: 0,
            markers,
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
        match paragraph.paragraph_type {
            ParagraphType::Text => self.render_text_paragraph(paragraph, prefix, prefix),
            ParagraphType::Header1 => self.render_header(paragraph, prefix, HeaderLevel::One),
            ParagraphType::Header2 => self.render_header(paragraph, prefix, HeaderLevel::Two),
            ParagraphType::Header3 => self.render_header(paragraph, prefix, HeaderLevel::Three),
            ParagraphType::CodeBlock => self.render_code_block(paragraph, prefix),
            ParagraphType::Quote => self.render_quote(paragraph, prefix),
            ParagraphType::UnorderedList => self.render_unordered_list(paragraph, prefix),
            ParagraphType::OrderedList => self.render_ordered_list(paragraph, prefix),
            ParagraphType::Checklist => self.render_checklist(paragraph, prefix),
            ParagraphType::ChecklistItem => self.render_checklist_item(paragraph, prefix),
        }
    }

    fn render_text_paragraph(
        &mut self,
        paragraph: &Paragraph,
        first_prefix: &str,
        continuation_prefix: &str,
    ) {
        let mut fragments = Vec::new();
        for span in &paragraph.content {
            collect_fragments(span, Style::default(), self.sentinel, &mut fragments);
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
        for span in &paragraph.content {
            collect_fragments(span, Style::default(), self.sentinel, &mut fragments);
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
            let underline = underline_string(width, underline_char);
            self.push_plain_line(&underline, false);
        }
    }

    fn render_code_block(&mut self, paragraph: &Paragraph, prefix: &str) {
        let fence = self.code_block_fence(prefix);
        self.push_plain_line(&fence, false);

        let mut fragments = Vec::new();
        for span in &paragraph.content {
            collect_fragments(span, Style::default(), self.sentinel, &mut fragments);
        }
        let lines = wrap_fragments(&fragments, prefix, prefix, usize::MAX / 4);
        self.consume_lines(lines);

        self.push_plain_line(&fence, false);
    }

    fn render_quote(&mut self, paragraph: &Paragraph, prefix: &str) {
        let quote_prefix = format!("{}| ", prefix);
        if !paragraph.content.is_empty() {
            self.render_text_paragraph(paragraph, &quote_prefix, &quote_prefix);
        }
        for (idx, child) in paragraph.children.iter().enumerate() {
            if idx > 0 || !paragraph.content.is_empty() {
                self.push_plain_line(&quote_prefix, false);
            }
            self.render_paragraph(child, &quote_prefix);
        }
    }

    fn render_unordered_list(&mut self, paragraph: &Paragraph, prefix: &str) {
        for (idx, entry) in paragraph.entries.iter().enumerate() {
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
        for (idx, entry) in paragraph.entries.iter().enumerate() {
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
        for (idx, entry) in paragraph.entries.iter().enumerate() {
            if idx > 0 {
                self.push_plain_line("", false);
            }
            if let Some(item) = entry
                .iter()
                .find(|p| p.paragraph_type == ParagraphType::ChecklistItem)
            {
                self.render_checklist_item(item, prefix);
                for rest in entry
                    .iter()
                    .filter(|p| p.paragraph_type != ParagraphType::ChecklistItem)
                {
                    self.render_paragraph(rest, prefix);
                }
            } else if let Some(first) = entry.first() {
                self.render_text_paragraph(first, prefix, prefix);
                for rest in entry.iter().skip(1) {
                    self.render_paragraph(rest, prefix);
                }
            }
        }
    }

    fn render_checklist_item(&mut self, paragraph: &Paragraph, prefix: &str) {
        let marker = if paragraph.checklist_item_checked.unwrap_or(false) {
            "[✓] "
        } else {
            "[ ] "
        };
        let first_prefix = format!("{}{}", prefix, marker);
        let continuation_prefix = format!("{}{}", prefix, " ".repeat(marker.chars().count()));
        self.render_text_paragraph(paragraph, &first_prefix, &continuation_prefix);
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
            match first.paragraph_type {
                ParagraphType::Text => {
                    self.render_text_paragraph(first, first_prefix, continuation_prefix);
                }
                ParagraphType::ChecklistItem => {
                    self.render_checklist_item(first, first_prefix);
                }
                _ => {
                    self.push_plain_line(first_prefix, false);
                    self.render_paragraph(first, continuation_prefix);
                }
            }
        }

        for rest in iter {
            if rest.paragraph_type == ParagraphType::Text {
                self.push_plain_line("", false);
            }
            self.render_paragraph(rest, continuation_prefix);
        }
    }

    fn push_plain_line(&mut self, content: &str, counts_as_content: bool) {
        let span = Span::raw(content.to_string()).to_owned();
        let line = Line::from(vec![span]);
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
        for output in outputs {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(output.spans.len());
            for segment in output.spans {
                spans.push(Span::styled(segment.text.clone(), segment.style).to_owned());
            }
            let line = Line::from(spans);
            self.line_metrics.push(LineMetric {
                counts_as_content: output.has_word,
            });
            for event in output.events {
                let pending = PendingPosition {
                    line: self.current_line_index,
                    column: event.column,
                    content_column: event.content_column,
                };
                match event.kind {
                    TextEventKind::Cursor => {
                        self.cursor_pending = Some(pending);
                    }
                    TextEventKind::Marker(id) => {
                        self.marker_pending.insert(id, pending);
                    }
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
    events: Vec<TextEvent>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FragmentKind {
    Word,
    Whitespace,
}

#[derive(Clone)]
enum FragmentItem {
    Token(Fragment),
    LineBreak,
}

#[derive(Clone)]
struct TextEvent {
    offset: usize,
    kind: TextEventKind,
}

#[derive(Clone, Copy)]
enum TextEventKind {
    Marker(usize),
    Cursor,
}

fn collect_fragments(
    span: &DocSpan,
    base_style: Style,
    sentinel: char,
    fragments: &mut Vec<FragmentItem>,
) {
    let style = merge_style(base_style, span.style, span.link_target.as_deref());
    if !span.text.is_empty() {
        tokenize_text(&span.text, style, sentinel, fragments);
    }
    for child in &span.children {
        collect_fragments(child, style, sentinel, fragments);
    }
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

fn tokenize_text(text: &str, style: Style, sentinel: char, fragments: &mut Vec<FragmentItem>) {
    let mut builder: Option<TokenBuilder> = None;
    let mut pending_events: Vec<TextEvent> = Vec::new();
    let mut buffer: Vec<char> = Vec::new();
    let bytes = text.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if let Some((id, next_idx)) = parse_marker(bytes, idx) {
            pending_events.push(TextEvent {
                offset: 0,
                kind: TextEventKind::Marker(id),
            });
            idx = next_idx;
            continue;
        }
        if let Some(ch) = text[idx..].chars().next() {
            idx += ch.len_utf8();
            if ch == sentinel {
                pending_events.push(TextEvent {
                    offset: 0,
                    kind: TextEventKind::Cursor,
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
                        events: pending_events.drain(..).collect(),
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
            events: pending_events,
        }));
    }
}

fn parse_marker(bytes: &[u8], idx: usize) -> Option<(usize, usize)> {
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
    let prefix = &bytes[cursor..cursor + 6];
    if prefix != b"1337;M" {
        return None;
    }
    cursor += 6;
    let start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    if cursor >= bytes.len() || bytes[cursor] != 0x1B {
        return None;
    }
    let id = std::str::from_utf8(&bytes[start..cursor])
        .ok()?
        .parse()
        .ok()?;
    cursor += 1;
    if cursor >= bytes.len() || bytes[cursor] != b'\\' {
        return None;
    }
    Some((id, cursor + 1))
}

struct TokenBuilder {
    text: String,
    style: Style,
    kind: FragmentKind,
    width: usize,
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
            event.offset = self.width;
            self.events.push(event);
        }
    }

    fn push_char(&mut self, ch: char) {
        self.text.push(ch);
        self.width += UnicodeWidthChar::width(ch).unwrap_or(0);
    }

    fn finish(self) -> Fragment {
        Fragment {
            text: self.text,
            style: self.style,
            kind: self.kind,
            width: self.width,
            events: self.events,
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
    let mut builder = LineBuilder::new(first_prefix.to_string(), width);
    let mut pending_whitespace: Vec<Fragment> = Vec::new();

    for fragment in fragments {
        match fragment {
            FragmentItem::LineBreak => {
                builder.consume_pending(&mut pending_whitespace);
                outputs.push(builder.build_line());
                builder = LineBuilder::new(continuation_prefix.to_string(), width);
            }
            FragmentItem::Token(token) => match token.kind {
                FragmentKind::Whitespace => {
                    pending_whitespace.push(token.clone());
                }
                FragmentKind::Word => {
                    let mut token = token.clone();
                    loop {
                        let whitespace_width: usize =
                            pending_whitespace.iter().map(|item| item.width).sum();
                        if builder.current_width() > builder.prefix_width
                            && builder.current_width() + whitespace_width + token.width > width
                        {
                            builder.consume_pending(&mut pending_whitespace);
                            outputs.push(builder.build_line());
                            builder = LineBuilder::new(continuation_prefix.to_string(), width);
                            continue;
                        }

                        let line_start = builder.current_width() == builder.prefix_width;
                        let available = width.saturating_sub(builder.prefix_width);
                        if line_start && token.width > available {
                            let split_limit = available.max(1);
                            let (head, tail_opt) = split_fragment(token, split_limit);
                            builder.append_with_pending(head, &mut pending_whitespace);
                            outputs.push(builder.build_line());
                            builder = LineBuilder::new(continuation_prefix.to_string(), width);
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
    outputs.push(builder.build_line());
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

    let mut head_events = Vec::new();
    let mut tail_events = Vec::new();
    for mut event in fragment.events {
        if event.offset < head_width {
            head_events.push(event);
        } else {
            event.offset = event.offset.saturating_sub(head_width);
            tail_events.push(event);
        }
    }

    let head_fragment = Fragment {
        text: head_text,
        style: fragment.style,
        kind: fragment.kind,
        width: head_width,
        events: head_events,
    };
    let tail_fragment = if tail_text.is_empty() && tail_events.is_empty() {
        None
    } else {
        Some(Fragment {
            text: tail_text,
            style: fragment.style,
            kind: fragment.kind,
            width: tail_width,
            events: tail_events,
        })
    };

    (head_fragment, tail_fragment)
}

struct LineBuilder {
    segments: Vec<LineSegment>,
    events: Vec<LocatedEvent>,
    width: usize,
    prefix_width: usize,
    has_word: bool,
}

impl LineBuilder {
    fn new(prefix: String, _width_limit: usize) -> Self {
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
            has_word: false,
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
        if fragment.kind == FragmentKind::Word && fragment.width > 0 {
            self.has_word = true;
        }
        if !fragment.text.is_empty() {
            self.segments.push(LineSegment {
                text: fragment.text.clone(),
                style: fragment.style,
            });
            self.width += fragment.width;
        }

        for event in fragment.events {
            let column = self.width.saturating_sub(fragment.width) + event.offset;
            let display_column = column.min(u16::MAX as usize) as u16;
            let content_column = column
                .saturating_sub(self.prefix_width)
                .min(u16::MAX as usize) as u16;
            self.events.push(LocatedEvent {
                column: display_column,
                content_column,
                kind: event.kind,
            });
        }
    }

    fn build_line(mut self) -> LineOutput {
        if self.segments.is_empty() {
            self.segments.push(LineSegment {
                text: String::new(),
                style: Style::default(),
            });
        }
        self.events.sort_by_key(|event| event.column);
        LineOutput {
            spans: self.segments,
            events: self.events,
            has_word: self.has_word,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::DocumentEditor;
    use std::io::Cursor;
    use tdoc::parse;

    const SENTINEL: char = '\u{F8FF}';

    fn render_input(input: &str) -> RenderResult {
        let document = parse(Cursor::new(input)).expect("failed to parse document");
        render_document(&document, 120, &[], SENTINEL)
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

        let (doc_with_markers, markers, _) = editor.clone_with_markers(SENTINEL);
        let rendered = render_document(&doc_with_markers, 120, &markers, SENTINEL);
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
    <p>{SENTINEL}Showcase the FTML standard formatting enforced by fmtftml.</p>
  </li>
</ul>
"#
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
        let input = format!(r#"<p>{SENTINEL}Hello</p>"#);
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
        let (doc_with_markers, markers, _) = editor.clone_with_markers(SENTINEL);
        let rendered = render_document(&doc_with_markers, 12, &markers, SENTINEL);

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
        let (doc_with_markers, markers, inserted_cursor) = editor.clone_with_markers(SENTINEL);
        assert!(
            inserted_cursor,
            "cursor sentinel should be inserted at wrap boundary"
        );

        let rendered = render_document(&doc_with_markers, 10, &markers, SENTINEL);
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
}
