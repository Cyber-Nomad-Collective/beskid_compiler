use super::SemanticPipelineRule;
use crate::analysis::rules::RuleContext;
use crate::syntax::{Block, Expression, ForStatement, LetStatement, Parameter, Program};
use crate::syntax::Spanned;
use crate::syntax_query::{NodeKind, NodeRef, Visit, AstWalker};
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
                    let mut walker = AstWalker::new().with_visitor(Box::new(MutabilityVisitor::new(ctx)));
                    walker.walk(NodeRef::from(&definition.node.body.node));
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
