use std::collections::{HashMap, HashSet};

use crate::resolve::collect::use_imported_name;
use crate::resolve::resolver::path_segments;
use crate::syntax::{Node, Spanned};
use crate::types::TypeInfo;

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
