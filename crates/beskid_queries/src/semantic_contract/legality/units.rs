//! Unit-level `check` obligations (E1102, E1220, E1601, E1602, E1607, E1608, E1609), judged once
//! per root unit: declaration shapes the legacy `TypeChecker` and resolver judged outside any
//! callable body.
//!
//! * E1102: the resolver's `collect_item` duplicates within one unit: two module-scope items
//!   (function, type, enum, contract, test, inline module) of one name in one module scope (the
//!   unit, or one inline module), two constants of one name in one module scope, a `type` method
//!   named like one of the type's fields, two top-level `use` declarations binding one alias, and
//!   two `mod` declarations of one module path. Reported at the later declaration with the
//!   earlier one's span.
//! * E1102 across units (`collect_program_in_module` collecting every unit of one logical module
//!   into one module scope): a top-level item or constant of a root unit whose name an earlier
//!   unit of the same logical module (dependency registry order, the legacy collection order)
//!   already declares. Reported in the later unit, with the earlier declaration's span.
//! * E1102 builtin collision (`collect_builtins` registers every manifest builtin as an item of
//!   its module before any unit is collected): a module-scope item whose module path plus name is
//!   a builtin path.
//! * E1101, E1104, E1107, E1108, E1201: value paths that name no value (`value_paths.rs`).
//! * E1220: an `event` field declared with capacity `0` (`register_struct_definition_fields`).
//! * E1601 / E1602 / E1607: every `type X : C` and `impl X : C` conformance
//!   (`check_contract_conformances`), generic implementors (`type Box<T> : C`) and applied generic
//!   contracts (`type X : Seq<i64>`) included, classified by `contract_conformance_failure`; a
//!   conformance path that names a type instead of a contract is the resolver's
//!   `InvalidConformanceTarget` (E1607). Method presence (E1601) and associated-type bindings
//!   (E1607) are judged by name, which generics do not change. E1602 compares closed signatures
//!   only: a contract signature that mentions a contract generic is open and skipped
//!   (`contract_conformance_failure`), and an implementing signature that mentions an implementor
//!   generic has no source identity (`generic_source_type_identity` fails on a bare type
//!   parameter) and is skipped, so the legacy substitution of `check_contract_conformances` is
//!   never needed for a judged signature.
//! * E1608 / E1609: `This` outside a method, contract, `impl`, or `type` scope, and every
//!   associated-type reference (`type_id_for_type`, which fails closed on `Type::Associated`).
//! * E1201 for `where` bounds (`type_item` for a function and an `impl` block): a bound whose
//!   parameter is not one of the declaration's generics, and a bound whose contract path names
//!   no contract. A single-segment bound names the nearest lexical declaration of that name
//!   (`function_where_bounds`): a type or enum of that name in a nearer module scope shadows an
//!   outer contract, an inline module's contract is invisible outside it, and an import binds
//!   only public contracts. Reported at the declaring item (where-bound syntax carries no node of
//!   its own).

use super::value_paths::{ValuePathKind, collect_value_path_obligations};
use super::*;
use beskid_analysis::syntax::{
    AssociatedTypeBinding, ConstantDefinition, ContractDefinition, ContractNode, EnumDefinition, Field, FieldKind,
    FunctionDefinition, ImplBlock, InlineModule, ModuleDeclaration, Path, Spanned, TestDefinition, Type, TypeDefinition,
    UseDeclaration, Visibility,
};
use beskid_analysis::syntax_query::DynNodeRef;

/// One unit-level obligation a root unit fails.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum UnitObligationKind {
    /// E1102: a second declaration of one name in one module scope (or one type's members).
    DuplicateItem { name: Arc<str>, previous: SourceSpan },
    /// E1220: an event field with capacity `0`.
    InvalidEventCapacity,
    /// E1601: a contract method the implementor does not declare (presence by name).
    ContractMethodMissingImplementation { contract_name: Arc<str>, method_name: Arc<str>, expected: Arc<str> },
    /// E1602: an implementing method whose closed signature differs from the contract's.
    ContractImplementationSignatureMismatch { method_name: Arc<str>, expected: Arc<str>, actual: Arc<str> },
    /// E1607: a contract associated type with no default and no implementor binding.
    ContractAssociatedTypeMissingBinding { contract_name: Arc<str>, assoc_name: Arc<str> },
    /// E1607: a conformance path that names a type or enum, not a contract.
    InvalidConformanceTarget { name: Arc<str> },
    /// E1608: `This` outside a contract, `impl`, `type`, or method scope.
    ThisUsedOutsideContractOrImpl,
    /// E1609: an associated-type reference (`T::Item`), which the type system fails closed on.
    UnresolvedAssociatedType { name: Arc<str> },
    /// A value path that names no value (`value_paths.rs`).
    ValuePath(ValuePathKind),
    /// E1201: a `where` bound whose parameter is not a generic of its declaration, or whose
    /// contract path names no contract (rendered as `TypeUnknownType`, the legacy class).
    UnresolvedWhereBound { name: Arc<str> },
}

