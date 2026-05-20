//! Generate or refresh leading `///` documentation blocks (`@arg` / `@returns` / `@variant` / `@par`).

use crate::doc::LeadingDocComment;
use crate::doc::callable::callable_signatures_for_span;
use crate::doc::item_shape::{enum_variant_names_for_span, generic_param_names_for_span};
use crate::resolve::Resolution;
use crate::resolve::items::ItemInfo;
use crate::resolve::items::ItemKind;
use crate::syntax::{Program, SpanInfo};

/// Text edit to insert or replace a leading doc block (IDE quick-fix shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocCommentEdit {
    Insert {
        at: usize,
        text: String,
    },
    Replace {
        start: usize,
        end: usize,
        text: String,
    },
}

fn leading_doc_for_item_span(program: &Program, item_span: SpanInfo) -> Option<LeadingDocComment> {
    for (span, doc_opt) in crate::doc::flatten_leading_docs(program) {
        if span == item_span {
            return doc_opt;
        }
    }
    None
}

fn innermost_item_at_offset(resolution: &Resolution, offset: usize) -> Option<&ItemInfo> {
    resolution
        .items
        .iter()
        .filter(|it| it.span.start <= offset && offset < it.span.end)
        .min_by_key(|it| it.span.end.saturating_sub(it.span.start))
}

/// When `offset` lies inside a documented declaration, propose a documentation edit.
pub fn doc_comment_edit_for_offset(
    program: &Program,
    resolution: &Resolution,
    _source: &str,
    offset: usize,
) -> Option<DocCommentEdit> {
    let item = innermost_item_at_offset(resolution, offset)?;
    let existing = leading_doc_for_item_span(program, item.span);
    let stub = build_stub_for_item(program, item, existing.as_ref())?;

    if let Some(leading) = existing {
        Some(DocCommentEdit::Replace {
            start: leading.span.start,
            end: leading.span.end,
            text: stub,
        })
    } else {
        Some(DocCommentEdit::Insert {
            at: item.span.start,
            text: stub,
        })
    }
}

fn first_summary_line(existing: Option<&LeadingDocComment>) -> Option<String> {
    let d = existing?;
    let line = d
        .normalized_source
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())?;
    if line.starts_with('@') {
        return None;
    }
    Some(line.to_string())
}

fn build_stub_for_item(
    program: &Program,
    item: &ItemInfo,
    existing: Option<&LeadingDocComment>,
) -> Option<String> {
    let summary = first_summary_line(existing).unwrap_or_else(|| "TODO: Summary.".to_string());
    let mut out = String::new();
    out.push_str("/// ");
    out.push_str(&summary);
    out.push('\n');
    out.push_str("///\n");

    let generics = generic_param_names_for_span(program, item.span)
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    for g in &generics {
        out.push_str("/// @par(");
        out.push_str(g);
        out.push_str(") TODO\n");
    }

    match item.kind {
        ItemKind::Enum => {
            if let Some(names) = enum_variant_names_for_span(program, item.span) {
                for v in names {
                    out.push_str("/// @variant(");
                    out.push_str(&v);
                    out.push_str(") TODO\n");
                }
            }
        }
        ItemKind::Function | ItemKind::Method | ItemKind::ContractMethodSignature => {
            let sig = callable_signatures_for_span(program, item.span)?;
            for p in &sig.param_names {
                out.push_str("/// @arg(");
                out.push_str(p);
                out.push_str(") TODO\n");
            }
            if !sig.returns_unit {
                out.push_str("/// @returns TODO\n");
            }
        }
        ItemKind::Type => {}
        _ => {
            if generics.is_empty() {
                return None;
            }
        }
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}
