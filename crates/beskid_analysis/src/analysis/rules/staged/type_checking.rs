use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::{ImmutableBinding, SemanticIssueKind, issue_diagnostic};
use crate::analysis::rules::RuleContext;
use crate::syntax::{Block, Expression, ForStatement, LetStatement, Parameter, Program};
use crate::syntax::{SpanInfo, Spanned};
use crate::syntax_query::{AstWalker, NodeKind, NodeRef, Visit};
use std::collections::HashMap;

impl SemanticPipelineRule {
    /// Structural immutability checks only; full type-check runs in the lower spine.
    pub(super) fn stage2_type_check(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        self.check_immutable_assignments(ctx, program);
    }

    /// E1214, one diagnostic per immutable binding that is assigned: anchored at the binding's
    /// name where the `mut` fix goes, with every assignment as a related location.
    fn check_immutable_assignments(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let mut writes = Vec::new();
        for item in &program.node.items {
            match &item.node {
                crate::syntax::Node::Function(definition) => {
                    walk_callable(&mut writes, Some(definition.node.parameters.as_slice()), NodeRef::from(&definition.node.body.node));
                }
                crate::syntax::Node::Method(definition) => {
                    walk_callable(&mut writes, Some(definition.node.parameters.as_slice()), NodeRef::from(&definition.node.body.node));
                }
                crate::syntax::Node::ExtendTypeDefinition(definition) => {
                    for method in &definition.node.methods {
                        walk_callable(&mut writes, Some(method.node.parameters.as_slice()), NodeRef::from(&method.node.body.node));
                    }
                }
                crate::syntax::Node::TestDefinition(definition) => {
                    // A test body is a statement list, not a `Block`, so no block scope opens for
                    // its top-level `let`s. Open the body scope explicitly; without it those
                    // bindings are never recorded and reassigning one escapes E1214.
                    let mut visitor = MutabilityVisitor::new(&mut writes);
                    visitor.scopes.push(HashMap::new());
                    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                    for statement in &definition.node.statements {
                        walker.walk(NodeRef::from(&statement.node));
                    }
                }
                _ => {}
            }
        }
        for write in writes.into_iter().filter(|write| !write.assignments.is_empty()) {
            let kind = SemanticIssueKind::ImmutableAssignment { name: write.name, binding: write.binding };
            let mut diagnostic = issue_diagnostic(ctx.source_name(), ctx.source(), write.name_span, &kind);
            for assignment in write.assignments {
                diagnostic = diagnostic.with_related(assignment, "assigned again");
            }
            ctx.emit(diagnostic);
        }
    }
}

fn walk_callable<'a>(writes: &mut Vec<ImmutableWrites>, parameters: Option<&'a [Spanned<Parameter>]>, body: NodeRef<'a>) {
    let mut visitor = MutabilityVisitor::new(writes);
    if let Some(parameters) = parameters {
        visitor.seed_parameters(parameters);
    }
    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
    walker.walk(body);
}

/// One immutable binding and every assignment to it.
struct ImmutableWrites {
    name: String,
    name_span: SpanInfo,
    binding: ImmutableBinding,
    assignments: Vec<SpanInfo>,
}

/// A name in scope: mutable, or the index of its immutable binding in the writes list.
type Mutability = Option<usize>;

struct MutabilityVisitor<'w> {
    writes: &'w mut Vec<ImmutableWrites>,
    scopes: Vec<HashMap<String, Mutability>>,
    kind_stack: Vec<NodeKind>,
    for_iterators: Vec<(String, SpanInfo)>,
    pending_parameters: Option<Vec<(String, Mutability)>>,
}

impl<'w> MutabilityVisitor<'w> {
    fn new(writes: &'w mut Vec<ImmutableWrites>) -> Self {
        Self { writes, scopes: Vec::new(), kind_stack: Vec::new(), for_iterators: Vec::new(), pending_parameters: None }
    }

    fn immutable(&mut self, name: &str, name_span: SpanInfo, binding: ImmutableBinding) -> Mutability {
        self.writes.push(ImmutableWrites { name: name.to_string(), name_span, binding, assignments: Vec::new() });
        Some(self.writes.len() - 1)
    }