/// A failed unit-level obligation and the exact node that carries the diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnitObligation {
    pub kind: UnitObligationKind,
    pub site: AstNodeKey,
}

/// Report every unit-level obligation the unit owning `key` fails, in source order. `key` is
/// the unit's root (`AstNodeId(0)`); any other node contains no fact.
pub fn unit_obligations(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[UnitObligation]>> {
    with_registered_syntax(db, key, unit_obligations_tracked)
}

#[salsa::tracked(persist)]
fn unit_obligations_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[UnitObligation]>> {
    if key.node != beskid_analysis::syntax::AstNodeId(0) {
        return Ok(None);
    }
    with_node(db, syntax, key, |program, index, _node| {
        let mut findings = Vec::new();
        collect_duplicate_item_obligations(program, index, key, &mut findings);
        collect_event_capacity_obligations(program, index, key, &mut findings);
        collect_type_scope_obligations(program, index, key, &mut findings);
        collect_conformance_obligations(db, program, index, key, &mut findings);
        collect_module_declaration_obligations(program, index, key, &mut findings);
        collect_cross_unit_duplicate_obligations(db, syntax, program, index, key, &mut findings);
        collect_builtin_collision_obligations(db, syntax, program, index, key, &mut findings);
        collect_where_bound_obligations(db, program, index, key, &mut findings);
        let mut value_paths = Vec::new();
        collect_value_path_obligations(db, program, index, key, &mut value_paths);
        findings.extend(value_paths.into_iter().map(|obligation| UnitObligation {
            kind: UnitObligationKind::ValuePath(obligation.kind),
            site: obligation.site,
        }));
        findings.sort_by_key(|finding| finding.site.node.0);
        Some(Arc::from(findings))
    })
}

/// Evaluate the unit-level obligations of every unit root in `roots`, rendered with the legacy
/// codes.
pub fn check_unit_obligations(db: &dyn Db, roots: &[AstNodeKey]) -> Vec<SemanticFinding> {
    let mut findings = Vec::new();
    for &root in roots {
        let Ok(Some(obligations)) = unit_obligations(db, root) else { continue };
        for obligation in obligations.iter() {
            let kind = match &obligation.kind {
                UnitObligationKind::DuplicateItem { name, previous } => {
                    SemanticIssueKind::ResolveDuplicateItem { name: name.to_string(), previous: *previous }
                }
                UnitObligationKind::InvalidEventCapacity => SemanticIssueKind::TypeInvalidEventCapacity,
                UnitObligationKind::ContractMethodMissingImplementation { contract_name, method_name, expected } => {
                    SemanticIssueKind::ContractMethodMissingImplementation {
                        contract_name: contract_name.to_string(),
                        method_name: method_name.to_string(),
                        expected: expected.to_string(),
                    }
                }
                UnitObligationKind::ContractImplementationSignatureMismatch { method_name, expected, actual } => {
                    SemanticIssueKind::ContractImplementationSignatureMismatch {
                        method_name: method_name.to_string(),
                        expected: expected.to_string(),
                        actual: actual.to_string(),
                    }
                }
                UnitObligationKind::ContractAssociatedTypeMissingBinding { contract_name, assoc_name } => {
                    SemanticIssueKind::ContractAssociatedTypeMissingBinding {
                        contract_name: contract_name.to_string(),
                        assoc_name: assoc_name.to_string(),
                    }
                }
                UnitObligationKind::InvalidConformanceTarget { name } => {
                    SemanticIssueKind::ResolveInvalidConformanceTarget { name: name.to_string() }
                }
                UnitObligationKind::ThisUsedOutsideContractOrImpl => SemanticIssueKind::ThisUsedOutsideContractOrImpl,
                UnitObligationKind::UnresolvedAssociatedType { name } => {
                    SemanticIssueKind::UnresolvedAssociatedType { name: name.to_string() }
                }
                UnitObligationKind::UnresolvedWhereBound { name } => {
                    SemanticIssueKind::TypeUnknownType { name: name.to_string() }
                }
                UnitObligationKind::ValuePath(kind) => match kind {
                    ValuePathKind::UnknownValueType => SemanticIssueKind::TypeUnknownValueType,
                    ValuePathKind::UnknownModulePath { path } => {
                        SemanticIssueKind::ResolveUnknownModulePath { path: path.to_string() }
                    }
                    ValuePathKind::AmbiguousModulePath { path, candidates } => {
                        SemanticIssueKind::ResolveAmbiguousModulePath { path: path.to_string(), candidates: *candidates }
                    }
                    ValuePathKind::UnknownValueInModule { module_path, name } => {
                        SemanticIssueKind::ResolveUnknownValueInModule {
                            module_path: module_path.to_string(),
                            name: name.to_string(),
                        }
                    }
                    ValuePathKind::PrivateItemInModule { module_path, name } => {
                        SemanticIssueKind::ResolvePrivateItemInModule {
                            module_path: module_path.to_string(),
                            name: name.to_string(),
                        }
                    }
                    ValuePathKind::AmbiguousItemInModule { module_path, name, candidates } => {
                        SemanticIssueKind::ResolveAmbiguousItemInModule {
                            module_path: module_path.to_string(),
                            name: name.to_string(),
                            candidates: *candidates,
                        }
                    }
                },
            };
            findings.push(SemanticFinding { kind, site: obligation.site, related: Vec::new() });
        }
    }
    findings
}

/// The module scope (`Program` root or `InlineModule`) that directly owns the item `node`, when
/// `node` is a module-scope item (`scope -> Node -> item`).
fn module_scope_of_item(
    index: &SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let wrapper = parent_node(index, node)?;
    if index.kind(wrapper) != Some(NodeKind::Node) {
        return None;
    }
    let scope = parent_node(index, wrapper)?;
    matches!(index.kind(scope), Some(NodeKind::Program | NodeKind::InlineModule)).then_some(scope)
}

/// Report every later declaration of a name already declared earlier in the same namespace, in
/// source order.
fn report_duplicate_items(
    index: &SyntaxIndex,
    key: AstNodeKey,
    mut declarations: Vec<(beskid_analysis::syntax::AstNodeId, String, beskid_analysis::syntax::AstNodeId)>,
    findings: &mut Vec<UnitObligation>,
) {
    declarations.sort_by_key(|(_, _, node)| node.0);
    let mut seen: HashMap<(beskid_analysis::syntax::AstNodeId, String), beskid_analysis::syntax::AstNodeId> =
        HashMap::new();
    for (scope, name, node) in declarations {
        match seen.get(&(scope, name.clone())) {
            Some(previous) => {
                let previous = index
                    .metadata_for(key.generation, *previous)
                    .and_then(|metadata| metadata.span)
                    .unwrap_or_default();
                findings.push(UnitObligation {
                    kind: UnitObligationKind::DuplicateItem { name: Arc::from(name.as_str()), previous },
                    site: AstNodeKey { node, ..key },
                });
            }
            None => {
                seen.insert((scope, name), node);
            }
        }
    }
}

fn collect_duplicate_item_obligations(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<UnitObligation>,
) {
    let mut items = Vec::new();
    let mut constants = Vec::new();
    for kind in [
        NodeKind::FunctionDefinition,
        NodeKind::TypeDefinition,
        NodeKind::EnumDefinition,
        NodeKind::ContractDefinition,
        NodeKind::TestDefinition,
        NodeKind::InlineModule,
        NodeKind::ConstantDefinition,
    ] {
        for node in index.ids_of_kind(kind) {
            let Some(scope) = module_scope_of_item(index, node) else { continue };
            let Some(reference) = index.node_at(program, node) else { continue };
            let name = if let Some(item) = reference.of::<FunctionDefinition>() {
                item.name.node.name.clone()
            } else if let Some(item) = reference.of::<TypeDefinition>() {
                item.name.node.name.clone()
            } else if let Some(item) = reference.of::<EnumDefinition>() {
                item.name.node.name.clone()
            } else if let Some(item) = reference.of::<ContractDefinition>() {
                item.name.node.name.clone()
            } else if let Some(item) = reference.of::<TestDefinition>() {
                item.name.node.name.clone()
            } else if let Some(item) = reference.of::<InlineModule>() {
                item.name.node.name.clone()
            } else if let Some(item) = reference.of::<ConstantDefinition>() {
                constants.push((scope, item.name.node.name.clone(), node));
                continue;
            } else {
                continue;
            };
            items.push((scope, name, node));
        }
    }
    report_duplicate_items(index, key, items, findings);
    report_duplicate_items(index, key, constants, findings);

    let mut aliases = Vec::new();
    for node in index.ids_of_kind(NodeKind::UseDeclaration) {
        let Some(scope) = module_scope_of_item(index, node) else { continue };
        if index.kind(scope) != Some(NodeKind::Program) {
            continue;
        }
        let Some(declaration) = index.node_at(program, node).and_then(|node| node.of::<UseDeclaration>()) else {
            continue;
        };
        let alias = match declaration.alias.as_ref() {
            Some(alias) => alias.node.name.clone(),
            None => match declaration.path.node.segments.last() {
                Some(segment) => segment.node.name.node.name.clone(),
                None => continue,
            },
        };
        aliases.push((scope, alias, node));
    }
    report_duplicate_items(index, key, aliases, findings);

    for node in index.ids_of_kind(NodeKind::TypeDefinition) {
        let Some(definition) = index.node_at(program, node).and_then(|node| node.of::<TypeDefinition>()) else {
            continue;
        };
        for method in &definition.methods {
            let name = method.node.name.node.name.as_str();
            let Some(field) = definition.fields.iter().find(|field| field.node.name.node.name == name) else { continue };
            let Some(method_node) = index.direct_child_id(program, node, DynNodeRef::from(method)) else { continue };
            let previous = index
                .direct_child_id(program, node, DynNodeRef::from(field))
                .and_then(|field_node| index.metadata_for(key.generation, field_node))
                .and_then(|metadata| metadata.span)
                .unwrap_or_default();
            findings.push(UnitObligation {
                kind: UnitObligationKind::DuplicateItem {
                    name: Arc::from(format!("{}::{name}", definition.name.node.name).as_str()),
                    previous,
                },
                site: AstNodeKey { node: method_node, ..key },
            });
        }
    }
}

fn collect_event_capacity_obligations(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<UnitObligation>,
) {
    for node in index.ids_of_kind(NodeKind::Field) {
        let Some(field) = index.node_at(program, node).and_then(|node| node.of::<Field>()) else { continue };
        if field.kind == FieldKind::Event && field.event_capacity == Some(0) {
            findings.push(UnitObligation { kind: UnitObligationKind::InvalidEventCapacity, site: AstNodeKey { node, ..key } });
        }
    }
}

fn collect_type_scope_obligations(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<UnitObligation>,
) {
    for node in index.ids_of_kind(NodeKind::Type) {
        let Some(ty) = index.node_at(program, node).and_then(|node| node.of::<Type>()) else { continue };
        let site = AstNodeKey { node, ..key };
        match ty {
            Type::This => {
                let scoped = nearest_ancestor(index, node, |kind| {
                    matches!(
                        kind,
                        NodeKind::MethodDefinition
                            | NodeKind::ContractDefinition
                            | NodeKind::ImplBlock
                            | NodeKind::TypeDefinition
                    )
                })
                .is_some();
                if !scoped {
                    findings.push(UnitObligation { kind: UnitObligationKind::ThisUsedOutsideContractOrImpl, site });
                }
            }
            Type::Associated { name, .. } => findings.push(UnitObligation {
                kind: UnitObligationKind::UnresolvedAssociatedType { name: Arc::from(name.node.name.as_str()) },
                site,
            }),
            _ => {}
        }
    }
}

fn collect_conformance_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<UnitObligation>,
) {
    for node in index.ids_of_kind(NodeKind::TypeDefinition) {
        let Some(definition) = index.node_at(program, node).and_then(|node| node.of::<TypeDefinition>()) else {
            continue;
        };
        let concrete = AstNodeKey { node, ..key };
        for conformance in &definition.conformances {
            judge_conformance(db, program, index, concrete, concrete, conformance, &definition.associated_type_bindings, findings);
        }
    }
    for node in index.ids_of_kind(NodeKind::ImplBlock) {
        let Some(block) = index.node_at(program, node).and_then(|node| node.of::<ImplBlock>()) else { continue };
        let Type::Complex(receiver) = &block.receiver_type.node else { continue };
        let owner = AstNodeKey { node, ..key };
        let Some(concrete) = resolve_type_declaration(db, owner, &receiver.node) else { continue };
        if concrete.unit != key.unit {
            continue;
        }
        let Some(definition) = index.node_at(program, concrete.node).and_then(|node| node.of::<TypeDefinition>()) else {
            continue;
        };
        let bindings = definition
            .associated_type_bindings
            .iter()
            .chain(block.associated_type_bindings.iter())
            .cloned()
            .collect::<Vec<_>>();
        for conformance in &block.conformances {
            judge_conformance(db, program, index, owner, concrete, conformance, &bindings, findings);
        }
    }
}

