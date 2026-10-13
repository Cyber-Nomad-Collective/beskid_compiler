//! Spawn, iterator, and event obligations (E1215-E1219, E1221, E1223-E1225).
//!
//! * Spawn: `spawn_legality` is the authority emission consumes; its target and capture
//!   diagnostics are rendered with the legacy codes at the spawn expression. A join of a handle
//!   from inside a fiber spawned within the handle's own fiber body is the legacy
//!   `JoinWouldDeadlock` (fiber scope strictly below the handle's scope).
//! * Iterators: a `for` iterable that is not the `range(..)` form and has no iterator fact is
//!   judged when its value is a proven primitive (no `Next` method) or of a nominal type, whose
//!   `Next` declaration is then held to the legacy protocol (E1216-E1218). Outside a generic or
//!   contract-parameterized item every other iterable without an iterator fact is E1215: the
//!   fact is the lowering authority, so `check` never admits a loop `build` cannot lower. Inside
//!   such an item the fact is proven per specialization and only the declared shape is judged.
//! * Events: a raise (`event_operation` `Raise`) outside a method of the declaring type, and a
//!   lambda subscribed to a target that no event fact describes.

use super::super::typing::{is_numeric, nodes_of_kind};
use super::*;
use beskid_analysis::syntax::{
    AssignExpression, AssignOp, CallExpression, Expression, ForStatement, LetStatement, MethodDefinition, Parameter,
    PathExpression, SpawnExpression, Type,
};
use beskid_analysis::syntax_query::DynNodeRef;

pub(super) fn collect_spawn_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::SpawnExpression) {
        let site = AstNodeKey { node, ..key };
        let Ok(Some(fact)) = spawn_legality(db, site) else { continue };
        let mut target_reported = false;
        let mut capture_reported = false;
        for diagnostic in fact.diagnostics.iter() {
            match diagnostic.kind {
                SpawnDiagnosticKind::TargetNotCallable
                | SpawnDiagnosticKind::TargetRequiresArguments
                | SpawnDiagnosticKind::CalleeArgumentsUnsupported => {
                    if !target_reported {
                        target_reported = true;
                        findings.push(GateObligation { kind: GateObligationKind::SpawnTargetNotFiberCompatible, site });
                    }
                }
                SpawnDiagnosticKind::StackReferenceEscapesSpawn => {
                    if !capture_reported {
                        capture_reported = true;
                        findings.push(GateObligation { kind: GateObligationKind::StackReferenceEscapesSpawn, site });
                    }
                }
                // Handle ownership (discarded handle, use after move) is the emission authority's
                // own diagnostic family; the legacy checker has no code for it.
                SpawnDiagnosticKind::DiscardedHandle | SpawnDiagnosticKind::UseAfterMove => {}
            }
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::CallExpression) {
        let Some(call) = index.node_at(program, node).and_then(|node| node.of::<CallExpression>()) else { continue };
        let Expression::Path(path) = &call.callee.node else { continue };
        let names = path.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
        if !beskid_analysis::builtins::builtin_for_path(&names)
            .is_some_and(|(_, spec)| spec.runtime_symbol == "fiber_join_status")
        {
            continue;
        }
        let Some(handle) = call.args.first() else { continue };
        let Expression::Path(handle_path) = &handle.node else { continue };
        let [segment] = handle_path.node.path.node.segments.as_slice() else { continue };
        let Some(declaration) = resolve_lexical_declaration(program, index, node, segment.node.name.node.name.as_str())
        else {
            continue;
        };
        let Some(binding) = parent_node(index, declaration) else { continue };
        let Some(statement) = index.node_at(program, binding).and_then(|node| node.of::<LetStatement>()) else {
            continue;
        };
        let Some(initializer) = index.direct_child_id(program, binding, DynNodeRef::from(&statement.value)) else {
            continue;
        };
        let spawn_node = normalized_expression_node(index, initializer);
        let Some(spawn) = index.node_at(program, spawn_node).and_then(|node| node.of::<SpawnExpression>()) else {
            continue;
        };
        let Some(callee) = index.direct_child_id(program, spawn_node, DynNodeRef::from(spawn.callee.as_ref())) else {
            continue;
        };
        let lambda = normalized_expression_node(index, callee);
        if index.kind(lambda) != Some(NodeKind::LambdaExpression) || !is_ancestor(index, lambda, node) {
            continue;
        }
        let mut current = parent_node(index, node);
        while let Some(ancestor) = current {
            if ancestor == lambda {
                break;
            }
            if index.kind(ancestor) == Some(NodeKind::SpawnExpression) {
                findings.push(GateObligation { kind: GateObligationKind::JoinWouldDeadlock, site: AstNodeKey { node, ..key } });
                break;
            }
            current = parent_node(index, ancestor);
        }
    }
}

