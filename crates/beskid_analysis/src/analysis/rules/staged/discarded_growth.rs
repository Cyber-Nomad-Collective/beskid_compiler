//! W1240: `Array.Append` or `Array.RemoveLast` used as a statement on an array its caller does not
//! own.
//!
//! Since 0.6.7 a growth or a shrink changes its first argument only when that argument owns the
//! array (a `let` local, a `mut` or `move` parameter, a field); any other array (a parameter that
//! is not `mut`, a loop or pattern variable, a lambda parameter, a call result, an element) is
//! copied into a fresh array and the changed copy is the call's result. A statement that discards
//! that result changes nothing.

use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::{DiscardedGrowthRepair, ImmutableBinding, SemanticIssueKind, issue_diagnostic};
use crate::analysis::rules::RuleContext;
use crate::syntax::{
    Block, Expression, ExpressionStatement, LambdaExpression, LetStatement, MatchArm, Parameter, Pattern, Program,
    SpanInfo, Spanned,
};
use crate::syntax_query::{AstWalker, NodeKind, NodeRef, Visit};
use std::collections::HashMap;

impl SemanticPipelineRule {
    pub(super) fn check_discarded_growth(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let mut findings = Vec::new();
        for item in &program.node.items {
            match &item.node {
                crate::syntax::Node::Function(definition) => {
                    walk(
                        ctx.source(),
                        &mut findings,
                        &definition.node.parameters,
                        NodeRef::from(&definition.node.body.node),
                    );
                }
                crate::syntax::Node::Method(definition) => {
                    walk(
                        ctx.source(),
                        &mut findings,
                        &definition.node.parameters,
                        NodeRef::from(&definition.node.body.node),
                    );
                }
                crate::syntax::Node::ExtendTypeDefinition(definition) => {
                    for method in &definition.node.methods {
                        walk(
                            ctx.source(),
                            &mut findings,
                            &method.node.parameters,
                            NodeRef::from(&method.node.body.node),
                        );
                    }
                }
                crate::syntax::Node::TestDefinition(definition) => {
                    let mut visitor = GrowthVisitor::new(ctx.source(), &mut findings);
                    visitor.scopes.push(HashMap::new());
                    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                    for statement in &definition.node.statements {
                        walker.walk(NodeRef::from(&statement.node));
                    }
                }
                _ => {}
            }
        }
        for (span, kind) in findings {
            let diagnostic = issue_diagnostic(ctx.source_name(), ctx.source(), span, &kind);
            ctx.emit(diagnostic);
        }
    }
}

fn walk<'a>(
    source: &str,
    findings: &mut Vec<(SpanInfo, SemanticIssueKind)>,
    parameters: &'a [Spanned<Parameter>],
    body: NodeRef<'a>,
) {
    let mut visitor = GrowthVisitor::new(source, findings);
    visitor.pending = Some(
        parameters
            .iter()
            .map(|parameter| {
                let binding = if parameter.node.mutable || parameter.node.movable {
                    Binding::Owner
                } else {
                    Binding::Parameter { insert_at: ImmutableBinding::of_parameter(parameter).mut_at }
                };
                (parameter.node.name.node.name.clone(), binding)
            })
            .collect(),
    );
    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
    walker.walk(body);
}

/// What a name in scope is to a growth call.
#[derive(Clone, Copy)]
enum Binding {
    /// A `let` local or a `mut` parameter: the call grows it in place.
    Owner,
    /// A parameter that is not `mut`; `insert_at` is where `mut ` goes.
    Parameter { insert_at: usize },
    /// A name that may be assigned (a pattern binding, a lambda parameter).
    Assignable,
    /// A name that may not be assigned (a `for` variable).
    Fixed,
}

struct GrowthVisitor<'s, 'f> {
    source: &'s str,
    findings: &'f mut Vec<(SpanInfo, SemanticIssueKind)>,
    scopes: Vec<HashMap<String, Binding>>,
    kind_stack: Vec<NodeKind>,
    for_iterators: Vec<String>,
    pending: Option<Vec<(String, Binding)>>,
}