/// `owner` is the declaration carrying the conformance path (the diagnostic unit), `concrete` the
/// implementing type declaration.
#[expect(clippy::too_many_arguments, reason = "one conformance site carries every legacy input")]
fn judge_conformance(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    owner: AstNodeKey,
    concrete: AstNodeKey,
    conformance: &Spanned<Path>,
    bindings: &[Spanned<AssociatedTypeBinding>],
    findings: &mut Vec<UnitObligation>,
) {
    let site = index
        .direct_child_id(program, owner.node, DynNodeRef::from(conformance))
        .map_or(owner, |node| AstNodeKey { node, ..owner });
    let Some(terminal) = conformance.node.segments.last() else { return };
    let name = terminal.node.name.node.name.as_str();
    // An applied contract (`Seq<i64>`) names the contract declaration of its terminal; the type
    // arguments only substitute contract generics, whose signatures stay open below.
    let mut contract_path = conformance.node.clone();
    if let Some(last) = contract_path.segments.last_mut() {
        last.node.type_args.clear();
    }
    let Some(contract) = resolve_contract(db, owner, &contract_path) else {
        if resolve_type_declaration(db, owner, &conformance.node).is_some() {
            findings.push(UnitObligation {
                kind: UnitObligationKind::InvalidConformanceTarget { name: Arc::from(name) },
                site,
            });
        }
        return;
    };
    let Some(contract_syntax) = db.syntax_unit(contract.unit).filter(|syntax| syntax.accepts_key(db, contract)) else {
        return;
    };
    let Some(contract_definition) = contract_syntax
        .syntax_index(db)
        .node_at(contract_syntax.expanded_program(db), contract.node)
        .and_then(|node| node.of::<ContractDefinition>())
    else {
        return;
    };
    let contract_name = Arc::<str>::from(contract_definition.name.node.name.as_str());
    for item in &contract_definition.items {
        let ContractNode::AssociatedType(associated) = &item.node else { continue };
        if associated.node.default.is_some() {
            continue;
        }
        let assoc_name = associated.node.name.node.name.as_str();
        if !bindings.iter().any(|binding| binding.node.name.node.name == assoc_name) {
            findings.push(UnitObligation {
                kind: UnitObligationKind::ContractAssociatedTypeMissingBinding {
                    contract_name: Arc::clone(&contract_name),
                    assoc_name: Arc::from(assoc_name),
                },
                site,
            });
        }
    }
    match contract_conformance_failure(db, concrete, contract) {
        Ok(Some(ContractConformanceFailure::MissingMethod { method_name, expected })) => {
            findings.push(UnitObligation {
                kind: UnitObligationKind::ContractMethodMissingImplementation {
                    contract_name,
                    method_name: Arc::from(method_name.as_str()),
                    expected: Arc::from(expected.as_str()),
                },
                site,
            });
        }
        Ok(Some(ContractConformanceFailure::SignatureMismatch { method_name, expected, actual })) => {
            findings.push(UnitObligation {
                kind: UnitObligationKind::ContractImplementationSignatureMismatch {
                    method_name: Arc::from(method_name.as_str()),
                    expected: Arc::from(expected.as_str()),
                    actual: Arc::from(actual.as_str()),
                },
                site,
            });
        }
        Ok(None) | Err(_) => {}
    }
}

