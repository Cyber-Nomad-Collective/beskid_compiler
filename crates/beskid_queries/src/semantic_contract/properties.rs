//! Semantic facts for stored and expression-bodied properties.

use super::contracts::{
    concrete_declaration, specialized_local_declaration_identity, specialized_source_expression_identity,
};
use super::layouts::{explicit_local_complex_type_path, is_implicit_receiver_name};
use super::*;
use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::parameter_defaults::{default_constant, default_fits};
use beskid_analysis::syntax::{
    AssignExpression, AssignOp, Field, MemberExpression, MethodDefinition, PathExpression, PathSegment,
    PropertyAccessor, PropertyAccessorKind, TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

mod derived_observation;
mod equality;
pub use derived_observation::observation_stored_dependencies;
pub(super) use derived_observation::{ObservationDependencyKind, checked_observation_dependencies};
pub use equality::observation_property_equality_instance;
pub(super) use equality::{observation_equality_instance, observation_has_bounded_equality, observation_property_value_identity};

/// A source reference to a property or its contextual backing storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PropertyReference {
    pub declaration: AstNodeKey,
    pub receiver: AstNodeKey,
    pub name: AstNodeKey,
    pub is_backing_field: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PropertyGetter {
    pub declaration: AstNodeKey,
    pub receiver: AstNodeKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PropertySetter {
    pub declaration: AstNodeKey,
    pub receiver: AstNodeKey,
    pub value: AstNodeKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(super) enum Accessor {
    Automatic,
    Custom(AstNodeKey),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(super) struct PropertyAccess {
    pub(super) reference: PropertyReference,
    pub(super) property_name: Arc<str>,
    pub(super) get: Option<Accessor>,
    pub(super) set: Option<Accessor>,
    pub(super) accessible: bool,
}

pub fn property_reference(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<PropertyReference> {
    property_reference_in_environment(db, key, None)
}

pub fn property_reference_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<PropertyReference> {
    Ok(property_access_in_environment(db, key, enclosing)?.filter(|fact| fact.accessible).map(|fact| fact.reference))
}

pub fn property_always(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<bool> {
    with_registered_syntax(db, key, |db, syntax, key| {
        with_node(db, syntax, key, |_, _, node| node.of::<Field>().map(|field| field.property_always))
    })
}

/// Resolve an observation site to the accessible property, retaining its concrete receiver.
pub fn observation_target(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<PropertyReference> {
    observation_target_in_environment(db, key, None)
}

pub fn observation_target_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<PropertyReference> {
    with_registered_syntax(db, key, |db, syntax, key| {
        if !syntax.accepts_key(db, key) { return Ok(None); }
        let index = syntax.syntax_index(db);
        let program = syntax.expanded_program(db);
        let Some(statement) = index.node_at(program, key.node).and_then(|node| node.of::<beskid_analysis::syntax::ObserveStatement>()) else {
            return Ok(None);
        };
        let target = index.direct_child_id(program, key.node, DynNodeRef::from(&statement.target))
            .ok_or_else(|| SemanticError::unavailable("observe.target"))?;
        let key = AstNodeKey { node: normalized_expression_node(index, target), ..key };
        let reference = property_reference_in_environment(db, key, enclosing)?;
        Ok(reference.filter(|reference| {
            matches!(node_kind(db, reference.declaration), Ok(Some(NodeKind::Field | NodeKind::MethodDefinition)))
        }))
    })
}

/// The type observed by the callback, including a scalar declared on an open generic owner.
pub(super) fn observation_value_identity(
    db: &dyn Db,
    target: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    if let Some(reference) = property_reference_in_environment(db, target, enclosing)? {
        if let Some(identity) = equality::observation_property_value_identity(
            db, target, reference.declaration, reference.receiver, enclosing,
        )? {
            return Ok(identity);
        }
    }
    specialized_source_expression_identity(db, target, enclosing)
}

pub struct ObservedPropertyInstance {
    pub owner: AstNodeKey,
    pub property: AstNodeKey,
    /// Dynamic contract delivery conservatively includes every possible conformer application.
    pub through_contract: bool,
    pub specialization: Option<AggregateSpecialization>,
}

/// Resolve subscriptions inside one reachable specialization before allocating owner layouts.
pub fn observed_property_instances(
    db: &dyn Db,
    root: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<Vec<ObservedPropertyInstance>> {
    with_registered_syntax(db, root, |db, syntax, key| {
        let mut observed = Vec::new();
        if !syntax.accepts_key(db, key) { return Ok(None); }
        let index = syntax.syntax_index(db);
        for node in index.ids_of_kind(NodeKind::ObserveStatement) {
            if !std::iter::successors(Some(node), |node| parent_node(index, *node)).any(|parent| parent == key.node) { continue; }
            let site = AstNodeKey { node, ..key };
            if let Some(property) = contract_observation(db, site)? {
                observed.extend(super::contract_observation::conforming_properties(db, site, &property)?
                    .into_iter().map(|(owner, property)| ObservedPropertyInstance {
                        owner, property, through_contract: true, specialization: None,
                    }));
                continue;
            }
            let Some(reference) = observation_target_in_environment(db, site, enclosing)? else {
                return Err(SemanticError::unavailable("observe.property"));
            };
            let property = reference.declaration;
            let owner_syntax = db.syntax_unit(property.unit).ok_or_else(|| SemanticError::unavailable("observe.owner"))?;
            let owner_node = nearest_ancestor(owner_syntax.syntax_index(db), property.node, |kind| kind == NodeKind::TypeDefinition)
                .ok_or_else(|| SemanticError::unavailable("observe.owner"))?;
            let (owner, layout) = observation_receiver_layout(db, site, reference.receiver, enclosing)?
                .ok_or_else(|| SemanticError::unavailable_at("observe.receiver_layout", site))?;
            if owner != (AstNodeKey { node: owner_node, ..property }) {
                return Err(SemanticError::unavailable_at("observe.owner", site));
            }
            observed.push(ObservedPropertyInstance {
                owner, property, through_contract: false, specialization: layout.specialization,
            });
        }
        Ok(Some(observed))
    })
}

pub fn observation_property_name(db: &dyn Db, property: AstNodeKey) -> SemanticQueryResult<Arc<str>> {
    with_registered_syntax(db, property, |db, syntax, key| {
        with_node(db, syntax, key, |_, _, node| {
            if let Some(field) = node.of::<Field>() {
                return Some(Arc::from(field.name.node.name.as_str()));
            }
            let method = node.of::<MethodDefinition>()?;
            method.property_accessor?.property_name(&method.name.node.name).map(Arc::from)
        })
    })
}

/// Display an applied property type only when the expression's source identity is proven.
pub fn property_type_display(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<String> {
    if property_reference(db, key)?.is_none() {
        return Ok(None);
    }
    match generic_source_expression_identity(db, key) {
        Ok(identity) => Ok(Some(identity.display_name())),
        Err(error) if error.is_unavailable() => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn property_getter(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<PropertyGetter> {
    property_getter_in_environment(db, key, None)
}

pub fn property_getter_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<PropertyGetter> {
    let Some(access) = property_access_in_environment(db, key, enclosing)?.filter(|fact| fact.accessible) else {
        return Ok(None);
    };
    Ok(match access.get {
        Some(Accessor::Custom(declaration)) => {
            Some(PropertyGetter { declaration, receiver: access.reference.receiver })
        }
        _ => None,
    })
}

/// Custom setter selected for the target of an assignment, with its receiver and RHS.
pub fn property_setter(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<PropertySetter> {
    property_setter_in_environment(db, key, None)
}

pub fn property_setter_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<PropertySetter> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else { return Ok(None) };
    with_node(db, syntax, key, |program, index, node| {
        let assignment = node.of::<AssignExpression>()?;
        let target = index.direct_child_id(program, key.node, DynNodeRef::from(assignment.target.as_ref()))?;
        let access =
            property_access_in_environment(db, AstNodeKey { node: target, ..key }, enclosing).ok().flatten()?;
        let Some(Accessor::Custom(declaration)) = access.set else { return None };
        if !access.accessible {
            return None;
        }
        let value = index.direct_child_id(program, key.node, DynNodeRef::from(assignment.value.as_ref()))?;
        let value = AstNodeKey { node: normalized_expression_node(index, value), ..key };
        Some(PropertySetter { declaration, receiver: access.reference.receiver, value })
    })
}

pub(super) fn property_access(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<PropertyAccess> {
    with_registered_syntax(db, key, property_access_tracked)
}

fn property_access_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<PropertyAccess> {
    let Some(enclosing) = enclosing else { return property_access(db, key) };
    with_registered_syntax(db, key, |db, syntax, key| property_access_for_site(db, syntax, key, Some(enclosing)))
}

/// Translate a bare, unshadowed `field` to the accessor's property name.
/// A spelling such as `this.field` still selects an ordinary member called field.
pub(in crate::semantic_contract) fn contextual_backing_name<'a>(
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
    name: &str,
) -> Option<&'a str> {
    if name != "field" || resolve_lexical_declaration(program, index, node, name).is_some() {
        return None;
    }
    let method = nearest_ancestor(index, node, |kind| kind == NodeKind::MethodDefinition)?;
    let method = index.node_at(program, method)?.of::<MethodDefinition>()?;
    method.property_accessor?.property_name(&method.name.node.name)
}

#[salsa::tracked(persist)]
fn property_access_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<PropertyAccess> {
    property_access_for_site(db, syntax, key, None)
}

fn property_access_for_site(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<PropertyAccess> {
    with_node(db, syntax, key, |program, index, _| {
        let node = normalized_expression_node(index, key.node);
        if !matches!(
            index.kind(node),
            Some(NodeKind::PathExpression | NodeKind::MemberExpression | NodeKind::PathSegment)
        ) {
            return None;
        }
        let key = AstNodeKey { node, ..key };
        let expression = index.node_at(program, node)?;
        let projection = if let Some(enclosing) = enclosing {
            aggregate_field_access_specialization(db, key, enclosing).ok().flatten()
        } else {
            nominal_field_projection(db, key).and_then(Result::ok).map(|(access, _)| access)
        };
        let (declaration, receiver, field_name) = if let Some(projection) = &projection {
            let name = projection.layout.fields.get(projection.index as usize)?.0.as_ref();
            (projection.declaration, projection.receiver, name)
        } else if let Some(Ok(FieldAccessReceiver { declaration, receiver, field_name, .. })) =
            field_access_receiver(db, program, index, key, expression, None, None)
        {
            (declaration, receiver, field_name)
        } else {
            unapplied_property_receiver(db, program, index, key, expression)?
        };
        let (name, is_backing_field) = if let Some(member) = expression.of::<MemberExpression>() {
            (index.direct_child_id(program, node, DynNodeRef::from(&member.member))?, false)
        } else if let Some(segment) = expression.of::<PathSegment>() {
            let name = index.direct_child_id(program, node, DynNodeRef::from(&segment.name))?;
            let backing = index.kind(receiver.node) == Some(NodeKind::MethodDefinition)
                && parent_node(index, node).and_then(|path| index.children(path)?.first().copied()) == Some(node)
                && contextual_backing_name(program, index, node, &segment.name.node.name).is_some();
            (name, backing)
        } else {
            let path = expression.of::<PathExpression>()?;
            let path_id = index.direct_child_id(program, node, DynNodeRef::from(&path.path))?;
            let last = path.path.node.segments.last()?;
            let segment = index.direct_child_id(program, path_id, DynNodeRef::from(last))?;
            let identifier = index.direct_child_id(program, segment, DynNodeRef::from(&last.node.name))?;
            (
                identifier,
                path.path.node.segments.len() == 1
                    && contextual_backing_name(program, index, node, &last.node.name.node.name).is_some(),
            )
        };
        let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
        let owner = syntax.expanded_program(db);
        let owner_index = syntax.syntax_index(db);
        let definition = owner_index.node_at(owner, declaration.node)?.of::<TypeDefinition>()?;
        let name = AstNodeKey { node: name, ..key };
        let declaration_node = |node| {
            let node = owner_index.direct_child_id(owner, declaration.node, node)?;
            Some(AstNodeKey { node, ..declaration })
        };
        let accessible = |visibility| {
            visibility == Visibility::Public
                || declaration.unit == crate::contribution_attribution::contribution_attributed_unit(db, key)
        };
        let field = definition.fields.iter().find(|field| field.node.name.node.name == field_name);
        let (member, get, set, accessible) = if let Some(field) = field {
            let (get, set) = if is_backing_field
                || (field.node.property_accessors.is_none() && field.node.property_default.is_some())
            {
                (Some(Accessor::Automatic), Some(Accessor::Automatic))
            } else {
                let accessors = field.node.property_accessors.as_ref()?;
                let accessor = |kind: PropertyAccessorKind, accessor| match accessor {
                    Some(PropertyAccessor::Automatic) => Some(Accessor::Automatic),
                    Some(PropertyAccessor::Custom) => {
                        let name = kind.method_name(field_name);
                        let method = definition.methods.iter().find(|method| method.node.name.node.name == name)?;
                        Some(Accessor::Custom(declaration_node(DynNodeRef::from(method))?))
                    }
                    None => None,
                };
                (
                    accessor(PropertyAccessorKind::Get, accessors.get.as_ref().map(|entry| entry.node)),
                    accessor(PropertyAccessorKind::Set, accessors.set.as_ref().map(|entry| entry.node)),
                )
            };
            (declaration_node(DynNodeRef::from(field))?, get, set, accessible(field.node.visibility.node))
        } else {
            let mut methods = definition.methods.iter().filter(|method| method.node.name.node.name == field_name);
            let method = methods.next()?;
            if !method.node.is_property_getter || methods.next().is_some() {
                return None;
            }
            let declaration = declaration_node(DynNodeRef::from(method))?;
            (declaration, Some(Accessor::Custom(declaration)), None, accessible(method.node.visibility.node))
        };
        Some(PropertyAccess {
            reference: PropertyReference { declaration: member, receiver, name, is_backing_field },
            property_name: Arc::from(field_name),
            get,
            set,
            accessible,
        })
    })
}

/// Source member resolution inside an unapplied type still has an exact declaration. It
/// does not require an executable layout (for example when navigating field in a generic body).
fn unapplied_property_receiver<'a>(
    db: &dyn Db,
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    expression: DynNodeRef<'a>,
) -> Option<(AstNodeKey, AstNodeKey, &'a str)> {
    let implicit = || {
        let method = nearest_ancestor(index, key.node, |kind| kind == NodeKind::MethodDefinition)?;
        Some((
            AstNodeKey { node: method_owner_node(program, index, method)?, ..key },
            AstNodeKey { node: method, ..key },
        ))
    };
    let local = |declaration| {
        let written_type = explicit_local_complex_type_path(program, index, declaration)?;
        let owner = resolve_type_declaration(db, key, written_type)?;
        Some((owner, AstNodeKey { node: declaration, ..key }))
    };
    if let Some(member) = expression.of::<MemberExpression>() {
        let receiver = index.direct_child_id(program, key.node, DynNodeRef::from(member.target.as_ref()))?;
        let receiver = AstNodeKey { node: normalized_expression_node(index, receiver), ..key };
        let (owner, _) = if implicit_method_receiver(db, receiver).ok().flatten().is_some() {
            implicit()?
        } else {
            let path = index.node_at(program, receiver.node)?.of::<PathExpression>()?;
            let [name] = path.path.node.segments.as_slice() else { return None };
            local(resolve_lexical_declaration(program, index, key.node, &name.node.name.node.name)?)?
        };
        return Some((owner, receiver, &member.member.node.name));
    }
    let path = expression.of::<PathExpression>()?;
    let (owner, receiver, name) = match path.path.node.segments.as_slice() {
        [name] if name.node.type_args.is_empty() => {
            let name = name.node.name.node.name.as_str();
            if resolve_lexical_declaration(program, index, key.node, name).is_some()
                || is_implicit_receiver_name(program, index, key, name)
            {
                return None;
            }
            let (owner, receiver) = implicit()?;
            (owner, receiver, contextual_backing_name(program, index, key.node, name).unwrap_or(name))
        }
        [receiver, name] if receiver.node.type_args.is_empty() && name.node.type_args.is_empty() => {
            let receiver = receiver.node.name.node.name.as_str();
            let (owner, receiver) = if is_implicit_receiver_name(program, index, key, receiver) {
                implicit()?
            } else {
                local(resolve_lexical_declaration(program, index, key.node, receiver)?)?
            };
            (owner, receiver, name.node.name.node.name.as_str())
        }
        _ => return None,
    };
    Some((owner, receiver, name))
}

/// A custom accessor on a generic owner uses the receiver's complete source type arguments.
pub fn property_accessor_specialization(
    db: &dyn Db,
    site: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let (declaration, receiver) = match node_kind(db, site)? {
        Some(IndexedNodeKind::AssignExpression) => {
            let Some(setter) = property_setter_in_environment(db, site, enclosing)? else { return Ok(None) };
            (setter.declaration, setter.receiver)
        }
        Some(IndexedNodeKind::PathExpression | IndexedNodeKind::MemberExpression | IndexedNodeKind::PathSegment) => {
            if simple_property_write_target(db, site)? == Some(true) {
                return Ok(None);
            }
            let Some(getter) = property_getter_in_environment(db, site, enclosing)? else { return Ok(None) };
            (getter.declaration, getter.receiver)
        }
        _ => return Ok(None),
    };
    observation_getter_specialization(db, site, declaration, receiver, enclosing)
}

/// Specialize a getter with the exact receiver at a backing-store write site.
pub fn observation_getter_specialization(
    db: &dyn Db,
    site: AstNodeKey,
    declaration: AstNodeKey,
    receiver: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(substitutions) = accessor_receiver_substitutions(db, site, declaration, receiver, enclosing)? else {
        return Ok(None);
    };
    let signature = accessor_signature(db, declaration, &substitutions)?;
    let specialization =
        GenericCallSpecialization { declaration, signature, substitutions, contract_witnesses: Arc::from([]) };
    generic_call_specialization_instance(db, specialization)
}

/// Read the getter only when the site consumes its value. A terminal segment can duplicate the
/// whole path, whereas an intermediate segment still supplies the receiver for a write.
pub(super) fn simple_property_write_target(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<bool> {
    with_registered_syntax(db, key, |db, syntax, key| {
        with_node(db, syntax, key, |program, index, _| {
            let mut target = key.node;
            if index.kind(target) == Some(NodeKind::PathSegment) {
                let path = parent_node(index, target)?;
                if index.children(path)?.last() != Some(&target) {
                    return Some(false);
                }
                target = parent_node(index, path)?;
            }
            while let Some(parent) = parent_node(index, target) {
                match index.kind(parent) {
                    Some(NodeKind::Expression | NodeKind::GroupedExpression) => target = parent,
                    Some(NodeKind::AssignExpression) if index.children(parent)?.first() == Some(&target) => {
                        return Some(
                            index.node_at(program, parent)?.of::<AssignExpression>()?.op.node == AssignOp::Assign,
                        );
                    }
                    _ => break,
                }
            }
            Some(false)
        })
    })
}

fn accessor_receiver_substitutions(
    db: &dyn Db,
    site: AstNodeKey,
    declaration: AstNodeKey,
    receiver: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<Arc<[GenericSubstitution]>> {
    let unavailable = || SemanticError::unavailable_at("property_accessor_specialization", site);
    let Some((generic_names, _)) = generic_callable_parameters(db, declaration) else {
        return Ok(None);
    };
    let owner = method_owner_declaration(db, declaration).ok_or_else(unavailable)?;
    let implicit = if node_kind(db, receiver)? == Some(IndexedNodeKind::MethodDefinition) {
        Some(receiver)
    } else {
        implicit_method_receiver(db, receiver).ok().flatten()
    };
    if let Some(method) = implicit {
        let enclosing = enclosing.filter(|enclosing| enclosing.declaration == method).ok_or_else(unavailable)?;
        if method_owner_declaration(db, method) != Some(owner) {
            return Err(unavailable());
        }
        return generic_names
            .iter()
            .map(|name| {
                enclosing
                    .substitutions
                    .iter()
                    .find(|binding| binding.parameter.as_ref() == *name)
                    .cloned()
                    .ok_or_else(unavailable)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|bindings| Some(bindings.into()));
    }
    let source = explicit_receiver_identity(db, receiver, enclosing)?;
    if concrete_declaration(db, site, &source) != Some(owner) {
        return Err(unavailable());
    }
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = source else { return Err(unavailable()) };
    if generic_names.len() != arguments.len() {
        return Err(unavailable());
    }
    Ok(Some(
        generic_names
            .iter()
            .zip(arguments.iter())
            .map(|(name, argument)| GenericSubstitution::from_source(*name, argument.abi_type(), argument.clone()))
            .collect::<Vec<_>>()
            .into(),
    ))
}

fn explicit_receiver_identity(
    db: &dyn Db,
    receiver: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    if node_kind(db, receiver)? == Some(IndexedNodeKind::Identifier) {
        let specialized =
            enclosing.and_then(|enclosing| specialized_local_declaration_identity(db, receiver, enclosing));
        match specialized {
            Some(source) => source,
            None => with_registered_syntax(db, receiver, |db, syntax, key| {
                with_node(db, syntax, key, |program, index, _| {
                    Some(generic_source_local_identity(db, program, index, key, key.node))
                })?
                .transpose()
            })?
            .ok_or_else(|| SemanticError::unavailable_at("property_accessor_receiver", receiver)),
        }
    } else {
        specialized_source_expression_identity(db, receiver, enclosing)
    }
}

/// The applied layout of the concrete object that owns the subscription.
pub fn observation_receiver_layout(
    db: &dyn Db,
    site: AstNodeKey,
    receiver: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<(AstNodeKey, AggregateLayoutFact)> {
    let Some(identity) = observation_receiver_identity(db, receiver, enclosing)? else { return Ok(None) };
    super::layouts::nominal_identity_layout(db, site, &identity).map(Some)
}

fn observation_receiver_identity(
    db: &dyn Db,
    receiver: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSourceTypeIdentity> {
    let implicit = if node_kind(db, receiver)? == Some(IndexedNodeKind::MethodDefinition) {
        Some(receiver)
    } else {
        implicit_method_receiver(db, receiver).ok().flatten()
    };
    if let Some(method) = implicit {
        let substitutions = enclosing
            .filter(|instance| instance.declaration == method)
            .into_iter()
            .flat_map(|instance| instance.substitutions.iter())
            .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
            .collect();
        with_registered_syntax(db, method, |db, syntax, key| {
            with_node(db, syntax, key, |_, _, node| {
                let method = node.of::<MethodDefinition>()?;
                Some(generic_source_type_identity_with_substitutions(db, key, &method.receiver_type.node, &substitutions))
            })?
            .transpose()
        })
    } else {
        explicit_receiver_identity(db, receiver, enclosing).map(Some)
    }
}

fn accessor_signature(
    db: &dyn Db,
    declaration: AstNodeKey,
    substitutions: &[GenericSubstitution],
) -> Result<ItemSignature, SemanticError> {
    let environment =
        substitutions.iter().map(|binding| (binding.parameter.as_ref(), binding.source_identity())).collect();
    with_registered_syntax(db, declaration, |db, syntax, key| {
        with_node(db, syntax, key, |_, _, node| {
            let method = node.of::<MethodDefinition>()?;
            let parameters = std::iter::once(Ok(SemanticTypeId::POINTER))
                .chain(method.parameters.iter().map(|parameter| {
                    generic_source_type_identity_with_substitutions(db, key, &parameter.node.ty.node, &environment)
                        .map(|identity| identity.abi_type())
                }))
                .collect::<Result<Vec<_>, _>>();
            let result = method.return_type.as_ref().map_or(Ok(SemanticTypeId::UNIT), |result| {
                generic_source_type_identity_with_substitutions(db, key, &result.node, &environment)
                    .map(|identity| identity.abi_type())
            });
            Some(
                parameters.and_then(|parameters| {
                    result.map(|result| ItemSignature { parameters: parameters.into(), result })
                }),
            )
        })?
        .transpose()
    })?
    .ok_or_else(|| SemanticError::unavailable("property_accessor_signature"))
}

pub(super) fn property_result_identity(
    db: &dyn Db,
    site: AstNodeKey,
    getter: PropertyGetter,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let substitutions = accessor_receiver_substitutions(db, site, getter.declaration, getter.receiver, enclosing)?;
    let environment = substitutions
        .iter()
        .flat_map(|bindings| bindings.iter())
        .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
        .collect();
    with_registered_syntax(db, getter.declaration, |db, syntax, key| {
        with_node(db, syntax, key, |_, _, node| {
            let result = node.of::<MethodDefinition>()?.return_type.as_ref()?;
            Some(generic_source_type_identity_with_substitutions(db, key, &result.node, &environment))
        })?
        .transpose()
    })?
    .ok_or_else(|| SemanticError::unavailable_at("property_result_identity", site))
}

/// Property defaults are indexed in their declaring unit. Emit the same typed expression
/// at each construction that omits the automatic property.
pub(super) fn automatic_property_defaults(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> SemanticQueryResult<Vec<(Arc<str>, AstNodeKey)>> {
    with_registered_syntax(db, declaration, |db, syntax, key| {
        with_node(db, syntax, key, |program, index, node| {
            let definition = node.of::<TypeDefinition>()?;
            definition
                .fields
                .iter()
                .filter_map(|field| field.node.property_default.as_ref().map(|value| (field, value)))
                .map(|(field, value)| {
                    let field_node = index.direct_child_id(program, key.node, DynNodeRef::from(field))?;
                    let value_node = index.direct_child_id(program, field_node, DynNodeRef::from(value))?;
                    let value_key = AstNodeKey { node: value_node, ..key };
                    default_matches_declaration(db, &field.node, value_key)
                        .then(|| (Arc::from(field.node.name.node.name.as_str()), value_key))
                })
                .collect()
        })
    })
}

/// A default must already be a literal whose representation matches the property's declared
/// type. The construction path can evaluate it without introducing hidden call edges.
fn default_matches_declaration(db: &dyn Db, field: &Field, value: AstNodeKey) -> bool {
    field.property_default.as_ref().and_then(|default| default_constant(default, &|_| None)).is_some_and(|literal| {
        default_fits(&literal, &field.ty.node)
            && expected_type(db, value)
                .ok()
                .flatten()
                .is_some_and(|expected| value_abi_type(db, value).ok().flatten() == Some(expected.abi_type()))
    })
}

pub(super) fn property_declaration_findings(db: &dyn Db, root: AstNodeKey) -> Vec<SemanticFinding> {
    with_registered_syntax(db, root, |db, syntax, key| {
        with_node(db, syntax, key, |program, index, _| {
            Some(
                index
                    .ids_of_kind(NodeKind::Field)
                    .into_iter()
                    .filter_map(|node| {
                        let field = index.node_at(program, node)?.of::<Field>()?;
                        let value =
                            index.direct_child_id(program, node, DynNodeRef::from(field.property_default.as_ref()?))?;
                        let site = AstNodeKey { node: value, ..key };
                        if default_matches_declaration(db, field, site) {
                            return None;
                        }
                        Some(SemanticFinding {
                            kind: SemanticIssueKind::TypeInvalidPropertyDefault {
                                name: field.name.node.name.clone(),
                                expected: beskid_analysis::doc::format_type(&field.ty),
                            },
                            site,
                            related: Vec::new(),
                        })
                    })
                    .collect(),
            )
        })
    })
    .ok()
    .flatten()
    .unwrap_or_default()
}