impl<'s, 'f> GrowthVisitor<'s, 'f> {
    fn new(source: &'s str, findings: &'f mut Vec<(SpanInfo, SemanticIssueKind)>) -> Self {
        Self { source, findings, scopes: Vec::new(), kind_stack: Vec::new(), for_iterators: Vec::new(), pending: None }
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name).copied())
    }

    fn text(&self, span: SpanInfo) -> String {
        self.source.get(span.start..span.end).unwrap_or_default().to_string()
    }

    fn declare(&mut self, name: String, binding: Binding) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, binding);
        }
    }

    fn statement(&mut self, statement: &ExpressionStatement) {
        let mut expression = &statement.expression;
        while let Expression::Grouped(grouped) = &expression.node {
            expression = &grouped.node.expr;
        }
        let Expression::Call(call) = &expression.node else { return };
        let Expression::Path(callee) = &call.node.callee.node else { return };
        let segments = &callee.node.path.node.segments;
        let [.., array, append] = segments.as_slice() else { return };
        let operation = append.node.name.node.name.as_str();
        if array.node.name.node.name != "Array" || !matches!(operation, "Append" | "RemoveLast") {
            return;
        }
        let Some(first) = call.node.args.first() else { return };
        let repair = match &first.node {
            Expression::Path(path) if path.node.path.node.segments.len() == 1 => {
                let name = path.node.path.node.segments[0].node.name.node.name.clone();
                match self.lookup(&name) {
                    None | Some(Binding::Owner) => return,
                    Some(Binding::Parameter { insert_at }) => DiscardedGrowthRepair::MutParameter { name, insert_at },
                    Some(Binding::Assignable) => {
                        DiscardedGrowthRepair::Assign { name, insert_at: expression.span.start }
                    }
                    Some(Binding::Fixed) => self.bind(append, expression.span),
                }
            }
            // A field (`holder.items`, `this.items`) owns its array.
            Expression::Path(_) | Expression::Member(_) => return,
            _ => self.bind(append, expression.span),
        };
        let kind = SemanticIssueKind::DiscardedArrayGrowth {
            operation: append.node.name.node.name.clone(),
            owner: self.text(first.span),
            call: self.text(expression.span),
            repair,
        };
        self.findings.push((expression.span, kind));
    }

    /// Bind the result to a new local, written with the element type when the call names it.
    fn bind(&self, append: &Spanned<crate::syntax::PathSegment>, call: SpanInfo) -> DiscardedGrowthRepair {
        let declaration = match append.node.type_args.as_slice() {
            [element] => Some(format!("{}[]", self.text(element.span))),
            _ => None,
        };
        DiscardedGrowthRepair::Bind { declaration, insert_at: call.start }
    }
}

impl Visit for GrowthVisitor<'_, '_> {
    fn enter(&mut self, node: NodeRef<'_>) {
        let parent = self.kind_stack.last().copied();
        if let Some(for_statement) = node.of::<crate::syntax::ForStatement>() {
            self.for_iterators.push(for_statement.iterator.node.name.clone());
        }
        if node.of::<Block>().is_some() {
            self.scopes.push(HashMap::new());
            if let Some(parameters) = self.pending.take()
                && let Some(scope) = self.scopes.last_mut()
            {
                scope.extend(parameters);
            }
            if parent == Some(NodeKind::ForStatement)
                && let Some(iterator) = self.for_iterators.last().cloned()
            {
                self.declare(iterator, Binding::Fixed);
            }
        }
        if let Some(arm) = node.of::<MatchArm>() {
            self.scopes.push(HashMap::new());
            let mut names = Vec::new();
            pattern_names(&arm.pattern.node, &mut names);
            for name in names {
                self.declare(name, Binding::Assignable);
            }
        }
        if let Some(lambda) = node.of::<LambdaExpression>() {
            self.scopes.push(HashMap::new());
            for parameter in &lambda.parameters {
                self.declare(parameter.node.name.node.name.clone(), Binding::Assignable);
            }
        }
        if let Some(statement) = node.of::<ExpressionStatement>() {
            self.statement(statement);
        }
        self.kind_stack.push(node.node_kind());
    }

    fn exit(&mut self, node: NodeRef<'_>) {
        if let Some(let_statement) = node.of::<LetStatement>() {
            self.declare(let_statement.name.node.name.clone(), Binding::Owner);
        }
        if node.of::<Block>().is_some() || node.of::<MatchArm>().is_some() || node.of::<LambdaExpression>().is_some() {
            self.scopes.pop();
        }
        if node.of::<crate::syntax::ForStatement>().is_some() {
            self.for_iterators.pop();
        }
        self.kind_stack.pop();
    }
}