/// The index of the unit's file-scoped module declaration: the first top-level `mod` without `pub`
/// and without attributes, which names the unit's own module instead of declaring a child
/// (`resolver::file_scoped_module_index`).
fn file_scoped_module_declaration(program: &Spanned<beskid_analysis::syntax::Program>) -> Option<usize> {
    program.node.items.iter().position(|item| match &item.node {
        beskid_analysis::syntax::Node::ModuleDeclaration(declaration) => {
            declaration.node.visibility.node == Visibility::Private && declaration.node.attributes.is_empty()
        }
        _ => false,
    })
}

fn path_names(path: &Path) -> Vec<String> {
    path.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect()
}

/// E1102 for a second `mod` declaration of one module path (`collect_item`, `Node::ModuleDeclaration`:
/// the declaration's tail is inserted into its parent module, so a repeated path collides). The
/// file-scoped declaration names the unit's own module and declares no child; it is excluded, as
/// in `collect_items`.
fn collect_module_declaration_obligations(
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<UnitObligation>,
) {
    let file_scoped = file_scoped_module_declaration(program).and_then(|position| {
        let item = program.node.items.get(position)?;
        let beskid_analysis::syntax::Node::ModuleDeclaration(declaration) = &item.node else { return None };
        index.ids_of_kind(NodeKind::ModuleDeclaration).find(|candidate| {
            module_scope_of_item(index, *candidate).is_some_and(|scope| index.kind(scope) == Some(NodeKind::Program))
                && index
                    .node_at(program, *candidate)
                    .and_then(|node| node.of::<ModuleDeclaration>())
                    .is_some_and(|node| std::ptr::eq(node, &declaration.node))
        })
    });
    let mut declarations = index
        .ids_of_kind(NodeKind::ModuleDeclaration)
        .filter(|node| Some(*node) != file_scoped)
        .filter_map(|node| {
            let declaration = index.node_at(program, node)?.of::<ModuleDeclaration>()?;
            Some((path_names(&declaration.path.node), node))
        })
        .collect::<Vec<_>>();
    declarations.sort_by_key(|(_, node)| node.0);
    let mut seen: HashMap<Vec<String>, beskid_analysis::syntax::AstNodeId> = HashMap::new();
    for (path, node) in declarations {
        match seen.get(&path) {
            Some(previous) => {
                let previous =
                    index.metadata_for(key.generation, *previous).and_then(|metadata| metadata.span).unwrap_or_default();
                findings.push(UnitObligation {
                    kind: UnitObligationKind::DuplicateItem {
                        name: Arc::from(path.last().map(String::as_str).unwrap_or_default()),
                        previous,
                    },
                    site: AstNodeKey { node, ..key },
                });
            }
            None => {
                seen.insert(path, node);
            }
        }
    }
}

