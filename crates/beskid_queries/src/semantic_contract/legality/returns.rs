//! Definite return (E1258): a function or method whose declared result is not `unit` or `never`
//! must not reach the end of its body.
//!
//! The rule decides, statement by statement, whether execution can complete a statement normally
//! and continue with the next one. The end of the body is reachable when every statement of the
//! body can complete normally. The rule is the one lowering implements, so a body this check
//! accepts always ends its final block (`beskid_isle::emitter`, E2103 otherwise):
//!
//! * `return`, `break` and `continue` never complete normally. A reachable `break` makes the end
//!   of its innermost loop reachable.
//! * `let`, `launch` and `for` complete normally (a `for` can run zero times).
//! * `while` completes normally unless its condition is the literal `true` (also in grouping
//!   parentheses, as `beskid fmt` writes it) and its body has no reachable `break` for that loop.
//!   No other condition is evaluated at compile time.
//! * `if` without `else` completes normally, whatever its condition. `if`/`else` completes
//!   normally when one of its branches does.
//! * `with` and a scoped `use` complete normally when their body does.
//! * An expression statement completes normally unless its expression is a block that does not
//!   complete normally, a `match` that cannot fall through, or a call whose proven result is
//!   `never`. A `match` cannot fall through when every arm body does not complete normally and
//!   the match cannot end without taking an arm: a match with an enum pattern (its variants are
//!   exhaustive, E1304), or a match with an unguarded `_` or binding arm. The patterns decide
//!   this, so a generic body whose match facts need a specialization is judged the same way.
//!
//! Statements after one that does not complete normally are unreachable and are not judged here.

use super::*;
use beskid_analysis::syntax::{
    AstNodeId, Block, ElseBranch, Expression, FunctionDefinition, IfStatement, Literal, MatchExpression,
    MethodDefinition, Pattern, PrimitiveType, Spanned, Statement, Type,
};
use beskid_analysis::syntax_query::{DynNodeRef, SyntaxIndex};

/// A function or method whose body can end without a value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MissingReturn {
    /// The body block; diagnostics point at its closing brace.
    pub body: AstNodeKey,
    /// The declared name of the callable.
    pub name: AstNodeKey,
    pub callable: String,
    pub result_name: String,
}

/// The missing-return fact of `key` (a function or method item); `None` when the item is not a
/// valued callable or every path of its body ends without reaching its end.
pub fn missing_return(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<MissingReturn> {
    with_registered_syntax(db, key, missing_return_tracked)
}

#[salsa::tracked(persist)]
fn missing_return_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<MissingReturn> {
    with_node(db, syntax, key, |program, index, node| {
        let (name, result, body) = if let Some(function) = node.of::<FunctionDefinition>() {
            (&function.name, function.return_type.as_ref()?, &function.body)
        } else if let Some(method) = node.of::<MethodDefinition>() {
            (&method.name, method.return_type.as_ref()?, &method.body)
        } else {
            return None;
        };
        if matches!(result.node, Type::Primitive(Spanned { node: PrimitiveType::Unit | PrimitiveType::Never, .. })) {
            return None;
        }
        let body_id = index.direct_child_id(program, key.node, DynNodeRef::from(body))?;
        let name_id = index.direct_child_id(program, key.node, DynNodeRef::from(name))?;
        let walker = Walker { db, program, index, key };
        if !walker.block(Some(body_id), &body.node).completes {
            return None;
        }
        Some(MissingReturn {
            body: AstNodeKey { node: body_id, ..key },
            name: AstNodeKey { node: name_id, ..key },
            callable: name.node.name.clone(),
            result_name: type_text(result),
        })
    })
}

/// The declared result as written, with type arguments (`Result<i32, Error>`, `i64[]`).
fn type_text(ty: &Spanned<Type>) -> String {
    match &ty.node {
        Type::Complex(path) => path
            .node
            .segments
            .iter()
            .map(|segment| {
                let name = segment.node.name.node.name.clone();
                if segment.node.type_args.is_empty() {
                    name
                } else {
                    let arguments = segment.node.type_args.iter().map(type_text).collect::<Vec<_>>().join(", ");
                    format!("{name}<{arguments}>")
                }
            })
            .collect::<Vec<_>>()
            .join("."),
        Type::Array(inner) => format!("{}[]", type_text(inner)),
        _ => beskid_analysis::doc::format_type(ty),
    }
}

/// Whether a statement (or a sequence) can complete normally, and whether it holds a reachable
/// `break` for the innermost enclosing loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Flow {
    completes: bool,
    breaks: bool,
}

