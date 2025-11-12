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
    sentinel: char,
    marker_positions: HashMap<usize, CursorVisualPosition>,
    cursor: Option<CursorVisualPosition>,
    lines: Vec<Line<'static>>,
    current_line_index: usize,
    markers: &'a [MarkerRef],
}

impl<'a> Renderer<'a> {
    fn new(wrap_width: usize, sentinel: char, markers: &'a [MarkerRef]) -> Self {
        Self {
            wrap_width,
            sentinel,
            marker_positions: HashMap::new(),
            cursor: None,
            lines: Vec::new(),
            current_line_index: 0,
            markers,
        }
    }

    fn render_document(&mut self, document: &Document) {
        for (idx, paragraph) in document.paragraphs.iter().enumerate() {
            if idx > 0 {
                self.lines.push(Line::from(""));
                self.current_line_index += 1;
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
        let lines = wrap_fragments(
            &fragments,
            first_prefix,
            continuation_prefix,
            self.wrap_width,
        );
        self.consume_lines(lines);
    }

    fn render_header(&mut self, paragraph: &Paragraph, prefix: &str, level: HeaderLevel) {
        let mut fragments = Vec::new();
        for span in &paragraph.content {
            collect_fragments(span, Style::default(), self.sentinel, &mut fragments);
        }
        let mut lines = wrap_fragments(&fragments, prefix, prefix, self.wrap_width);

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
            let span = Span::raw(underline).to_owned();
            let line = Line::from(vec![span]);
            self.lines.push(line);
            self.current_line_index += 1;
        }
    }

    fn render_code_block(&mut self, paragraph: &Paragraph, prefix: &str) {
        let fence = self.code_block_fence(prefix);
        self.push_plain_line(&fence);

        let mut fragments = Vec::new();
        for span in &paragraph.content {
            collect_fragments(span, Style::default(), self.sentinel, &mut fragments);
        }
        let lines = wrap_fragments(&fragments, prefix, prefix, usize::MAX / 4);
        self.consume_lines(lines);

        self.push_plain_line(&fence);
    }

    fn render_quote(&mut self, paragraph: &Paragraph, prefix: &str) {
        let quote_prefix = format!("{}| ", prefix);
        if !paragraph.content.is_empty() {
            self.render_text_paragraph(paragraph, &quote_prefix, &quote_prefix);
        }
        for (idx, child) in paragraph.children.iter().enumerate() {
            if idx > 0 || !paragraph.content.is_empty() {
                self.push_blank_line();
            }
            self.render_paragraph(child, &quote_prefix);
        }
    }

    fn render_unordered_list(&mut self, paragraph: &Paragraph, prefix: &str) {
        for (idx, entry) in paragraph.entries.iter().enumerate() {
            if idx > 0 {
                self.push_blank_line();
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
                self.push_blank_line();
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
                self.push_blank_line();
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
            self.push_plain_line(first_prefix);
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
                    self.push_plain_line(first_prefix);
                    self.render_paragraph(first, continuation_prefix);
                }
            }
        }

        for rest in iter {
            self.render_paragraph(rest, continuation_prefix);
        }
    }

    fn push_blank_line(&mut self) {
        self.lines.push(Line::from(""));
        self.current_line_index += 1;
    }

    fn push_plain_line(&mut self, content: &str) {
        let span = Span::raw(content.to_string()).to_owned();
        let line = Line::from(vec![span]);
        self.lines.push(line);
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
            for event in output.events {
                let position = CursorVisualPosition {
                    line: self.current_line_index,
                    column: event.column,
                };
                match event.kind {
                    TextEventKind::Cursor => {
                        self.cursor = Some(position);
                    }
                    TextEventKind::Marker(id) => {
                        self.marker_positions.insert(id, position);
                    }
                }
            }
            self.lines.push(line);
            self.current_line_index += 1;
        }
    }

    fn finish(mut self) -> RenderResult {
        if self.lines.is_empty() {
            self.lines.push(Line::from(""));
        }
        let total_lines = self.lines.len();

        let mut cursor_map = Vec::new();
        for marker in self.markers {
            if let Some(position) = self.marker_positions.get(&marker.id) {
                cursor_map.push((marker.pointer.clone(), *position));
            }
        }

        RenderResult {
            lines: self.lines,
            cursor: self.cursor,
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
}

#[derive(Clone, Copy)]
struct LocatedEvent {
    column: u16,
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

#[derive(Clone, Copy)]
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
                    if let Some(mut existing) = builder.take() {
                        existing.add_events(&mut pending_events);
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
                    let whitespace_width: usize =
                        pending_whitespace.iter().map(|item| item.width).sum();
                    if builder.current_width() > builder.prefix_width
                        && builder.current_width() + whitespace_width + token.width > width
                    {
                        builder.consume_pending(&mut pending_whitespace);
                        outputs.push(builder.build_line());
                        builder = LineBuilder::new(continuation_prefix.to_string(), width);
                    }

                    builder.append_with_pending(token.clone(), &mut pending_whitespace);
                }
            },
        }
    }

    builder.consume_pending(&mut pending_whitespace);
    outputs.push(builder.build_line());
    outputs
}

struct LineBuilder {
    segments: Vec<LineSegment>,
    events: Vec<LocatedEvent>,
    width: usize,
    prefix_width: usize,
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
        if !fragment.text.is_empty() {
            self.segments.push(LineSegment {
                text: fragment.text.clone(),
                style: fragment.style,
            });
            self.width += fragment.width;
        }

        for event in fragment.events {
            let column = self.width.saturating_sub(fragment.width) + event.offset;
            self.events.push(LocatedEvent {
                column: column as u16,
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