/// The resolver namespace a module-scope declaration is collected into: items share the module
/// scope (`ModuleGraph::insert_item`), constants their own table (`Resolver::constants`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModuleNamespace {
    Item,
    Constant,
}

/// Every top-level declaration of a unit that the resolver collects into the unit's own module
/// scope, in source order: functions, types, enums, contracts, tests, inline modules, and
/// constants.
fn top_level_declarations(
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
) -> Vec<(ModuleNamespace, String, beskid_analysis::syntax::AstNodeId)> {
    let mut declarations = Vec::new();
    for kind in [
        NodeKind::FunctionDefinition,
        NodeKind::TypeDefinition,
        NodeKind::EnumDefinition,
        NodeKind::ContractDefinition,
        NodeKind::TestDefinition,
        NodeKind::InlineModule,
        NodeKind::ConstantDefinition,
    ] {
        for node in index.ids_of_kind(kind) {
            if module_scope_of_item(index, node).is_none_or(|scope| index.kind(scope) != Some(NodeKind::Program)) {
                continue;
            }
            let Some(reference) = index.node_at(program, node) else { continue };
            let declaration = if let Some(item) = reference.of::<FunctionDefinition>() {
                (ModuleNamespace::Item, item.name.node.name.clone())
            } else if let Some(item) = reference.of::<TypeDefinition>() {
                (ModuleNamespace::Item, item.name.node.name.clone())
            } else if let Some(item) = reference.of::<EnumDefinition>() {
                (ModuleNamespace::Item, item.name.node.name.clone())
            } else if let Some(item) = reference.of::<ContractDefinition>() {
                (ModuleNamespace::Item, item.name.node.name.clone())
            } else if let Some(item) = reference.of::<TestDefinition>() {
                (ModuleNamespace::Item, item.name.node.name.clone())
            } else if let Some(item) = reference.of::<InlineModule>() {
                (ModuleNamespace::Item, item.name.node.name.clone())
            } else if let Some(item) = reference.of::<ConstantDefinition>() {
                (ModuleNamespace::Constant, item.name.node.name.clone())
            } else {
                continue;
            };
            declarations.push((declaration.0, declaration.1, node));
        }
    }
    declarations.sort_by_key(|(_, _, node)| node.0);
    declarations
}

