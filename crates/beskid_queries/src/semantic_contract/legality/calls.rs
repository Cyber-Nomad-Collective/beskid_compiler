//! Unknown callee legality fact (E1101, E1107, E1108, E1203).
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
use beskid_analysis::syntax_query::DynNodeRef;

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
    /// A callee that names a private runtime builtin or Corelib adapter which only an admitted
    /// corelib unit may call (E1107). `name` is the callee spelling.
    PrivateBuiltin { name: Arc<str> },
    /// A qualified callee whose module resolves to exactly one assembled unit that declares no
    /// function of the terminal name (E1101). `module_path` is the qualifier as written.
    UnknownValueInModule { module_path: Arc<str>, name: Arc<str> },
    /// A qualified callee whose module declares the function privately (E1107).
    PrivateItemInModule { module_path: Arc<str>, name: Arc<str> },
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
    if call_lowering(db, call).is_ok() {
        return Ok(None);
    }
    with_node(db, syntax, call, |program, index, node| {
        if is_spawn_target(index, call.node) {
            return None;
        }
        let expression = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &expression.callee.node else { return None };
        let path = &path.node.path.node;
        let resolution = path_call_resolution(db, program, index, call, expression, path);
        // A private builtin is a name-resolution denial. The runtime intrinsic authority does not
        // excuse it (user code can name an intrinsic spelling without owning it), but the other
        // call authorities (conversions, array allocation, closures, `range`) still claim theirs.
        if claimed_by_another_call_authority(db, call, !matches!(resolution, PathCallResolution::PrivateBuiltin(_))) {
            return None;
        }
        let kind = match resolution {
            PathCallResolution::UnresolvedTarget(_) => unresolved_callee_kind(db, program, index, call, path)?,
            PathCallResolution::MissingTypeArguments => UnresolvedCallKind::MissingTypeArguments,
            PathCallResolution::PrivateBuiltin(name) => UnresolvedCallKind::PrivateBuiltin { name: Arc::from(name) },
            PathCallResolution::Lowered(_) | PathCallResolution::Unavailable(_) | PathCallResolution::Failed(_) => {
                return None;
            }
        };
        Some(UnresolvedCallTarget { call, kind })
    })
}

/// ISLE's `call_kind` asks these authorities before `call_lowering`; a call any of them claims (or
/// rejects with its own diagnostic) is not an unknown callee.
pub(super) fn claimed_by_another_call_authority(
    db: &dyn Db,
    call: AstNodeKey,
    runtime_intrinsic_claims: bool,
) -> bool {
    !matches!(primitive_numeric_conversion(db, call), Ok(None))
        || !matches!(typed_array_allocation(db, call), Ok(None))
        || !matches!(closure_call_target(db, call), Ok(None))
        || !matches!(range_for_fact(db, call), Ok(None))
        || (runtime_intrinsic_claims && matches!(runtime_intrinsic(db, call), Ok(Some(_))))
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
    if resolve_lexical_declaration(program, index, call.node, root).is_some() || matches!(root, "this" | "self") {
        return None;
    }
    let qualifier = qualifier.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    // A qualifier that names exactly one assembled module unit through this unit's imports is
    // judged against that unit's own function declarations: a unit that re-exports child modules
    // may route the name elsewhere, so only a route-free unit is a positive "unknown value".
    if let Some(unit) = resolve_qualified_module_unit(db, call, &qualifier) {
        let name = terminal.node.name.node.name.as_str();
        if unique_function_in_unit(db, unit, call.generation, name).is_some() {
            if unique_public_function_in_unit(db, unit, call.generation, name).is_none() {
                return Some(UnresolvedCallKind::PrivateItemInModule {
                    module_path: Arc::from(qualifier.join(".")),
                    name: Arc::from(name),
                });
            }
            return None;
        }
        if public_module_routes(db, unit, call.generation).is_empty()
            && !unit_declares_function_name(db, unit, call.generation, name)
        {
            return Some(UnresolvedCallKind::UnknownValueInModule {
                module_path: Arc::from(qualifier.join(".")),
                name: Arc::from(name),
            });
        }
        return None;
    }
    if import_binds_name(db, call, root) || unit_declares_namespace_name(program, index, root) {
        return None;
    }
    if assembly_declares_module(db, call, &qualifier) {
        return None;
    }
    Some(UnresolvedCallKind::UnknownModulePath { path: Arc::from(qualifier.join(".")) })
}

