//! Adapter from rendered markdown lines into the shared document model.

use ratatui::text::Line;

use crate::widgets::document_viewer::{DocumentLine, DocumentLineKind, RenderedDocument};

use super::markdown_outline_adapter::markdown_outline_from_content;

/// Converts visible markdown lines into a shared rendered document.
pub fn markdown_lines_to_document(
    lines: Vec<Line<'static>>,
    source_start_line: usize,
    content: &str,
) -> RenderedDocument {
    let source_lines = (0..lines.len())
        .map(|index| source_start_line.saturating_add(index))
        .collect::<Vec<_>>();
    markdown_lines_to_document_with_source_lines(lines, source_lines, content)
}

/// Converts markdown lines into a document while preserving original source lines.
pub fn markdown_lines_to_document_with_source_lines(
    lines: Vec<Line<'static>>,
    source_lines: Vec<usize>,
    content: &str,
) -> RenderedDocument {
    let document_lines = lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let source_line = source_lines
                .get(index)
                .copied()
                .unwrap_or_else(|| index.saturating_add(1));
            let kind = markdown_line_kind(&line);
            DocumentLine::new(source_line, line.spans, kind)
        })
        .collect();
    RenderedDocument::new(document_lines, markdown_outline_from_content(content))
}

/// Classifies a rendered markdown line for the shared model.
fn markdown_line_kind(line: &Line<'static>) -> DocumentLineKind {
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        DocumentLineKind::Empty
    } else if trimmed.starts_with('#') {
        DocumentLineKind::Heading {
            level: trimmed.chars().take_while(|ch| *ch == '#').count() as u8,
            collapsed: false,
        }
    } else if trimmed.starts_with('─') || trimmed.starts_with("---") {
        DocumentLineKind::Separator
    } else {
        DocumentLineKind::Text
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Span;

    use super::*;

    #[test]
    fn converts_lines_to_document_model() {
        let document =
            markdown_lines_to_document(vec![Line::from(vec![Span::raw("hello")])], 3, "# T");
        assert_eq!(document.lines[0].source_line, 3);
        assert_eq!(document.lines[0].plain_text(), "hello");
        assert_eq!(document.outline[0].title, "T");
    }

    #[test]
    fn preserves_explicit_source_lines() {
        let document = markdown_lines_to_document_with_source_lines(
            vec![
                Line::from(vec![Span::raw("wrapped part one")]),
                Line::from(vec![Span::raw("wrapped part two")]),
            ],
            vec![7, 7],
            "# T",
        );
        assert_eq!(document.lines[0].source_line, 7);
        assert_eq!(document.lines[1].source_line, 7);
    }
}