/// The assembly entry of the session that owns `syntax`. The legacy resolver collects the entry
/// at the root module (`collect_program`) and binds it into its logical module without
/// reporting collisions (`bind_root_items_in_module`), so it never takes part in a cross-unit
/// module collision.
fn session_entry(db: &dyn Db, syntax: SyntaxUnitInput) -> SourceUnitId {
    SourceUnitId::new(db, syntax.project(db).entry_path(db))
}

/// The logical module path and the units of that module, in assembly order, for `unit`.
fn logical_module(db: &dyn Db, key: AstNodeKey) -> Option<(Vec<String>, Vec<SourceUnitId>)> {
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    registry
        .modules
        .iter()
        .find(|((generation, _), units)| *generation == key.generation && units.contains(&key.unit))
        .map(|((_, path), units)| (path.clone(), units.clone()))
}

/// E1102 across the units of one logical module (`collect_program_in_module` collects every
/// non-entry unit of a module into one module scope, in assembly order; the later declaration
/// collides). The root unit judges its own declarations against the units collected before it,
/// so each collision is reported once, in the later unit. A root's second declaration of a name
/// it already declares is the in-unit duplicate and is not reported again. Units with a
/// file-scoped module declaration and the assembly entry are collected elsewhere and are not
/// judged.
fn collect_cross_unit_duplicate_obligations(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<UnitObligation>,
) {
    let entry = session_entry(db, syntax);
    if key.unit == entry || file_scoped_module_declaration(program).is_some() {
        return;
    }
    let Some((_, units)) = logical_module(db, key) else { return };
    let Some(position) = units.iter().position(|unit| *unit == key.unit) else { return };
    let mut earlier = Vec::new();
    for unit in &units[..position] {
        if *unit == entry {
            continue;
        }
        let Some(sibling) = db.syntax_unit(*unit).filter(|sibling| sibling.generation(db) == key.generation) else {
            continue;
        };
        let sibling_program = sibling.expanded_program(db);
        if file_scoped_module_declaration(sibling_program).is_some() {
            continue;
        }
        let sibling_index = sibling.syntax_index(db);
        for (namespace, name, node) in top_level_declarations(sibling_program, sibling_index) {
            let span = sibling_index.metadata_for(key.generation, node).and_then(|metadata| metadata.span);
            earlier.push((namespace, name, span.unwrap_or_default()));
        }
    }
    let mut own_names = HashSet::new();
    for (namespace, name, node) in top_level_declarations(program, index) {
        let first_in_unit = own_names.insert((namespace == ModuleNamespace::Constant, name.clone()));
        if !first_in_unit {
            continue;
        }
        let Some((_, _, previous)) =
            earlier.iter().find(|(earlier_namespace, earlier_name, _)| *earlier_namespace == namespace && *earlier_name == name)
        else {
            continue;
        };
        findings.push(UnitObligation {
            kind: UnitObligationKind::DuplicateItem { name: Arc::from(name.as_str()), previous: *previous },
            site: AstNodeKey { node, ..key },
        });
    }
}

