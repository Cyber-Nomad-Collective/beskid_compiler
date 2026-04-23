//! Documentation comments (`///`) and `@ref` cross-references.

mod render;

pub use render::ResolvedDoc;

use crate::doc_comment_parser::DocSyntaxParser;
use crate::resolve::{ItemInfo, Resolution};
use crate::syntax::{Program, SpanInfo, Spanned};
use crate::doc_comment_parser::Rule as DocSyntaxRule;
use pest::Parser;
use std::collections::HashMap;

/// Raw `///` block extracted by the main grammar (normalized body text + source span).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadingDocComment {
    pub span: SpanInfo,
    pub normalized_source: String,
}

/// Build markdown documentation per resolved item (parallel to `resolution.items` indices).
pub fn build_item_docs_markdown(
    syntax: &Program,
    resolution: &Resolution,
) -> Vec<Option<ResolvedDoc>> {
    let mut by_span: HashMap<(usize, usize), LeadingDocComment> = HashMap::new();
    for (span, doc_opt) in flatten_leading_docs(syntax) {
        if let Some(d) = doc_opt {
            by_span.insert((span.start, span.end), d);
        }
    }

    let mut out: Vec<Option<ResolvedDoc>> = vec![None; resolution.items.len()];
    for item in &resolution.items {
        let key = (item.span.start, item.span.end);
        let Some(leading) = by_span.get(&key) else {
            continue;
        };
        if leading.normalized_source.trim().is_empty() {
            continue;
        }
        let md = render_doc_body(&leading.normalized_source, resolution, item);
        out[item.id.0] = Some(ResolvedDoc { markdown: md });
    }
    out
}

fn render_doc_body(body: &str, resolution: &Resolution, current: &ItemInfo) -> String {
    let Ok(mut pairs) = DocSyntaxParser::parse(DocSyntaxRule::DocBody, body) else {
        return body.to_string();
    };
    let pair = pairs.next().unwrap();
    let mut out = String::new();
    for piece in pair.into_inner() {
        match piece.as_rule() {
            DocSyntaxRule::RefInline => {
                let inner = piece
                    .into_inner()
                    .find(|p| p.as_rule() == DocSyntaxRule::inner)
                    .map(|p| p.as_str().trim().to_string())
                    .unwrap_or_default();
                let link = resolve_ref_markdown(&inner, resolution, current);
                out.push_str(&link);
            }
            DocSyntaxRule::Run => {
                out.push_str(piece.as_str());
            }
            _ => {}
        }
    }
    if out.is_empty() {
        body.to_string()
    } else {
        out
    }
}

fn resolve_ref_markdown(path: &str, resolution: &Resolution, _current: &ItemInfo) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "`@ref()`".to_string();
    }
    for item in &resolution.items {
        if item.name == path {
            return format!("`{}`", item.name);
        }
    }
    let suffix = format!("::{path}");
    for item in &resolution.items {
        if item.name.ends_with(&suffix) {
            return format!("`{}`", item.name);
        }
    }
    let needle = path.rsplit('.').next().unwrap_or(path);
    for item in &resolution.items {
        if item.name == needle {
            return format!("`{}`", item.name);
        }
        if item.name.ends_with(&format!("::{needle}")) {
            return format!("`{}`", item.name);
        }
    }
    format!("`{path}` _(unresolved)_")
}

/// DFS order matches `Resolver::collect_item` (item, then inline-module children).
pub fn flatten_leading_docs(program: &Program) -> Vec<(SpanInfo, Option<LeadingDocComment>)> {
    let mut out = Vec::new();
    for (i, item) in program.items.iter().enumerate() {
        let doc = program.leading_docs.get(i).cloned().flatten();
        walk_item_doc(item, doc, &mut out);
    }
    out
}

fn walk_item_doc(
    item: &Spanned<crate::syntax::Node>,
    leading: Option<LeadingDocComment>,
    out: &mut Vec<(SpanInfo, Option<LeadingDocComment>)>,
) {
    use crate::syntax::Node;
    out.push((item.span, leading));
    if let Node::InlineModule(im) = &item.node {
        for (j, nested) in im.node.items.iter().enumerate() {
            let d = im.node.leading_docs.get(j).cloned().flatten();
            walk_item_doc(nested, d, out);
        }
    }
}

/// Extract span + normalized text from a Pest `DocRun` pair.
pub fn leading_doc_from_doc_run(
    pair: &pest::iterators::Pair<crate::parser::Rule>,
) -> LeadingDocComment {
    debug_assert_eq!(pair.as_rule(), crate::parser::Rule::DocRun);
    let span = SpanInfo::from_span(&pair.as_span());
    let mut lines = Vec::new();
    for line in pair.clone().into_inner() {
        if line.as_rule() != crate::parser::Rule::DocLineContent {
            continue;
        }
        let s = line.as_str();
        let rest = s.strip_prefix("///").unwrap_or(s);
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        lines.push(rest.to_string());
    }
    LeadingDocComment {
        span,
        normalized_source: lines.join("\n"),
    }
}
