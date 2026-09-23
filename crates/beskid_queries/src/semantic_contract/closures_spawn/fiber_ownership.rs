//! Spawn legality and generation-bound fiber handle ownership.

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

fn containing_fiber_callable(
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

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn callable_fiber_ownership_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<FiberOwnership> {
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
        Some(FiberOwnership { callable: key, diagnostics: spawn_handle_diagnostics(db, program, index, key).into() })
    })
}

/// A nominal Fiber annotation is evidence; a pointer-shaped ABI is not.
fn fiber_type_declaration(db: &dyn Db, key: AstNodeKey, ty: &beskid_analysis::syntax::Type) -> Option<AstNodeKey> {
    let beskid_analysis::syntax::Type::Complex(path) = ty else {
        return None;
    };
    let actual = resolve_type_declaration(db, key, &path.node)?;
    let fiber = layouts::unique_assembled_type_in_module(db, key, &["Concurrency".into(), "Fiber".into()], "Fiber", 1)
        .or_else(|| unique_type_in_unit(db, key.unit, key.generation, "Fiber", 1))?;
    (actual == fiber).then_some(actual)
}

/// Seed ownership from declared parameters/locals, spawn, or resolved callable result identity.
/// Ordinary source identity preserves method receiver and generic substitutions, without
/// deriving nominal ownership from a pointer-shaped ABI or querying ownership legality.
fn fiber_owner_seed(
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
        let fiber =
            layouts::unique_assembled_type_in_module(db, key, &["Concurrency".into(), "Fiber".into()], "Fiber", 1)
                .or_else(|| unique_type_in_unit(db, key.unit, key.generation, "Fiber", 1))?;
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

/// Handle ownership belongs to one generation-bound callable fact, projected into spawn legality.
/// Resolve terminal uses by declaration identity, so a shadowing local never consumes its parent.
fn spawn_handle_diagnostics(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Vec<SpawnDiagnostic> {
    use beskid_analysis::{
        syntax::{LetStatement, PathExpression},
        syntax_query::NodeKind,
    };
    type BranchPath = Vec<(beskid_analysis::syntax::AstNodeId, beskid_analysis::syntax::AstNodeId)>;
    let mut owners = HashMap::<_, Vec<BranchPath>>::new();
    for kind in [NodeKind::Parameter, NodeKind::LetStatement] {
        for node in index.ids_of_kind(kind).filter(|node| containing_fiber_callable(index, *node) == Some(key.node)) {
            if let Some(owner) = fiber_owner_seed(db, program, index, AstNodeKey { node, ..key }) {
                owners.insert(owner, Vec::new());
            }
        }
    }
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
    let mut uses = index
        .ids_of_kind(NodeKind::PathExpression)
        .filter_map(|use_id| {
            if !is_ancestor(index, key.node, use_id) {
                return None;
            }
            let node = index.node_at(program, use_id)?;
            let path = node.of::<PathExpression>()?;
            let [receiver, rest @ ..] = path.path.node.segments.as_slice() else {
                return None;
            };
            let local = resolve_lexical_declaration(program, index, use_id, &receiver.node.name.node.name)?;
            let method = rest.first().map(|segment| segment.node.name.node.name.as_str());
            Some((node.span()?, use_id, local, method))
        })
        .collect::<Vec<_>>();
    uses.sort_by_key(|(span, ..)| span.start);
    for (span, use_id, local, method) in uses {
        let Some(consumed) = owners.get_mut(&local) else {
            continue;
        };
        let mut branches = BranchPath::new();
        let mut repeated_move = false;
        let mut child = use_id;
        while let Some(parent) = parent_node(index, child) {
            if index.kind(parent) == Some(NodeKind::LambdaExpression) && !is_ancestor(index, parent, local) {
                // Ordinary closures are repeatable; they cannot own a consuming
                // Fiber capture. A lambda directly transferred to spawn has one
                // invocation and is checked as that one-shot ownership transfer.
                // The spawn-target fact shares spawn_entry_operand normalization
                // with emission, including the empty-call entry spelling.
                let one_shot = nearest_ancestor(index, parent, |kind| kind == NodeKind::SpawnExpression)
                    .and_then(|node| spawn_target(db, AstNodeKey { node, ..key }).ok().flatten())
                    .is_some_and(|target| target.callee.node == parent);
                repeated_move |= !one_shot;
            }
            if index.kind(parent) == Some(NodeKind::IfStatement)
                && matches!(index.kind(child), Some(NodeKind::Block | NodeKind::ElseBranch))
            {
                branches.push((parent, child));
            }
            if matches!(index.kind(parent), Some(NodeKind::WhileStatement | NodeKind::ForStatement)) {
                let loop_span = index.node_at(program, parent).and_then(|node| node.span());
                let declaration_span = index.node_at(program, local).and_then(|node| node.span());
                if let (Some(loop_span), Some(declaration_span)) = (loop_span, declaration_span) {
                    repeated_move |= declaration_span.start < loop_span.start;
                }
            }
            child = parent;
        }
        let consumed_on_this_path = consumed.iter().any(|prior| {
            !prior.iter().any(|(condition, arm)| {
                branches.iter().any(|(other, other_arm)| condition == other && arm != other_arm)
            })
        });
        if consumed_on_this_path || (repeated_move && method != Some("Cancel")) {
            diagnostics.push(SpawnDiagnostic { kind: SpawnDiagnosticKind::UseAfterMove, span, capture: None });
            continue;
        }
        if method == Some("Cancel") {
            continue;
        }
        consumed.push(branches);
        // A local assignment transfers the join capability to the new declaration.
        // Passing/returning the value consumes this owner without manufacturing a copy.
        if method.is_none() {
            let mut container = use_id;
            while let Some(parent) = parent_node(index, container) {
                match index.kind(parent) {
                    Some(NodeKind::Expression | NodeKind::GroupedExpression) => container = parent,
                    Some(NodeKind::LetStatement) => {
                        if let Some(binding) = index.node_at(program, parent).and_then(|node| node.of::<LetStatement>())
                            && let Some(alias) = index.direct_child_id(
                                program,
                                parent,
                                beskid_analysis::syntax_query::DynNodeRef::from(&binding.name),
                            )
                        {
                            owners.insert(alias, Vec::new());
                        }
                        break;
                    }
                    _ => break,
                }
            }
        }
    }
    diagnostics
}
