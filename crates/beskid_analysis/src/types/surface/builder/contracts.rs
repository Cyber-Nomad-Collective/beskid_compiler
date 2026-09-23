use std::collections::{HashMap, HashSet};

use crate::resolve::ItemKind;
use crate::syntax::{ContractNode, PrimitiveType, Program, Spanned};
use crate::types::TypeInfo;
use crate::types::result::FunctionSignature;

use super::state::TypeSurfaceBuilder;

/// A contract's resolved method signatures, in declaration order, and the names of methods
/// whose signature does not resolve in this surface.
type ContractSignatureCollection = (Vec<(String, FunctionSignature)>, Vec<String>);

impl<'a> TypeSurfaceBuilder<'a> {
    pub(super) fn seed_contract_signatures(&mut self, program: &Spanned<Program>) {
        let definitions: HashMap<String, &Spanned<crate::syntax::ContractDefinition>> = program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                crate::syntax::Node::ContractDefinition(def) => Some((def.node.name.node.name.clone(), def)),
                _ => None,
            })
            .collect();
        let mut cache: HashMap<String, ContractSignatureCollection> = HashMap::new();
        let contract_names = definitions.keys().cloned().collect::<Vec<_>>();

        for contract_name in contract_names {
            // Mirror `TypeChecker::seed_contract_signatures`: the contract's own generics, the
            // synthetic `This`, and its declared associated types are generic parameters inside
            // its signatures. Without this scope a dependency contract's `This`/`Item`/`T`
            // fails to resolve here and its merged signature silently degrades to `unit`.
            let definition = definitions.get(contract_name.as_str()).copied();
            let mut inserted = Vec::new();
            let mut associated = Vec::new();
            if let Some(definition) = definition {
                let mut scope_names =
                    definition.node.generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>();
                scope_names.push("This".to_string());
                for node in &definition.node.items {
                    if let ContractNode::AssociatedType(assoc) = &node.node {
                        scope_names.push(assoc.node.name.node.name.clone());
                    }
                }
                for name in scope_names {
                    let type_id = self.types.intern(TypeInfo::GenericParam(name.clone()));
                    let previous = self.generic_params.insert(name.clone(), type_id);
                    inserted.push((name, previous));
                }
                for node in &definition.node.items {
                    if let ContractNode::AssociatedType(assoc) = &node.node {
                        let default = assoc.node.default.as_ref().and_then(|ty| self.type_id_for_type(ty));
                        associated.push((assoc.node.name.node.name.clone(), default));
                    }
                }
            }
            let (signatures, unresolved) = self.collect_contract_signatures_recursive(
                contract_name.as_str(),
                &definitions,
                &mut cache,
                &mut HashSet::new(),
            );
            for (name, previous) in inserted.into_iter().rev() {
                match previous {
                    Some(previous) => {
                        self.generic_params.insert(name, previous);
                    }
                    None => {
                        self.generic_params.remove(&name);
                    }
                }
            }
            let Some(contract_item_id) = self.item_id_for_name(&contract_name, ItemKind::Contract) else {
                continue;
            };
            if !associated.is_empty() {
                self.surface.contract_associated_types.insert(contract_item_id, associated);
            }
            if !unresolved.is_empty() {
                self.surface.contract_unresolved_methods.insert(contract_item_id, unresolved);
            }
            self.surface
                .contract_method_order
                .insert(contract_item_id, signatures.iter().map(|(name, _)| name.clone()).collect());
            for (method_name, signature) in signatures {
                self.surface.contract_signatures.insert((contract_item_id, method_name), signature);
            }
        }
    }

    pub(super) fn collect_contract_signatures_recursive(
        &mut self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::syntax::ContractDefinition>>,
        cache: &mut HashMap<String, ContractSignatureCollection>,
        active: &mut HashSet<String>,
    ) -> ContractSignatureCollection {
        if let Some(cached) = cache.get(contract_name) {
            return cached.clone();
        }
        if !active.insert(contract_name.to_string()) {
            return (Vec::new(), Vec::new());
        }

        let mut methods = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        let Some(definition) = definitions.get(contract_name) else {
            active.remove(contract_name);
            return (methods, unresolved);
        };

        for node in &definition.node.items {
            match &node.node {
                ContractNode::MethodSignature(signature) => {
                    if methods.iter().any(|(name, _)| name == &signature.node.name.node.name) {
                        continue;
                    }
                    if signature
                        .node
                        .parameters
                        .iter()
                        .any(|param| self.type_references_ambiguous_module_import(&param.node.ty))
                        || signature
                            .node
                            .return_type
                            .as_ref()
                            .is_some_and(|ty| self.type_references_ambiguous_module_import(ty))
                    {
                        continue;
                    }
                    // Fail closed: a parameter or return type that does not resolve in this
                    // surface is recorded as unresolved (the consuming checker reports E1201 at
                    // each conformance to this contract). It never reads as the omitted-return
                    // `unit`.
                    let method_name = signature.node.name.node.name.clone();
                    let mut params = Vec::new();
                    let mut valid = true;
                    for param in &signature.node.parameters {
                        let Some(type_id) = self.type_id_for_type(&param.node.ty) else {
                            valid = false;
                            break;
                        };
                        params.push(type_id);
                    }
                    let return_type = match signature.node.return_type.as_ref() {
                        Some(ty) => self.type_id_for_type(ty),
                        None => self.primitive_type_id(PrimitiveType::Unit),
                    };
                    match return_type {
                        Some(return_type) if valid => {
                            methods.push((method_name, FunctionSignature { params, return_type }));
                        }
                        _ => {
                            if !unresolved.contains(&method_name) {
                                unresolved.push(method_name);
                            }
                        }
                    }
                }
                ContractNode::Embedding(embedding) => {
                    let (embedded, embedded_unresolved) = self.collect_contract_signatures_recursive(
                        embedding.node.name.node.name.as_str(),
                        definitions,
                        cache,
                        active,
                    );
                    for (method_name, signature) in embedded {
                        if methods.iter().any(|(name, _)| name == &method_name) {
                            continue;
                        }
                        methods.push((method_name, signature));
                    }
                    for method_name in embedded_unresolved {
                        if !unresolved.contains(&method_name) {
                            unresolved.push(method_name);
                        }
                    }
                }
                // Associated-type declarations are not methods; not part of the cross-unit
                // merged signature cache (associated-type binding is resolved per-conformance-
                // site within a single unit's `TypeChecker::check_contract_conformances`).
                ContractNode::AssociatedType(_) => {}
            }
        }

        active.remove(contract_name);
        let collection = (methods, unresolved);
        cache.insert(contract_name.to_string(), collection.clone());
        collection
    }
}