/// Whether `unit` declares any function, method, type, enum, contract, constant, or inline module
/// named `name` (ambiguous declarations included), so an unresolved terminal is never reported
/// as unknown when the module does declare the name in some form.
fn unit_declares_function_name(db: &dyn Db, unit: SourceUnitId, generation: SyntaxGenerationId, name: &str) -> bool {
    let Some(syntax) = db.syntax_unit(unit) else { return true };
    if syntax.generation(db) != generation {
        return true;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    unit_declares_namespace_name(program, index, name)
        || index.metadata().iter().any(|metadata| {
            index.node_at(program, metadata.id).is_some_and(|node| {
                node.of::<beskid_analysis::syntax::FunctionDefinition>().is_some_and(|item| item.name.node.name == name)
                    || node.of::<beskid_analysis::syntax::MethodDefinition>().is_some_and(|item| item.name.node.name == name)
                    || node.of::<beskid_analysis::syntax::ConstantDefinition>().is_some_and(|item| item.name.node.name == name)
            })
        })
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
    (1..=module_path.len())
        .any(|length| registry.visible_module_units(key.generation, &module_path[..length]).is_some())
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
                || node
                    .of::<beskid_analysis::syntax::ContractDefinition>()
                    .is_some_and(|item| item.name.node.name == name)
                || node.of::<beskid_analysis::syntax::InlineModule>().is_some_and(|item| item.name.node.name == name)
        })
    })
}

/// A bare value argument with no lexical, field, constant or callable declaration.
/// This is positive name-resolution evidence, not inference from an unavailable type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnresolvedValueArgument {
    pub site: AstNodeKey,
    pub name: Arc<str>,
}

pub fn unresolved_value_argument(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<UnresolvedValueArgument> {
    with_registered_syntax(db, key, unresolved_value_argument_tracked)
}

#[salsa::tracked(persist)]
fn unresolved_value_argument_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<UnresolvedValueArgument> {
    with_node(db, syntax, key, |program, index, _| {
        let mut calls = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::CallExpression, &mut calls);
        for call_node in calls {
            let call = index.node_at(program, call_node)?.of::<beskid_analysis::syntax::CallExpression>()?;
            for argument in &call.args {
                let beskid_analysis::syntax::Expression::Path(path) = &argument.node else { continue };
                let [segment] = path.node.path.node.segments.as_slice() else { continue };
                if !segment.node.type_args.is_empty() {
                    continue;
                }
                let expression = index.direct_child_id(program, call_node, DynNodeRef::from(argument))?;
                let node = normalized_expression_node(index, expression);
                let site = AstNodeKey { node, ..key };
                let name = segment.node.name.node.name.as_str();
                if resolve_lexical_declaration(program, index, node, name).is_some()
                    || matches!(name, "this" | "self")
                    || matches!(aggregate_field_access(db, site), Ok(Some(_)))
                    || matches!(constant_integer(db, site), Ok(Some(_)))
                    || import_binds_name(db, site, name)
                    || unit_declares_namespace_name(program, index, name)
                    || index.metadata().iter().any(|metadata| {
                        index.node_at(program, metadata.id).is_some_and(|node| {
                            node.of::<beskid_analysis::syntax::ConstantDefinition>()
                                .is_some_and(|constant| constant.name.node.name == name)
                                || node
                                    .of::<beskid_analysis::syntax::FunctionDefinition>()
                                    .is_some_and(|function| function.name.node.name == name)
                        })
                    })
                {
                    continue;
                }
                return Some(UnresolvedValueArgument { site, name: Arc::from(name) });
            }
        }
        None
    })
}
