//! Documentation comments (`///`), `@ref` / `@arg` / `@returns` / `@variant` / `@par`, and API doc snapshots.

mod api_snapshot;
mod callable;
mod edit;
mod item_shape;
mod refs;
mod render;
mod validate;

pub use api_snapshot::{
    API_JSON_NAVIGATION_MODEL_GRAPH_V1, API_JSON_SCHEMA_VERSION,
    API_JSON_SCHEMA_VERSION_BEFORE_GRAPH, ApiDocItem, ApiDocRoot, ApiDocumentationPointer,
    ApiLocation, ItemDocArgument, ItemDocStructured,
};
pub use callable::callable_signatures_for_span;
pub use edit::{DocCommentEdit, doc_comment_edit_for_offset};
pub use item_shape::{enum_variant_names_for_span, generic_param_names_for_span};
pub use refs::{DocRefLinkContext, ref_path_resolves, resolve_ref_markdown};
pub use render::ResolvedDoc;
pub use validate::collect_doc_diagnostics;

use crate::doc_comment_parser::DocSyntaxParser;
use crate::doc_comment_parser::Rule as DocSyntaxRule;
use crate::resolve::{ItemInfo, Resolution};
use crate::syntax::{Program, SpanInfo, Spanned};
use pest::Parser;
use pest::iterators::Pair;
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
    docs_ref_links: Option<&DocRefLinkContext>,
) -> Vec<Option<ResolvedDoc>> {
    let mut by_span: HashMap<(usize, usize), LeadingDocComment> = HashMap::new();
    for (span, doc_opt) in flatten_leading_docs(syntax) {
        if let Some(d) = doc_opt {
            debug_assert!(
                !by_span.contains_key(&(span.start, span.end)),
                "duplicate leading-doc span detected: {}..{}",
                span.start,
                span.end
            );
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
        let md = render_doc_body(&leading.normalized_source, resolution, item, docs_ref_links);
        let structured = extract_structured_doc(&leading.normalized_source);
        out[item.id.0] = Some(ResolvedDoc {
            markdown: md,
            structured,
        });
    }
    out
}

fn render_doc_body(
    body: &str,
    resolution: &Resolution,
    _item: &ItemInfo,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> String {
    let Ok(mut pairs) = DocSyntaxParser::parse(DocSyntaxRule::DocBody, body) else {
        return body.to_string();
    };
    let pair = pairs.next().unwrap();
    let mut out = String::new();
    for wrapper in pair.into_inner() {
        if matches!(wrapper.as_rule(), DocSyntaxRule::EOI) {
            continue;
        }
        let Some(piece) = wrapper.into_inner().next() else {
            continue;
        };
        match piece.as_rule() {
            DocSyntaxRule::RefInline => {
                let inner = inner_text(&piece, DocSyntaxRule::inner);
                let link = resolve_ref_markdown(&inner, resolution, docs_ref_links);
                out.push_str(&link);
            }
            DocSyntaxRule::ArgTag => {
                let name = first_ident(&piece);
                let rest = arg_body_after_close(&piece);
                let rest = rest.trim();
                if !name.is_empty() {
                    out.push_str("\n\n**Parameter `");
                    out.push_str(&name);
                    out.push_str("`**");
                    if !rest.is_empty() {
                        out.push('\n');
                        out.push_str(rest);
                    }
                    out.push('\n');
                }
            }
            DocSyntaxRule::ReturnsTag => {
                let rest = first_rule_text(&piece, DocSyntaxRule::ReturnsSuffix);
                let rest = rest.trim();
                out.push_str("\n\n**Returns**\n\n");
                if rest.is_empty() {
                    out.push_str("_(no description)_");
                } else {
                    out.push_str(rest);
                }
                out.push('\n');
            }
            DocSyntaxRule::VariantTag => {
                let name = first_ident(&piece);
                let rest = closing_paren_suffix_rest(&piece, DocSyntaxRule::VariantSuffix);
                let rest = rest.trim();
                if !name.is_empty() {
                    out.push_str("\n\n**Variant `");
                    out.push_str(&name);
                    out.push_str("`**");
                    if !rest.is_empty() {
                        out.push('\n');
                        out.push_str(rest);
                    }
                    out.push('\n');
                }
            }
            DocSyntaxRule::ParTag => {
                let name = first_ident(&piece);
                let rest = closing_paren_suffix_rest(&piece, DocSyntaxRule::ParSuffix);
                let rest = rest.trim();
                if !name.is_empty() {
                    out.push_str("\n\n**Type parameter `");
                    out.push_str(&name);
                    out.push_str("`**");
                    if !rest.is_empty() {
                        out.push('\n');
                        out.push_str(rest);
                    }
                    out.push('\n');
                }
            }
            DocSyntaxRule::UnknownDirective => {
                let name = first_ident(&piece);
                let rest = first_rule_text(&piece, DocSyntaxRule::UnknownSuffix);
                out.push_str("\n\n`@");
                out.push_str(&name);
                out.push('`');
                let rest = rest.trim();
                if !rest.is_empty() {
                    out.push(' ');
                    out.push_str(rest);
                }
                out.push('\n');
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

fn inner_text(pair: &Pair<'_, DocSyntaxRule>, rule: DocSyntaxRule) -> String {
    pair.clone()
        .into_inner()
        .find(|p| p.as_rule() == rule)
        .map(|p| p.as_str().trim().to_string())
        .unwrap_or_default()
}

fn first_ident(pair: &Pair<'_, DocSyntaxRule>) -> String {
    pair.clone()
        .into_inner()
        .find(|p| p.as_rule() == DocSyntaxRule::DocIdent)
        .map(|p| p.as_str().to_string())
        .unwrap_or_default()
}

fn first_rule_text(pair: &Pair<'_, DocSyntaxRule>, rule: DocSyntaxRule) -> String {
    pair.clone()
        .into_inner()
        .find(|p| p.as_rule() == rule)
        .map(|p| p.as_str().to_string())
        .unwrap_or_default()
}

fn arg_body_after_close(pair: &Pair<'_, DocSyntaxRule>) -> String {
    closing_paren_suffix_rest(pair, DocSyntaxRule::ArgSuffix)
}

fn closing_paren_suffix_rest(pair: &Pair<'_, DocSyntaxRule>, suffix: DocSyntaxRule) -> String {
    first_rule_text(pair, suffix)
        .trim_start_matches(')')
        .trim()
        .to_string()
}

fn extract_structured_doc(body: &str) -> Option<ItemDocStructured> {
    let Ok(mut pairs) = DocSyntaxParser::parse(DocSyntaxRule::DocBody, body) else {
        return None;
    };
    let pair = pairs.next()?;
    let mut summary = String::new();
    let mut returns_markdown = None::<String>;
    let mut arguments: Vec<ItemDocArgument> = Vec::new();
    let mut enum_variants: Vec<ItemDocArgument> = Vec::new();
    let mut type_parameters: Vec<ItemDocArgument> = Vec::new();
    for wrapper in pair.into_inner() {
        if matches!(wrapper.as_rule(), DocSyntaxRule::EOI) {
            continue;
        }
        let Some(piece) = wrapper.into_inner().next() else {
            continue;
        };
        match piece.as_rule() {
            DocSyntaxRule::Run => summary.push_str(piece.as_str()),
            DocSyntaxRule::RefInline => {
                summary.push_str(piece.as_str());
            }
            DocSyntaxRule::ArgTag => {
                let name = first_ident(&piece);
                let rest = arg_body_after_close(&piece);
                if !name.is_empty() {
                    arguments.push(ItemDocArgument {
                        name,
                        markdown: rest.trim().to_string(),
                    });
                }
            }
            DocSyntaxRule::ReturnsTag => {
                let rest = first_rule_text(&piece, DocSyntaxRule::ReturnsSuffix);
                returns_markdown = Some(rest.trim().to_string());
            }
            DocSyntaxRule::VariantTag => {
                let name = first_ident(&piece);
                let rest = closing_paren_suffix_rest(&piece, DocSyntaxRule::VariantSuffix);
                if !name.is_empty() {
                    enum_variants.push(ItemDocArgument {
                        name,
                        markdown: rest.trim().to_string(),
                    });
                }
            }
            DocSyntaxRule::ParTag => {
                let name = first_ident(&piece);
                let rest = closing_paren_suffix_rest(&piece, DocSyntaxRule::ParSuffix);
                if !name.is_empty() {
                    type_parameters.push(ItemDocArgument {
                        name,
                        markdown: rest.trim().to_string(),
                    });
                }
            }
            DocSyntaxRule::UnknownDirective => {
                summary.push_str(piece.as_str());
            }
            _ => {}
        }
    }
    let summary_trim = summary.trim();
    let summary_markdown = if summary_trim.is_empty() {
        None
    } else {
        Some(summary_trim.to_string())
    };
    if summary_markdown.is_none()
        && returns_markdown.is_none()
        && arguments.is_empty()
        && enum_variants.is_empty()
        && type_parameters.is_empty()
    {
        None
    } else {
        Some(ItemDocStructured {
            summary_markdown,
            returns_markdown,
            arguments,
            enum_variants,
            type_parameters,
        })
    }
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
    match &item.node {
        Node::InlineModule(im) => walk_inline_module_docs(im, out),
        Node::TypeDefinition(def) => walk_type_docs(def, out),
        Node::EnumDefinition(def) => walk_enum_docs(def, out),
        Node::ContractDefinition(def) => walk_contract_docs(def, out),
        Node::Function(def) => {
            walk_parameter_docs(&def.node.parameters, &def.node.parameter_docs, out)
        }
        Node::Method(def) => {
            walk_parameter_docs(&def.node.parameters, &def.node.parameter_docs, out)
        }
        Node::ExtendTypeDefinition(def) => {
            for (index, method) in def.node.methods.iter().enumerate() {
                let docs = def.node.method_docs.get(index).cloned().flatten();
                out.push((method.span, docs));
                walk_parameter_docs(&method.node.parameters, &method.node.parameter_docs, out);
            }
        }
        Node::TestDefinition(def) => walk_statement_docs(def, out),
        _ => {}
    }
}

fn walk_inline_module_docs(
    inline_module: &Spanned<crate::syntax::InlineModule>,
    out: &mut Vec<(SpanInfo, Option<LeadingDocComment>)>,
) {
    for (j, nested) in inline_module.node.items.iter().enumerate() {
        let d = inline_module.node.leading_docs.get(j).cloned().flatten();
        walk_item_doc(nested, d, out);
    }
}

fn walk_parameter_docs<T>(
    parameters: &[Spanned<T>],
    parameter_docs: &[Option<LeadingDocComment>],
    out: &mut Vec<(SpanInfo, Option<LeadingDocComment>)>,
) {
    for (idx, param) in parameters.iter().enumerate() {
        let doc = parameter_docs.get(idx).cloned().flatten();
        out.push((param.span, doc));
    }
}

fn walk_type_docs(
    type_definition: &Spanned<crate::syntax::TypeDefinition>,
    out: &mut Vec<(SpanInfo, Option<LeadingDocComment>)>,
) {
    for (idx, field) in type_definition.node.fields.iter().enumerate() {
        let doc = type_definition.node.field_docs.get(idx).cloned().flatten();
        out.push((field.span, doc));
    }
}

fn walk_enum_docs(
    enum_definition: &Spanned<crate::syntax::EnumDefinition>,
    out: &mut Vec<(SpanInfo, Option<LeadingDocComment>)>,
) {
    for (variant_idx, variant) in enum_definition.node.variants.iter().enumerate() {
        let variant_doc = enum_definition
            .node
            .variant_docs
            .get(variant_idx)
            .cloned()
            .flatten();
        out.push((variant.span, variant_doc));
        for (field_idx, field) in variant.node.fields.iter().enumerate() {
            let field_doc = variant.node.field_docs.get(field_idx).cloned().flatten();
            out.push((field.span, field_doc));
        }
    }
}

fn walk_contract_docs(
    contract_definition: &Spanned<crate::syntax::ContractDefinition>,
    out: &mut Vec<(SpanInfo, Option<LeadingDocComment>)>,
) {
    for (idx, item) in contract_definition.node.items.iter().enumerate() {
        let node_doc = contract_definition
            .node
            .item_docs
            .get(idx)
            .cloned()
            .flatten();
        out.push((item.span, node_doc));
        if let crate::syntax::ContractNode::MethodSignature(signature) = &item.node {
            walk_parameter_docs(
                &signature.node.parameters,
                &signature.node.parameter_docs,
                out,
            );
        }
    }
}

fn walk_statement_docs(
    test_definition: &Spanned<crate::syntax::TestDefinition>,
    out: &mut Vec<(SpanInfo, Option<LeadingDocComment>)>,
) {
    for (idx, statement) in test_definition.node.statements.iter().enumerate() {
        let doc = test_definition
            .node
            .statement_docs
            .get(idx)
            .cloned()
            .flatten();
        out.push((statement.span, doc));
    }
}

/// Extract span + normalized text from a Pest `DocRun` pair.
pub fn leading_doc_from_doc_run(
    pair: &pest::iterators::Pair<crate::parser::Rule>,
) -> LeadingDocComment {
    debug_assert_eq!(pair.as_rule(), crate::parser::Rule::DocRun);
    let span = SpanInfo::from_span(&pair.as_span());
    let mut lines = Vec::new();
    let mut saw_explicit_lines = false;
    for line in pair.clone().into_inner() {
        if line.as_rule() != crate::parser::Rule::DocLineContent {
            continue;
        }
        saw_explicit_lines = true;
        let s = line.as_str();
        let rest = s.strip_prefix("///").unwrap_or(s);
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        lines.push(rest.trim_end_matches(['\n', '\r']).to_string());
    }
    if !saw_explicit_lines {
        for raw in pair.as_str().lines() {
            if !raw.starts_with("///") {
                continue;
            }
            let rest = raw.strip_prefix("///").unwrap_or(raw);
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            lines.push(rest.to_string());
        }
    }
    LeadingDocComment {
        span,
        normalized_source: lines.join("\n"),
    }
}

pub(crate) fn parse_doc_body_pairs<'a>(
    body: &'a str,
) -> Option<impl Iterator<Item = pest::iterators::Pair<'a, DocSyntaxRule>> + 'a> {
    let mut pairs = DocSyntaxParser::parse(DocSyntaxRule::DocBody, body).ok()?;
    let body_pair = pairs.next()?;
    Some(body_pair.into_inner().filter_map(|wrapper| {
        if wrapper.as_rule() != DocSyntaxRule::piece {
            return None;
        }
        wrapper.into_inner().next()
    }))
}
