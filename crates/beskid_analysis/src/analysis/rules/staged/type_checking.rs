use super::SemanticPipelineRule;
use crate::analysis::rules::RuleContext;
use crate::syntax::Spanned;
use crate::syntax::{Block, Expression, ForStatement, LetStatement, Parameter, Program};
use crate::syntax_query::{AstWalker, NodeKind, NodeRef, Visit};
use std::collections::HashMap;

impl SemanticPipelineRule {
    /// Structural immutability checks only; full type-check runs in the lower spine.
    pub(super) fn stage2_type_check(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        self.check_immutable_assignments(ctx, program);
    }

    fn check_immutable_assignments(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        for item in &program.node.items {
            match &item.node {
                crate::syntax::Node::Function(definition) => {
                    let mut visitor = MutabilityVisitor::new(ctx);
                    visitor.seed_parameters(&definition.node.parameters);
                    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                    walker.walk(NodeRef::from(&definition.node.body.node));
                }
                crate::syntax::Node::Method(definition) => {
                    let mut visitor = MutabilityVisitor::new(ctx);
                    visitor.seed_parameters(&definition.node.parameters);
                    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                    walker.walk(NodeRef::from(&definition.node.body.node));
                }
                crate::syntax::Node::ExtendTypeDefinition(definition) => {
                    for method in &definition.node.methods {
                        let mut visitor = MutabilityVisitor::new(ctx);
                        visitor.seed_parameters(&method.node.parameters);
                        let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                        walker.walk(NodeRef::from(&method.node.body.node));
                    }
                }
                crate::syntax::Node::TestDefinition(definition) => {
                    // A test body is a statement list, not a `Block`, so no block scope opens for
                    // its top-level `let`s. Open the body scope explicitly; without it those
                    // bindings are never recorded and reassigning one escapes E1214.
                    let mut visitor = MutabilityVisitor::new(ctx);
                    visitor.scopes.push(HashMap::new());
                    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                    for statement in &definition.node.statements {
                        walker.walk(NodeRef::from(&statement.node));
                    }
                }
                _ => {}
            }
        }
    }
}

struct MutabilityVisitor<'a> {
    ctx: &'a mut RuleContext,
    scopes: Vec<HashMap<String, bool>>,
    kind_stack: Vec<NodeKind>,
    for_iterators: Vec<String>,
    pending_parameters: Option<HashMap<String, bool>>,
}

impl<'a> MutabilityVisitor<'a> {
    fn new(ctx: &'a mut RuleContext) -> Self {
        Self { ctx, scopes: Vec::new(), kind_stack: Vec::new(), for_iterators: Vec::new(), pending_parameters: None }
    }

    fn seed_parameters(&mut self, parameters: &[Spanned<Parameter>]) {
        self.pending_parameters =
            Some(parameters.iter().map(|param| (param.node.name.node.name.clone(), param.node.mutable)).collect());
    }

    fn lookup_mutability(&self, name: &str) -> Option<bool> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(*value);
            }
        }
        None
    }
}

impl Visit for MutabilityVisitor<'_> {
    fn enter(&mut self, node: NodeRef<'_>) {
        let parent = self.kind_stack.last().copied();

        if let Some(for_statement) = node.of::<ForStatement>() {
            self.for_iterators.push(for_statement.iterator.node.name.clone());
        }

        if node.of::<Block>().is_some() {
            self.scopes.push(HashMap::new());
            if let Some(parameters) = self.pending_parameters.take()
                && let Some(scope) = self.scopes.last_mut()
            {
                scope.extend(parameters);
            }
            if parent == Some(NodeKind::ForStatement)
                && let Some(iterator_name) = self.for_iterators.last().cloned()
                && let Some(scope) = self.scopes.last_mut()
            {
                scope.insert(iterator_name, false);
            }
        }

        if let Some(expression) = node.of::<Expression>()
            && let Expression::Assign(assign_expression) = expression
            && let Expression::Path(path_expr) = &assign_expression.node.target.node
            && path_expr.node.path.node.segments.len() == 1
            && let Some(name) = path_expr.node.path.node.segments.first()
        {
            let name_value = &name.node.name.node.name;
            if let Some(is_mutable) = self.lookup_mutability(name_value)
                && !is_mutable
            {
                self.ctx.emit_issue(
                    assign_expression.node.target.span,
                    crate::analysis::diagnostic_kinds::SemanticIssueKind::ImmutableAssignment {
                        name: name_value.clone(),
                    },
                );
            }
        }

        self.kind_stack.push(node.node_kind());
    }

    fn exit(&mut self, node: NodeRef<'_>) {
        if let Some(let_statement) = node.of::<LetStatement>()
            && let Some(scope) = self.scopes.last_mut()
        {
            scope.insert(let_statement.name.node.name.clone(), let_statement.mutable);
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

    #[test]
    fn immutable_parameter_reassignment_reports_e1214() {
        assert_reports("pub unit F(i64 n) { while true { n = 1; } return; }", "n");
    }
}
