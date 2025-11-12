use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

pub const MARKER_PREFIX: &str = "1337;M";

#[derive(Clone, Copy, Debug)]
pub struct CursorVisualPosition {
    pub line: usize,
    pub column: u16,
}

#[derive(Debug)]
pub struct ParseResult {
    pub lines: Vec<Line<'static>>,
    pub cursor: Option<CursorVisualPosition>,
    pub total_lines: usize,
    pub markers: Vec<(usize, CursorVisualPosition)>,
}

pub fn parse_ansi(input: &str, sentinel: char) -> ParseResult {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut segments: Vec<(String, Style)> = Vec::new();
    let mut current_text = String::new();
    let mut current_modifiers = Modifier::empty();
    let mut current_style = Style::default();

    let mut cursor: Option<CursorVisualPosition> = None;
    let mut line_idx: usize = 0;
    let mut column: u16 = 0;
    let mut markers = Vec::new();

    let bytes = input.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        match bytes[idx] {
            b'\x1b' => {
                idx += 1;
                if idx >= bytes.len() {
                    break;
                }
                match bytes[idx] {
                    b'[' => {
                        idx += 1;
                        let start = idx;
                        while idx < bytes.len() {
                            let b = bytes[idx];
                            idx += 1;
                            if (0x40..=0x7E).contains(&b) {
                                let command = b;
                                let params = &input[start..idx - 1];
                                flush_segment(&mut current_text, &mut segments, current_style);
                                if command == b'm' {
                                    apply_sgr(params, &mut current_modifiers, &mut current_style);
                                }
                                break;
                            }
                        }
                    }
                    b']' => {
                        idx += 1;
                        let start = idx;
                        while idx + 1 < bytes.len() {
                            if bytes[idx] == b'\x1b' && bytes[idx + 1] == b'\\' {
                                let content = &input[start..idx];
                                handle_osc(content, line_idx, column, &mut markers);
                                idx += 2;
                                break;
                            }
                            idx += 1;
                        }
                    }
                    _ => {}
                }
            }
            b'\r' => {
                idx += 1;
                column = 0;
            }
            b'\n' => {
                idx += 1;
                flush_segment(&mut current_text, &mut segments, current_style);
                push_line(&mut lines, &mut segments);
                line_idx += 1;
                column = 0;
            }
            _ => {
                let remaining = &input[idx..];
                if let Some(ch) = remaining.chars().next() {
                    let len = ch.len_utf8();
                    idx += len;
                    if ch == sentinel {
                        if cursor.is_none() {
                            cursor = Some(CursorVisualPosition { line: line_idx, column });
                        }
                        continue;
                    }
                    current_text.push(ch);
                    let width = UnicodeWidthChar::width(ch).unwrap_or(0);
                    column = column.saturating_add(width as u16);
                } else {
                    break;
                }
            }
        }
    }

    flush_segment(&mut current_text, &mut segments, current_style);
    if !segments.is_empty() || lines.is_empty() {
        push_line(&mut lines, &mut segments);
    }

    let total_lines = lines.len().max(1);

    ParseResult {
        lines,
        cursor,
        total_lines,
        markers,
    }
}

fn flush_segment(text: &mut String, segments: &mut Vec<(String, Style)>, style: Style) {
    if text.is_empty() {
        return;
    }
    let segment_text = std::mem::take(text);
    segments.push((segment_text, style));
}

fn push_line(lines: &mut Vec<Line<'static>>, segments: &mut Vec<(String, Style)>) {
    if segments.is_empty() {
        lines.push(Line::from(""));
        return;
    }
    let spans: Vec<Span<'static>> = segments
        .drain(..)
        .map(|(text, style)| Span::styled(text, style))
        .collect();
    lines.push(Line::from(spans));
}

fn apply_sgr(params: &str, modifiers: &mut Modifier, style: &mut Style) {
    if params.is_empty() {
        *modifiers = Modifier::empty();
        *style = Style::default();
        return;
    }

    for param in params.split(';') {
        let code = if param.is_empty() {
            0
        } else {
            param.parse::<u16>().unwrap_or(0)
        };
        match code {
            0 => {
                *modifiers = Modifier::empty();
                *style = Style::default();
            }
            1 => {
                *modifiers |= Modifier::BOLD;
                *style = Style::default().add_modifier(*modifiers);
            }
            3 => {
                *modifiers |= Modifier::ITALIC;
                *style = Style::default().add_modifier(*modifiers);
            }
            4 => {
                *modifiers |= Modifier::UNDERLINED;
                *style = Style::default().add_modifier(*modifiers);
            }
            7 => {
                *modifiers |= Modifier::REVERSED;
                *style = Style::default().add_modifier(*modifiers);
            }
            9 => {
                *modifiers |= Modifier::CROSSED_OUT;
                *style = Style::default().add_modifier(*modifiers);
            }
            22 => {
                *modifiers &= !Modifier::BOLD;
                *style = Style::default().add_modifier(*modifiers);
            }
            23 => {
                *modifiers &= !Modifier::ITALIC;
                *style = Style::default().add_modifier(*modifiers);
            }
            24 => {
                *modifiers &= !Modifier::UNDERLINED;
                *style = Style::default().add_modifier(*modifiers);
            }
            27 => {
                *modifiers &= !Modifier::REVERSED;
                *style = Style::default().add_modifier(*modifiers);
            }
            29 => {
                *modifiers &= !Modifier::CROSSED_OUT;
                *style = Style::default().add_modifier(*modifiers);
            }
            _ => {}
        }
    }
}

fn handle_osc(
    content: &str,
    line_idx: usize,
    column: u16,
    markers: &mut Vec<(usize, CursorVisualPosition)>,
) {
    if let Some(rest) = content.strip_prefix(MARKER_PREFIX) {
        if let Ok(id) = rest.trim().parse::<usize>() {
            markers.push((id, CursorVisualPosition { line: line_idx, column }));
        }
    }
}
