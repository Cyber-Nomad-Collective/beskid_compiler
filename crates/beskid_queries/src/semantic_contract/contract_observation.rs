//! Runtime property observation on a boxed contract receiver.
use super::contracts::{
    contract_declaration_for_identity, contract_method_slots, possible_property_conformer,
    specialized_source_expression_identity,
};
use super::*;
use beskid_analysis::syntax::{
    ContractDefinition, ContractMethodSignature, MemberExpression, ObserveStatement, PropertyAccessorKind,
    TypeDefinition,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind};

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractObservedProperty {
    pub receiver: AstNodeKey,
    pub signature: AstNodeKey,
    pub contract: AstNodeKey,
    pub metadata_offset: u32,
    pub name: String,
    pub slot: u32,
    pub value: Option<AstNodeKey>,
    pub abi: SemanticTypeId,
    pub managed: ManagedReferenceKind,
    pub compound: Option<(CompoundAssignmentOperator, u32)>,
    arguments: Arc<[GenericSourceTypeIdentity]>,
    pub(super) identity: GenericSourceTypeIdentity,
}

pub fn contract_observation(db: &dyn Db, site: AstNodeKey) -> SemanticQueryResult<ContractObservedProperty> {
    let Some(syntax) = db.syntax_unit(site.unit).filter(|s| s.accepts_key(db, site)) else { return Ok(None) };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(statement) = index.node_at(program, site.node).and_then(|n| n.of::<ObserveStatement>()) else {
        return Ok(None);
    };
    let target = index
        .direct_child_id(program, site.node, DynNodeRef::from(&statement.target))
        .ok_or_else(|| SemanticError::unavailable("observe.target"))?;
    let target = normalized_expression_node(index, target);
    contract_property_access(db, AstNodeKey { node: target, ..site })
}

/// Resolve a property read or assignment on a contract receiver to its typed witness slot.
pub fn contract_property_access(db: &dyn Db, site: AstNodeKey) -> SemanticQueryResult<ContractObservedProperty> {
    with_registered_syntax(db, site, contract_property_access_tracked)
}

#[salsa::tracked(persist)]
fn contract_property_access_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    site: AstNodeKey,
) -> SemanticQueryResult<ContractObservedProperty> {
    use beskid_analysis::syntax::{AssignExpression, PathExpression};
    if !syntax.accepts_key(db, site) {
        return Ok(None);
    }
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let direct = |node, child| {
        index
            .direct_child_id(program, node, child)
            .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..site })
    };
    let expr = normalized_expression_node(index, site.node);
    let Some(refnode) = index.node_at(program, expr) else { return Ok(None) };
    let (target, value, kind) = if let Some(assign) = refnode.of::<AssignExpression>() {
        let Some(key) = direct(expr, DynNodeRef::from(assign.target.as_ref())) else { return Ok(None) };
        (key.node, direct(expr, DynNodeRef::from(assign.value.as_ref())), PropertyAccessorKind::Set)
    } else {
        (expr, None, PropertyAccessorKind::Get)
    };
    let Some(target_expr) = index.node_at(program, target) else { return Ok(None) };
    let (receiver, name, identity) = if let Some(member) = target_expr.of::<MemberExpression>() {
        let Some(receiver) = direct(target, DynNodeRef::from(member.target.as_ref())) else { return Ok(None) };
        (receiver, &member.member.node.name, specialized_source_expression_identity(db, receiver, None).ok())
    } else if let Some(path) = target_expr.of::<PathExpression>() {
        let Some((last, prefix)) = path.path.node.segments.split_last() else { return Ok(None) };
        let Some(position) = prefix.len().checked_sub(1) else { return Ok(None) };
        let local = match prefix {
            [first] => resolve_lexical_declaration(program, index, target, &first.node.name.node.name),
            _ => None,
        };
        let (receiver, identity) = if let Some(local) = local {
            (
                AstNodeKey { node: local, ..site },
                super::calls::generic_source_local_identity(db, program, index, site, local).ok(),
            )
        } else {
            let path_key = AstNodeKey { node: target, ..site };
            let Some(receiver) = super::layouts::path_projection_segment(db, path_key, position) else {
                return Ok(None);
            };
            (receiver, super::contract_values::segment_identity(db, receiver, None))
        };
        (receiver, &last.node.name.node.name, identity)
    } else {
        return Ok(None);
    };
    let Some(GenericSourceTypeIdentity::Contract { qualified_name, arguments }) = identity else { return Ok(None) };
    let missing = || SemanticError::unavailable("observe.contract_property");
    let contract = contract_declaration_for_identity(db, site, &qualified_name).ok_or_else(missing)?;
    let methods = contract_method_slots(db, contract).ok_or_else(missing)?;
    let getter = kind.method_name(name);
    for (slot, key) in methods.iter().copied().enumerate() {
        let unit = db.syntax_unit(key.unit).ok_or_else(missing)?;
        let ix = unit.syntax_index(db);
        let p = unit.expanded_program(db);
        let sig = ix.node_at(p, key.node).and_then(|n| n.of::<ContractMethodSignature>()).ok_or_else(missing)?;
        if sig.name.node.name != getter {
            continue;
        }
        let owner = nearest_ancestor(ix, key.node, |kind| kind == NodeKind::ContractDefinition).ok_or_else(missing)?;
        let generics = &ix.node_at(p, owner).and_then(|n| n.of::<ContractDefinition>()).ok_or_else(missing)?.generics;
        if !generics.is_empty()
            && (generics.len() != arguments.len() || contract.node != owner || contract.unit != key.unit)
        {
            return Err(missing());
        }
        let env = generics.iter().zip(arguments.iter()).map(|(param, arg)| (param.node.name.as_str(), arg)).collect();
        let ty = match kind {
            PropertyAccessorKind::Get => sig.return_type.as_ref(),
            PropertyAccessorKind::Set => sig.parameters.first().map(|p| &p.node.ty),
        }
        .ok_or_else(missing)?;
        let identity = generic_source_type_identity_with_substitutions(db, key, &ty.node, &env)?;
        let embedded = super::contract_values::upcast_targets(db, contract).ok_or_else(missing)?;
        let word = WITNESS_HEADER_WORDS as usize + methods.len() + embedded.len() + slot * 2;
        let compound = refnode
            .of::<AssignExpression>()
            .and_then(|a| CompoundAssignmentOperator::of(a.op.node))
            .map(|op| {
                let getter = methods
                    .iter()
                    .position(|s| contract_getter_property_name(db, *s).as_ref() == Some(name))
                    .ok_or_else(missing)?;
                Ok::<_, SemanticError>((op, u32::try_from(getter).map_err(|_| missing())?))
            })
            .transpose()?;
        return Ok(Some(ContractObservedProperty {
            compound,
            receiver,
            arguments,
            contract,
            signature: key,
            name: name.clone(),
            slot: u32::try_from(slot).map_err(|_| missing())?,
            value,
            abi: identity.abi_type(),
            managed: identity.managed_reference_kind(),
            metadata_offset: u32::try_from(word * 8).map_err(|_| missing())?,
            identity,
        }));
    }
    Ok(None)
}