/// The names a pattern binds.
fn pattern_names(pattern: &Pattern, names: &mut Vec<String>) {
    match pattern {
        Pattern::Identifier(identifier) => names.push(identifier.node.name.clone()),
        Pattern::Enum(inner) => {
            for item in &inner.node.items {
                pattern_names(&item.node, names);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::AnalysisOptions;
    use crate::analysis::Severity;
    use crate::services::parse_program;

    fn discarded(source: &str) -> Vec<crate::analysis::diagnostics::SemanticDiagnostic> {
        let program = parse_program(source).expect("parse");
        let mut ctx = RuleContext::new("test.bd", source, AnalysisOptions::default());
        SemanticPipelineRule.check_discarded_growth(&mut ctx, &program);
        ctx.diagnostics.into_iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("W1240")).collect()
    }

    fn apply(source: &str, diagnostic: &crate::analysis::diagnostics::SemanticDiagnostic) -> String {
        let mut text = source.to_string();
        for edit in &diagnostic.fixes[0].edits {
            let (start, end) = edit.range();
            text.replace_range(start..end, &edit.replacement);
        }
        text
    }

    #[test]
    fn a_discarded_append_to_a_parameter_that_is_not_mut_warns_and_offers_mut() {
        let source = "unit Add(string[] xs, string n) { Array.Append<string>(xs, n); return; }";
        let found = discarded(source);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(
            found[0].message,
            "the result of `Array.Append` is discarded; the call works on a copy, so `xs` is not changed"
        );
        assert!(found[0].help.as_deref().is_some_and(|help| help.contains("declare the parameter `mut`")), "{found:?}");
        assert_eq!(
            apply(source, &found[0]),
            "unit Add(mut string[] xs, string n) { Array.Append<string>(xs, n); return; }"
        );
    }

    #[test]
    fn a_discarded_append_to_a_pattern_binding_offers_the_assignment() {
        let source = "unit Grow(Option<string[]> held) { match held { Option::Some(xs) => { Array.Append<string>(xs, \"n\"); }, Option::None => { }, }; return; }";
        let found = discarded(source);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0]
                .help
                .as_deref()
                .is_some_and(|help| help.contains("assign the result: `xs = Array.Append<string>(xs, \"n\");`")),
            "{found:?}"
        );
        assert!(apply(source, &found[0]).contains("{ xs = Array.Append<string>(xs, \"n\"); }"));
    }

    #[test]
    fn a_discarded_append_to_a_loop_variable_or_call_result_offers_a_new_local() {
        let source = "unit Grow(string[][] rows) { for row in rows { Array.Append<string>(row, \"n\"); } Array.Append<string>(Make(), \"n\"); return; }";
        let found = discarded(source);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(apply(source, &found[0]).contains("{ string[] grown = Array.Append<string>(row, \"n\"); }"));
    }

    #[test]
    fn appends_that_grow_an_owner_or_use_the_result_do_not_warn() {
        for source in [
            "unit A(mut string[] xs) { Array.Append<string>(xs, \"n\"); return; }",
            "string[] B() { string[] out = []; Array.Append<string>(out, \"n\"); return out; }",
            "string[] C(string[] xs) { return Array.Append<string>(xs, \"n\"); }",
            "pub type Holder { string[] items; } unit D(Holder h) { Array.Append<string>(h.items, \"n\"); return; }",
            "string[] E(string[] xs) { string[] ys = Array.Append<string>(xs, \"n\"); return ys; }",
        ] {
            assert!(discarded(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn a_discarded_remove_last_on_a_parameter_or_element_warns() {
        let source =
            "unit Trim(i64[] xs, i64[][] rows) { Array.RemoveLast<i64>(xs); Array.RemoveLast<i64>(rows[0]); return; }";
        let found = discarded(source);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|diagnostic| diagnostic.severity == Severity::Warning));
        assert_eq!(
            found[0].message,
            "the result of `Array.RemoveLast` is discarded; the call works on a copy, so `xs` is not changed"
        );
        assert!(found[0].help.as_deref().is_some_and(|help| help.contains("use the result")), "{found:?}");
        assert_eq!(
            found[1].message,
            "the result of `Array.RemoveLast` is discarded; the call works on a copy, so `rows[0]` is not changed"
        );
        assert!(apply(source, &found[1]).contains("i64[] shortened = Array.RemoveLast<i64>(rows[0]);"), "{found:?}");
    }

    #[test]
    fn remove_last_on_an_owner_or_with_its_result_used_does_not_warn() {
        for source in [
            "unit A(mut i64[] xs) { Array.RemoveLast<i64>(xs); return; }",
            "unit B(move i64[] xs) { Array.RemoveLast<i64>(xs); return; }",
            "i64[] C() { mut i64[] out = [1, 2]; Array.RemoveLast<i64>(out); return out; }",
            "i64[] D(i64[] xs) { return Array.RemoveLast<i64>(xs); }",
            "pub type Holder { i64[] items; } unit E(Holder h) { Array.RemoveLast<i64>(h.items); return; }",
            "i64[] F(i64[][] rows) { i64[] ys = Array.RemoveLast<i64>(rows[0]); return ys; }",
            "unit G(i64[] xs) { mut i64[] local = xs; local = Array.RemoveLast<i64>(local); return; }",
        ] {
            assert!(discarded(source).is_empty(), "{source}");
        }
    }
}