impl Flow {
    const CONTINUES: Self = Self { completes: true, breaks: false };
    const STOPS: Self = Self { completes: false, breaks: false };
}

struct Walker<'a> {
    db: &'a dyn Db,
    program: &'a Spanned<beskid_analysis::syntax::Program>,
    index: &'a SyntaxIndex,
    key: AstNodeKey,
}

impl Walker<'_> {
    /// The node id of `child` under `parent`. Ids are needed only to read typing and match
    /// facts; a lookup that fails leaves the statement completing normally (the safe direction:
    /// the end of the body stays reachable).
    fn child(&self, parent: Option<AstNodeId>, child: DynNodeRef<'_>) -> Option<AstNodeId> {
        self.index.direct_child_id(self.program, parent?, child)
    }

    fn block(&self, id: Option<AstNodeId>, block: &Block) -> Flow {
        let mut flow = Flow::CONTINUES;
        for statement in &block.statements {
            if !flow.completes {
                break;
            }
            let statement_id = self.child(id, DynNodeRef::from(statement));
            let next = self.statement(statement_id, &statement.node);
            flow = Flow { completes: next.completes, breaks: flow.breaks || next.breaks };
        }
        flow
    }

    fn statement(&self, id: Option<AstNodeId>, statement: &Statement) -> Flow {
        match statement {
            Statement::Return(_) | Statement::Continue(_) => Flow::STOPS,
            Statement::Break(_) => Flow { completes: false, breaks: true },
            Statement::Let(_) | Statement::Launch(_) | Statement::For(_) => Flow::CONTINUES,
            Statement::While(while_statement) => {
                let while_id = self.child(id, DynNodeRef::from(while_statement));
                let body = &while_statement.node.body;
                let body_flow = self.block(self.child(while_id, DynNodeRef::from(body)), &body.node);
                let infinite = is_literal_true(&while_statement.node.condition) && !body_flow.breaks;
                if infinite { Flow::STOPS } else { Flow::CONTINUES }
            }
            Statement::If(if_statement) => self.if_statement(self.child(id, DynNodeRef::from(if_statement)), if_statement),
            Statement::With(with_statement) => {
                let with_id = self.child(id, DynNodeRef::from(with_statement));
                let body = &with_statement.node.body;
                self.block(self.child(with_id, DynNodeRef::from(body)), &body.node)
            }
            Statement::Use(scoped) => match &scoped.node.body {
                Some(body) => {
                    let use_id = self.child(id, DynNodeRef::from(scoped));
                    self.block(self.child(use_id, DynNodeRef::from(body)), &body.node)
                }
                None => Flow::CONTINUES,
            },
            Statement::Expression(expression_statement) => {
                let statement_id = self.child(id, DynNodeRef::from(expression_statement));
                let expression = &expression_statement.node.expression;
                self.expression(self.child(statement_id, DynNodeRef::from(expression)))
            }
        }
    }

    fn if_statement(&self, id: Option<AstNodeId>, if_statement: &Spanned<IfStatement>) -> Flow {
        let then_block = &if_statement.node.then_block;
        let then_flow = self.block(self.child(id, DynNodeRef::from(then_block)), &then_block.node);
        let Some(else_branch) = &if_statement.node.else_branch else {
            return Flow { completes: true, breaks: then_flow.breaks };
        };
        let else_id = self.child(id, DynNodeRef::from(else_branch));
        let else_flow = match &else_branch.node {
            ElseBranch::If(nested) => self.if_statement(self.child(else_id, DynNodeRef::from(nested.as_ref())), nested),
            ElseBranch::Block(block) => self.block(self.child(else_id, DynNodeRef::from(block)), &block.node),
        };
        Flow { completes: then_flow.completes || else_flow.completes, breaks: then_flow.breaks || else_flow.breaks }
    }

    /// An expression in statement position (an expression statement or a match arm body).
    fn expression(&self, id: Option<AstNodeId>) -> Flow {
        let Some(id) = id else { return Flow::CONTINUES };
        let id = normalized_expression_node(self.index, id);
        let Some(node) = self.index.node_at(self.program, id) else { return Flow::CONTINUES };
        if let Some(block) = node.of::<beskid_analysis::syntax::BlockExpression>() {
            return self.block(self.child(Some(id), DynNodeRef::from(&block.block)), &block.block.node);
        }
        if let Some(match_expression) = node.of::<MatchExpression>() {
            return self.match_expression(id, match_expression);
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
            && super::typing::proven_type(self.db, AstNodeKey { node: id, ..self.key }) == Some(SemanticTypeId::NEVER)
        {
            return Flow::STOPS;
        }
        Flow::CONTINUES
    }

    /// The patterns decide whether a match can end without taking an arm, as in lowering: an
    /// unguarded `_` or binding arm always matches, and a match with an enum pattern is an enum
    /// match (its variants are exhaustive, E1304; lowering traps when no arm matches). A match
    /// whose patterns are all literals is a primitive match that can match no arm.
    fn match_expression(&self, id: AstNodeId, match_expression: &MatchExpression) -> Flow {
        let catch_all = match_expression.arms.iter().any(|arm| {
            arm.node.guard.is_none() && matches!(arm.node.pattern.node, Pattern::Wildcard | Pattern::Identifier(_))
        });
        let enum_match = match_expression.arms.iter().any(|arm| matches!(arm.node.pattern.node, Pattern::Enum(_)));
        let mut flow = Flow { completes: !(catch_all || enum_match), breaks: false };
        for arm in &match_expression.arms {
            let arm_id = self.child(Some(id), DynNodeRef::from(arm));
            let arm_flow = self.expression(self.child(arm_id, DynNodeRef::from(&arm.node.value)));
            flow.completes |= arm_flow.completes;
            flow.breaks |= arm_flow.breaks;
        }
        flow
    }
}

