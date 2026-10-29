//! Dead assignment to a mutable parameter (E1259).
//!
//! Parameters pass by value (`language-meta--memory-model--memory-and-references`, "Mutable
//! parameters pass by value"). An assignment to a `mut` parameter itself (`p = v;`, `p += v;`)
//! changes only the callee's binding; an assignment through it (`p.f = v`, `p[i] = v`) writes the
//! object the caller also references. A whole assignment that no later read of the parameter can
//! observe therefore has no effect at all, and the gate rejects it instead of compiling a silent
//! no-op.
//!
//! The rule is a backward liveness walk over the structured body, one parameter at a time:
//!
//! * the parameter is dead at the end of the body and after `return`;
//! * after `break` it is live as after its loop, after `continue` as at the loop head;
//! * a `while` or `for` head is solved by fixpoint (a read in the next iteration keeps an
//!   assignment at the end of the body live);
//! * `if`/`else` and `match` join their branches (a `match` can also end without an arm);
//! * every occurrence of the parameter name other than the target of a judged whole assignment
//!   is a read, also inside a lambda, a nested assignment or a field or element place.
//!
//! The walk never reports when it cannot see every read: a parameter whose name is declared again
//! in the body, or occurs in a `${ ... }` interpolation, is not judged, and a body with a macro
//! invocation, a CLIF block or a code string is not judged at all.

use super::*;
use beskid_analysis::syntax::{
    AstNodeId, Block, ElseBranch, Expression, FunctionDefinition, IfStatement, Literal, MatchExpression,
    MethodDefinition, Parameter, Spanned, Statement,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

/// One whole assignment to a `mut` parameter whose value no later read observes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DeadParameterAssignment {
    /// The assignment expression (the diagnostic site).
    pub assignment: AstNodeKey,
    /// The parameter's name in its declaration.
    pub declaration: AstNodeKey,
    pub parameter: String,
}

