//! Value-path obligations of the `beskid check` gate (E1101, E1104, E1107, E1108, E1201): a path
//! expression in value position (not a call's callee, not a `spawn` target) whose base is not a
//! lexical local, `this`, or `self`.
//!
//! Legacy sites mirrored here:
//!
//! * the retired legacy checker's `type_id_for_path`: a
//!   single-segment path that the resolver bound to an item (`ResolvedValue::Item`) is
//!   `TypeError::UnknownValueType` (E1201) at the path. Only items these facts prove are judged:
//!   a function of the lexical module scope chain (`resolve_unqualified_item_declaration`), an
//!   imported function (`unique_imported_function`), or a type, enum, contract, or inline module
//!   of the lexical module scope chain. A name that is also a constant, an implicit-receiver
//!   field, or an import binding is never judged.
//! * `TypeChecker::type_struct_field_path`: a multi-segment value path whose base is not a local
//!   is `TypeError::UnknownValueType` (E1201) at the path. When the base names a type, enum,
//!   contract, or function item, the resolver's `resolve_value_path` additionally reports the
//!   path as `UnknownModulePath`; the query gate reports one finding per site, the E1201.
//! * `Resolver::resolve_value_path` (`beskid_analysis::resolve::resolve_refs::types_paths`): a
//!   module-qualified value path is classified like a module-qualified callee
//!   (`unresolved_call_target`): a qualifier that names no module is `UnknownModulePath` (E1108),
//!   a qualifier claimed by more than one assembled module is `AmbiguousModulePath` (E1104), a
//!   module that declares no value of the terminal name is `UnknownValueInModule` (E1101), and a
//!   module that declares the terminal only without `pub` is `PrivateItemInModule` (E1107). A
//!   module that declares the terminal as a `pub` item (or as a constant) resolves, and the
//!   legacy checker then reports the path as `UnknownValueType` (E1201): neither authority has a
//!   value for a module-qualified item (`constant_integer` is single-segment, and a function item
//!   is a value only as a `spawn` target).
//!
//! Fail-closed direction: a path is judged only when the registered syntax and the dependency
//! registry prove the shape. Paths inside methods of `impl` and `extend type` blocks (whose
//! receiver fields these facts do not enumerate), paths whose base names a field of the enclosing
//! `type`, paths with type arguments, inline-module qualifiers, and module members reached
//! through public re-export routes are not judged.

use super::calls::{
    assembly_declares_module, import_binds_name, is_spawn_target, unit_declares_function_name,
    unit_declares_namespace_name,
};
use super::members::is_call_callee;
use super::*;
use beskid_analysis::syntax::{
    AstNodeId, ConstantDefinition, ContractDefinition, EnumDefinition, FunctionDefinition, InlineModule, PathExpression,
    Program, Spanned, TypeDefinition, Visibility,
};

/// Why one value path names no value, rendered with the legacy code by `check_unit_obligations`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ValuePathKind {
    /// E1201 `TypeError::UnknownValueType`: the path names an item, not a value.
    UnknownValueType,
    /// E1108 `ResolveError::UnknownModulePath`: the qualifier names no module. `path` is the
    /// qualifier as written.
    UnknownModulePath { path: Arc<str> },
    /// E1104: the qualifier names more than one assembled module.
    AmbiguousModulePath { path: Arc<str>, candidates: usize },
    /// E1101 `ResolveError::UnknownValueInModule`: the module declares nothing of the name.
    UnknownValueInModule { module_path: Arc<str>, name: Arc<str> },
    /// E1107 `ResolveError::PrivateItemInModule`: the module declares the name only without `pub`.
    PrivateItemInModule { module_path: Arc<str>, name: Arc<str> },
    /// E1104 `ResolveError::AmbiguousItemInModule`: the module declares no member of the name
    /// itself and reaches `candidates` distinct ones through its public re-exports at one distance.
    AmbiguousItemInModule { module_path: Arc<str>, name: Arc<str>, candidates: usize },
}

/// One value path of a unit that names no value, at the path expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ValuePathObligation {
    pub kind: ValuePathKind,
    pub site: AstNodeKey,
}

