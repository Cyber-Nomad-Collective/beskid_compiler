//! Generation-bound documentation facts derived from expanded syntax (no HIR / Resolution).

use beskid_analysis::doc::{
    DocCommentEdit, LeadingDocComment, callable_signatures_for_span, enum_variant_names_for_span,
    flatten_leading_docs, generic_param_names_for_span,
};
use beskid_analysis::syntax::{Node, Program, SpanInfo, Spanned};

/// Declaration kinds that documentation actions can generate stubs for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxDocumentationKind {
    Function,
    Method,
    ContractMethod,
    Enum,
    Type,
}

/// One declaration's documentation shape bound to a specific buffer revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDocumentationFact {
    pub name: String,
    pub kind: SyntaxDocumentationKind,
    pub declaration_span: SpanInfo,
    pub param_names: Vec<String>,
    pub generic_names: Vec<String>,
    pub variant_names: Vec<String>,
    pub returns_unit: bool,
    pub leading_doc_start: Option<usize>,
    pub leading_doc_end: Option<usize>,
    pub leading_summary: Option<String>,
}

impl SyntaxDocumentationFact {
    pub fn declaration_start(&self) -> usize {
        self.declaration_span.start
    }

    pub fn declaration_end(&self) -> usize {
        self.declaration_span.end
    }
}

/// Collect documentation facts for every documentable top-level or nested declaration.
pub fn syntax_documentation_facts_for_program(program: &Program) -> Vec<SyntaxDocumentationFact> {
    let leading = flatten_leading_docs(program);
    let mut facts = Vec::new();
    collect_from_items(&program.items, &leading, &mut facts);
    facts
}

fn collect_from_items(
    items: &[Spanned<Node>],
    leading: &[(SpanInfo, Option<LeadingDocComment>)],
    out: &mut Vec<SyntaxDocumentationFact>,
) {
    for item in items {
        match &item.node {
            Node::Function(definition) => {
                push_fact(
                    out,
                    leading,
                    item.span,
                    definition.node.name.node.name.clone(),
                    SyntaxDocumentationKind::Function,
                );
            }
            Node::Method(definition) => {
                push_fact(
                    out,
                    leading,
                    item.span,
                    definition.node.name.node.name.clone(),
                    SyntaxDocumentationKind::Method,
                );
            }
            Node::TestDefinition(definition) => {
                push_fact(
                    out,
                    leading,
                    item.span,
                    definition.node.name.node.name.clone(),
                    SyntaxDocumentationKind::Function,
                );
            }
            Node::TypeDefinition(definition) => {
                push_fact(
                    out,
                    leading,
                    item.span,
                    definition.node.name.node.name.clone(),
                    SyntaxDocumentationKind::Type,
                );
            }
            Node::EnumDefinition(definition) => {
                push_fact(
                    out,
                    leading,
                    item.span,
                    definition.node.name.node.name.clone(),
                    SyntaxDocumentationKind::Enum,
                );
            }
            Node::ExtendTypeDefinition(extension) => {
                for method in &extension.node.methods {
                    push_fact(
                        out,
                        leading,
                        method.span,
                        method.node.name.node.name.clone(),
                        SyntaxDocumentationKind::Method,
                    );
                }
            }
            Node::ContractDefinition(contract) => {
                for contract_item in &contract.node.items {
                    if let beskid_analysis::syntax::ContractNode::MethodSignature(sig) =
                        &contract_item.node
                    {
                        push_fact(
                            out,
                            leading,
                            sig.span,
                            sig.node.name.node.name.clone(),
                            SyntaxDocumentationKind::ContractMethod,
                        );
                    }
                }
            }
            Node::InlineModule(module) => {
                collect_from_items(&module.node.items, leading, out);
            }
            _ => {}
        }
    }
}

fn push_fact(
    out: &mut Vec<SyntaxDocumentationFact>,
    leading: &[(SpanInfo, Option<LeadingDocComment>)],
    span: SpanInfo,
    name: String,
    kind: SyntaxDocumentationKind,
) {
    let leading_doc = leading
        .iter()
        .find(|(item_span, _)| *item_span == span)
        .and_then(|(_, doc)| doc.clone());
    let (leading_doc_start, leading_doc_end, leading_summary) = match leading_doc.as_ref() {
        Some(doc) => (
            Some(doc.span.start),
            Some(doc.span.end),
            first_summary_line(Some(doc)),
        ),
        None => (None, None, None),
    };

    // Shape helpers need a Program view; callers fill param/generic/variant via
    // [`complete_shape_from_program`] after collection when a program is available.
    out.push(SyntaxDocumentationFact {
        name,
        kind,
        declaration_span: span,
        param_names: Vec::new(),
        generic_names: Vec::new(),
        variant_names: Vec::new(),
        returns_unit: true,
        leading_doc_start,
        leading_doc_end,
        leading_summary,
    });
}