/// E1102 for a module-scope item named like a manifest builtin (`collect_builtins` registers every
/// builtin as an item of its module before any unit is collected, so the unit's item collides).
/// The module a unit is collected into is its file-scoped module declaration, else the root
/// module for the assembly entry (`collect_program`), else its logical module path. The previous
/// span is the builtin's synthetic span, as in the legacy resolver.
fn collect_builtin_collision_obligations(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<UnitObligation>,
) {
    let module_path = if let Some(position) = file_scoped_module_declaration(program) {
        let Some(beskid_analysis::syntax::Node::ModuleDeclaration(declaration)) =
            program.node.items.get(position).map(|item| &item.node)
        else {
            return;
        };
        path_names(&declaration.node.path.node)
    } else if key.unit == session_entry(db, syntax) {
        Vec::new()
    } else {
        let Some((path, _)) = logical_module(db, key) else { return };
        path
    };
    let builtin_previous = SourceSpan { start: 0, end: 0, line_col_start: (1, 1), line_col_end: (1, 1) };
    for (namespace, name, node) in top_level_declarations(program, index) {
        if namespace != ModuleNamespace::Item {
            continue;
        }
        let mut path = module_path.clone();
        path.push(name.clone());
        if beskid_analysis::builtins::builtin_for_path(&path).is_some() {
            findings.push(UnitObligation {
                kind: UnitObligationKind::DuplicateItem { name: Arc::from(name.as_str()), previous: builtin_previous },
                site: AstNodeKey { node, ..key },
            });
        }
    }
}

