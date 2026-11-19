//! Equality used by observed writes, proven against the Foundation contract.

use super::*;
use crate::semantic_contract::contracts::{
    declared_where_bounds, implementation_method_instance, prove_box_conformance,
};
use beskid_analysis::projects::VerifiedPackageSource;
use beskid_analysis::syntax::{ContractDefinition, Type};

/// The concrete equality method for the property's exact applied value type.
pub fn observation_property_equality_instance(
    db: &dyn Db,
    site: AstNodeKey,
    declaration: AstNodeKey,
    receiver: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(identity) = observation_property_value_identity(db, site, declaration, receiver, enclosing)? else {
        return Ok(None);
    };
    observation_equality_instance(db, site, &identity)
}

pub(in crate::semantic_contract) fn observation_equality_instance(
    db: &dyn Db,
    site: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    if !matches!(identity, GenericSourceTypeIdentity::Nominal { .. }) {
        return Ok(None);
    }
    let Some(contract) = foundation_equatable(db, site) else { return Ok(None) };
    let Ok((_, substitutions, methods)) =
        prove_box_conformance(db, site, identity, contract, std::slice::from_ref(identity))
    else {
        return Ok(None);
    };
    let [(_, implementation)] = methods.as_ref() else { return Ok(None) };
    implementation_method_instance(db, *implementation, substitutions).map(Some)
}

/// Accept an open property type only when the enclosing callable promises equality for that
/// exact parameter. The concrete property and callback types remain deferred to specialization.
pub(in crate::semantic_contract) fn observation_has_bounded_equality(
    db: &dyn Db,
    site: AstNodeKey,
    property: PropertyReference,
) -> bool {
    let Some(contract) = foundation_equatable(db, site) else { return false };
    let Some((callable, parameter)) = observed_generic_parameter(db, site, property) else { return false };
    declared_where_bounds(db, callable).is_ok_and(|bounds| {
        bounds.iter().any(|bound| {
            bound.contract == contract
                && bound.generic == parameter
                && bound.path.segments.last().is_some_and(|segment| {
                    let [argument] = segment.node.type_args.as_slice() else { return false };
                    generic_parameter_reference_name(&argument.node) == Some(parameter.as_str())
                })
        })
    })
}

fn observed_generic_parameter(
    db: &dyn Db,
    site: AstNodeKey,
    property: PropertyReference,
) -> Option<(AstNodeKey, String)> {
    let declared =
        db.syntax_unit(property.declaration.unit).filter(|syntax| syntax.accepts_key(db, property.declaration))?;
    let index = declared.syntax_index(db);
    let program = declared.expanded_program(db);
    let member = index.node_at(program, property.declaration.node)?;
    let ty = match member.of::<Field>() {
        Some(field) => &field.ty.node,
        None => &member.of::<MethodDefinition>()?.return_type.as_ref()?.node,
    };
    let owner_parameter = generic_parameter_reference_name(ty)?;
    let owner = nearest_ancestor(index, property.declaration.node, |kind| kind == NodeKind::TypeDefinition)?;
    let definition = index.node_at(program, owner)?.of::<TypeDefinition>()?;
    let position = definition.generics.iter().position(|generic| generic.node.name == owner_parameter)?;

    let receiver = property.receiver;
    let syntax = db.syntax_unit(receiver.unit).filter(|syntax| syntax.accepts_key(db, receiver))?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let receiver_node = index.node_at(program, receiver.node)?;
    let receiver_type = if let Some(method) = receiver_node.of::<MethodDefinition>() {
        let Type::Complex(path) = &method.receiver_type.node else { return None };
        &path.node
    } else if let Some(method) = implicit_method_receiver(db, receiver).ok().flatten() {
        let method = index.node_at(program, method.node)?.of::<MethodDefinition>()?;
        let Type::Complex(path) = &method.receiver_type.node else { return None };
        &path.node
    } else {
        let local = if receiver_node.of::<beskid_analysis::syntax::Identifier>().is_some() {
            receiver.node
        } else {
            let path = receiver_node.of::<PathExpression>()?;
            let [segment] = path.path.node.segments.as_slice() else { return None };
            resolve_lexical_declaration(program, index, receiver.node, &segment.node.name.node.name)?
        };
        explicit_local_complex_type_path(program, index, local)?
    };
    if resolve_type_declaration(db, receiver, receiver_type)? != (AstNodeKey { node: owner, ..property.declaration }) {
        return None;
    }
    let arguments = &receiver_type.segments.last()?.node.type_args;
    if arguments.len() != definition.generics.len() {
        return None;
    }
    let parameter = generic_parameter_reference_name(&arguments.get(position)?.node)?;
    let syntax = db.syntax_unit(site.unit).filter(|syntax| syntax.accepts_key(db, site))?;
    let callable = nearest_ancestor(syntax.syntax_index(db), site.node, |kind| {
        matches!(kind, NodeKind::FunctionDefinition | NodeKind::MethodDefinition)
    })?;
    let callable = AstNodeKey { node: callable, ..site };
    let (generics, _) = generic_callable_parameters(db, callable)?;
    generics.contains(&parameter).then(|| (callable, parameter.to_owned()))
}

