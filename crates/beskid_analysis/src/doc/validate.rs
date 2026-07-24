//! Doc-comment diagnostics: unresolved `@ref`, arity mismatches vs signatures, and unknown tags.

use std::collections::{HashMap, HashSet};

use crate::analysis::SemanticDiagnostic;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::diagnostics::make_diagnostic;
use crate::doc::callable::callable_signatures_for_span;
use crate::doc::refs::ref_path_resolves;
use crate::doc::{
    enum_variant_names_for_span, flatten_leading_docs, generic_param_names_for_span, parse_doc_body_pairs,
};
use crate::doc_comment_parser::Rule as DocSyntaxRule;
use crate::resolve::Resolution;
use crate::resolve::items::ItemKind;
use crate::syntax::{Program, SpanInfo};

fn push_issue(
    out: &mut Vec<SemanticDiagnostic>,
    source_name: &str,
    source: &str,
    span: SpanInfo,
    issue: SemanticIssueKind,
) {
    out.push(make_diagnostic(
        source_name,
        source,
        span,
        issue.message(),
        issue.label(),
        issue.help(),
        Some(issue.code().to_string()),
        issue.severity(),
    ));
}

fn doc_span(leading: &crate::doc::LeadingDocComment) -> SpanInfo {
    leading.span
}

fn first_ident(pair: &pest::iterators::Pair<'_, DocSyntaxRule>) -> String {
    pair.clone()
        .into_inner()
        .find(|p| p.as_rule() == DocSyntaxRule::DocIdent)
        .map(|p| p.as_str().to_string())
        .unwrap_or_default()
}

fn inner_text(pair: &pest::iterators::Pair<'_, DocSyntaxRule>, rule: DocSyntaxRule) -> String {
    pair.clone().into_inner().find(|p| p.as_rule() == rule).map(|p| p.as_str().trim().to_string()).unwrap_or_default()
}

