//! Marker attribute and compiler-derived contract legality at declarations, judged once per unit:
//! E1244 (`[Immutable]` field not Shared), E1246 (marker on a never-transferable type), E1247
//! (`[Isolated]`/`[Copyable]` field class), E1510 (exclusive markers, corelib-only
//! `[ImmutableStorage]`), E1607 (declared conformance to a `[CompilerDerived]` contract, or
//! `[CompilerDerived]` on a non-marker contract) and E1003 (a member of a derived contract).
//!
//! A field whose type mentions a type parameter of its declaration is judged per instantiation
//! by the transfer class query, never here.
use super::typing::nodes_of_kind;
use super::*;
use crate::semantic_contract::transfer::declared_field_class;
use beskid_analysis::syntax::{ContractDefinition, ContractNode, ImplBlock, Path, Spanned, TypeDefinition};

/// Whether the unit at `key` belongs to a `Core.*` module (corelib).
fn unit_in_core_module(db: &dyn Db, key: AstNodeKey) -> bool {
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    registry.modules.iter().any(|((generation, path), units)| {
        *generation == key.generation
            && path.first().is_some_and(|segment| segment == "Core")
            && units.contains(&key.unit)
    })
}

/// Whether the contract named by `path` at `key` carries the recognized `[CompilerDerived]`.
fn compiler_derived_contract(db: &dyn Db, key: AstNodeKey, path: &Path) -> Option<String> {
    let mut declaration_path = path.clone();
    if let Some(segment) = declaration_path.segments.last_mut() {
        segment.node.type_args.clear();
    }
    let contract = resolve_contract(db, key, &declaration_path)?;
    let syntax = db.syntax_unit(contract.unit).filter(|syntax| syntax.accepts_key(db, contract))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), contract.node)?
        .of::<ContractDefinition>()?;
    has_recognized_attribute(db, contract, &definition.attributes, "CompilerDerived")
        .then(|| definition.name.node.name.clone())
}

fn conformance_findings(db: &dyn Db, root: AstNodeKey, conformances: &[Spanned<Path>], findings: &mut Vec<SemanticFinding>) {
    for conformance in conformances {
        let site = AstNodeKey { node: conformance.id, ..root };
        if let Some(contract) = compiler_derived_contract(db, site, &conformance.node) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::CompilerDerivedConformance { contract },
                site,
                related: Vec::new(),
            });
        }
    }
}

fn contract_findings(db: &dyn Db, key: AstNodeKey, definition: &ContractDefinition, findings: &mut Vec<SemanticFinding>) {
    let Some(attribute) = recognized_attribute_use(db, key, &definition.attributes, "CompilerDerived") else { return };
    let contract = definition.name.node.name.clone();
    if TransferMarker::from_name(&contract).is_none() {
        findings.push(SemanticFinding {
            kind: SemanticIssueKind::CompilerDerivedNotMarker { contract: contract.clone() },
            site: AstNodeKey { node: attribute.id, ..key },
            related: Vec::new(),
        });
    }
    for item in &definition.items {
        let (member, id) = match &item.node {
            ContractNode::MethodSignature(signature) => (signature.node.name.node.name.clone(), signature.id),
            ContractNode::Embedding(embedding) => (embedding.node.name.node.name.clone(), embedding.id),
            ContractNode::AssociatedType(associated) => (associated.node.name.node.name.clone(), associated.id),
        };
        findings.push(SemanticFinding {
            kind: SemanticIssueKind::DerivedContractMember { contract: contract.clone(), member },
            site: AstNodeKey { node: id, ..key },
            related: Vec::new(),
        });
    }
}

/// Whether a Never class comes from a `pointer`, function-typed or Never field (E1246) rather
/// than from a marker that does not admit a transferable class (E1244/E1247).
fn never_cause(class: &TransferClass) -> bool {
    !class.is_transferable()
}

