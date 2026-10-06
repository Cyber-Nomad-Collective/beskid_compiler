//! Unit-level `check` obligations (E1102, E1220, E1601, E1602, E1607, E1608, E1609), judged once
//! per root unit: declaration shapes the legacy `TypeChecker` and resolver judged outside any
//! callable body.
//!
//! * E1102: the resolver's `collect_item` duplicates within one unit: two module-scope items
//!   (function, type, enum, contract, test, inline module) of one name in one module scope (the
//!   unit, or one inline module), two constants of one name in one module scope, a `type` method
//!   named like one of the type's fields, and two top-level `use` declarations binding one alias.
//!   Reported at the later declaration with the earlier one's span.
//! * E1220: an `event` field declared with capacity `0` (`register_struct_definition_fields`).
//! * E1601 / E1602 / E1607: every `type X : C` and `impl X : C` conformance of a non-generic
//!   implementor to a non-generic contract (`check_contract_conformances`), classified by
//!   `contract_conformance_failure`; a conformance path that names a type instead of a contract
//!   is the resolver's `InvalidConformanceTarget` (E1607).
//! * E1608 / E1609: `This` outside a method, contract, `impl`, or `type` scope, and every
//!   associated-type reference (`type_id_for_type`, which fails closed on `Type::Associated`).

use super::*;
use beskid_analysis::syntax::{
    AssociatedTypeBinding, ConstantDefinition, ContractDefinition, ContractNode, EnumDefinition, Field, FieldKind,
    FunctionDefinition, ImplBlock, InlineModule, Path, Spanned, TestDefinition, Type, TypeDefinition, UseDeclaration,
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
        if !definition.generics.is_empty() {
            continue;
        }
        let concrete = AstNodeKey { node, ..key };
        for conformance in &definition.conformances {
            judge_conformance(db, program, index, concrete, concrete, conformance, &definition.associated_type_bindings, findings);
        }
    }
    for node in index.ids_of_kind(NodeKind::ImplBlock) {
        let Some(block) = index.node_at(program, node).and_then(|node| node.of::<ImplBlock>()) else { continue };
        if !block.generics.is_empty() {
            continue;
        }
        let Type::Complex(receiver) = &block.receiver_type.node else { continue };
        let owner = AstNodeKey { node, ..key };
        let Some(concrete) = resolve_type_declaration(db, owner, &receiver.node) else { continue };
        if concrete.unit != key.unit {
            continue;
        }
        let Some(definition) = index.node_at(program, concrete.node).and_then(|node| node.of::<TypeDefinition>()) else {
            continue;
        };
        if !definition.generics.is_empty() {
            continue;
        }
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
    let Some(contract) = resolve_contract(db, owner, &conformance.node) else {
        if resolve_type_declaration(db, owner, &conformance.node).is_some() {
            findings.push(UnitObligation {
                kind: UnitObligationKind::InvalidConformanceTarget { name: Arc::from(name) },
                site,
            });
        }
        return;
    };
    if !terminal.node.type_args.is_empty() {
        return;
    }
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
    if !contract_definition.generics.is_empty() {
        return;
    }
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
