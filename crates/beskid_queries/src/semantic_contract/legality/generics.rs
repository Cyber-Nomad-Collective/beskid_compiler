//! Generic parameter conflict legality fact (E1229).
//!
//! A call that binds one generic parameter to two different types (`Equal(AsI64(), AsWord())`
//! for `unit Equal<T>(T actual, T expected)`) has no specialization. The call specialization
//! authority (`abi/specialization`, reached through `generic_call_specialization`) already
//! decides this while binding the call's arguments, and reports it as
//! `SemanticError::generic_binding_conflict`. This fact never re-derives the binding: it asks
//! the same authority module emission asks, for exactly the calls module emission asks it about
//! (`module_emission::specialization::direct_generic_call_declaration` in a concrete executable
//! item), and reports the conflict the authority found.
//!
//! Calls inside a generic body are specialized only once an enclosing instance supplies their
//! environment; such a body has no concrete ABI of its own and is not judged here.

use super::*;

/// One call that binds a generic parameter to two different types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericParameterConflict {
    /// The call expression (the diagnostic site).
    pub call: AstNodeKey,
    pub conflict: GenericBindingConflict,
}

/// Report the first call in `key` (a concrete executable item) whose generic specialization the
/// specialization authority rejects as a binding conflict.
pub fn generic_parameter_conflict(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<GenericParameterConflict> {
    with_registered_syntax(db, key, generic_parameter_conflict_tracked)
}

#[salsa::tracked(persist)]
fn generic_parameter_conflict_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericParameterConflict> {
    // Only a concrete executable item seeds specialization collection; a generic body is judged
    // through the environment its callers supply, which this fact does not model.
    if !matches!(item_abi_signature(db, key), Ok(Some(_))) {
        return Ok(None);
    }
    let calls = with_node(db, syntax, key, |_program, index, _node| {
        let mut calls = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::CallExpression, &mut calls);
        Some(calls)
    })?
    .unwrap_or_default();
    for node in calls {
        let call = AstNodeKey { node, ..key };
        if !calls_generic_template(db, call) {
            continue;
        }
        if let Err(error) = generic_call_specialization(db, call)
            && let Some(conflict) = error.binding_conflict()
        {
            return Ok(Some(GenericParameterConflict { call, conflict: conflict.clone() }));
        }
    }
    Ok(None)
}

/// The exact predicate module emission uses before it asks for a call's specialization: a direct
/// call to a function or method declaration whose declaration-level ABI is absent (a generic or
/// contract template).
fn calls_generic_template(db: &dyn Db, call: AstNodeKey) -> bool {
    let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, call) else { return false };
    matches!(
        node_kind(db, declaration),
        Ok(Some(IndexedNodeKind::FunctionDefinition | IndexedNodeKind::MethodDefinition))
    ) && matches!(item_abi_signature(db, declaration), Ok(None))
}