pub(super) fn collect_iterator_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::ForStatement) {
        let Some(statement) = index.node_at(program, node).and_then(|node| node.of::<ForStatement>()) else { continue };
        if matches!(for_iterator_fact(db, AstNodeKey { node, ..key }), Ok(Some(_))) {
            continue;
        }
        let Some((site_node, subject)) = super::super::typing::expression_nodes(program, index, node, &statement.iterable)
        else {
            continue;
        };
        let site = AstNodeKey { node: site_node, ..key };
        let statement_key = AstNodeKey { node, ..key };
        // A `range(..)` loop is lowered from syntax alone; its operands are judged elsewhere.
        let lowers_without_fact = crate::semantic_contract::typing::range_iterable_bounds(&statement.iterable.node).is_some();
        let unproven_fails = !lowers_without_fact && !for_statement_in_generic_context(db, statement_key);
        // `pointer` is also the ABI of every nominal value: such an iterable is judged by its
        // declared nominal type below, never as a primitive.
        if let Some(ty) = super::super::typing::proven_type(db, AstNodeKey { node: subject, ..key }) {
            if ty == SemanticTypeId::NEVER {
                continue;
            }
            if ty != SemanticTypeId::POINTER {
                findings.push(GateObligation { kind: GateObligationKind::NonIterableForTarget, site });
                continue;
            }
        }
        let declaration = nominal_local_type_declaration(db, program, index, key, subject)
            .or_else(|| nominal_source_type_declaration(db, AstNodeKey { node: subject, ..key }));
        let Some(declaration) = declaration else {
            if unproven_fails {
                findings.push(GateObligation { kind: GateObligationKind::NonIterableForTarget, site });
            }
            continue;
        };
        let Some(next) = unique_nominal_method_declaration(db, declaration, "Next") else {
            findings.push(GateObligation { kind: GateObligationKind::NonIterableForTarget, site });
            continue;
        };
        let shape = next_shape_obligation(db, next);
        match shape {
            Some(kind) => findings.push(GateObligation { kind, site }),
            // The declared shape holds, yet no iterator fact proves the loop (an `Option` that is
            // not the two-variant `Some`/`None` enum, a unit element, an unprovable receiver):
            // the loop cannot lower, so it is not iterable.
            None if unproven_fails => {
                findings.push(GateObligation { kind: GateObligationKind::NonIterableForTarget, site })
            }
            None => {}
        }
    }
}

/// The legacy protocol obligation a `Next` declaration fails (E1215-E1218), if any. A
/// declaration whose syntax is not registered is never a finding.
fn next_shape_obligation(db: &dyn Db, next: AstNodeKey) -> Option<GateObligationKind> {
    let next_syntax = db.syntax_unit(next.unit).filter(|syntax| syntax.accepts_key(db, next))?;
    let method = next_syntax
        .syntax_index(db)
        .node_at(next_syntax.expanded_program(db), next.node)
        .and_then(|node| node.of::<MethodDefinition>())?;
    if !method.parameters.is_empty() {
        return Some(GateObligationKind::IterableNextArityMismatch { expected: 0, actual: method.parameters.len() });
    }
    match method.return_type.as_ref().map(|result| &result.node) {
        Some(Type::Complex(path))
            if path.node.segments.last().is_some_and(|segment| segment.node.name.node.name == "Option") =>
        {
            let arguments = path.node.segments.last().map_or(0, |segment| segment.node.type_args.len());
            (arguments != 0 && arguments != 1)
                .then_some(GateObligationKind::IterableOptionSomeArityMismatch { expected: 1, actual: arguments })
        }
        _ => Some(GateObligationKind::IterableNextReturnNotOption),
    }
}