/// Every `where` bound of every function and `impl` block of the unit, judged as the legacy
/// `type_item` judges them: the bound parameter must be one of the declaration's generics, and
/// the bound path must name a contract visible from the declaration.
fn collect_where_bound_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<UnitObligation>,
) {
    for kind in [NodeKind::FunctionDefinition, NodeKind::ImplBlock] {
        for node in index.ids_of_kind(kind) {
            let Some(reference) = index.node_at(program, node) else { continue };
            let (generics, bounds) = if let Some(function) = reference.of::<FunctionDefinition>() {
                (&function.generics, &function.where_bounds)
            } else if let Some(block) = reference.of::<ImplBlock>() {
                (&block.generics, &block.where_bounds)
            } else {
                continue;
            };
            let site = AstNodeKey { node, ..key };
            for bound in bounds {
                let parameter = bound.parameter.node.name.as_str();
                if !generics.iter().any(|generic| generic.node.name == parameter) {
                    findings.push(UnitObligation {
                        kind: UnitObligationKind::UnresolvedWhereBound { name: Arc::from(parameter) },
                        site,
                    });
                    continue;
                }
                if !where_bound_names_a_contract(db, program, index, site, &bound.contract.node) {
                    let name = bound
                        .contract
                        .node
                        .segments
                        .iter()
                        .map(|segment| segment.node.name.node.name.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    findings.push(UnitObligation {
                        kind: UnitObligationKind::UnresolvedWhereBound { name: Arc::from(name.as_str()) },
                        site,
                    });
                }
            }
        }
    }
}

/// Whether a `where` bound path names a contract from `declaration`. A single-segment path is
/// resolved through the lexical module scopes first, where the nearest declaration of the name
/// decides: a type or enum shadows any outer contract (the legacy surface resolves the bound
/// through the module's item namespace, not a contract-only namespace); only then do the unit's
/// imports (public contracts of other units) apply, through `resolve_contract`.
fn where_bound_names_a_contract(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    declaration: AstNodeKey,
    path: &Path,
) -> bool {
    let mut declaration_path = path.clone();
    if let Some(segment) = declaration_path.segments.last_mut() {
        segment.node.type_args.clear();
    }
    if let [segment] = declaration_path.segments.as_slice() {
        let name = segment.node.name.node.name.as_str();
        let declares = |scope: beskid_analysis::syntax::AstNodeId, kind: NodeKind| {
            index.ids_of_kind(kind).any(|candidate| {
                module_scope_of_item(index, candidate) == Some(scope)
                    && index.node_at(program, candidate).is_some_and(|node| {
                        node.of::<TypeDefinition>().is_some_and(|item| item.name.node.name == name)
                            || node.of::<EnumDefinition>().is_some_and(|item| item.name.node.name == name)
                            || node.of::<ContractDefinition>().is_some_and(|item| item.name.node.name == name)
                    })
            })
        };
        let mut scope = super::super::resolution::module_scope(index, declaration.node);
        while let Some(current) = scope {
            if declares(current, NodeKind::TypeDefinition) || declares(current, NodeKind::EnumDefinition) {
                return false;
            }
            if declares(current, NodeKind::ContractDefinition) {
                return true;
            }
            scope = super::super::resolution::outer_module_scope(index, current);
        }
    }
    if inline_module_contract(program, index, declaration.node, &declaration_path) {
        return true;
    }
    resolve_contract(db, declaration, &declaration_path).is_some()
}

/// Whether a qualified bound path names a contract of an inline module reachable from the
/// declaration's lexical scope chain (`mod A { contract Reader {..} ... where T: A.Reader }`),
/// as `resolve_inline_module_item_declaration` resolves callables. Visibility does not apply
/// inside the declaring unit.
fn inline_module_contract(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
    path: &Path,
) -> bool {
    let Some((terminal, module_path)) = path.segments.split_last() else { return false };
    if module_path.is_empty() {
        return false;
    }
    let name = terminal.node.name.node.name.as_str();
    let mut scope = super::super::resolution::module_scope(index, declaration);
    while let Some(start) = scope {
        let mut current = Some(start);
        for segment in module_path {
            current = current.and_then(|scope| {
                super::super::resolution::unique_inline_module_in_scope(
                    program,
                    index,
                    scope,
                    segment.node.name.node.name.as_str(),
                )
            });
        }
        if let Some(module) = current {
            return index.ids_of_kind(NodeKind::ContractDefinition).any(|candidate| {
                module_scope_of_item(index, candidate) == Some(module)
                    && index
                        .node_at(program, candidate)
                        .and_then(|node| node.of::<ContractDefinition>())
                        .is_some_and(|contract| contract.name.node.name == name)
            });
        }
        scope = super::super::resolution::outer_module_scope(index, start);
    }
    false
}