/// Every value path of the unit whose root is `key` (`AstNodeId(0)`) that names no value, in
/// source order. Called from the unit-level obligation fact, so it shares its Salsa key.
pub(super) fn collect_value_path_obligations(
    db: &dyn Db,
    program: &Spanned<Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValuePathObligation>,
) {
    if canonical_runtime_intrinsic_scope(db, key) {
        return;
    }
    for node in index.ids_of_kind(NodeKind::PathExpression) {
        let Some(path) = index.node_at(program, node).and_then(|node| node.of::<PathExpression>()) else { continue };
        let segments = &path.path.node.segments;
        if segments.is_empty() || segments.iter().any(|segment| !segment.node.type_args.is_empty()) {
            continue;
        }
        let in_body = nearest_ancestor(index, node, |kind| {
            matches!(kind, NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::TestDefinition)
        })
        .is_some();
        if !in_body || is_call_callee(program, index, node) || is_spawn_target(index, node) {
            continue;
        }
        let base = segments[0].node.name.node.name.as_str();
        if matches!(base, "this" | "self") || resolve_lexical_declaration(program, index, node, base).is_some() {
            continue;
        }
        // A method of an `impl` or `extend type` block reaches its receiver's fields by bare name;
        // those fields are not enumerated here.
        let method = nearest_ancestor(index, node, |kind| kind == NodeKind::MethodDefinition);
        if method.is_some() && nearest_ancestor(index, node, |kind| kind == NodeKind::TypeDefinition).is_none() {
            continue;
        }
        if enclosing_type_declares_field(program, index, node, base) {
            continue;
        }
        let site = AstNodeKey { node, ..key };
        // A named function read as a value is a closure record with the item's entry
        // (`function_item_value`); it is a value, not an item named in value position. A generic
        // function whose type arguments cannot be determined is judged by the legality gate
        // (`legality::function_values`), not as an item in value position.
        if segments.len() == 1
            && (matches!(function_item_value(db, site), Ok(Some(_)))
                || names_generic_function_item(db, program, index, site, &path.path.node))
        {
            continue;
        }
        let kind = if segments.len() == 1 {
            single_segment_item(db, program, index, site, base).then_some(ValuePathKind::UnknownValueType)
        } else {
            qualified_value_path(db, program, index, site, path)
        };
        if let Some(kind) = kind {
            findings.push(ValuePathObligation { kind, site });
        }
    }
}

/// What a name declares in the lexical module scope chain of a node.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LexicalItem {
    /// A `type`, `enum`, `contract`, or function: an item that is not a value.
    Item,
    /// An inline `mod`.
    InlineModule,
}

/// The nearest declaration of `name` among the types, enums, contracts, and inline modules of
/// the module scope chain of `node` (the lexical namespace `qualified_module_resolution` lets
/// shadow the absolute module namespace).
fn lexical_item(program: &Spanned<Program>, index: &SyntaxIndex, node: AstNodeId, name: &str) -> Option<LexicalItem> {
    let mut scope = module_scope(index, node);
    while let Some(current) = scope {
        for kind in [
            NodeKind::TypeDefinition,
            NodeKind::EnumDefinition,
            NodeKind::ContractDefinition,
            NodeKind::InlineModule,
        ] {
            for candidate in index.ids_of_kind(kind) {
                let candidate_scope = if kind == NodeKind::InlineModule {
                    outer_module_scope(index, candidate)
                } else {
                    module_scope(index, candidate)
                };
                if candidate_scope != Some(current) {
                    continue;
                }
                let Some(reference) = index.node_at(program, candidate) else { continue };
                let found = if let Some(item) = reference.of::<TypeDefinition>() {
                    (item.name.node.name == name).then_some(LexicalItem::Item)
                } else if let Some(item) = reference.of::<EnumDefinition>() {
                    (item.name.node.name == name).then_some(LexicalItem::Item)
                } else if let Some(item) = reference.of::<ContractDefinition>() {
                    (item.name.node.name == name).then_some(LexicalItem::Item)
                } else if let Some(item) = reference.of::<InlineModule>() {
                    (item.name.node.name == name).then_some(LexicalItem::InlineModule)
                } else {
                    None
                };
                if found.is_some() {
                    return found;
                }
            }
        }
        scope = outer_module_scope(index, current);
    }
    None
}

/// Whether the nearest enclosing `type` declares a field named `name` (an implicit-receiver field
/// read, which the legacy resolver binds to `this`).
fn enclosing_type_declares_field(program: &Spanned<Program>, index: &SyntaxIndex, node: AstNodeId, name: &str) -> bool {
    nearest_ancestor(index, node, |kind| kind == NodeKind::TypeDefinition)
        .and_then(|owner| index.node_at(program, owner))
        .and_then(|owner| owner.of::<TypeDefinition>())
        .is_some_and(|definition| definition.fields.iter().any(|field| field.node.name.node.name == name))
}

/// Whether the unit declares a constant named `name` anywhere (constants shadow items in the
/// legacy resolver and are values).
fn unit_declares_constant(program: &Spanned<Program>, index: &SyntaxIndex, name: &str) -> bool {
    index.ids_of_kind(NodeKind::ConstantDefinition).any(|candidate| {
        index
            .node_at(program, candidate)
            .and_then(|node| node.of::<ConstantDefinition>())
            .is_some_and(|constant| constant.name.node.name == name)
    })
}

/// Whether the name of a value path's base (or single segment) is proven to be a function or
/// nominal item: E1201 at the path. Constants, import bindings, and names these facts cannot
/// place are never judged.
fn names_item(
    db: &dyn Db,
    program: &Spanned<Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    name: &str,
) -> Option<LexicalItem> {
    if unit_declares_constant(program, index, name)
        || import_binds_name(db, site, name)
        || !matches!(constant_integer(db, site), Ok(None))
    {
        return None;
    }
    if let Some(item) = lexical_item(program, index, site.node, name) {
        return Some(item);
    }
    (resolve_unqualified_item_declaration(program, index, site, name).is_some()
        || unique_imported_function(db, site, name).is_some())
    .then_some(LexicalItem::Item)
}