/// The `type` declaration an iterable expression's proven nominal source identity names (call
/// results, field projections, inferred locals).
fn nominal_source_type_declaration(db: &dyn Db, subject: AstNodeKey) -> Option<AstNodeKey> {
    let identity = generic_source_expression_identity(db, subject).ok()?;
    matches!(identity, GenericSourceTypeIdentity::Nominal { .. }).then_some(())?;
    crate::semantic_contract::contracts::concrete_declaration(db, subject, &identity)
}

/// The declaration a local path's declared nominal type (`Iter it = ...`, `Iter it` parameter)
/// resolves to.
fn nominal_local_type_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    subject: beskid_analysis::syntax::AstNodeId,
) -> Option<AstNodeKey> {
    let path = index.node_at(program, subject)?.of::<PathExpression>()?;
    let [segment] = path.path.node.segments.as_slice() else { return None };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let local = resolve_lexical_declaration(program, index, subject, segment.node.name.node.name.as_str())?;
    let owner = parent_node(index, local)?;
    let owner_node = index.node_at(program, owner)?;
    let declared = if let Some(statement) = owner_node.of::<LetStatement>() {
        statement.type_annotation.as_ref()?.node.clone()
    } else if let Some(parameter) = owner_node.of::<Parameter>() {
        parameter.ty.node.clone()
    } else {
        return None;
    };
    let Type::Complex(path) = declared else { return None };
    resolve_type_declaration(db, AstNodeKey { node: subject, ..key }, &path.node)
}

pub(super) fn collect_event_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::CallExpression) {
        let site = AstNodeKey { node, ..key };
        let Ok(Some(fact)) = event_operation(db, site) else { continue };
        if fact.operation != EventOperationKind::Raise {
            continue;
        }
        let owner = nearest_ancestor(index, node, |kind| kind == NodeKind::MethodDefinition)
            .and_then(|method| method_owner_node(program, index, method));
        match owner {
            Some(owner) if AstNodeKey { node: owner, ..key } == fact.declaration => {}
            _ => findings.push(GateObligation { kind: GateObligationKind::InvalidEventInvocationScope, site }),
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::AssignExpression) {
        let Some(assign) = index.node_at(program, node).and_then(|node| node.of::<AssignExpression>()) else {
            continue;
        };
        if assign.op.node == AssignOp::Assign {
            continue;
        }
        let site = AstNodeKey { node, ..key };
        let Some((_, value)) = super::super::typing::expression_nodes(program, index, node, assign.value.as_ref()) else {
            continue;
        };
        let event = event_operation(db, site);
        if index.kind(value) == Some(NodeKind::LambdaExpression) {
            if matches!(event, Ok(None)) {
                findings.push(GateObligation { kind: GateObligationKind::InvalidEventSubscriptionTarget, site });
            }
            continue;
        }
        if matches!(event, Ok(Some(_))) {
            continue;
        }
        let Some((_, _, target)) = proven_operand(db, program, index, key, node, assign.target.as_ref()) else {
            continue;
        };
        if !judgeable_primitive(target) {
            continue;
        }
        let supported = match assign.op.node {
            AssignOp::AddAssign => is_numeric(target) || target == SemanticTypeId::STRING,
            AssignOp::SubAssign => is_numeric(target),
            AssignOp::Assign => true,
        };
        if !supported {
            findings.push(GateObligation { kind: GateObligationKind::UnsupportedExpression, site });
        }
    }
}
