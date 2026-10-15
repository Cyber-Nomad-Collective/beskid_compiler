//! Spawn legality, the spawn-facing Fiber projection and the generation-bound affine ownership
//! query entry (`affine_ownership`); the analysis itself lives in `affine.rs`.

use super::super::*;
use super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn spawn_legality_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnLegality> {
    let target = spawn_target_tracked(db, syntax, key)?;
    let Some(target) = target else {
        return Ok(None);
    };
    let span = node_span_tracked(db, syntax, key)?.ok_or_else(|| SemanticError::unavailable("spawn_legality"))?;
    let index = syntax.syntax_index(db);
    if !target.arguments.is_empty()
        && index.kind(target.callee.node) != Some(beskid_analysis::syntax_query::NodeKind::PathExpression)
    {
        // Eager spawn arguments are transferred only to direct item entries. A lambda or other
        // computed callee with arguments has no source-proven entry signature to receive them.
        return Ok(Some(SpawnLegality {
            target,
            result: None,
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::CalleeArgumentsUnsupported,
                span,
                capture: None,
            }]),
        }));
    }
    let signature = callable_signature_tracked(db, syntax, target.callee)?;
    let Some(signature) = signature else {
        return Ok(Some(SpawnLegality {
            target,
            result: None,
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::TargetNotCallable,
                span,
                capture: None,
            }]),
        }));
    };

    if signature.parameters.len() != target.arguments.len() {
        return Ok(Some(SpawnLegality {
            target,
            result: Some(signature.result),
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::TargetRequiresArguments,
                span,
                capture: None,
            }]),
        }));
    }

    let capture = spawn_stack_capture(db, syntax, target.callee, &target.captures)?;
    let diagnostics = match capture {
        None => containing_fiber_callable(index, key.node)
            .map(|node| callable_fiber_ownership_tracked(db, syntax, AstNodeKey { node, ..key }))
            .transpose()?
            .flatten()
            .map_or_else(|| Arc::from([]), |fact| fact.diagnostics),
        Some(capture) => Arc::from([SpawnDiagnostic {
            kind: SpawnDiagnosticKind::StackReferenceEscapesSpawn,
            span: capture.span,
            capture: Some(capture),
        }]),
    };
    Ok(Some(SpawnLegality { target, result: Some(signature.result), span, diagnostics }))
}

pub(super) fn containing_fiber_callable(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    use beskid_analysis::syntax_query::NodeKind;
    nearest_ancestor(index, node, |kind| {
        matches!(
            kind,
            NodeKind::FunctionDefinition
                | NodeKind::MethodDefinition
                | NodeKind::TestDefinition
                | NodeKind::LambdaExpression
        )
    })
}

/// Spawn-facing projection of [`callable_affine_ownership_tracked`]: discarded `spawn` handles
/// and every use after move of a `Fiber` handle. Emission refuses a callable with any entry.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn callable_fiber_ownership_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<FiberOwnership> {
    let Some(affine) = callable_affine_ownership_tracked(db, syntax, key)? else {
        return Ok(None);
    };
    let mut diagnostics = with_node(db, syntax, key, |program, index, _node| {
        Some(discarded_handle_diagnostics(program, index, key))
    })?
    .unwrap_or_default();
    diagnostics.extend(
        affine
            .findings
            .iter()
            .filter(|finding| {
                finding.fiber
                    && matches!(
                        finding.kind,
                        AffineFindingKind::UseAfterMove | AffineFindingKind::MoveInLoopWithoutReassignment
                    )
            })
            .map(|finding| SpawnDiagnostic { kind: SpawnDiagnosticKind::UseAfterMove, span: finding.span, capture: None }),
    );
    Ok(Some(FiberOwnership { callable: key, diagnostics: diagnostics.into() }))
}

/// The ownership fact of one callable (function, method, test or lambda): owning and borrowed
/// names, consuming uses, `move` forms and every affine finding (E1234, E1239-E1243).
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn callable_affine_ownership_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AffineOwnership> {
    use beskid_analysis::syntax_query::NodeKind;
    with_node(db, syntax, key, |program, index, _node| {
        if !matches!(
            index.kind(key.node),
            Some(
                NodeKind::FunctionDefinition
                    | NodeKind::MethodDefinition
                    | NodeKind::TestDefinition
                    | NodeKind::LambdaExpression
            )
        ) {
            return None;
        }
        Some(super::affine::analyze(db, program, index, key))
    })
}

fn discarded_handle_diagnostics(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Vec<SpawnDiagnostic> {
    use beskid_analysis::syntax_query::NodeKind;
    let mut diagnostics = Vec::new();
    for spawn in index
        .ids_of_kind(NodeKind::SpawnExpression)
        .filter(|node| containing_fiber_callable(index, *node) == Some(key.node))
    {
        let mut owner = spawn;
        while let Some(parent) = parent_node(index, owner) {
            match index.kind(parent) {
                Some(NodeKind::Expression | NodeKind::GroupedExpression) => owner = parent,
                Some(NodeKind::ExpressionStatement) => {
                    if let Some(span) = index.node_at(program, spawn).and_then(|node| node.span()) {
                        diagnostics.push(SpawnDiagnostic {
                            kind: SpawnDiagnosticKind::DiscardedHandle,
                            span,
                            capture: None,
                        });
                    }
                    break;
                }
                _ => break,
            }
        }
    }
    diagnostics
}

/// A nominal Fiber annotation is evidence; a pointer-shaped ABI is not.
fn fiber_type_declaration(db: &dyn Db, key: AstNodeKey, ty: &beskid_analysis::syntax::Type) -> Option<AstNodeKey> {
    let beskid_analysis::syntax::Type::Complex(path) = ty else {
        return None;
    };
    let actual = resolve_type_declaration(db, key, &path.node)?;
    let fiber = fiber_declaration_in_scope(db, key)?;
    (actual == fiber).then_some(actual)
}

/// Seed ownership from declared parameters/locals, spawn, or resolved callable result identity.
/// Ordinary source identity preserves method receiver and generic substitutions, without
/// deriving nominal ownership from a pointer-shaped ABI or querying ownership legality.
pub(super) fn fiber_owner_seed(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    use beskid_analysis::syntax::{LetStatement, Parameter};
    let node = index.node_at(program, key.node)?;
    if let Some(parameter) = node.of::<Parameter>() {
        fiber_type_declaration(db, key, &parameter.ty.node)?;
        return index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(&parameter.name),
        );
    }
    let binding = node.of::<LetStatement>()?;
    let initializer =
        index.direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&binding.value))?;
    let initializer = normalized_expression_node(index, initializer);
    let is_fiber = if index.kind(initializer) == Some(beskid_analysis::syntax_query::NodeKind::SpawnExpression) {
        true
    } else if let Some(annotation) = &binding.type_annotation {
        fiber_type_declaration(db, key, &annotation.node).is_some()
    } else if index.kind(initializer) == Some(beskid_analysis::syntax_query::NodeKind::CallExpression) {
        let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } =
            generic_source_expression_identity(db, AstNodeKey { node: initializer, ..key }).ok()?
        else {
            return None;
        };
        let fiber = fiber_declaration_in_scope(db, key)?;
        stable_declaration_identity(db, fiber)? == qualified_name && arguments.len() == 1
    } else {
        false
    };
    is_fiber
        .then(|| {
            index.direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&binding.name))
        })
        .flatten()
}