fn type_findings(db: &dyn Db, key: AstNodeKey, definition: &TypeDefinition, findings: &mut Vec<SemanticFinding>) {
    let type_name = definition.name.node.name.clone();
    let markers = marker_attributes(db, key, &definition.attributes);
    for window in markers.windows(2) {
        findings.push(SemanticFinding {
            kind: SemanticIssueKind::ExclusiveMarkerAttribute {
                marker: window[1].0.name().to_owned(),
                previous: markers[0].0.name().to_owned(),
            },
            site: window[1].1,
            related: Vec::new(),
        });
    }
    let Some(&(marker, marker_site)) = markers.first() else { return };
    if let Some(cause) = never_transferable_cause(db, key, &definition.attributes) {
        findings.push(SemanticFinding {
            kind: SemanticIssueKind::MarkerOnNeverTransferable {
                marker: marker.name().to_owned(),
                type_name,
                cause: cause.to_owned(),
            },
            site: marker_site,
            related: Vec::new(),
        });
        return;
    }
    for field in &definition.fields {
        let Ok(Some((class, immutable_storage))) = declared_field_class(db, key, &definition.generics, field) else {
            continue;
        };
        let field_name = field.node.name.node.name.clone();
        let field_site = AstNodeKey { node: field.id, ..key };
        if never_cause(&class) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::MarkerOnNeverTransferable {
                    marker: marker.name().to_owned(),
                    type_name: type_name.clone(),
                    cause: format!("field `{field_name}`"),
                },
                site: marker_site,
                related: vec![(field_site, "never-transferable field")],
            });
            continue;
        }
        if marker.admits(&class) || (marker == TransferMarkerAttribute::Immutable && immutable_storage) {
            continue;
        }
        let kind = match marker {
            TransferMarkerAttribute::Immutable => SemanticIssueKind::ImmutableFieldNotShared {
                type_name: render_type_syntax(&field.node.ty.node),
                field: field_name,
                class: class.name().to_owned(),
            },
            TransferMarkerAttribute::Isolated | TransferMarkerAttribute::Copyable => {
                SemanticIssueKind::MarkerFieldClassMismatch {
                    marker: marker.name().to_owned(),
                    type_name: type_name.clone(),
                    field: field_name,
                    class: class.name().to_owned(),
                }
            }
        };
        findings.push(SemanticFinding { kind, site: field_site, related: Vec::new() });
    }
}