/// Documentation-only diagnostics (stable codes `W161x` / `W162x`). Requires successful name resolution.
pub fn collect_doc_diagnostics(
    program: &Program,
    resolution: &Resolution,
    source_name: &str,
    source: &str,
) -> Vec<SemanticDiagnostic> {
    let mut by_span: HashMap<(usize, usize), crate::doc::LeadingDocComment> = HashMap::new();
    for (span, doc_opt) in flatten_leading_docs(program) {
        if let Some(d) = doc_opt {
            by_span.insert((span.start, span.end), d);
        }
    }

    let mut out = Vec::new();
    for item in &resolution.items {
        let key = (item.span.start, item.span.end);
        let Some(leading) = by_span.get(&key) else {
            continue;
        };
        let body = leading.normalized_source.trim();
        if body.is_empty() {
            continue;
        };
        let Some(pieces_iter) = parse_doc_body_pairs(body) else {
            continue;
        };
        let pieces_vec: Vec<_> = pieces_iter.collect();

        let callable = callable_signatures_for_span(program, item.span);
        let supports_arg_returns =
            matches!(item.kind, ItemKind::Function | ItemKind::Method | ItemKind::ContractMethodSignature);

        let variant_names = enum_variant_names_for_span(program, item.span);
        let generic_names = generic_param_names_for_span(program, item.span);

        let mut arg_names_seen: HashSet<String> = HashSet::new();
        let mut variant_tag_names_seen: HashSet<String> = HashSet::new();
        let mut par_tag_names_seen: HashSet<String> = HashSet::new();

        for piece in &pieces_vec {
            match piece.as_rule() {
                DocSyntaxRule::RefInline => {
                    let inner = inner_text(piece, DocSyntaxRule::inner);
                    if !inner.is_empty() && !ref_path_resolves(&inner, resolution) {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocUnresolvedRef { path: inner },
                        );
                    }
                }
                DocSyntaxRule::ArgTag => {
                    let name = first_ident(piece);
                    if name.is_empty() {
                        continue;
                    }
                    if !supports_arg_returns || callable.is_none() {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocArgOrReturnsOnNonCallable,
                        );
                        continue;
                    }
                    let sig = callable.as_ref().expect("callable");
                    if arg_names_seen.contains(&name) {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocDuplicateArgName { name: name.clone() },
                        );
                    }
                    arg_names_seen.insert(name.clone());
                    if !sig.param_names.iter().any(|p| p == &name) {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocUnknownArgName { name: name.clone() },
                        );
                    }
                }
                DocSyntaxRule::ReturnsTag => {
                    if !supports_arg_returns || callable.is_none() {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocArgOrReturnsOnNonCallable,
                        );
                        continue;
                    }
                    let sig = callable.as_ref().expect("callable");
                    if sig.returns_unit {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocReturnsOnUnit,
                        );
                    }
                }
                DocSyntaxRule::VariantTag => {
                    let name = first_ident(piece);
                    if name.is_empty() {
                        continue;
                    }
                    if item.kind != ItemKind::Enum {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocVariantOnNonEnum,
                        );
                        continue;
                    }
                    let Some(vnames) = variant_names.as_ref() else {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocVariantOnNonEnum,
                        );
                        continue;
                    };
                    if variant_tag_names_seen.contains(&name) {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocDuplicateVariantName { name: name.clone() },
                        );
                    }
                    variant_tag_names_seen.insert(name.clone());
                    if !vnames.iter().any(|v| v == &name) {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocUnknownVariantName { name: name.clone() },
                        );
                    }
                }
                DocSyntaxRule::ParTag => {
                    let name = first_ident(piece);
                    if name.is_empty() {
                        continue;
                    }
                    let Some(glist) = generic_names.as_ref().filter(|g| !g.is_empty()) else {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocParWithoutGenerics,
                        );
                        continue;
                    };
                    if par_tag_names_seen.contains(&name) {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocDuplicateGenericName { name: name.clone() },
                        );
                    }
                    par_tag_names_seen.insert(name.clone());
                    if !glist.iter().any(|g| g == &name) {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocUnknownGenericName { name: name.clone() },
                        );
                    }
                }
                DocSyntaxRule::UnknownDirective => {
                    let name = first_ident(piece);
                    if !name.is_empty() {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(leading),
                            SemanticIssueKind::DocUnknownDirective { name },
                        );
                    }
                }
                _ => {}
            }
        }
    }

    for (span, doc_opt) in flatten_leading_docs(program) {
        let Some(leading) = doc_opt else { continue };
        if resolution.items.iter().any(|it| it.span == span) {
            continue;
        }
        let body = leading.normalized_source.trim();
        if body.is_empty() {
            continue;
        }
        let Some(pieces_iter) = parse_doc_body_pairs(body) else {
            continue;
        };
        let pieces_vec: Vec<_> = pieces_iter.collect();

        let mut bad_callable_directive = false;
        let mut bad_variant_directive = false;
        let mut bad_par_directive = false;
        for piece in &pieces_vec {
            match piece.as_rule() {
                DocSyntaxRule::RefInline => {
                    let inner = inner_text(piece, DocSyntaxRule::inner);
                    if !inner.is_empty() && !ref_path_resolves(&inner, resolution) {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(&leading),
                            SemanticIssueKind::DocUnresolvedRef { path: inner },
                        );
                    }
                }
                DocSyntaxRule::ArgTag | DocSyntaxRule::ReturnsTag => {
                    bad_callable_directive = true;
                }
                DocSyntaxRule::VariantTag => {
                    bad_variant_directive = true;
                }
                DocSyntaxRule::ParTag => {
                    bad_par_directive = true;
                }
                DocSyntaxRule::UnknownDirective => {
                    let name = first_ident(piece);
                    if !name.is_empty() {
                        push_issue(
                            &mut out,
                            source_name,
                            source,
                            doc_span(&leading),
                            SemanticIssueKind::DocUnknownDirective { name },
                        );
                    }
                }
                _ => {}
            }
        }
        if bad_callable_directive {
            push_issue(
                &mut out,
                source_name,
                source,
                doc_span(&leading),
                SemanticIssueKind::DocArgOrReturnsOnNonCallable,
            );
        }
        if bad_variant_directive {
            push_issue(&mut out, source_name, source, doc_span(&leading), SemanticIssueKind::DocVariantOnNonEnum);
        }
        if bad_par_directive {
            push_issue(&mut out, source_name, source, doc_span(&leading), SemanticIssueKind::DocParWithoutGenerics);
        }
    }

    out.sort_by(|a, b| a.span.offset().cmp(&b.span.offset()).then(a.code.cmp(&b.code)).then(a.message.cmp(&b.message)));
    out
}
