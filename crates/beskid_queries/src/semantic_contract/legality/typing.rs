//! Typing obligations of one item (E1206, E1207, E1208), derived from generation-bound typing
//! facts instead of the legacy `beskid_analysis` `TypeChecker`.
//!
//! `typing_obligations` judges the value positions the legacy checker judges through
//! `require_same_type` and `require_bool`: a `return` value against the callable's declared
//! result, a typed `let` initializer against its annotation, a local assignment value against its
//! target, a direct call argument against the callee's declared parameter, every value arm of a
//! `match` against the first typed arm, and every `if`/`while` condition and `match` guard
//! against `bool`. It reports the same diagnostic codes at the same sites as the legacy checker,
//! so the two authorities deduplicate in the prepare spine.
//!
//! The fact is fail-closed in the query direction: a position is judged only when `node_type`
//! proves both sides. An unavailable type (nominal aggregates, inferred shapes, generic
//! parameters) is never a finding; neither is a `never`-typed side (a block that cannot fall
//! through, such as a match arm ending in `return`), nor a side represented as `pointer`, which
//! is also the ABI representation of every nominal value. Numeric primitives are mutually
//! compatible here exactly as in the legacy checker's `require_same_type`, so an unsuffixed
//! integer literal never conflicts with its destination.

use super::*;
use beskid_analysis::syntax::{
    AssignExpression, AstNodeId, CallExpression, Expression, FunctionDefinition, IfStatement, LetStatement,
    MatchExpression, MethodDefinition, ReturnStatement, Spanned, WhileStatement,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

/// One typing obligation an item fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypingObligationKind {
    /// E1206: a value whose proven type is incompatible with its proven destination type.
    Mismatch { expected: SemanticTypeId, actual: SemanticTypeId },
    /// E1207: a bare `return` inside a callable whose declared result is not `unit`.
    MissingReturnValue { expected: SemanticTypeId },
    /// E1208: a condition or match guard whose proven type is not `bool`.
    NonBoolCondition { actual: SemanticTypeId },
}

/// A failed typing obligation and the exact node that carries the diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TypingObligation {
    pub kind: TypingObligationKind,
    pub site: AstNodeKey,
}

