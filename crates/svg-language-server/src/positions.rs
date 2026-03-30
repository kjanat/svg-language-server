use super::{Location, Position, Range, Uri};

#[inline]
pub fn u32_from_usize(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Convert a byte-offset column to UTF-16 code unit count within a given row.
///
/// LSP positions use UTF-16 code units by default. Tree-sitter reports byte offsets,
/// so we must re-encode the line prefix to count UTF-16 units.
pub fn byte_col_to_utf16(source: &[u8], row: usize, byte_col: usize) -> u32 {
    let line_start: usize = source
        .split(|&b| b == b'\n')
        .take(row)
        .map(|line| line.len() + 1)
        .sum();

    let end = (line_start + byte_col).min(source.len());
    let line_bytes = &source[line_start..end];
    u32_from_usize(String::from_utf8_lossy(line_bytes).encode_utf16().count())
}

/// Convert a UTF-16 column offset to a byte offset within a given row.
///
/// Inverse of `byte_col_to_utf16`: LSP sends UTF-16 positions, but tree-sitter
/// uses byte offsets.
pub fn utf16_to_byte_col(source: &[u8], row: usize, utf16_col: u32) -> usize {
    let line_start: usize = source
        .split(|&b| b == b'\n')
        .take(row)
        .map(|line| line.len() + 1)
        .sum();
    let line_end = source[line_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(source.len(), |p| line_start + p);
    let line_str = String::from_utf8_lossy(&source[line_start..line_end]);
    let mut utf16_count = 0u32;
    let mut byte_offset = 0usize;
    for ch in line_str.chars() {
        if utf16_count >= utf16_col {
            break;
        }
        utf16_count += u32_from_usize(ch.len_utf16());
        byte_offset += ch.len_utf8();
    }
    byte_offset
}

pub fn byte_offset_for_position(source: &[u8], position: Position) -> usize {
    let byte_col = utf16_to_byte_col(source, position.line as usize, position.character);
    byte_offset_for_row_col(source, position.line as usize, byte_col)
}

pub fn byte_offset_for_row_col(source: &[u8], row: usize, byte_col: usize) -> usize {
    let line_start: usize = source
        .split(|&b| b == b'\n')
        .take(row)
        .map(|line| line.len() + 1)
        .sum();
    line_start + byte_col
}

pub fn end_position_utf16(source: &str) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;
    for ch in source.chars() {
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += u32_from_usize(ch.len_utf16());
        }
    }
    Position::new(line, character)
}

pub fn span_range_utf16(source: &[u8], span: &svg_references::Span) -> Range {
    Range::new(
        Position::new(
            u32_from_usize(span.start_row),
            byte_col_to_utf16(source, span.start_row, span.start_col),
        ),
        Position::new(
            u32_from_usize(span.end_row),
            byte_col_to_utf16(source, span.end_row, span.end_col),
        ),
    )
}

pub fn named_span_location(uri: Uri, source: &[u8], named: &svg_references::NamedSpan) -> Location {
    Location::new(uri, span_range_utf16(source, &named.span))
}

pub fn position_for_byte_offset(source: &[u8], byte_offset: usize) -> Position {
    let clamped = byte_offset.min(source.len());
    let row = source[..clamped]
        .split(|&byte| byte == b'\n')
        .count()
        .saturating_sub(1);
    let line_start = source[..clamped]
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |idx| idx + 1);
    let col = byte_col_to_utf16(source, row, clamped.saturating_sub(line_start));
    Position::new(u32_from_usize(row), col)
}
