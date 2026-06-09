//! Syntax highlighting for code-widget lines.

use ratatui::text::Span;

use crate::widgets::{
    code_widget::foundation::CodeLanguage,
    document_viewer::{DocumentLine, DocumentLineKind},
};

/// Highlights source code into normalized document lines.
pub fn highlight_code_lines(content: &str, language: &CodeLanguage) -> Vec<DocumentLine> {
    let plain_lines = plain_document_lines(content);
    let Some(token) = language.syntect_token() else {
        return plain_lines;
    };

    highlight_with_syntect(content, token).unwrap_or(plain_lines)
}

/// Converts source content into plain document lines.
fn plain_document_lines(content: &str) -> Vec<DocumentLine> {
    let mut lines: Vec<DocumentLine> = content
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let kind = if line.is_empty() {
                DocumentLineKind::Empty
            } else {
                DocumentLineKind::Code
            };
            DocumentLine::plain(index + 1, line.to_string(), kind)
        })
        .collect();

    if lines.is_empty() {
        lines.push(DocumentLine::plain(1, "", DocumentLineKind::Empty));
    }
    lines
}

/// Uses syntect to highlight source when a syntax exists.
fn highlight_with_syntect(content: &str, token: &str) -> Option<Vec<DocumentLine>> {
    let syntax_set = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let theme_set = syntect::highlighting::ThemeSet::load_defaults();
    let theme = theme_set.themes.get("base16-ocean.dark")?;
    let syntax = syntax_set
        .find_syntax_by_token(token)
        .or_else(|| syntax_set.find_syntax_by_extension(token))?;
    let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let spans = match highlighter.highlight_line(line, &syntax_set) {
            Ok(regions) => regions
                .into_iter()
                .map(|(style, text)| {
                    let style = syntax_style_without_background(style);
                    Span::styled(text.to_string(), style)
                })
                .collect(),
            Err(_) => vec![Span::raw(line.to_string())],
        };
        let kind = if line.is_empty() {
            DocumentLineKind::Empty
        } else {
            DocumentLineKind::Code
        };
        lines.push(DocumentLine::new(index + 1, spans, kind));
    }

    if lines.is_empty() {
        lines.push(DocumentLine::plain(1, "", DocumentLineKind::Empty));
    }
    Some(lines)
}

/// Translates syntect foreground styling while removing theme background colors.
fn syntax_style_without_background(style: syntect::highlighting::Style) -> ratatui::style::Style {
    let mut style = syntect_tui::translate_style(style).unwrap_or_default();
    style.bg = None;
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_plain_text_for_unknown_language() {
        let lines = highlight_code_lines("one\ntwo", &CodeLanguage::Unknown);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].plain_text(), "two");
    }
}