/// Report every typing obligation `key` (a function, method, or test item) fails, in source
/// order. Non-item nodes contain no fact.
pub fn typing_obligations(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[TypingObligation]>> {
    with_registered_syntax(db, key, typing_obligations_tracked)
}

#[salsa::tracked(persist)]
fn typing_obligations_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[TypingObligation]>> {
    with_node(db, syntax, key, |program, index, node| {
        if !matches!(
            node.node_kind(),
            NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::TestDefinition
        ) {
            return None;
        }
        let mut findings = Vec::new();
        collect_return_obligations(db, program, index, key, node, &mut findings);
        collect_let_obligations(db, program, index, key, &mut findings);
        collect_assignment_obligations(db, program, index, key, &mut findings);
        collect_condition_obligations(db, program, index, key, &mut findings);
        collect_match_arm_obligations(db, program, index, key, &mut findings);
        collect_call_argument_obligations(db, program, index, key, &mut findings);
        findings.sort_by_key(|finding| finding.site.node.0);
        Some(Arc::from(findings))
    })
}

pub(super) fn is_numeric(ty: SemanticTypeId) -> bool {
    matches!(
        ty,
        SemanticTypeId::I8
            | SemanticTypeId::I16
            | SemanticTypeId::U16
            | SemanticTypeId::U64
            | SemanticTypeId::I32
            | SemanticTypeId::I64
            | SemanticTypeId::U32
            | SemanticTypeId::U8
            | SemanticTypeId::WORD
            | SemanticTypeId::F32
            | SemanticTypeId::F64
    )
}

/// The legacy `require_same_type` rule restricted to proven primitives: equal identities,
/// either side `never`, or two numeric primitives are compatible. `pointer` is also the ABI
/// representation of nominal values, so it is never judged.
pub(super) fn compatible(expected: SemanticTypeId, actual: SemanticTypeId) -> bool {
    expected == actual
        || expected == SemanticTypeId::NEVER
        || actual == SemanticTypeId::NEVER
        || expected == SemanticTypeId::POINTER
        || actual == SemanticTypeId::POINTER
        || (is_numeric(expected) && is_numeric(actual))
}

pub(super) fn proven_type(db: &dyn Db, key: AstNodeKey) -> Option<SemanticTypeId> {
    match node_type(db, key) {
        Ok(Some(ty)) => Some(ty),
        Ok(None) | Err(_) => None,
    }
}

/// The exact child node of `parent` that holds `expression`, before normalization (the
/// diagnostic site), and its normalized value node (the typing subject).
pub(super) fn expression_nodes(
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    parent: AstNodeId,
    expression: &Spanned<Expression>,
) -> Option<(AstNodeId, AstNodeId)> {
    let site = index.direct_child_id(program, parent, DynNodeRef::from(expression))?;
    Some((site, normalized_expression_node(index, site)))
}

pub(super) fn enclosing_callable(index: &SyntaxIndex, node: AstNodeId) -> Option<AstNodeId> {
    let mut current = parent_node(index, node)?;
    loop {
        match index.kind(current)? {
            NodeKind::FunctionDefinition
            | NodeKind::MethodDefinition
            | NodeKind::TestDefinition
            | NodeKind::LambdaExpression => return Some(current),
            _ => current = parent_node(index, current)?,
        }
    }
}

pub(super) fn nodes_of_kind(index: &SyntaxIndex, root: AstNodeId, kind: NodeKind) -> Vec<AstNodeId> {
    let mut found = Vec::new();
    collect_nodes_of_kind(index, root, kind, &mut found);
    found
}

/// `return` statements owned directly by the item (not by a nested lambda), judged against the
/// item's declared result. Only functions and methods declare a result syntactically; a test
/// body keeps its legacy authority.
fn collect_return_obligations(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    item: DynNodeRef<'_>,
    findings: &mut Vec<TypingObligation>,
) {
    let declared_syntax = if let Some(function) = item.of::<FunctionDefinition>() {
        function.return_type.as_ref()
    } else if let Some(method) = item.of::<MethodDefinition>() {
        method.return_type.as_ref()
    } else {
        return;
    };
    let expected = match declared_syntax {
        Some(syntax_type) => match semantic_type_from_syntax(&syntax_type.node) {
            Ok(expected) => expected,
            Err(_) => return,
        },
        None => SemanticTypeId::UNIT,
    };
    for statement in nodes_of_kind(index, key.node, NodeKind::ReturnStatement) {
        if enclosing_callable(index, statement) != Some(key.node) {
            continue;
        }
        let Some(return_statement) = index.node_at(program, statement).and_then(|node| node.of::<ReturnStatement>())
        else {
            continue;
        };
        let site = AstNodeKey { node: statement, ..key };
        match return_statement.value.as_ref() {
            Some(value) => {
                let Some((_, subject)) = expression_nodes(program, index, statement, value) else { continue };
                let Some(actual) = proven_type(db, AstNodeKey { node: subject, ..key }) else { continue };
                if !compatible(expected, actual) {
                    findings.push(TypingObligation { kind: TypingObligationKind::Mismatch { expected, actual }, site });
                }
            }
            None => {
                if expected != SemanticTypeId::UNIT && expected != SemanticTypeId::NEVER {
                    findings.push(TypingObligation {
                        kind: TypingObligationKind::MissingReturnValue { expected },
                        site,
                    });
                }
            }
        }
    }
}

/// Typed `let` initializers, reported at the declared name exactly as the legacy checker does.
fn collect_let_obligations(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<TypingObligation>,
) {
    for statement in nodes_of_kind(index, key.node, NodeKind::LetStatement) {
        let Some(let_statement) = index.node_at(program, statement).and_then(|node| node.of::<LetStatement>()) else {
            continue;
        };
        let Some(annotation) = let_statement.type_annotation.as_ref() else { continue };
        let Ok(expected) = semantic_type_from_syntax(&annotation.node) else { continue };
        let Some((_, subject)) = expression_nodes(program, index, statement, &let_statement.value) else { continue };
        let Some(actual) = proven_type(db, AstNodeKey { node: subject, ..key }) else { continue };
        if compatible(expected, actual) {
            continue;
        }
        let Some(name) = index.direct_child_id(program, statement, DynNodeRef::from(&let_statement.name)) else {
            continue;
        };
        findings.push(TypingObligation {
            kind: TypingObligationKind::Mismatch { expected, actual },
            site: AstNodeKey { node: name, ..key },
        });
    }
}

/// Assignments to a local path, reported at the assignment expression.
fn collect_assignment_obligations(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<TypingObligation>,
) {
    for assignment in nodes_of_kind(index, key.node, NodeKind::AssignExpression) {
        let Some(assign) = index.node_at(program, assignment).and_then(|node| node.of::<AssignExpression>()) else {
            continue;
        };
        if !matches!(assign.target.node, Expression::Path(_)) {
            continue;
        }
        let Some((_, target)) = expression_nodes(program, index, assignment, assign.target.as_ref()) else { continue };
        let Some((_, value)) = expression_nodes(program, index, assignment, assign.value.as_ref()) else { continue };
        let Some(expected) = proven_type(db, AstNodeKey { node: target, ..key }) else { continue };
        let Some(actual) = proven_type(db, AstNodeKey { node: value, ..key }) else { continue };
        if !compatible(expected, actual) {
            findings.push(TypingObligation {
                kind: TypingObligationKind::Mismatch { expected, actual },
                site: AstNodeKey { node: assignment, ..key },
            });
        }
    }
}

fn judge_condition(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    parent: AstNodeId,
    condition: &Spanned<Expression>,
    findings: &mut Vec<TypingObligation>,
) {
    let Some((site, subject)) = expression_nodes(program, index, parent, condition) else { return };
    let Some(actual) = proven_type(db, AstNodeKey { node: subject, ..key }) else { return };
    if actual == SemanticTypeId::BOOL || actual == SemanticTypeId::NEVER || actual == SemanticTypeId::POINTER {
        return;
    }
    findings.push(TypingObligation {
        kind: TypingObligationKind::NonBoolCondition { actual },
        site: AstNodeKey { node: site, ..key },
    });
}

/// `if` and `while` conditions, reported at the condition expression.
fn collect_condition_obligations(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<TypingObligation>,
) {
    for statement in nodes_of_kind(index, key.node, NodeKind::IfStatement) {
        let Some(if_statement) = index.node_at(program, statement).and_then(|node| node.of::<IfStatement>()) else {
            continue;
        };
        judge_condition(db, program, index, key, statement, &if_statement.condition, findings);
    }
    for statement in nodes_of_kind(index, key.node, NodeKind::WhileStatement) {
        let Some(while_statement) = index.node_at(program, statement).and_then(|node| node.of::<WhileStatement>())
        else {
            continue;
        };
        judge_condition(db, program, index, key, statement, &while_statement.condition, findings);
    }
}

/// Value arms of every `match` in the item: the first arm with a proven, non-`never` type sets
/// the expectation; later proven arms must be compatible with it. Guards must be `bool`.
fn collect_match_arm_obligations(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<TypingObligation>,
) {
    for match_node in nodes_of_kind(index, key.node, NodeKind::MatchExpression) {
        let Some(match_expression) =
            index.node_at(program, match_node).and_then(|node| node.of::<MatchExpression>())
        else {
            continue;
        };
        let mut expected = None;
        for arm in &match_expression.arms {
            let Some(arm_node) = index.direct_child_id(program, match_node, DynNodeRef::from(arm)) else { continue };
            if let Some(guard) = arm.node.guard.as_ref() {
                judge_condition(db, program, index, key, arm_node, guard, findings);
            }
            let Some((site, subject)) = expression_nodes(program, index, arm_node, &arm.node.value) else { continue };
            let Some(actual) = proven_type(db, AstNodeKey { node: subject, ..key }) else { continue };
            if actual == SemanticTypeId::NEVER {
                continue;
            }
            match expected {
                None => expected = Some(actual),
                Some(expected) if !compatible(expected, actual) => findings.push(TypingObligation {
                    kind: TypingObligationKind::Mismatch { expected, actual },
                    site: AstNodeKey { node: site, ..key },
                }),
                Some(_) => {}
            }
        }
    }
}

/// Arguments of direct calls to a non-generic function whose parameters are all primitive,
/// reported at the argument. Method calls, generic callees, `bulk` callees, and callees with a
/// nominal parameter keep their own authority (or none yet).
fn collect_call_argument_obligations(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<TypingObligation>,
) {
    for call_node in nodes_of_kind(index, key.node, NodeKind::CallExpression) {
        let Some(call) = index.node_at(program, call_node).and_then(|node| node.of::<CallExpression>()) else {
            continue;
        };
        if !matches!(call.callee.node, Expression::Path(_)) {
            continue;
        }
        let call_key = AstNodeKey { node: call_node, ..key };
        let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, call_key) else { continue };
        let Some(declaration_syntax) = db.syntax_unit(declaration.unit) else { continue };
        if !declaration_syntax.accepts_key(db, AstNodeKey { node: declaration.node, ..declaration }) {
            continue;
        }
        let declaration_index = declaration_syntax.syntax_index(db);
        let declaration_program = declaration_syntax.expanded_program(db);
        let Some(function) = declaration_index
            .node_at(declaration_program, declaration.node)
            .and_then(|node| node.of::<FunctionDefinition>())
        else {
            continue;
        };
        if !function.generics.is_empty() || function.parameters.iter().any(|parameter| parameter.node.bulk) {
            continue;
        }
        let Ok(expected_types) = function
            .parameters
            .iter()
            .map(|parameter| semantic_type_from_syntax(&parameter.node.ty.node))
            .collect::<Result<Vec<_>, _>>()
        else {
            continue;
        };
        let Ok(Some(arguments)) = call_arguments(db, call_key) else { continue };
        if arguments.len() != expected_types.len() {
            continue;
        }
        for (argument, expected) in arguments.iter().zip(expected_types) {
            let Some(actual) = proven_type(db, *argument) else { continue };
            if !compatible(expected, actual) {
                findings.push(TypingObligation {
                    kind: TypingObligationKind::Mismatch { expected, actual },
                    site: *argument,
                });
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::super::tests::{function_definitions, one_unit_assembly};
    use super::*;

    fn obligations(source: &str, generation: u64, item: usize) -> Vec<TypingObligation> {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        let key = function_definitions(&db, root)[item];
        typing_obligations(&db, key).expect("typing_obligations query").expect("item has a fact").to_vec()
    }

    fn kinds(source: &str, generation: u64, item: usize) -> Vec<TypingObligationKind> {
        obligations(source, generation, item).into_iter().map(|finding| finding.kind).collect()
    }

    #[test]
    fn return_value_of_another_primitive_is_a_mismatch_at_the_return_statement() {
        let source = "i64 Main() { return true; }";
        let findings = obligations(source, 300, 0);
        assert_eq!(
            findings.iter().map(|finding| finding.kind).collect::<Vec<_>>(),
            vec![TypingObligationKind::Mismatch { expected: SemanticTypeId::I64, actual: SemanticTypeId::BOOL }]
        );
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(301));
        let key = function_definitions(&db, root)[0];
        let finding = typing_obligations(&db, key).expect("query").expect("fact")[0];
        assert_eq!(node_kind(&db, finding.site).expect("kind"), Some(IndexedNodeKind::ReturnStatement));
    }

    #[test]
    fn bare_return_in_a_valued_function_is_a_missing_return_value() {
        assert_eq!(
            kinds("i64 Main() { return; }", 302, 0),
            vec![TypingObligationKind::MissingReturnValue { expected: SemanticTypeId::I64 }]
        );
        assert_eq!(kinds("unit Main() { return; }", 303, 0), Vec::new());
    }

    #[test]
    fn numeric_primitives_and_unsuffixed_literals_are_compatible() {
        let cases = [
            "i64 Main() { return 1; }",
            "i64 Main() { return 1_i32; }",
            "unit Main() { i64 n = 1; i32 m = n; f64 f = 2; return; }",
            "unit Take(i64 n) { return; } unit Main() { Take(1_u8); return; }",
        ];
        for (offset, source) in cases.iter().enumerate() {
            let (db, root) = one_unit_assembly(source, SyntaxGenerationId(310 + offset as u64));
            for item in function_definitions(&db, root) {
                let findings = typing_obligations(&db, item).expect("query").expect("fact");
                assert!(findings.is_empty(), "{source}: {findings:?}");
            }
        }
    }

    #[test]
    fn typed_let_initializer_is_reported_at_the_declared_name() {
        let source = "unit Main() { bool flag = 1_i64; return; }";
        let findings = obligations(source, 330, 0);
        assert_eq!(
            findings.iter().map(|finding| finding.kind).collect::<Vec<_>>(),
            vec![TypingObligationKind::Mismatch { expected: SemanticTypeId::BOOL, actual: SemanticTypeId::I64 }]
        );
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(331));
        let key = function_definitions(&db, root)[0];
        let finding = typing_obligations(&db, key).expect("query").expect("fact")[0];
        assert_eq!(node_kind(&db, finding.site).expect("kind"), Some(IndexedNodeKind::Identifier));
    }

    #[test]
    fn local_assignment_of_another_primitive_is_a_mismatch() {
        assert_eq!(
            kinds("unit Main() { mut i64 n = 0_i64; n = true; return; }", 340, 0),
            vec![TypingObligationKind::Mismatch { expected: SemanticTypeId::I64, actual: SemanticTypeId::BOOL }]
        );
        assert_eq!(kinds("unit Main() { mut i64 n = 0_i64; n = 2_i32; return; }", 341, 0), Vec::new());
    }

    #[test]
    fn non_bool_conditions_are_reported_for_if_and_while() {
        assert_eq!(
            kinds("unit Main() { if 1_i64 { return; } return; }", 350, 0),
            vec![TypingObligationKind::NonBoolCondition { actual: SemanticTypeId::I64 }]
        );
        assert_eq!(
            kinds("unit Main() { while \"x\" { return; } return; }", 351, 0),
            vec![TypingObligationKind::NonBoolCondition { actual: SemanticTypeId::STRING }]
        );
        assert_eq!(kinds("unit Main(bool c) { if c { return; } while !c { return; } return; }", 352, 0), Vec::new());
    }

    #[test]
    fn match_arms_of_different_primitives_are_a_mismatch_but_never_arms_are_not() {
        let shape = "enum Shape { Dot, Circle(i64 radius) } ";
        assert_eq!(
            kinds(&format!("{shape}i64 Main(Shape s) {{ return match s {{ Shape::Dot => 0_i64, Shape::Circle(r) => true, }}; }}"), 360, 0),
            vec![TypingObligationKind::Mismatch { expected: SemanticTypeId::I64, actual: SemanticTypeId::BOOL }]
        );
        assert_eq!(
            kinds(
                &format!(
                    "{shape}i64 Main(Shape s) {{ i64 v = match s {{ Shape::Dot => 1_i64, Shape::Circle(r) => {{ return r; }}, }}; return v; }}"
                ),
                361,
                0
            ),
            Vec::new()
        );
    }

    #[test]
    fn direct_call_argument_of_another_primitive_is_reported_at_the_argument() {
        let source = "unit Take(bool flag, i64 n) { return; } unit Main() { Take(1_i64, 2); return; }";
        let findings = obligations(source, 370, 1);
        assert_eq!(
            findings.iter().map(|finding| finding.kind).collect::<Vec<_>>(),
            vec![TypingObligationKind::Mismatch { expected: SemanticTypeId::BOOL, actual: SemanticTypeId::I64 }]
        );
        assert_eq!(kinds("unit Take(bool flag) { return; } unit Main() { Take(true); return; }", 371, 1), Vec::new());
    }

    #[test]
    fn nominal_and_unproven_positions_are_never_judged() {
        let cases = [
            "pub type Pair { i64 a } unit Main() { Pair p = Pair { a: 1 }; Pair q = p; return; }",
            "pub type Pair { i64 a } Pair Make() { return Pair { a: 1 }; } unit Main() { string s = Make(); return; }",
            "unit Forward<U>(U value) { return; } unit Main() { Forward(true); return; }",
        ];
        for (offset, source) in cases.iter().enumerate() {
            let (db, root) = one_unit_assembly(source, SyntaxGenerationId(380 + offset as u64));
            for item in function_definitions(&db, root) {
                let findings = typing_obligations(&db, item).expect("query").expect("fact");
                assert!(findings.is_empty(), "{source}: {findings:?}");
            }
        }
    }
}