/// Every dead whole assignment to a `mut` parameter of the function or method `key`, in
/// parameter order and then source order; `None` when there is none or the item is no callable.
pub fn dead_parameter_assignments(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Vec<DeadParameterAssignment>> {
    with_registered_syntax(db, key, dead_parameter_assignments_tracked)
}

#[salsa::tracked(persist)]
fn dead_parameter_assignments_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Vec<DeadParameterAssignment>> {
    with_node(db, syntax, key, |program, index, node| {
        let (parameters, body) = if let Some(function) = node.of::<FunctionDefinition>() {
            (&function.parameters, &function.body)
        } else if let Some(method) = node.of::<MethodDefinition>() {
            (&method.parameters, &method.body)
        } else {
            return None;
        };
        if !parameters.iter().any(|parameter| parameter.node.mutable) {
            return None;
        }
        let body_id = index.direct_child_id(program, key.node, DynNodeRef::from(body))?;
        let opaque = [
            NodeKind::MacroInvocation,
            NodeKind::MacroMetavariable,
            NodeKind::ClifBlockExpression,
            NodeKind::CodeStringLiteral,
        ]
        .into_iter()
        .any(|kind| index.ids_of_kind(kind).any(|node| is_ancestor(index, body_id, node)));
        if opaque {
            return None;
        }
        let mut paths = Vec::new();
        collect_nodes_of_kind(index, body_id, NodeKind::PathExpression, &mut paths);
        let mut literals = Vec::new();
        collect_nodes_of_kind(index, body_id, NodeKind::LiteralExpression, &mut literals);
        let mut found = Vec::new();
        for parameter in parameters.iter().filter(|parameter| parameter.node.mutable) {
            let Some(declaration) = parameter_declaration(program, index, key.node, parameter) else { continue };
            let name = parameter.node.name.node.name.as_str();
            if interpolates(program, index, &literals, name) {
                continue;
            }
            let Some(reads) = parameter_reads(program, index, &paths, declaration, name) else { continue };
            let mut walker = Walker { program, index, reads: &reads, loops: Vec::new(), dead: Vec::new() };
            walker.block(Some(body_id), &body.node, false);
            walker.dead.sort_unstable();
            walker.dead.dedup();
            found.extend(walker.dead.into_iter().map(|assignment| DeadParameterAssignment {
                assignment: AstNodeKey { node: assignment, ..key },
                declaration: AstNodeKey { node: declaration, ..key },
                parameter: name.to_string(),
            }));
        }
        (!found.is_empty()).then_some(found)
    })
}

/// The identifier node that declares `parameter` under the callable `callable`.
fn parameter_declaration(
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    callable: AstNodeId,
    parameter: &Spanned<Parameter>,
) -> Option<AstNodeId> {
    let parameter_id = index.direct_child_id(program, callable, DynNodeRef::from(parameter))?;
    index.direct_child_id(program, parameter_id, DynNodeRef::from(&parameter.node.name))
}

/// Whether a string literal of the body interpolates text that contains `name`. The interpolated
/// source is not part of the syntax tree, so such a parameter is not judged.
fn interpolates(
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    literals: &[AstNodeId],
    name: &str,
) -> bool {
    literals.iter().any(|literal| {
        index
            .node_at(program, *literal)
            .and_then(|node| node.of::<beskid_analysis::syntax::LiteralExpression>())
            .is_some_and(|literal| match &literal.literal.node {
                Literal::String(text) => text.contains("${") && text.contains(name),
                _ => false,
            })
    })
}

/// Every path expression of the body whose first segment names the parameter. `None` when one of
/// them resolves to another declaration (the name is declared again in the body), so the
/// parameter is not judged.
fn parameter_reads(
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    paths: &[AstNodeId],
    declaration: AstNodeId,
    name: &str,
) -> Option<Vec<AstNodeId>> {
    let mut reads = Vec::new();
    for &path_node in paths {
        let Some(path) =
            index.node_at(program, path_node).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
        else {
            continue;
        };
        if path.path.node.segments.first().is_none_or(|segment| segment.node.name.node.name != name) {
            continue;
        }
        if resolve_lexical_declaration(program, index, path_node, name) != Some(declaration) {
            return None;
        }
        reads.push(path_node);
    }
    Some(reads)
}

/// Liveness after `break` and `continue` of the innermost loop.
#[derive(Debug, Clone, Copy)]
struct Loop {
    after: bool,
    head: bool,
}

struct Walker<'a> {
    program: &'a Spanned<beskid_analysis::syntax::Program>,
    index: &'a SyntaxIndex,
    /// Every occurrence of the parameter in the body, also the targets of whole assignments.
    reads: &'a [AstNodeId],
    loops: Vec<Loop>,
    /// Whole assignments after which the parameter is dead.
    dead: Vec<AstNodeId>,
}

