//! Unresolved nominal type name in any type position of an item.
//!
//! `resolve_type_declaration` is the sole authority every later layout/ABI query uses to turn a
//! nominal type-path segment into its declaration. When a name used in a type position (a `let`
//! declaration's declared type, a parameter, a return type, a field, an enum variant's field, a
//! lambda parameter, or an explicit call type argument) does not resolve — commonly a missing
//! `use` import — nothing in World A rejects it outside the entry unit (`AGENTS.md`: no per-
//! request HIR rebuilds or dual snapshot paths; this crate is the sole semantic authority for
//! dependency units). The gap surfaces only when some downstream layout/ABI query happens to need
//! that position's layout, and it fails closed there with an opaque `SemanticError::unavailable`,
//! or (worse) not until ISLE lowering reports an unrelated-looking `MissingRuleOrFact`.
//!
//! This fact closes that gap: fail at the declaration itself, with the unresolved name, before
//! any lowering is attempted. It is the legality authority for E1201 (`SemanticIssueKind::TypeUnknownType`,
//! `beskid_queries::semantic_contract::legality`) and walks every type-bearing position reachable
//! from one item exactly once, using the same `resolve_type_declaration` and
//! `type_syntax_is_enclosing_generic_parameter_reference` authorities every other layout/ABI fact
//! already uses, so its verdict can never disagree with lowering's.

use super::*;
use beskid_analysis::syntax::{
    AstNodeId, CallExpression, EnumDefinition, Expression, Field, FunctionDefinition, LambdaParameter, LetStatement,
    MethodDefinition, Parameter, Path, Program, Spanned, Type, TypeDefinition,
};
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};

/// One nominal type name, named in a type position of an item, that `resolve_type_declaration`
/// cannot resolve (no local declaration, no `use` import, no fully qualified module path) and
/// that is not an enclosing generic parameter. A contract named in a type position (a
/// contract-typed parameter such as `Reader reader`) is resolved through `resolve_contract`, the
/// namespace authority contract conformance and specialization already use: contracts have no
/// nominal aggregate declaration, so `resolve_type_declaration` alone never finds them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnresolvedTypeReference {
    /// The exact type-bearing node carrying the unresolved name (the diagnostic site): a
    /// `let` declaration, a parameter, an item's return type, a field, a lambda parameter, or a
    /// call's explicit type argument.
    pub site: AstNodeKey,
    /// The unresolved type name.
    pub name: Arc<str>,
    /// Why the path names no type, as the legacy resolver classifies it.
    pub kind: UnresolvedTypeKind,
}

/// The legacy resolver's classification of an unresolved type path (`Resolver::resolve_type_path`
/// and `resolve_type_item_in_module_path` in `beskid_analysis::resolve::resolve_refs::types_paths`).
///
/// A qualified path (two or more segments) is classified through `qualified_module_resolution`,
/// the module authority `resolve_type_declaration` itself consults: no module is
/// `ModulePathLookup::ModuleMissing`, one module that declares the type only without `pub` is
/// `NotVisible`, and one module that declares no type of the name is `NameMissing`. Every shape
/// these facts cannot prove (a single segment, a qualifier with type arguments, a qualifier the
/// unit's own namespace or an import owns, a module that declares the name publicly at another
/// generic arity, or a module with public re-export routes) stays `UnknownType`, which carries
/// the same code as `UnknownTypeInModule`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum UnresolvedTypeKind {
    /// E1201 `ResolveError::UnknownType` / `TypeError::UnknownType`.
    UnknownType,
    /// E1108 `ResolveError::UnknownModulePath`: the qualifier names no module. `path` is the
    /// qualifier as written.
    UnknownModulePath { path: Arc<str> },
    /// E1104: the qualifier names more than one assembled module.
    AmbiguousModulePath { path: Arc<str>, candidates: usize },
    /// E1107 `ResolveError::PrivateItemInModule`: the module declares the type without `pub`.
    PrivateItemInModule { module_path: Arc<str>, name: Arc<str> },
    /// E1201 `ResolveError::UnknownTypeInModule`: the module declares no type of the name.
    UnknownTypeInModule { module_path: Arc<str>, name: Arc<str> },
    /// E1104 `ResolveError::AmbiguousItemInModule`: the module declares no type of the name
    /// itself and reaches `candidates` distinct ones through its public re-exports at one distance.
    AmbiguousItemInModule { module_path: Arc<str>, name: Arc<str>, candidates: usize },
}