/// The marker and derived-contract findings of the unit rooted at `root`, in node order.
pub fn transfer_declaration_findings(db: &dyn Db, root: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(root.unit).filter(|syntax| syntax.accepts_key(db, root)) else {
        return Vec::new();
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let mut findings = Vec::new();
    for node in nodes_of_kind(index, root.node, NodeKind::TypeDefinition) {
        let key = AstNodeKey { node, ..root };
        let Some(definition) = index.node_at(program, node).and_then(|node| node.of::<TypeDefinition>()) else {
            continue;
        };
        type_findings(db, key, definition, &mut findings);
        conformance_findings(db, key, &definition.conformances, &mut findings);
    }
    for node in nodes_of_kind(index, root.node, NodeKind::ImplBlock) {
        let key = AstNodeKey { node, ..root };
        if let Some(block) = index.node_at(program, node).and_then(|node| node.of::<ImplBlock>()) {
            conformance_findings(db, key, &block.conformances, &mut findings);
        }
    }
    for node in nodes_of_kind(index, root.node, NodeKind::ContractDefinition) {
        let key = AstNodeKey { node, ..root };
        if let Some(definition) = index.node_at(program, node).and_then(|node| node.of::<ContractDefinition>()) {
            contract_findings(db, key, definition, &mut findings);
        }
    }
    if !unit_in_core_module(db, root) {
        for node in nodes_of_kind(index, root.node, NodeKind::TypeDefinition) {
            let key = AstNodeKey { node, ..root };
            let Some(definition) = index.node_at(program, node).and_then(|node| node.of::<TypeDefinition>()) else {
                continue;
            };
            if let Some(attribute) = recognized_attribute_use(db, key, &definition.attributes, "ScopeBorrowed") {
                findings.push(SemanticFinding {
                    kind: SemanticIssueKind::CorelibOnlyAttribute { attribute: "ScopeBorrowed".to_owned() },
                    site: AstNodeKey { node: attribute.id, ..key },
                    related: Vec::new(),
                });
            }
        }
        for node in nodes_of_kind(index, root.node, NodeKind::Field) {
            let key = AstNodeKey { node, ..root };
            let Some(field) = index.node_at(program, node).and_then(|node| node.of::<beskid_analysis::syntax::Field>())
            else {
                continue;
            };
            if let Some(attribute) = recognized_attribute_use(db, key, &field.attributes, "ImmutableStorage") {
                findings.push(SemanticFinding {
                    kind: SemanticIssueKind::CorelibOnlyAttribute { attribute: "ImmutableStorage".to_owned() },
                    site: AstNodeKey { node: attribute.id, ..key },
                    related: Vec::new(),
                });
            }
        }
    }
    findings.sort_by_key(|finding| finding.site.node);
    findings
}

/// The field name a field read names: the member of `a.f`, or the last segment of the path
/// `a.f` (a path of at least two segments). `None` for every other expression.
fn projected_field_name<'a>(
    program: &'a Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<&'a str> {
    let found = index.node_at(program, node)?;
    if let Some(member) = found.of::<beskid_analysis::syntax::MemberExpression>() {
        return Some(member.member.node.name.as_str());
    }
    let path = found.of::<beskid_analysis::syntax::PathExpression>()?;
    match path.path.node.segments.as_slice() {
        [_, .., last] => Some(last.node.name.node.name.as_str()),
        _ => None,
    }
}

/// The `[Immutable]` owner type name, field name and `[ImmutableStorage]` flag of the field a
/// field read (`a.f` as a member access or a path) projects, when the projection's receiver is
/// an `[Immutable]` nominal type.
fn immutable_member_field(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<(String, String, bool)> {
    let field_name = projected_field_name(program, index, node)?;
    let site = AstNodeKey { node, ..key };
    // A two-segment path from a parameter or `let` root (`job.id`) keeps its aggregate field
    // fact; longer paths, member accesses and implicit-receiver reads are nominal projections.
    let declaration = match aggregate_field_access(db, site) {
        Ok(Some(access)) => access.declaration,
        _ => match nominal_field_projection(db, site) {
            Some(projection) => projection.ok()?.0.declaration,
            // `this.f` spelled as a path projects at its last segment.
            None => {
                let segments = index
                    .node_at(program, node)?
                    .of::<beskid_analysis::syntax::PathExpression>()?
                    .path
                    .node
                    .segments
                    .len();
                let segment = crate::semantic_contract::layouts::path_projection_segment(db, site, segments.checked_sub(1)?)?;
                nominal_field_projection(db, segment)?.ok()?.0.declaration
            }
        },
    };
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    let definition =
        syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?.of::<TypeDefinition>()?;
    if !marker_attributes(db, declaration, &definition.attributes)
        .iter()
        .any(|(marker, _)| *marker == TransferMarkerAttribute::Immutable)
    {
        return None;
    }
    let field = definition.fields.iter().find(|field| field.node.name.node.name == field_name)?;
    let storage = has_recognized_attribute(db, declaration, &field.node.attributes, "ImmutableStorage");
    Some((definition.name.node.name.clone(), field_name.to_owned(), storage))
}

/// The nearest ancestor of `node` that is not an expression wrapper (`Expression`, grouping).
fn context_node(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<(beskid_analysis::syntax::AstNodeId, beskid_analysis::syntax::AstNodeId)> {
    let mut child = node;
    let mut current = parent_node(index, node)?;
    while matches!(index.kind(current)?, NodeKind::Expression | NodeKind::GroupedExpression) {
        child = current;
        current = parent_node(index, current)?;
    }
    Some((current, child))
}

/// Whether passing the argument expression `argument` (an `Expression` node) to the call `call`
/// binds it to a parameter declared `mut` or `move`.
fn passed_to_mut_parameter(
    db: &dyn Db,
    key: AstNodeKey,
    call: beskid_analysis::syntax::AstNodeId,
    argument: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let call_key = AstNodeKey { node: call, ..key };
    let Ok(Some(arguments)) = call_arguments(db, call_key) else { return false };
    let Some(position) = arguments.iter().position(|candidate| candidate.node == argument) else { return false };
    let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, call_key) else { return false };
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return false;
    };
    let Some(node) = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node) else { return false };
    let parameter = if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        function.parameters.get(position)
    } else if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        position.checked_sub(1).and_then(|position| method.parameters.get(position))
    } else {
        None
    };
    parameter.is_some_and(|parameter| parameter.node.mutable || parameter.node.movable)
}