impl Walker<'_> {
    fn child(&self, parent: Option<AstNodeId>, child: DynNodeRef<'_>) -> Option<AstNodeId> {
        self.index.direct_child_id(self.program, parent?, child)
    }

    /// Whether the subtree at `id` holds an occurrence of the parameter. A node without an id is
    /// unknown and counts as a read (the safe direction: nothing before it is reported).
    fn reads(&self, id: Option<AstNodeId>) -> bool {
        let Some(id) = id else { return true };
        self.reads.iter().any(|read| is_ancestor(self.index, id, *read))
    }

    /// Liveness before `block`, given the liveness `after` it.
    fn block(&mut self, id: Option<AstNodeId>, block: &Block, after: bool) -> bool {
        let mut live = after;
        for statement in block.statements.iter().rev() {
            let statement_id = self.child(id, DynNodeRef::from(statement));
            live = self.statement(statement_id, &statement.node, live);
        }
        live
    }

    fn statement(&mut self, id: Option<AstNodeId>, statement: &Statement, after: bool) -> bool {
        match statement {
            Statement::Return(_) => self.reads(id),
            Statement::Break(_) => self.loops.last().is_none_or(|innermost| innermost.after),
            Statement::Continue(_) => self.loops.last().is_none_or(|innermost| innermost.head),
            Statement::Let(_) | Statement::Launch(_) => after || self.reads(id),
            Statement::While(while_statement) => {
                let while_id = self.child(id, DynNodeRef::from(while_statement));
                let condition = self.reads(self.child(while_id, DynNodeRef::from(&while_statement.node.condition)));
                let body = &while_statement.node.body;
                let body_id = self.child(while_id, DynNodeRef::from(body));
                self.repeat(body_id, &body.node, after, condition)
            }
            Statement::For(for_statement) => {
                let for_id = self.child(id, DynNodeRef::from(for_statement));
                let iterable = self.reads(self.child(for_id, DynNodeRef::from(&for_statement.node.iterable)));
                let body = &for_statement.node.body;
                let body_id = self.child(for_id, DynNodeRef::from(body));
                self.repeat(body_id, &body.node, after, false) || iterable
            }
            Statement::If(if_statement) => {
                self.if_statement(self.child(id, DynNodeRef::from(if_statement)), if_statement, after)
            }
            Statement::With(with_statement) => {
                let with_id = self.child(id, DynNodeRef::from(with_statement));
                let arguments = with_statement
                    .node
                    .arguments
                    .iter()
                    .any(|argument| self.reads(self.child(with_id, DynNodeRef::from(argument))));
                let body = &with_statement.node.body;
                let body_id = self.child(with_id, DynNodeRef::from(body));
                self.block(body_id, &body.node, after) || arguments
            }
            Statement::Use(scoped) => {
                let use_id = self.child(id, DynNodeRef::from(scoped));
                let binding = self.reads(self.child(use_id, DynNodeRef::from(&scoped.node.binding)));
                let body = match &scoped.node.body {
                    Some(body) => {
                        let body_id = self.child(use_id, DynNodeRef::from(body));
                        self.block(body_id, &body.node, after)
                    }
                    None => after,
                };
                body || binding
            }
            Statement::Expression(expression_statement) => {
                let statement_id = self.child(id, DynNodeRef::from(expression_statement));
                let expression = &expression_statement.node.expression;
                self.expression(self.child(statement_id, DynNodeRef::from(expression)), after)
            }
        }
    }

    /// A loop whose head reads the parameter when `condition` is set. The head liveness is the
    /// least fixpoint of `condition || after || live-before(body, head)`; findings of a pass whose
    /// head was still growing are discarded.
    fn repeat(&mut self, body_id: Option<AstNodeId>, body: &Block, after: bool, condition: bool) -> bool {
        let mut head = condition || after;
        loop {
            let mark = self.dead.len();
            self.loops.push(Loop { after, head });
            let entry = self.block(body_id, body, head);
            self.loops.pop();
            let next = head || entry;
            if next == head {
                return head;
            }
            self.dead.truncate(mark);
            head = next;
        }
    }

    fn if_statement(&mut self, id: Option<AstNodeId>, if_statement: &Spanned<IfStatement>, after: bool) -> bool {
        let condition = self.reads(self.child(id, DynNodeRef::from(&if_statement.node.condition)));
        let then_block = &if_statement.node.then_block;
        let then_id = self.child(id, DynNodeRef::from(then_block));
        let then_live = self.block(then_id, &then_block.node, after);
        let else_live = match &if_statement.node.else_branch {
            None => after,
            Some(else_branch) => {
                let else_id = self.child(id, DynNodeRef::from(else_branch));
                match &else_branch.node {
                    ElseBranch::If(nested) => {
                        self.if_statement(self.child(else_id, DynNodeRef::from(nested.as_ref())), nested, after)
                    }
                    ElseBranch::Block(block) => {
                        self.block(self.child(else_id, DynNodeRef::from(block)), &block.node, after)
                    }
                }
            }
        };
        condition || then_live || else_live
    }

    /// An expression in statement position (an expression statement or a match arm body).
    fn expression(&mut self, id: Option<AstNodeId>, after: bool) -> bool {
        let Some(id) = id else { return true };
        let id = normalized_expression_node(self.index, id);
        let Some(node) = self.index.node_at(self.program, id) else { return true };
        if let Some(assign) = node.of::<beskid_analysis::syntax::AssignExpression>()
            && let Some(target) = self.whole_target(id, assign)
        {
            if !after {
                self.dead.push(id);
            }
            let compound = !matches!(assign.op.node, beskid_analysis::syntax::AssignOp::Assign);
            let value = self.child(Some(id), DynNodeRef::from(assign.value.as_ref()));
            debug_assert!(self.reads.contains(&target));
            return compound || self.reads(value);
        }
        if let Some(block) = node.of::<beskid_analysis::syntax::BlockExpression>() {
            let block_id = self.child(Some(id), DynNodeRef::from(&block.block));
            return self.block(block_id, &block.block.node, after);
        }
        if let Some(match_expression) = node.of::<MatchExpression>() {
            return self.match_expression(id, match_expression, after);
        }
        after || self.reads(Some(id))
    }

    /// The target path of `assign` when it is the parameter itself (one segment), else `None`.
    fn whole_target(&self, id: AstNodeId, assign: &beskid_analysis::syntax::AssignExpression) -> Option<AstNodeId> {
        let mut target = &assign.target.node;
        while let Expression::Grouped(inner) = target {
            target = &inner.node.expr.node;
        }
        let Expression::Path(path) = target else { return None };
        if path.node.path.node.segments.len() != 1 {
            return None;
        }
        let target_id = self.child(Some(id), DynNodeRef::from(assign.target.as_ref()))?;
        let target_id = normalized_expression_node(self.index, target_id);
        self.reads.contains(&target_id).then_some(target_id)
    }

    fn match_expression(&mut self, id: AstNodeId, match_expression: &MatchExpression, after: bool) -> bool {
        let scrutinee = self.reads(self.child(Some(id), DynNodeRef::from(match_expression.scrutinee.as_ref())));
        let mut live = scrutinee || after;
        for arm in &match_expression.arms {
            let arm_id = self.child(Some(id), DynNodeRef::from(arm));
            if let Some(guard) = &arm.node.guard {
                live |= self.reads(self.child(arm_id, DynNodeRef::from(guard)));
            }
            live |= self.expression(self.child(arm_id, DynNodeRef::from(&arm.node.value)), after);
        }
        live
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::one_unit_assembly;
    use super::*;
    use crate::{IndexedNodeKind, child_nodes, node_kind};
    use beskid_analysis::syntax::SyntaxGenerationId;

    fn callables(db: &dyn Db, key: AstNodeKey, found: &mut Vec<AstNodeKey>) {
        if matches!(
            node_kind(db, key).expect("node kind"),
            Some(IndexedNodeKind::FunctionDefinition | IndexedNodeKind::MethodDefinition)
        ) {
            found.push(key);
        }
        if let Some(children) = child_nodes(db, key).expect("child nodes") {
            for child in children.iter().copied() {
                callables(db, child, found);
            }
        }
    }

    /// The source text of every dead parameter assignment, and a check that the gate reports
    /// each one once as E1259.
    fn dead(source: &str, generation: u64) -> Vec<String> {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        let mut items = Vec::new();
        callables(&db, root, &mut items);
        let mut texts = Vec::new();
        for &item in &items {
            for finding in
                dead_parameter_assignments(&db, item).expect("dead_parameter_assignments").unwrap_or_default()
            {
                let span = crate::node_span(&db, finding.assignment).expect("span").expect("span");
                texts.push(source[span.start..span.end].to_string());
            }
        }
        let gate = super::super::check_items(&db, &items)
            .err()
            .unwrap_or_default()
            .into_iter()
            .filter(|finding| finding.kind.code() == "E1259")
            .count();
        assert_eq!(gate, texts.len(), "the legality gate reports every dead assignment once: {source}");
        texts
    }

    const SIZE: &str = "type Size { i32 columns; i32 rows; } ";

    #[test]
    fn an_assignment_no_read_can_observe_is_rejected() {
        let cases = [
            (format!("{SIZE}unit AssignWhole(mut Size s) {{ s = Size {{ columns: 80, rows: 24, }}; }}"), "a struct"),
            ("unit Bump(mut i64 value) { value = value + 1; }".to_string(), "a primitive"),
            ("unit Bump(mut i64 value) { value += 1; }".to_string(), "a compound assignment"),
            ("enum Color { Red(), Green() } unit Paint(mut Color c) { c = Color::Green(); }".to_string(), "an enum"),
            (
                format!(
                    "{SIZE}unit Poll(mut Size last, Size now) {{ if (now.columns != last.columns) {{ last = now; }} }}"
                ),
                "the PollResize shape: read before, assigned last in a branch",
            ),
            ("i64 F(mut i64 x) { x = 1; return 0; }".to_string(), "dead before a return that does not read it"),
            ("i64 F(mut i64 x) { x = 1; x = 2; return x; }".to_string(), "overwritten before the read"),
            ("unit F(mut i64 x, bool c) { while (c) { x = 1; break; } }".to_string(), "dead after a break"),
            (
                "i64 F(mut i64 x, i64 y) { match y { 1 => { x = 5; }, _ => { }, }; return y; }".to_string(),
                "in a match arm",
            ),
            ("unit F(mut i64 x) { { x = 3; }; }".to_string(), "in a nested block"),
            (
                "type Box { i64 v; } impl Box { pub unit Set(mut i64 x) { this.v = x; x = 0; } }".to_string(),
                "a method parameter",
            ),
        ];
        for (index, (source, case)) in cases.iter().enumerate() {
            let texts = dead(source, 9910 + index as u64);
            assert_eq!(texts.len(), 1, "{case} must be rejected once, got {texts:?}: {source}");
        }
    }

    #[test]
    fn an_assignment_a_read_can_observe_is_accepted() {
        let cases = [
            (format!("{SIZE}unit AssignFields(mut Size s) {{ s.columns = 80; s.rows = 24; }}"), "field writes"),
            ("i64 Clamp(mut i64 v) { if (v < 0) { v = 0; } return v; }".to_string(), "read by the return"),
            (
                "i64 Digits(mut u64 value) { mut i64 count = 0; while (value > 0_u64) { value = value / 10_u64; count = count + 1; } return count; }".to_string(),
                "read by the next loop condition",
            ),
            (
                "u64 Sum(mut u64 value, i64 n) { mut u64 total = 0_u64; mut i64 i = 0; while (i < n) { total = total + value; value = value + 1_u64; i = i + 1; } return total; }".to_string(),
                "read earlier in the next iteration",
            ),
            (
                "i64 F(mut i64 x, i64[] xs) { for y in xs { x = x + y; } return x; }".to_string(),
                "a for loop accumulator",
            ),
            (
                "i64 F(mut i64 x, bool c) { while (c) { if (x > 3) { break; } x = x + 1; } return x; }".to_string(),
                "read after the loop",
            ),
            (
                "i64 F(mut i64 x) { x = 1; i64 y = x; return y; }".to_string(),
                "read by a later binding",
            ),
            (
                format!("{SIZE}unit Pass(mut Size s) {{ s = Size {{ columns: 1, rows: 1, }}; Use(s); }} unit Use(Size s) {{ }}"),
                "passed on after the assignment",
            ),
            ("unit F(i64 x) { mut i64 y = x; y = 2; }".to_string(), "a local binding is not judged"),
            (
                "i64 F(mut i64 x) { x = 2; let f = () => x; return 0; }".to_string(),
                "a lambda occurrence is a read",
            ),
            (
                "i64 F(mut i64 x, i64[] xs) { mut i64 t = 0; for x in xs { t = x; } x = 1; return t; }".to_string(),
                "a name declared again is not judged",
            ),
            (
                "string F(mut i64 x) { x = 2; return \"${x}\"; }".to_string(),
                "an interpolated name is not judged",
            ),
            (
                "i64 F(mut i64 x, bool c) { while (c) { x = 1; continue; } return x; }".to_string(),
                "read after continue and the loop",
            ),
        ];
        for (index, (source, case)) in cases.iter().enumerate() {
            let texts = dead(source, 9940 + index as u64);
            assert!(texts.is_empty(), "{case} must be accepted, got {texts:?}: {source}");
        }
    }

    #[test]
    fn the_finding_points_at_the_assignment_and_notes_the_parameter() {
        let source = "type Size { i32 columns; i32 rows; }\nunit AssignWhole(mut Size s)\n{\n    s = Size { columns: 80, rows: 24, };\n}\n";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(9970));
        let mut items = Vec::new();
        callables(&db, root, &mut items);
        let findings = super::super::check_items(&db, &items).expect_err("the gate rejects AssignWhole");
        let finding = findings.iter().find(|finding| finding.kind.code() == "E1259").expect("E1259");
        assert_eq!(finding.kind.message(), "the value assigned to the `mut` parameter `s` is never read");
        let span = finding.site_span(&db).expect("site span");
        assert_eq!(&source[span.start..span.end], "s = Size { columns: 80, rows: 24, }");
        assert_eq!(finding.related.len(), 1);
        assert_eq!(finding.related[0].1, "`s` is passed by value");
        let name = crate::node_span(&db, finding.related[0].0).expect("name span").expect("name span");
        assert_eq!(&source[name.start..name.end], "s");
    }
}
