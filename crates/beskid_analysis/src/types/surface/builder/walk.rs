use std::collections::{HashMap, HashSet};

use crate::resolve::collect::use_imported_name;
use crate::resolve::resolver::path_segments;
use crate::syntax::{ContractNode, Node, Spanned};
use crate::types::TypeInfo;

use super::super::model::FunctionBound;
use super::state::{ModuleImport, TypeSurfaceBuilder};

impl<'a> TypeSurfaceBuilder<'a> {
    pub(in crate::types::surface) fn walk_program(&mut self, program: &Spanned<crate::syntax::Program>) {
        self.push_module_import_scope(&program.node.items);
        for item in &program.node.items {
            self.walk_item(item);
        }
        self.seed_contract_signatures(program);
        for item in &program.node.items {
            match &item.node {
                Node::Method(def) => self.seed_method_receiver(item.span, def),
                Node::ImplBlock(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                Node::ExtendTypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                Node::TypeDefinition(def) => {
                    let Some(receiver_item_id) = self.canonical_item_id_for_span(item.span) else {
                        continue;
                    };
                    for method in &def.node.methods {
                        self.seed_owned_method_receiver(receiver_item_id, method.span, method);
                    }
                }
                _ => {}
            }
        }
        self.module_import_scopes.pop();
    }

    pub(super) fn push_module_import_scope(&mut self, items: &[Spanned<Node>]) {
        let mut imports = HashMap::new();
        let local_modules = items
            .iter()
            .filter_map(|item| match &item.node {
                Node::InlineModule(module) => Some(module.node.name.node.name.clone()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        for item in items {
            let Node::UseDeclaration(declaration) = &item.node else {
                continue;
            };
            let path = path_segments(&declaration.node.path);
            if path.is_empty() {
                continue;
            }
            let alias = use_imported_name(&declaration.node);
            if local_modules.contains(&alias) {
                imports.insert(alias, ModuleImport::Ambiguous);
                continue;
            }
            match imports.entry(alias) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(ModuleImport::Unique(path));
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.insert(ModuleImport::Ambiguous);
                }
            }
        }
        self.module_import_scopes.push(imports);
    }

    pub(super) fn walk_item(&mut self, item: &Spanned<Node>) {
        match &item.node {
            Node::Function(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                if let Some(item_id) = self.item_id_for_span(item.span) {
                    let bounds = def
                        .node
                        .where_bounds
                        .iter()
                        .map(|bound| {
                            let segments = path_segments(&bound.contract);
                            let contract_name = segments.join(".");
                            let contract = if segments.len() == 1 {
                                self.visible_contract_in_owner_scope(item_id, &segments[0])
                            } else {
                                self.item_id_for_type_path(&bound.contract)
                            }
                            .filter(|item| self.contract_visible_to_owner(item_id, *item));
                            FunctionBound { parameter: bound.parameter.node.name.clone(), contract_name, contract }
                        })
                        .collect::<Vec<_>>();
                    self.surface.function_bounds.insert(item_id, bounds);
                }
                self.register_foreign_function(item.span, &def.node);
            }
            Node::TypeDefinition(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                let mut inserted = Vec::new();
                for generic in &def.node.generics {
                    let name = generic.node.name.clone();
                    let type_id = self.types.intern(TypeInfo::GenericParam(name.clone()));
                    self.generic_params.insert(name.clone(), type_id);
                    inserted.push(name);
                }
                self.register_struct_definition(item.span, &def.node);
                for method in &def.node.methods {
                    self.register_foreign_method(method.span, method);
                }
                for name in inserted {
                    self.generic_params.remove(&name);
                }
            }
            Node::EnumDefinition(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                self.register_enum_definition(item.span, &def.node);
            }
            Node::ContractDefinition(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                if let Some(item_id) = self.item_id_for_span(item.span) {
                    let embedded = def
                        .node
                        .items
                        .iter()
                        .filter_map(|node| {
                            let ContractNode::Embedding(embedding) = &node.node else {
                                return None;
                            };
                            self.visible_contract_in_owner_scope(item_id, &embedding.node.name.node.name)
                        })
                        .collect::<Vec<_>>();
                    self.surface.contract_embeddings.insert(item_id, embedded);
                }
            }
            Node::ImplBlock(def) => {
                let previous = self.generic_params.clone();
                for generic in &def.node.generics {
                    let name = generic.node.name.clone();
                    let id = self.types.intern(TypeInfo::GenericParam(name.clone()));
                    self.generic_params.insert(name, id);
                }
                for method in &def.node.methods {
                    self.seed_generic_item(method.span, &def.node.generics);
                    if let Some(item_id) = self.item_id_for_span(method.span) {
                        let bounds = def
                            .node
                            .where_bounds
                            .iter()
                            .map(|bound| {
                                let segments = path_segments(&bound.contract);
                                let contract_name = segments.join(".");
                                // Method symbols name a receiver, not a lexical module. Use
                                // the source-scoped resolved bound reference rather than
                                // guessing its module from that receiver name.
                                let contract =
                                    match self.resolved_type_at(bound.contract.span) {
                                        Some(crate::resolve::tables::ResolvedType::Item(contract))
                                            if self.resolution.items.get(contract.0).is_some_and(|info| {
                                                info.kind == crate::resolve::ItemKind::Contract
                                            }) =>
                                        {
                                            Some(contract)
                                        }
                                        _ => None,
                                    };
                                FunctionBound { parameter: bound.parameter.node.name.clone(), contract_name, contract }
                            })
                            .collect();
                        self.surface.function_bounds.insert(item_id, bounds);
                    }
                    self.register_foreign_method(method.span, method);
                }
                self.generic_params = previous;
            }
            Node::ExtendTypeDefinition(def) => {
                for method in &def.node.methods {
                    self.register_foreign_method(method.span, method);
                }
            }
            Node::InlineModule(module) => {
                self.push_module_import_scope(&module.node.items);
                for nested in &module.node.items {
                    self.walk_item(nested);
                }
                self.module_import_scopes.pop();
            }
            _ => {}
        }
    }
}
