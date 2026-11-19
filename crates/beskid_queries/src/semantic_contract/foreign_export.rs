//! Protect property storage at the direct C export boundary.

use std::collections::HashSet;

use beskid_analysis::syntax::{
    ContractDefinition, ContractMethodSignature, EnumDefinition, FunctionDefinition, PropertyAccessorKind, Type,
    TypeDefinition,
};

use super::contracts::{contract_declaration_for_identity, contract_method_slots};
use super::{
    AstNodeKey, Db, GenericSourceTypeIdentity, SemanticError, generic_source_type_identity, resolve_type_declaration,
};

pub fn validate_raw_c_export_signature(db: &dyn Db, key: AstNodeKey) -> Result<(), SemanticError> {
    let absent = || SemanticError::unavailable_at("raw C export signature", key);
    let syntax = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)).ok_or_else(absent)?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)
        .and_then(|node| node.of::<FunctionDefinition>())
        .ok_or_else(absent)?;
    let mut visited = HashSet::new();
    for parameter in &definition.parameters {
        if let Some(owner) = property_owner(db, key, &parameter.node.ty.node, &mut visited) {
            return Err(SemanticError::new(format!(
                "raw C export `{}` parameter `{}` reaches property-bearing type `{owner}`",
                definition.name.node.name, parameter.node.name.node.name
            )));
        }
    }
    if let Some(result) = &definition.return_type
        && let Some(owner) = property_owner(db, key, &result.node, &mut visited)
    {
        return Err(SemanticError::new(format!(
            "raw C export `{}` result reaches property-bearing type `{owner}`",
            definition.name.node.name
        )));
    }
    Ok(())
}

/// The direct C ABI passes the lowered representation of a nominal value. Follow its
/// authored fields and type arguments, since a surrounding ordinary type can carry it.
fn property_owner(db: &dyn Db, site: AstNodeKey, ty: &Type, visited: &mut HashSet<AstNodeKey>) -> Option<String> {
    let path = match ty {
        Type::Array(element) => return property_owner(db, site, &element.node, visited),
        Type::Function { return_type, parameters } => {
            return parameters
                .iter()
                .chain(std::iter::once(return_type.as_ref()))
                .find_map(|ty| property_owner(db, site, &ty.node, visited));
        }
        Type::Complex(path) => &path.node,
        Type::Primitive(_) | Type::Associated { .. } | Type::This => return None,
    };
    if let Some(owner) = path
        .segments
        .iter()
        .flat_map(|segment| &segment.node.type_args)
        .find_map(|argument| property_owner(db, site, &argument.node, visited))
    {
        return Some(owner);
    }
    let declaration = resolve_type_declaration(db, site, path).or_else(|| {
        let GenericSourceTypeIdentity::Contract { qualified_name, .. } =
            generic_source_type_identity(db, site, ty).ok()?
        else {
            return None;
        };
        contract_declaration_for_identity(db, site, &qualified_name)
    })?;
    if !visited.insert(declaration) {
        return None;
    }
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?;
    if let Some(definition) = node.of::<TypeDefinition>() {
        if definition.fields.iter().any(|field| field.node.is_property())
            || definition.methods.iter().any(|method| method.node.is_property_getter)
        {
            return Some(definition.name.node.name.clone());
        }
        return definition
            .fields
            .iter()
            .find_map(|field| property_owner(db, declaration, &field.node.ty.node, visited));
    }
    if let Some(enumeration) = node.of::<EnumDefinition>() {
        return enumeration
            .variants
            .iter()
            .flat_map(|variant| &variant.node.fields)
            .find_map(|field| property_owner(db, declaration, &field.node.ty.node, visited));
    }
    let contract = node.of::<ContractDefinition>()?;
    contract_method_slots(db, declaration)?
        .iter()
        .any(|signature| {
            let Some(syntax) = db.syntax_unit(signature.unit) else { return false };
            let Some(method) = syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), signature.node)
                .and_then(|node| node.of::<ContractMethodSignature>())
            else {
                return false;
            };
            let name = &method.name.node.name;
            PropertyAccessorKind::Get.property_name(name).is_some()
                || PropertyAccessorKind::Set.property_name(name).is_some()
        })
        .then(|| contract.name.node.name.clone())
}