/// E1245 in the callable `item`: an assignment (`=`, `+=`, `-=`) whose target is a field of an
/// `[Immutable]` value, and an `[ImmutableStorage]` field that is returned, bound, stored,
/// captured by a lambda or passed to a `mut` parameter. Construction by struct literal is the
/// only way to set the fields.
pub fn immutable_field_findings(db: &dyn Db, item: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(item.unit).filter(|syntax| syntax.accepts_key(db, item)) else {
        return Vec::new();
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let mut findings = Vec::new();
    let mut assigned_targets = Vec::new();
    for assignment in nodes_of_kind(index, item.node, NodeKind::AssignExpression) {
        let Some(assign) =
            index.node_at(program, assignment).and_then(|node| node.of::<beskid_analysis::syntax::AssignExpression>())
        else {
            continue;
        };
        let Some((_, target)) = super::typing::expression_nodes(program, index, assignment, assign.target.as_ref())
        else {
            continue;
        };
        if projected_field_name(program, index, target).is_none() {
            continue;
        }
        assigned_targets.push(target);
        if let Some((type_name, field, _)) = immutable_member_field(db, program, index, item, target) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::ImmutableFieldAssignment { type_name, field },
                site: AstNodeKey { node: assignment, ..item },
                related: Vec::new(),
            });
        }
    }
    let mut reads = nodes_of_kind(index, item.node, NodeKind::MemberExpression);
    reads.extend(nodes_of_kind(index, item.node, NodeKind::PathExpression));
    for member in reads {
        if assigned_targets.contains(&member) || projected_field_name(program, index, member).is_none() {
            continue;
        }
        let Some((context, child)) = context_node(index, member) else { continue };
        let captured = super::typing::enclosing_callable(index, member)
            .is_some_and(|callable| index.kind(callable) == Some(NodeKind::LambdaExpression));
        let context_kind = index.kind(context);
        let candidate = captured
            || matches!(
                context_kind,
                Some(
                    NodeKind::ReturnStatement
                        | NodeKind::LetStatement
                        | NodeKind::AssignExpression
                        | NodeKind::StructLiteralField
                        | NodeKind::ArrayLiteralExpression
                        | NodeKind::CallExpression
                )
            );
        if !candidate {
            continue;
        }
        let Some((type_name, field, true)) = immutable_member_field(db, program, index, item, member) else { continue };
        let escapes = captured
            || match context_kind {
                Some(NodeKind::CallExpression) => passed_to_mut_parameter(db, item, context, child),
                _ => true,
            };
        if escapes {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::ImmutableFieldAssignment { type_name, field },
                site: AstNodeKey { node: member, ..item },
                related: Vec::new(),
            });
        }
    }
    findings.sort_by_key(|finding| finding.site.node);
    findings
}