/// Fill parameter / generic / variant / return shape from the owning program.
pub fn complete_shape_from_program(program: &Program, facts: &mut [SyntaxDocumentationFact]) {
    for fact in facts.iter_mut() {
        let span = fact.declaration_span;
        fact.generic_names = generic_param_names_for_span(program, span).unwrap_or_default();
        match fact.kind {
            SyntaxDocumentationKind::Enum => {
                fact.variant_names = enum_variant_names_for_span(program, span).unwrap_or_default();
            }
            SyntaxDocumentationKind::Function
            | SyntaxDocumentationKind::Method
            | SyntaxDocumentationKind::ContractMethod => {
                if let Some(sig) = callable_signatures_for_span(program, span) {
                    fact.param_names = sig.param_names;
                    fact.returns_unit = sig.returns_unit;
                }
            }
            SyntaxDocumentationKind::Type => {}
        }
    }
}

/// Build facts for a source buffer (parse must succeed).
pub fn syntax_documentation_facts_for_source(
    source_name: &str,
    source: &str,
) -> Vec<SyntaxDocumentationFact> {
    let Ok(program) = beskid_analysis::services::parse_program_with_source_name(source_name, source) else {
        return Vec::new();
    };
    let mut facts = syntax_documentation_facts_for_program(&program.node);
    complete_shape_from_program(&program.node, &mut facts);
    facts
}

/// Propose a documentation edit from generation-bound syntax facts at `offset`.
pub fn doc_comment_edit_from_syntax_facts(
    facts: &[SyntaxDocumentationFact],
    offset: usize,
) -> Option<DocCommentEdit> {
    let fact = facts
        .iter()
        .filter(|fact| {
            fact.declaration_start() <= offset && offset < fact.declaration_end()
        })
        .min_by_key(|fact| {
            fact.declaration_end()
                .saturating_sub(fact.declaration_start())
        })?;
    let stub = build_stub(fact)?;
    match (fact.leading_doc_start, fact.leading_doc_end) {
        (Some(start), Some(end)) => Some(DocCommentEdit::Replace {
            start,
            end,
            text: stub,
        }),
        _ => Some(DocCommentEdit::Insert {
            at: fact.declaration_start(),
            text: stub,
        }),
    }
}

fn first_summary_line(existing: Option<&LeadingDocComment>) -> Option<String> {
    let doc = existing?;
    let line = doc
        .normalized_source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    if line.starts_with('@') {
        return None;
    }
    Some(line.to_string())
}

fn build_stub(fact: &SyntaxDocumentationFact) -> Option<String> {
    let summary = fact
        .leading_summary
        .clone()
        .unwrap_or_else(|| "TODO: Summary.".to_string());
    let mut out = String::new();
    out.push_str("/// ");
    out.push_str(&summary);
    out.push('\n');
    out.push_str("///\n");

    for generic in &fact.generic_names {
        out.push_str("/// @par(");
        out.push_str(generic);
        out.push_str(") TODO\n");
    }

    match fact.kind {
        SyntaxDocumentationKind::Enum => {
            for variant in &fact.variant_names {
                out.push_str("/// @variant(");
                out.push_str(variant);
                out.push_str(") TODO\n");
            }
        }
        SyntaxDocumentationKind::Function
        | SyntaxDocumentationKind::Method
        | SyntaxDocumentationKind::ContractMethod => {
            for param in &fact.param_names {
                out.push_str("/// @arg(");
                out.push_str(param);
                out.push_str(") TODO\n");
            }
            if !fact.returns_unit {
                out.push_str("/// @returns TODO\n");
            }
        }
        SyntaxDocumentationKind::Type => {
            if fact.generic_names.is_empty() {
                return None;
            }
        }
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{
        doc_comment_edit_from_syntax_facts, syntax_documentation_facts_for_source,
        SyntaxDocumentationKind,
    };
    use beskid_analysis::doc::DocCommentEdit;

    #[test]
    fn documentation_facts_bind_to_current_buffer_declarations() {
        let source = "i32 Before() { return 0; }\n\ni32 Current(i32 value) { return value; }";
        let facts = syntax_documentation_facts_for_source("/tmp/docs.bd", source);
        let current = facts
            .iter()
            .find(|fact| fact.name == "Current")
            .expect("Current fact");
        assert_eq!(current.kind, SyntaxDocumentationKind::Function);
        assert_eq!(current.param_names, vec!["value".to_string()]);
        assert!(!current.returns_unit);
        let offset = source.find("Current").expect("Current") + 1;
        let edit = doc_comment_edit_from_syntax_facts(&facts, offset).expect("edit");
        match edit {
            DocCommentEdit::Insert { at, text } => {
                assert_eq!(at, current.declaration_start());
                assert!(text.contains("@arg(value)"));
                assert!(text.contains("@returns"));
            }
            DocCommentEdit::Replace { .. } => panic!("expected insert for undocumented Current"),
        }
    }

    #[test]
    fn documentation_facts_ignore_stale_prior_declaration_names() {
        let stale = "i32 Old() { return 0; }";
        let current = "i32 Before() { return 0; }\n\ni32 Current() { return 0; }";
        let stale_facts = syntax_documentation_facts_for_source("/tmp/docs.bd", stale);
        assert!(stale_facts.iter().any(|fact| fact.name == "Old"));
        assert!(!stale_facts.iter().any(|fact| fact.name == "Current"));
        let current_facts = syntax_documentation_facts_for_source("/tmp/docs.bd", current);
        assert!(current_facts.iter().any(|fact| fact.name == "Current"));
        assert!(!current_facts.iter().any(|fact| fact.name == "Old"));
    }
}