    fn seed_parameters(&mut self, parameters: &[Spanned<Parameter>]) {
        // A `move` parameter is an owner the callable may rebind (growth, `buffer = Append(buffer, v)`);
        // the grammar admits only one modifier, so `move` alone makes the binding assignable.
        let seeded = parameters
            .iter()
            .map(|param| {
                let name = param.node.name.node.name.clone();
                let mutability = if param.node.mutable || param.node.movable {
                    None
                } else {
                    self.immutable(&name, param.node.name.span, ImmutableBinding::of_parameter(param))
                };
                (name, mutability)
            })
            .collect();
        self.pending_parameters = Some(seeded);
    }

    fn lookup(&self, name: &str) -> Option<Mutability> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name).copied())
    }
}

impl Visit for MutabilityVisitor<'_> {
    fn enter(&mut self, node: NodeRef<'_>) {
        let parent = self.kind_stack.last().copied();

        if let Some(for_statement) = node.of::<ForStatement>() {
            self.for_iterators.push((for_statement.iterator.node.name.clone(), for_statement.iterator.span));
        }

        if node.of::<Block>().is_some() {
            self.scopes.push(HashMap::new());
            if let Some(parameters) = self.pending_parameters.take()
                && let Some(scope) = self.scopes.last_mut()
            {
                scope.extend(parameters);
            }
            if parent == Some(NodeKind::ForStatement)
                && let Some((iterator_name, iterator_span)) = self.for_iterators.last().cloned()
            {
                let mutability =
                    self.immutable(&iterator_name, iterator_span, ImmutableBinding::of_for_iterator(iterator_span));
                if let Some(scope) = self.scopes.last_mut() {
                    scope.insert(iterator_name, mutability);
                }
            }
        }

        if let Some(expression) = node.of::<Expression>()
            && let Expression::Assign(assign_expression) = expression
            && let Expression::Path(path_expr) = &assign_expression.node.target.node
            && path_expr.node.path.node.segments.len() == 1
            && let Some(name) = path_expr.node.path.node.segments.first()
            && let Some(Some(binding)) = self.lookup(&name.node.name.node.name)
            && let Some(write) = self.writes.get_mut(binding)
        {
            write.assignments.push(assign_expression.node.target.span);
        }

        self.kind_stack.push(node.node_kind());
    }

    fn exit(&mut self, node: NodeRef<'_>) {
        if let Some(let_statement) = node.of::<LetStatement>() {
            let name = let_statement.name.node.name.clone();
            let mutability = if let_statement.mutable {
                None
            } else {
                let declaration = node.span().unwrap_or(let_statement.name.span);
                let binding = ImmutableBinding::of_let(let_statement, declaration);
                self.immutable(&name, let_statement.name.span, binding)
            };
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(name, mutability);
            }
        }

        if node.of::<Block>().is_some() {
            self.scopes.pop();
        }

        if node.of::<ForStatement>().is_some() {
            self.for_iterators.pop();
        }

        self.kind_stack.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::AnalysisOptions;
    use crate::services::parse_program;

    fn immutable_assignment_names(src: &str) -> Vec<String> {
        let program = parse_program(src).expect("parse");
        let mut ctx = RuleContext::new("test.bd", src, AnalysisOptions::default());
        SemanticPipelineRule.check_immutable_assignments(&mut ctx, &program);
        ctx.diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code.as_deref() == Some("E1214"))
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    fn assert_reports(src: &str, name: &str) {
        let messages = immutable_assignment_names(src);
        assert_eq!(messages.len(), 1, "expected one E1214 for `{name}`: {messages:?}\n{src}");
        assert!(messages[0].contains(name), "E1214 must name `{name}`: {messages:?}");
    }

    const PAIR: &str = "pub type Pair { i64 a }\n";

    #[test]
    fn function_top_level_reassignment_reports_e1214_for_each_type() {
        assert_reports("pub unit F(pointer p) { pointer q = p; q = p; return; }", "q");
        assert_reports("pub unit F() { i64 n = 0; n = 1; return; }", "n");
        assert_reports(&format!("{PAIR}pub unit F() {{ Pair v = Pair {{ a: 1 }}; v = Pair {{ a: 2 }}; return; }}"), "v");
    }

    #[test]
    fn function_while_reassignment_reports_e1214_for_each_type() {
        assert_reports("pub unit F(pointer p) { pointer q = p; while true { q = p; } return; }", "q");
        assert_reports("pub unit F() { i64 n = 0; while n < 3 { n = n + 1; } return; }", "n");
        assert_reports(
            &format!("{PAIR}pub unit F() {{ Pair v = Pair {{ a: 1 }}; while true {{ v = Pair {{ a: 2 }}; }} return; }}"),
            "v",
        );
    }

    #[test]
    fn function_if_reassignment_reports_e1214_for_each_type() {
        assert_reports("pub unit F(pointer p, bool c) { pointer q = p; if c { q = p; } return; }", "q");
        assert_reports("pub unit F(bool c) { i64 n = 0; if c { n = 1; } return; }", "n");
        assert_reports(
            &format!("{PAIR}pub unit F(bool c) {{ Pair v = Pair {{ a: 1 }}; if c {{ v = Pair {{ a: 2 }}; }} return; }}"),
            "v",
        );
    }

    #[test]
    fn function_for_reassignment_reports_e1214_for_each_type() {
        assert_reports("pub unit F(pointer p) { pointer q = p; for i in range(0, 4) { q = p; } return; }", "q");
        assert_reports("pub unit F() { i64 n = 0; for i in range(0, 4) { n = 1; } return; }", "n");
        assert_reports(
            &format!(
                "{PAIR}pub unit F() {{ Pair v = Pair {{ a: 1 }}; for i in range(0, 4) {{ v = Pair {{ a: 2 }}; }} return; }}"
            ),
            "v",
        );
    }

    #[test]
    fn test_body_reassignment_reports_e1214_at_top_level_and_in_nested_control_flow() {
        assert_reports("test t { i64 n = 0; n = 1; }", "n");
        assert_reports("test t { pointer q = NativePointer(0); while true { q = NativePointer(0); } }", "q");
        assert_reports("test t { i64 n = 0; if true { n = 1; } }", "n");
        assert_reports("test t { i64 n = 0; for i in range(0, 4) { n = 1; } }", "n");
        assert_reports(&format!("{PAIR}test t {{ Pair v = Pair {{ a: 1 }}; while true {{ v = Pair {{ a: 2 }}; }} }}"), "v");
    }

    #[test]
    fn mutable_locals_and_parameters_may_be_reassigned() {
        let src = format!(
            "{PAIR}pub unit F(mut i64 m, pointer p) {{ mut pointer q = p; mut Pair v = Pair {{ a: 1 }}; \
             while true {{ q = p; v = Pair {{ a: 2 }}; m = 1; }} return; }}\n\
             test t {{ mut i64 n = 0; while n < 3 {{ n = n + 1; }} }}"
        );
        assert!(immutable_assignment_names(&src).is_empty());
    }

    /// A `move` parameter is a rebindable owner (v0.6.2 growth: `out = Append(out, v)`).
    #[test]
    fn move_parameters_may_be_reassigned() {
        let src = "u8[] Grow(move u8[] out) { out = out; while true { out = out; } return out; }";
        assert!(immutable_assignment_names(src).is_empty(), "{:?}", immutable_assignment_names(src));
    }

    #[test]
    fn repeated_assignments_are_one_diagnostic_at_the_declaration() {
        let src = "pub unit F() {\n    i32 count = 0;\n    count = count + 1;\n    count = 2;\n    return;\n}";
        let program = parse_program(src).expect("parse");
        let mut ctx = RuleContext::new("test.bd", src, AnalysisOptions::default());
        SemanticPipelineRule.check_immutable_assignments(&mut ctx, &program);
        let diagnostics: Vec<_> =
            ctx.diagnostics.into_iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("E1214")).collect();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let diagnostic = &diagnostics[0];
        let (start, end) = diagnostic.range();
        assert_eq!(&src[start..end], "count");
        assert_eq!(start, src.find("count").unwrap(), "anchored at the declaration");
        assert_eq!(diagnostic.message, "`count` is assigned again but is not mutable");
        assert_eq!(diagnostic.help.as_deref(), Some("write `mut i32 count = 0;`"));
        assert_eq!(diagnostic.related.len(), 2);
        assert!(diagnostic.related.iter().all(|related| related.label == "assigned again"));
        let fix = &diagnostic.fixes[0];
        assert!(fix.is_preferred());
        assert_eq!(fix.edits[0].range(), (src.find("i32 count").unwrap(), src.find("i32 count").unwrap()));
        assert_eq!(fix.edits[0].replacement, "mut ");
    }

    #[test]
    fn immutable_parameter_reassignment_reports_e1214() {
        assert_reports("pub unit F(i64 n) { while true { n = 1; } return; }", "n");
    }
}
