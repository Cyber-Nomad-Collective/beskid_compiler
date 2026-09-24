//! Unknown callee legality fact (E1101, E1108, E1203).
//!
//! `call_lowering` is the authority lowering uses to find the target of a call with a path
//! callee. When no declaration, import, inline module, builtin, runtime intrinsic, or extern
//! contract member names the callee, it fails closed with `unavailable("call_lowering")`, and
//! ISLE reports `MissingRuleOrFact CallExpression` far from the misspelled name. This fact reads
//! the same classification (`path_call_resolution`) and reports the call as a user error instead.
//!
//! A call is judged only when `call_lowering` fails for it and no other call authority that ISLE
//! consults before `call_lowering` claims it: primitive numeric conversions, typed array
//! allocations, closure calls through a local, `range(..)` iterators, runtime intrinsics, and
//! spawn targets each own their own diagnostics. Units of the embedded canonical runtime are not
//! judged: their compiler-minted operations resolve through the runtime intrinsic capability,
//! which only codegen holds.

use super::*;

/// Why a call's path callee names no call target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum UnresolvedCallKind {
    /// A single-segment callee that is not a local and names no function (E1101).
    UnknownValue { name: Arc<str> },
    /// A qualified callee whose leading segment names no import, module, type, contract,
    /// inline module, or local (E1108). `path` is the qualifier, without the terminal name.
    UnknownModulePath { path: Arc<str> },
    /// A generic function called with neither type arguments nor value arguments, so nothing
    /// fixes its type parameters (E1203).
    MissingTypeArguments,
}

/// One call with a path callee that lowering cannot resolve to a call target.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnresolvedCallTarget {
    /// The call expression (the diagnostic site).
    pub call: AstNodeKey,
    pub kind: UnresolvedCallKind,
}

/// Report the first call in `key` (an item) whose callee names no call target.
pub fn unresolved_call_target(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<UnresolvedCallTarget> {
    with_registered_syntax(db, key, unresolved_call_target_tracked)
}

#[salsa::tracked(persist)]
fn unresolved_call_target_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<UnresolvedCallTarget> {
    if canonical_runtime_intrinsic_scope(db, key) {
        return Ok(None);
    }
    let calls = with_node(db, syntax, key, |_program, index, _node| {
        let mut calls = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::CallExpression, &mut calls);
        Some(calls)
    })?
    .unwrap_or_default();
    for call_node in calls {
        let call = AstNodeKey { node: call_node, ..key };
        if let Some(finding) = unresolved_call_target_for_call(db, syntax, call)? {
            return Ok(Some(finding));
        }
    }
    Ok(None)
}

fn unresolved_call_target_for_call(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    call: AstNodeKey,
) -> SemanticQueryResult<UnresolvedCallTarget> {
    if call_lowering(db, call).is_ok() || claimed_by_another_call_authority(db, call) {
        return Ok(None);
    }
    with_node(db, syntax, call, |program, index, node| {
        if is_spawn_target(index, call.node) {
            return None;
        }
        let expression = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &expression.callee.node else { return None };
        let path = &path.node.path.node;
        let kind = match path_call_resolution(db, program, index, call, expression, path) {
            PathCallResolution::UnresolvedTarget(_) => unresolved_callee_kind(db, program, index, call, path)?,
            PathCallResolution::MissingTypeArguments => UnresolvedCallKind::MissingTypeArguments,
            PathCallResolution::Lowered(_) | PathCallResolution::Unavailable(_) | PathCallResolution::Failed(_) => {
                return None;
            }
        };
        Some(UnresolvedCallTarget { call, kind })
    })
}

/// ISLE's `call_kind` asks these authorities before `call_lowering`; a call any of them claims (or
/// rejects with its own diagnostic) is not an unknown callee.
fn claimed_by_another_call_authority(db: &dyn Db, call: AstNodeKey) -> bool {
    !matches!(primitive_numeric_conversion(db, call), Ok(None))
        || !matches!(typed_array_allocation(db, call), Ok(None))
        || !matches!(closure_call_target(db, call), Ok(None))
        || !matches!(range_for_fact(db, call), Ok(None))
        || matches!(runtime_intrinsic(db, call), Ok(Some(_)))
}

fn is_spawn_target(index: &SyntaxIndex, call: beskid_analysis::syntax::AstNodeId) -> bool {
    let mut current = call;
    while let Some(parent) = parent_node(index, current) {
        match index.kind(parent) {
            Some(NodeKind::Expression | NodeKind::GroupedExpression) => current = parent,
            Some(NodeKind::SpawnExpression) => return true,
            _ => return false,
        }
    }
    false
}

/// Name the unresolved callee. A single segment that is a lexical local is a closure call, which
/// has its own authority; a qualified callee whose leading segment names anything visible is not
/// judged here (its terminal member has other authorities).
fn unresolved_callee_kind(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    call: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<UnresolvedCallKind> {
    let (terminal, qualifier) = path.segments.split_last()?;
    let Some(first) = qualifier.first() else {
        let name = terminal.node.name.node.name.as_str();
        if resolve_lexical_declaration(program, index, call.node, name).is_some() {
            return None;
        }
        return Some(UnresolvedCallKind::UnknownValue { name: Arc::from(name) });
    };
    let root = first.node.name.node.name.as_str();
    if resolve_lexical_declaration(program, index, call.node, root).is_some()
        || matches!(root, "this" | "self")
        || import_binds_name(db, call, root)
        || unit_declares_namespace_name(program, index, root)
    {
        return None;
    }
    let qualifier = qualifier.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    if assembly_declares_module(db, call, &qualifier) {
        return None;
    }
    Some(UnresolvedCallKind::UnknownModulePath { path: Arc::from(qualifier.join(".")) })
}

fn import_binds_name(db: &dyn Db, key: AstNodeKey, name: &str) -> bool {
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .is_some_and(|imports| {
            imports.iter().any(|import| import.binding == name || import.path.iter().any(|segment| segment == name))
        })
}

fn assembly_declares_module(db: &dyn Db, key: AstNodeKey, module_path: &[String]) -> bool {
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    (1..=module_path.len()).any(|length| registry.modules.contains_key(&(key.generation, module_path[..length].to_vec())))
}

/// Whether the current unit declares a type, enum, contract, or inline module named `name`, at
/// any depth. A qualified callee rooted at such a name is a member path, not a module path.
fn unit_declares_namespace_name(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    name: &str,
) -> bool {
    index.metadata().iter().any(|metadata| {
        index.node_at(program, metadata.id).is_some_and(|node| {
            node.of::<beskid_analysis::syntax::TypeDefinition>().is_some_and(|item| item.name.node.name == name)
                || node.of::<beskid_analysis::syntax::EnumDefinition>().is_some_and(|item| item.name.node.name == name)
                || node.of::<beskid_analysis::syntax::ContractDefinition>().is_some_and(|item| item.name.node.name == name)
                || node.of::<beskid_analysis::syntax::InlineModule>().is_some_and(|item| item.name.node.name == name)
        })
    })
}