/// E1201 for a single-segment value path naming an item (`TypeChecker::type_id_for_path`,
/// `ResolvedValue::Item`).
fn single_segment_item(db: &dyn Db, program: &Spanned<Program>, index: &SyntaxIndex, site: AstNodeKey, name: &str) -> bool {
    names_item(db, program, index, site, name).is_some()
}

/// Classify a multi-segment value path whose base is not a local.
fn qualified_value_path(
    db: &dyn Db,
    program: &Spanned<Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    path: &PathExpression,
) -> Option<ValuePathKind> {
    let segments = &path.path.node.segments;
    let (terminal, qualifier) = segments.split_last()?;
    let root = qualifier.first()?.node.name.node.name.as_str();
    match names_item(db, program, index, site, root) {
        // `Pair.a`, `Helper.x`: `type_struct_field_path` finds no local base.
        Some(LexicalItem::Item) => return Some(ValuePathKind::UnknownValueType),
        // An inline-module qualifier resolves through the unit's own namespace, not the module
        // registry; it is not judged here.
        Some(LexicalItem::InlineModule) => return None,
        None => {}
    }
    let qualifier = qualifier.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    let module_path = Arc::<str>::from(qualifier.join("."));
    let name = terminal.node.name.node.name.as_str();
    match qualified_module_resolution(db, site, &qualifier) {
        QualifiedModuleResolution::Ambiguous(units) => {
            Some(ValuePathKind::AmbiguousModulePath { path: module_path, candidates: units.len() })
        }
        QualifiedModuleResolution::Unit(unit) => match module_member_visibility(db, unit, site.generation, name) {
            MemberVisibility::Public => Some(ValuePathKind::UnknownValueType),
            MemberVisibility::Private => {
                Some(ValuePathKind::PrivateItemInModule { module_path, name: Arc::from(name) })
            }
            // Not declared by the module itself: the member is reached through the module's
            // public re-exports under the same nearest-distance rule as callees and types. A
            // unit that declares the name publicly is one candidate, whatever it declares.
            MemberVisibility::Undeclared => {
                match nearest_reexport_route_outcome(db, unit, site.generation, |db, current, generation| {
                    matches!(module_member_visibility(db, current, generation, name), MemberVisibility::Public)
                        .then_some(AstNodeKey { unit: current, generation, node: AstNodeId(0) })
                }) {
                    ReexportRoute::Found(_) => Some(ValuePathKind::UnknownValueType),
                    ReexportRoute::Ambiguous(candidates) => {
                        Some(ValuePathKind::AmbiguousItemInModule { module_path, name: Arc::from(name), candidates })
                    }
                    ReexportRoute::Missing => (!unit_declares_function_name(db, unit, site.generation, name))
                        .then(|| ValuePathKind::UnknownValueInModule { module_path, name: Arc::from(name) }),
                }
            }
            MemberVisibility::Unknown => None,
        },
        QualifiedModuleResolution::Missing => {
            if import_binds_name(db, site, root)
                || unit_declares_namespace_name(program, index, root)
                || assembly_declares_module(db, site, &qualifier)
            {
                return None;
            }
            Some(ValuePathKind::UnknownModulePath { path: module_path })
        }
    }
}

/// How one module unit declares a member name.
enum MemberVisibility {
    /// A `pub` function, type, enum, or contract, or a constant (constants carry no visibility).
    Public,
    /// Only declarations without `pub`.
    Private,
    /// No function, method, type, enum, contract, constant, or inline module of the name.
    Undeclared,
    /// The unit is not registered for the generation.
    Unknown,
}

fn module_member_visibility(db: &dyn Db, unit: SourceUnitId, generation: SyntaxGenerationId, name: &str) -> MemberVisibility {
    let Some(syntax) = db.syntax_unit(unit).filter(|syntax| syntax.generation(db) == generation) else {
        return MemberVisibility::Unknown;
    };
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let mut visibilities = Vec::new();
    for metadata in index.metadata() {
        let Some(node) = index.node_at(program, metadata.id) else { continue };
        let visibility = if let Some(item) = node.of::<FunctionDefinition>() {
            (item.name.node.name == name).then_some(item.visibility.node)
        } else if let Some(item) = node.of::<TypeDefinition>() {
            (item.name.node.name == name).then_some(item.visibility.node)
        } else if let Some(item) = node.of::<EnumDefinition>() {
            (item.name.node.name == name).then_some(item.visibility.node)
        } else if let Some(item) = node.of::<ContractDefinition>() {
            (item.name.node.name == name).then_some(item.visibility.node)
        } else if let Some(item) = node.of::<ConstantDefinition>() {
            (item.name.node.name == name).then_some(Visibility::Public)
        } else {
            None
        };
        visibilities.extend(visibility);
    }
    if visibilities.is_empty() {
        MemberVisibility::Undeclared
    } else if visibilities.contains(&Visibility::Public) {
        MemberVisibility::Public
    } else {
        MemberVisibility::Private
    }
}