/// The literal `true`, also inside grouping parentheses.
fn is_literal_true(expression: &Spanned<Expression>) -> bool {
    match &expression.node {
        Expression::Literal(literal) => matches!(literal.node.literal.node, Literal::Bool(true)),
        Expression::Grouped(grouped) => is_literal_true(&grouped.node.expr),
        _ => false,
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

    /// The callables (by name) whose body can reach its end, and every E1258 finding of the gate.
    fn missing(source: &str, generation: u64) -> Vec<String> {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        let mut items = Vec::new();
        callables(&db, root, &mut items);
        let names = items
            .iter()
            .filter_map(|&item| missing_return(&db, item).expect("missing_return query"))
            .map(|missing| missing.callable)
            .collect::<Vec<_>>();
        let gate = super::super::check_items(&db, &items)
            .err()
            .unwrap_or_default()
            .into_iter()
            .filter(|finding| finding.kind.code() == "E1258")
            .count();
        assert_eq!(gate, names.len(), "the legality gate reports every missing return once: {source}");
        names
    }

    #[test]
    fn a_valued_body_that_can_reach_its_end_is_rejected() {
        let cases = [
            ("i32 Main() { 0; }", "expression statement only"),
            ("i32 F(bool c) { if (c) { return 1; } }", "if without else"),
            ("i32 F() { if (true) { return 1; } }", "if (true) without else: conditions are not folded"),
            ("i32 F(bool c) { if (c) { return 1; } else if (!c) { return 2; } }", "else-if chain without else"),
            ("i32 F() { while (true) { break; } }", "while (true) with a break"),
            ("i32 F(bool c) { while (true) { if (c) { break; } return 1; } }", "a conditional break"),
            ("i32 F(bool c) { while (c) { return 1; } }", "a non-literal condition"),
            ("i32 F(i32[] xs) { for x in xs { return x; } }", "for can run zero times"),
            ("i32 F(i32 x) { match x { 1 => { return 1; }, 2 => { return 2; }, } }", "a primitive match without a catch-all"),
            ("i32 F(i32 x) { match x { 1 => { return 1; }, _ => { }, } }", "a falling arm"),
            ("i32 F(i32 x) { match x { v when v > 0 => { return v; }, _ => { }, } }", "a guarded catch-all"),
            ("i32 F() { { 0; }; }", "a nested block that falls through"),
            ("string F() { }", "an empty body"),
            ("i32 F() { Helper(); } unit Helper() { }", "a call that returns"),
        ];
        for (index, (source, case)) in cases.iter().enumerate() {
            let names = missing(source, 9810 + index as u64);
            assert!(names.first().is_some(), "{case} must be rejected: {source}");
        }
    }

    #[test]
    fn a_body_whose_every_path_ends_is_accepted() {
        let cases = [
            ("i32 Main() { return 0; }", "a final return"),
            ("i32 F() { return 1; 0; }", "unreachable code after a return"),
            ("i32 F(bool c) { if (c) { return 1; } else { return 2; } }", "if-else whose branches return"),
            ("i32 F(bool c, bool d) { if (c) { return 1; } else if (d) { return 2; } else { return 3; } }", "else-if chain"),
            ("i32 F() { while (true) { } }", "while (true) without break"),
            ("i32 F() { while true { } }", "while true without break"),
            ("i32 F() { while ((true)) { } }", "nested parentheses"),
            ("i32 F(i32 x) { while (true) { if (x > 0) { return x; } } }", "a loop that only returns"),
            ("i32 F() { while (true) { while (true) { break; } } }", "a break of the inner loop"),
            ("i32 F(i32[] xs) { while (true) { for x in xs { break; } } }", "a break of a for loop"),
            ("i32 F() { while (true) { continue; } }", "continue"),
            (
                "enum Step { Stop(), Go(i32 value) } i32 F(Step s) { match s { Step::Stop() => { return 0; }, Step::Go(v) => { return v; }, } }",
                "an enum match whose arms return",
            ),
            (
                "enum Maybe<T> { Some(T value), Empty } i32 F<T>(Maybe<T> m) { match m { Maybe::Some(v) => { return 1; }, Maybe::Empty => { return 0; }, } }",
                "a generic enum match whose arms return",
            ),
            ("i32 F(i32 x) { match x { 1 => { return 1; }, _ => { return 0; }, } }", "a primitive match with `_`"),
            ("i32 F(i32 x) { match x { 1 => { return 1; }, other => { return other; }, } }", "a binding catch-all"),
            ("i32 F() { { return 1; }; }", "a nested block"),
            ("i32 F() { { { return 1; }; }; }", "nested blocks"),
            ("i32 F() { Stop(); } never Stop() { while (true) { } }", "a call that returns `never`"),
            ("unit F() { }", "a unit function"),
            ("type Counter { i32 value; } impl Counter { pub i32 Get() => 1; }", "an expression-bodied method"),
            ("never F() { }", "a never function is not judged by this rule"),
        ];
        for (index, (source, case)) in cases.iter().enumerate() {
            let names = missing(source, 9840 + index as u64);
            assert!(names.is_empty(), "{case} must be accepted, got {names:?}: {source}");
        }
    }

    #[test]
    fn methods_and_exported_functions_follow_the_same_rule() {
        let names = missing(
            "type Counter { i32 value; } impl Counter { pub i32 Get() { 0; } pub i32 Ok() { return 1; } } \
             [Export(Abi: \"C\", Symbol: \"answer\")] pub i32 Answer() { 42; }",
            9870,
        );
        assert_eq!(names, ["Get", "Answer"]);
    }

    #[test]
    fn the_finding_points_at_the_closing_brace_and_names_the_result() {
        let source = "pub i32 Main()\n{\n    0;\n}\n";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(9871));
        let mut items = Vec::new();
        callables(&db, root, &mut items);
        let findings = super::super::check_items(&db, &items).expect_err("the gate rejects Main");
        let finding = findings.iter().find(|finding| finding.kind.code() == "E1258").expect("E1258");
        assert_eq!(finding.kind.message(), "`Main` can reach the end of its body without returning a value");
        let span = finding.site_span(&db).expect("site span");
        assert_eq!(span.start, source.rfind('}').expect("closing brace"));
        assert_eq!(span.end, span.start + 1);
        assert_eq!(span.line_col_start, (4, 1));
        assert_eq!(finding.related.len(), 1);
        assert_eq!(finding.related[0].1, "`Main` returns `i32`");
        let name = crate::node_span(&db, finding.related[0].0).expect("name span").expect("name span");
        assert_eq!(&source[name.start..name.end], "Main");
        let (db, root) = one_unit_assembly("Result<i32, string>[] F() { }", SyntaxGenerationId(9873));
        let mut items = Vec::new();
        callables(&db, root, &mut items);
        let missing = missing_return(&db, items[0]).expect("query").expect("fact");
        assert_eq!(missing.result_name, "Result<i32, string>[]");
    }
}
