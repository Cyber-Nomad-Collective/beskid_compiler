//! Generic editor affordances for standalone `.bsol` documents.
//!
//! These documents deliberately do not use a project/workspace manifest schema. The native
//! Beskid LSP therefore exposes only the generic BSOL escape hatch rather than inventing
//! project-manifest keywords or Beskid source-language facts.

use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Hover, HoverContents, MarkupContent, MarkupKind, TextEdit,
};

use crate::position::offset_range_to_lsp;

const SCHEMALESS_MARKER: &str = "@schemaless";
const SCHEMALESS_DETAIL: &str = "Preserve this block body as raw text";

pub(crate) fn completion_items(text: &str, offset: usize) -> Vec<CompletionItem> {
    let Some((start, prefix)) = marker_prefix_at_offset(text, offset) else {
        return Vec::new();
    };
    if !SCHEMALESS_MARKER.starts_with(prefix) {
        return Vec::new();
    }

    vec![CompletionItem {
        label: SCHEMALESS_MARKER.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some(SCHEMALESS_DETAIL.to_string()),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: offset_range_to_lsp(text, start, offset),
            new_text: SCHEMALESS_MARKER.to_string(),
        })),
        ..CompletionItem::default()
    }]
}

pub(crate) fn hover(text: &str, offset: usize) -> Option<Hover> {
    let (start, end) = identifier_bounds_at_offset(text, offset)?;
    if &text[start..end] != "schemaless" || start == 0 || text[..start].chars().next_back()? != '@' {
        return None;
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "`@schemaless` preserves the block body as raw text instead of parsing generic BSOL items."
                .to_string(),
        }),
        range: Some(offset_range_to_lsp(text, start - 1, end)),
    })
}

fn marker_prefix_at_offset(text: &str, offset: usize) -> Option<(usize, &str)> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let mut start = offset;
    while start > 0 {
        let ch = text[..start].chars().next_back()?;
        if ch.is_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }
    if start > 0 && text[..start].chars().next_back()? == '@' {
        start -= 1;
        return Some((start, &text[start..offset]));
    }
    None
}

fn identifier_bounds_at_offset(text: &str, offset: usize) -> Option<(usize, usize)> {
    let offset = offset.min(text.len());
    if !text.is_char_boundary(offset) {
        return None;
    }
    let mut start = offset;
    while start > 0 {
        let ch = text[..start].chars().next_back()?;
        if ch.is_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }
    let mut end = offset;
    while end < text.len() {
        let ch = text[end..].chars().next()?;
        if ch.is_alphanumeric() || ch == '_' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    (start != end).then_some((start, end))
}