pub fn concrete_observation_property(db: &dyn Db, owner: AstNodeKey, name: &str) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(owner.unit)?;
    let p = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let ty = index.node_at(p, owner.node)?.of::<TypeDefinition>()?;
    let member = if let Some(f) = ty.fields.iter().find(|f| {
        f.node.name.node.name == name && (f.node.property_accessors.is_some() || f.node.property_default.is_some())
    }) {
        DynNodeRef::from(f)
    } else {
        DynNodeRef::from(ty.methods.iter().find(|m| m.node.is_property_getter && m.node.name.node.name == name)?)
    };
    let node = index.direct_child_id(p, owner.node, member)?;
    Some(AstNodeKey { node, ..owner })
}

pub(super) fn conforming_properties(
    db: &dyn Db,
    site: AstNodeKey,
    property: &ContractObservedProperty,
) -> Result<Vec<(AstNodeKey, AstNodeKey)>, SemanticError> {
    let mut units = HashSet::from([site.unit]);
    units.extend(
        db.syntax_dependency_registry()
            .lock()
            .expect("syntax dependency registry")
            .generation_units(site.generation)
            .iter()
            .copied(),
    );
    let mut out = Vec::new();
    for unit in units {
        let syntax = db.syntax_unit(unit).ok_or_else(|| SemanticError::unavailable("observe.contract_unit"))?;
        if syntax.generation(db) != site.generation {
            continue;
        }
        let index = syntax.syntax_index(db);
        for node in index.ids_of_kind(NodeKind::TypeDefinition) {
            let owner = AstNodeKey { unit, node, ..site };
            let Some(ty) = index.node_at(syntax.expanded_program(db), node).and_then(|n| n.of::<TypeDefinition>())
            else {
                continue;
            };
            let compatible = if !property.arguments.is_empty() && ty.generics.is_empty() {
                super::contracts::type_declaration_conforms_to_applied_contract(
                    db,
                    owner,
                    &[],
                    property.contract,
                    &property.arguments,
                )
            } else {
                possible_property_conformer(db, owner, ty, property.contract)
            };
            if compatible {
                let field = concrete_observation_property(db, owner, &property.name)
                    .ok_or_else(|| SemanticError::unavailable("observe.concrete_property"))?;
                out.push((owner, field));
            }
        }
    }
    Ok(out)
}

pub fn contract_getter_property_name(db: &dyn Db, declaration: AstNodeKey) -> Option<String> {
    contract_accessor_name(db, declaration).and_then(|(kind, name)| (kind == PropertyAccessorKind::Get).then_some(name))
}

pub(super) fn contract_accessor_name(db: &dyn Db, declaration: AstNodeKey) -> Option<(PropertyAccessorKind, String)> {
    let unit = db.syntax_unit(declaration.unit)?;
    let signature =
        unit.syntax_index(db).node_at(unit.expanded_program(db), declaration.node)?.of::<ContractMethodSignature>()?;
    [PropertyAccessorKind::Get, PropertyAccessorKind::Set]
        .into_iter()
        .find_map(|kind| kind.property_name(&signature.name.node.name).map(|name| (kind, name.to_owned())))
}