/// Report the first unresolved nominal type name among every type position reachable from
/// `key` (an item: a function, a method, a type declaration, or an enum declaration), if any.
///
/// Positions checked: every `let` declaration's declared type, every parameter's type, the
/// item's own return type (and that of any nested method), every field's type, every lambda
/// parameter's explicit type, and every explicit call type argument (`Foo<Bar>()`). Each type is
/// checked at its own root, not only its generic arguments: an unresolved `FiberError` in
/// `Result<u8[], FiberError> joined` and an unresolved `FiberError` used bare as a parameter type
/// are the same class of error and must both be caught here.
pub fn unresolved_type_reference(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<UnresolvedTypeReference> {
    with_registered_syntax(db, key, unresolved_type_reference_tracked)
}

#[salsa::tracked(persist)]
fn unresolved_type_reference_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<UnresolvedTypeReference> {
    with_node(db, syntax, key, |program, index, _node| {
        let mut positions = Vec::new();
        collect_type_positions(program, index, key.node, &mut positions);
        positions.into_iter().find_map(|(node, ty)| {
            let site = AstNodeKey { node, ..key };
            first_unresolved_nominal_reference(db, site, &ty)
                .map(|(name, kind)| Ok(UnresolvedTypeReference { site, name: Arc::from(name.as_str()), kind }))
        })
    })?
    .transpose()
}

/// Walk every descendant of `id` (inclusive), collecting `(node, declared type)` for every type
/// position a legality fact must check. This is a syntax-only structural walk: it does not
/// resolve or interpret any type, it only locates the positions to check.
fn collect_type_positions(
    program: &Spanned<Program>,
    index: &SyntaxIndex,
    id: AstNodeId,
    positions: &mut Vec<(AstNodeId, Type)>,
) {
    match index.kind(id) {
        Some(NodeKind::LetStatement) => {
            if let Some(annotation) = index
                .node_at(program, id)
                .and_then(|node| node.of::<LetStatement>())
                .and_then(|statement| statement.type_annotation.clone())
            {
                positions.push((id, annotation.node));
            }
        }
        Some(NodeKind::Parameter) => {
            if let Some(parameter) = index.node_at(program, id).and_then(|node| node.of::<Parameter>()) {
                positions.push((id, parameter.ty.node.clone()));
            }
        }
        Some(NodeKind::LambdaParameter) => {
            if let Some(ty) = index
                .node_at(program, id)
                .and_then(|node| node.of::<LambdaParameter>())
                .and_then(|parameter| parameter.ty.clone())
            {
                positions.push((id, ty.node));
            }
        }
        Some(NodeKind::Field) => {
            if let Some(field) = index.node_at(program, id).and_then(|node| node.of::<Field>()) {
                positions.push((id, field.ty.node.clone()));
            }
        }
        Some(NodeKind::FunctionDefinition) => {
            if let Some(return_type) = index
                .node_at(program, id)
                .and_then(|node| node.of::<FunctionDefinition>())
                .and_then(|function| function.return_type.clone())
            {
                positions.push((id, return_type.node));
            }
        }
        Some(NodeKind::MethodDefinition) => {
            if let Some(return_type) = index
                .node_at(program, id)
                .and_then(|node| node.of::<MethodDefinition>())
                .and_then(|method| method.return_type.clone())
            {
                positions.push((id, return_type.node));
            }
        }
        Some(NodeKind::CallExpression) => {
            if let Some(call) = index.node_at(program, id).and_then(|node| node.of::<CallExpression>()) {
                let mut callee = &call.callee.node;
                while let Expression::Grouped(inner) = callee {
                    callee = &inner.node.expr.node;
                }
                if let Expression::Path(path) = callee {
                    for segment in &path.node.path.node.segments {
                        for type_arg in &segment.node.type_args {
                            positions.push((id, type_arg.node.clone()));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    if let Some(children) = index.children(id) {
        for child in children {
            collect_type_positions(program, index, *child, positions);
        }
    }
}

/// Whether `ty` itself (and, recursively, every generic argument it carries) resolves. Returns
/// the first unresolved nominal name encountered, depth-first. `key` supplies the unit/generation
/// module context (`resolve_type_declaration`) and the lexical position used to recognize an
/// enclosing generic parameter (`type_syntax_is_enclosing_generic_parameter_reference`); callers
/// pass the exact node whose declared type is `ty`, so an enclosing-generic check made from a
/// nested position (for example a lambda parameter) still finds the correct enclosing
/// function/method.
fn first_unresolved_nominal_reference(db: &dyn Db, key: AstNodeKey, ty: &Type) -> Option<(String, UnresolvedTypeKind)> {
    match ty {
        // `This` is not a nominal reference: the type checker owns its scope
        // (`ThisUsedOutsideContractOrImpl`), like any enclosing generic parameter.
        Type::Primitive(_) | Type::Associated { .. } | Type::This => None,
        Type::Array(inner) => first_unresolved_nominal_reference(db, key, &inner.node),
        Type::Function { return_type, parameters } => first_unresolved_nominal_reference(db, key, &return_type.node)
            .or_else(|| {
                parameters.iter().find_map(|parameter| first_unresolved_nominal_reference(db, key, &parameter.node))
            }),
        Type::Complex(path) => {
            if !type_syntax_is_enclosing_generic_parameter_reference(db, key, ty)
                && !is_enclosing_container_generic_parameter(db, key, ty)
                && resolve_type_declaration(db, key, &path.node).is_none()
                && resolve_contract(db, key, &path.node).is_none()
            {
                return path
                    .node
                    .segments
                    .last()
                    .map(|segment| (segment.node.name.node.name.clone(), classify_unresolved_type_path(db, key, &path.node)));
            }
            path.node.segments.iter().find_map(|segment| {
                segment
                    .node
                    .type_args
                    .iter()
                    .find_map(|argument| first_unresolved_nominal_reference(db, key, &argument.node))
            })
        }
    }
}

/// Classify an unresolved type path at `key` (see [`UnresolvedTypeKind`]).
fn classify_unresolved_type_path(db: &dyn Db, key: AstNodeKey, path: &Path) -> UnresolvedTypeKind {
    use super::legality::calls::{assembly_declares_module, import_binds_name, unit_declares_namespace_name};

    let Some((terminal, qualifier)) = path.segments.split_last() else { return UnresolvedTypeKind::UnknownType };
    if qualifier.is_empty() || qualifier.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return UnresolvedTypeKind::UnknownType;
    }
    let qualifier = qualifier.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    let module_path = Arc::<str>::from(qualifier.join("."));
    let name = terminal.node.name.node.name.as_str();
    match qualified_module_resolution(db, key, &qualifier) {
        QualifiedModuleResolution::Ambiguous(units) => {
            UnresolvedTypeKind::AmbiguousModulePath { path: module_path, candidates: units.len() }
        }
        QualifiedModuleResolution::Unit(unit) => match module_type_visibility(db, unit, key.generation, name) {
            Some(false) => UnresolvedTypeKind::PrivateItemInModule { module_path, name: Arc::from(name) },
            Some(true) => UnresolvedTypeKind::UnknownType,
            None if db.syntax_unit(unit).is_some_and(|syntax| syntax.generation(db) == key.generation) => {
                let generic_arity = terminal.node.type_args.len();
                match nearest_reexport_route_outcome(db, unit, key.generation, |db, current, generation| {
                    unique_public_type_in_unit(db, current, generation, name, generic_arity)
                }) {
                    ReexportRoute::Found(_) => UnresolvedTypeKind::UnknownType,
                    ReexportRoute::Ambiguous(candidates) => {
                        UnresolvedTypeKind::AmbiguousItemInModule { module_path, name: Arc::from(name), candidates }
                    }
                    ReexportRoute::Missing => {
                        UnresolvedTypeKind::UnknownTypeInModule { module_path, name: Arc::from(name) }
                    }
                }
            }
            None => UnresolvedTypeKind::UnknownType,
        },
        QualifiedModuleResolution::Missing => {
            let root = qualifier[0].as_str();
            let namespace_owned = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)).is_none_or(|syntax| {
                unit_declares_namespace_name(syntax.expanded_program(db), syntax.syntax_index(db), root)
            });
            if namespace_owned || import_binds_name(db, key, root) || assembly_declares_module(db, key, &qualifier) {
                UnresolvedTypeKind::UnknownType
            } else {
                UnresolvedTypeKind::UnknownModulePath { path: module_path }
            }
        }
    }
}

/// Whether `unit` declares a type, enum, or contract named `name`: `Some(true)` when any such
/// declaration is `pub`, `Some(false)` when every one is private, `None` when there is none.
fn module_type_visibility(db: &dyn Db, unit: SourceUnitId, generation: SyntaxGenerationId, name: &str) -> Option<bool> {
    use beskid_analysis::syntax::{ContractDefinition, Visibility};

    let syntax = db.syntax_unit(unit).filter(|syntax| syntax.generation(db) == generation)?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let mut declared = None;
    for kind in [NodeKind::TypeDefinition, NodeKind::EnumDefinition, NodeKind::ContractDefinition] {
        for candidate in index.ids_of_kind(kind) {
            let Some(node) = index.node_at(program, candidate) else { continue };
            let visibility = if let Some(item) = node.of::<TypeDefinition>() {
                (item.name.node.name == name).then_some(item.visibility.node)
            } else if let Some(item) = node.of::<EnumDefinition>() {
                (item.name.node.name == name).then_some(item.visibility.node)
            } else if let Some(item) = node.of::<ContractDefinition>() {
                (item.name.node.name == name).then_some(item.visibility.node)
            } else {
                None
            };
            if let Some(visibility) = visibility {
                declared = Some(declared.unwrap_or(false) || visibility == Visibility::Public);
            }
        }
    }
    declared
}

/// Whether `ty` is a bare reference to a generic parameter declared by the nearest enclosing
/// `type` or `enum` declaration.
///
/// `type_syntax_is_enclosing_generic_parameter_reference` only recognizes a function's or
/// method's own generics (walking up to the nearest `FunctionDefinition`/`MethodDefinition`, then
/// that method's owner `TypeDefinition`). A struct field's or an enum variant's field's declared
/// type sits directly under its `type`/`enum` declaration with no enclosing function/method
/// ancestor at all -- `enum Box<T> { Some(T value) }`'s `T` is exactly this shape, and every
/// generic declaration's own field positions need it, not only calls that specialize the
/// declaration.
fn is_enclosing_container_generic_parameter(db: &dyn Db, key: AstNodeKey, ty: &Type) -> bool {
    let Some(parameter_name) = generic_parameter_reference_name(ty) else { return false };
    let Some(syntax) = db.syntax_unit(key.unit) else { return false };
    if !syntax.accepts_key(db, key) {
        return false;
    }
    let index = syntax.syntax_index(db);
    let Some(enclosing) =
        nearest_ancestor(index, key.node, |kind| matches!(kind, NodeKind::TypeDefinition | NodeKind::EnumDefinition))
    else {
        return false;
    };
    let program = syntax.expanded_program(db);
    let Some(node) = index.node_at(program, enclosing) else { return false };
    if let Some(type_definition) = node.of::<TypeDefinition>() {
        return type_definition.generics.iter().any(|generic| generic.node.name == parameter_name);
    }
    if let Some(enum_definition) = node.of::<EnumDefinition>() {
        return enum_definition.generics.iter().any(|generic| generic.node.name == parameter_name);
    }
    false
}
