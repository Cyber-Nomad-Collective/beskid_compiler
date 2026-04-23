use pest::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanInfo {
    pub start: usize,
    pub end: usize,
    pub line_col_start: (usize, usize),
    pub line_col_end: (usize, usize),
}

impl SpanInfo {
    pub fn from_span(span: &Span) -> Self {
        let start_pos = span.start_pos();
        let end_pos = span.end_pos();
        Self {
            start: start_pos.pos(),
            end: end_pos.pos(),
            line_col_start: start_pos.line_col(),
            line_col_end: end_pos.line_col(),
        }
    }

    /// Build span from UTF-8 byte offsets into `source` (for non-pest diagnostics).
    pub fn from_byte_range_in_source(source: &str, start: usize, end: usize) -> Self {
        let len = source.len();
        let start = start.min(len);
        let end = end.max(start.saturating_add(1)).min(len);
        Self {
            start,
            end,
            line_col_start: byte_offset_to_line_col(source, start),
            line_col_end: byte_offset_to_line_col(source, end),
        }
    }

    /// Span covering physical line `line_1` (1-based), excluding trailing newline when present.
    pub fn whole_line_in_source(source: &str, line_1: usize) -> Self {
        let target = line_1.saturating_sub(1);
        let mut cur = 0usize;
        for (idx, chunk) in source.split_inclusive('\n').enumerate() {
            if idx == target {
                let end = if chunk.ends_with('\n') && !chunk.is_empty() {
                    cur + chunk.len() - 1
                } else {
                    cur + chunk.len()
                };
                let end = end.max(cur + 1).min(source.len());
                return Self::from_byte_range_in_source(source, cur, end);
            }
            cur += chunk.len();
        }
        if source.is_empty() {
            return Self::from_byte_range_in_source(source, 0, 0);
        }
        Self::from_byte_range_in_source(source, 0, 1.min(source.len()))
    }
}

fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub trait HasSpan {
    fn span(&self) -> &SpanInfo;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: SpanInfo,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: SpanInfo) -> Self {
        Self { node, span }
    }
}

impl<T> HasSpan for Spanned<T> {
    fn span(&self) -> &SpanInfo {
        &self.span
    }
}