fn foundation_equatable(db: &dyn Db, site: AstNodeKey) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(site.unit).filter(|syntax| syntax.accepts_key(db, site))?;
    let project = syntax.project(db);
    let unit = {
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        let module = ["Core".to_owned(), "Equatable".to_owned()];
        let packages = registry.package_identities.get(&(project, site.generation))?;
        let mut units = registry.visible_module_units(site.generation, &module)?.iter().filter(|unit| {
            let path = unit.path(db);
            packages.for_source(&path).is_some_and(|owner| {
                owner.identity().package_name() == "corelib_foundation"
                    && match owner.identity().source() {
                        VerifiedPackageSource::Corelib => true,
                        VerifiedPackageSource::Local => packages.for_configured_corelib_source(&path).is_some(),
                        VerifiedPackageSource::Registry { .. } => false,
                    }
                    && owner.relative_source_path(&path).as_deref() == Some("Core/Equatable.bd")
            })
        });
        let unit = *units.next()?;
        if units.next().is_some() {
            return None;
        }
        unit
    };
    let syntax = db.syntax_unit(unit)?;
    if syntax.project(db) != project || syntax.generation(db) != site.generation {
        return None;
    }
    let index = syntax.syntax_index(db);
    let mut declarations = index.ids_named(NodeKind::ContractDefinition, "Equatable").filter(|node| {
        index
            .node_at(syntax.expanded_program(db), *node)
            .and_then(|item| item.of::<ContractDefinition>())
            .is_some_and(|contract| contract.visibility.node == Visibility::Public)
    });
    let node = declarations.next()?;
    declarations.next().is_none().then_some(AstNodeKey { unit, node, ..site })
}

pub(super) fn observation_property_value_identity(
    db: &dyn Db,
    site: AstNodeKey,
    declaration: AstNodeKey,
    receiver: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSourceTypeIdentity> {
    with_registered_syntax(db, declaration, |db, syntax, key| {
        with_node(db, syntax, key, |program, index, node| {
            if let Some(field) = node.of::<Field>().filter(|field| field.is_property()) {
                let owner = nearest_ancestor(index, key.node, |kind| kind == NodeKind::TypeDefinition)?;
                let definition = index.node_at(program, owner)?.of::<TypeDefinition>()?;
                let ty = &field.ty.node;
                let generics = definition.generics.iter().map(|generic| generic.node.name.as_str());
                if !needs_receiver_substitution(ty, generics) {
                    return Some(generic_source_type_identity(db, key, ty));
                }
                let owner = AstNodeKey { node: owner, ..key };
                let unavailable = || SemanticError::unavailable_at("observe.property_value_type", site);
                return Some(observation_receiver_identity(db, receiver, enclosing).and_then(|identity| {
                    let Some(identity) = identity else {
                        return Err(unavailable());
                    };
                    if concrete_declaration(db, site, &identity) != Some(owner) {
                        return Err(unavailable());
                    }
                    let GenericSourceTypeIdentity::Nominal { arguments, .. } = identity else {
                        return Err(unavailable());
                    };
                    if definition.generics.len() != arguments.len() {
                        return Err(unavailable());
                    }
                    let bindings = definition
                        .generics
                        .iter()
                        .map(|generic| generic.node.name.as_str())
                        .zip(arguments.iter())
                        .collect();
                    generic_source_type_identity_with_substitutions(db, key, ty, &bindings)
                }));
            }
            let method = node.of::<MethodDefinition>().filter(|method| method.is_property_getter)?;
            let ty = &method.return_type.as_ref()?.node;
            let generics = generic_callable_parameters(db, key).map_or_else(Vec::new, |(names, _)| names);
            if !needs_receiver_substitution(ty, generics) {
                return Some(generic_source_type_identity(db, key, ty));
            }
            Some(property_result_identity(db, site, PropertyGetter { declaration, receiver }, enclosing))
        })?
        .transpose()
    })
}

fn needs_receiver_substitution<'a>(ty: &Type, generics: impl IntoIterator<Item = &'a str>) -> bool {
    matches!(ty, Type::This | Type::Associated { .. })
        || generics.into_iter().any(|name| type_syntax_mentions_generic_parameter(ty, name))
}
