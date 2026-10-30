use crate::syntax::Spanned;
use crate::syntax::{Type, Visibility};

use super::super::collect::ReexportRoute;
use super::super::ids::ItemId;
use super::super::items::ItemKind;
use super::super::resolver::Resolver;
use super::super::tables::ResolvedType;

/// How the inline-module reading of a qualified path `Name.Rest` relates to its other readings.
pub(super) enum InlineQualifier {
    /// `Name` is no inline module visible here, or that module has no item for `Rest`.
    Absent,
    /// The inline module `Name` visible here reaches the item, and no import alias `Name`
    /// reaches a different one.
    Found(ItemId),
    /// The inline module and the module-import alias `Name` (of `import_path`) both reach an item
    /// for `Rest`, and the items differ.
    Ambiguous { import_path: String },
}

pub(super) enum ModulePathLookup {
    Found(ItemId),
    ModuleMissing,
    NameMissing {
        module_path: String,
        name: String,
    },
    NotVisible {
        module_path: String,
        name: String,
    },
    /// The module reaches `candidates` distinct items of the name at one re-export distance.
    Ambiguous {
        module_path: String,
        name: String,
        candidates: usize,
    },
}
impl Resolver {
    pub(super) fn resolve_item_in_scope(&self, name: &str) -> Option<ItemId> {
        let mut current = Some(self.current_module);
        while let Some(module_id) = current {
            let module = self.module_graph.module(module_id)?;
            if let Some(item) = module.scope.get(name).copied() {
                return Some(item);
            }
            current = module.parent;
        }
        None
    }

    /// The module scopes lexically enclosing the current position, innermost first: the current
    /// module, each enclosing inline module, and the file scope.
    pub(crate) fn lexical_scopes(&self) -> Vec<super::super::ids::ModuleId> {
        let mut scopes = vec![self.current_module];
        let mut current = self.current_module;
        while self.inline_modules.contains(&current) {
            let Some(parent) = self.module_graph.module(current).and_then(|module| module.parent) else { break };
            scopes.push(parent);
            current = parent;
        }
        scopes
    }

    /// The scope of the module-import alias `alias` visible here: the nearest enclosing scope
    /// whose `use` binds it.
    pub(crate) fn module_import_scope(&self, alias: &str) -> Option<super::super::ids::ModuleId> {
        self.lexical_scopes().into_iter().find(|scope| self.module_imports.contains_key(&(*scope, alias.to_string())))
    }

    /// The module path the alias `alias` visible here names.
    pub(crate) fn module_import(&self, alias: &str) -> Option<&Vec<String>> {
        let scope = self.module_import_scope(alias)?;
        self.module_imports.get(&(scope, alias.to_string()))
    }

    /// The constant `name` of the nearest enclosing lexical scope that declares one: the current
    /// module, then each enclosing inline module, then the file scope. A constant of the file
    /// scope is visible in every inline module of the file, and a constant of an inline module
    /// shadows an outer one of the same name inside that module only. The walk stops at the file
    /// scope: a constant of another unit's module is not a lexical binding.
    pub(super) fn constant_in_scope(&self, name: &str) -> Option<&Spanned<crate::syntax::Literal>> {
        let mut current = self.current_module;
        loop {
            if let Some(value) = self.constants.get(&(current, name.to_string())) {
                return Some(value);
            }
            if !self.inline_modules.contains(&current) {
                return None;
            }
            current = self.module_graph.module(current)?.parent?;
        }
    }

    /// The type, enum or contract `name` names in a type position, from the nearest enclosing
    /// scope that binds a type of that name. A binding of another kind (a module, a function)
    /// neither satisfies nor hides a type name: the lookup continues past it, and an imported type
    /// that a same-scope module or function shadows for values is still the type here. The second
    /// value is the `use` that supplied such a shadowed import.
    pub(super) fn resolve_type_item_in_scope(&self, name: &str) -> Option<(ItemId, Option<crate::syntax::SpanInfo>)> {
        let mut current = Some(self.current_module);
        while let Some(module_id) = current {
            let module = self.module_graph.module(module_id)?;
            if let Some(item) = module.scope.get(name).copied()
                && self.is_type_item(item)
            {
                return Some((item, None));
            }
            if let Some((item, origin)) = self.type_imports_beside_declarations.get(&(module_id, name.to_string())) {
                return Some((*item, Some(*origin)));
            }
            current = module.parent;
        }
        None
    }

    /// The inline module `name` declared by the nearest enclosing lexical scope that binds `name`.
    pub(super) fn inline_module_in_scope(&self, name: &str) -> Option<super::super::ids::ModuleId> {
        for scope in self.lexical_scopes() {
            let module = self.module_graph.module(scope)?;
            let Some(item) = module.scope.get(name).copied() else { continue };
            if self.items.get(item.0).is_none_or(|info| info.kind != ItemKind::Module) {
                return None;
            }
            let mut path = module.path.clone();
            path.push(name.to_string());
            return self.module_graph.module_id(&path).filter(|id| self.inline_modules.contains(id));
        }
        None
    }

    /// The inline-module reading of the qualified path `segments` (see [`InlineQualifier`]).
    /// `alias_reading` is the result of the import-alias and module-registry lookup. In a type
    /// position only a type, enum or contract counts as reached.
    pub(super) fn inline_module_qualifier(
        &self,
        segments: &[String],
        alias_reading: &ModulePathLookup,
        type_position: bool,
    ) -> InlineQualifier {
        let Some((member, qualifier)) = segments.split_last() else { return InlineQualifier::Absent };
        let Some(module) = qualifier.first().and_then(|name| self.inline_module_in_scope(name)) else {
            return InlineQualifier::Absent;
        };
        let Some(mut module_path) = self.module_graph.module(module).map(|module| module.path.clone()) else {
            return InlineQualifier::Absent;
        };
        module_path.extend_from_slice(&qualifier[1..]);
        let ModulePathLookup::Found(inline_item) = self.lookup_named_item_in_module(&module_path, member) else {
            return InlineQualifier::Absent;
        };
        if type_position && !self.is_type_item(inline_item) {
            return InlineQualifier::Absent;
        }
        match (self.module_import(&qualifier[0]), alias_reading) {
            (Some(import_path), ModulePathLookup::Found(other)) if *other != inline_item => {
                InlineQualifier::Ambiguous { import_path: import_path.join(".") }
            }
            _ => InlineQualifier::Found(inline_item),
        }
    }

    pub(super) fn expand_import_alias(&self, segments: &[String]) -> Vec<String> {
        if segments.len() < 2 {
            return segments.to_vec();
        }
        let Some(module_path) = self.module_import(&segments[0]) else {
            return segments.to_vec();
        };
        let mut expanded = module_path.clone();
        expanded.extend_from_slice(&segments[1..]);
        expanded
    }

    pub(super) fn resolve_item_in_module_path(
        &self,
        original_segments: &[String],
        lookup_segments: &[String],
    ) -> ModulePathLookup {
        if lookup_segments.len() < 2 {
            return ModulePathLookup::ModuleMissing;
        }
        let primary = self.lookup_item_in_parent_module(lookup_segments);
        if matches!(primary, ModulePathLookup::Found(_)) {
            return primary;
        }

        // `use Console.Controls.ProgressBar; ProgressBar.ProgressBar.New()` — member in aliased module.
        if original_segments.len() >= 3
            && let Some(base_module) = self.module_import(&original_segments[0])
        {
            let member = &original_segments[original_segments.len() - 1];
            if let ModulePathLookup::Found(item) = self.lookup_named_item_in_module(base_module, member) {
                return ModulePathLookup::Found(item);
            }
        }

        // `Console.Controls.Panel.Panel.Render` — skip homonymous type segment in fully qualified paths.
        if original_segments.len() >= 4 {
            let member = &original_segments[original_segments.len() - 1];
            let module_path: Vec<String> = original_segments[..original_segments.len() - 2].to_vec();
            if let ModulePathLookup::Found(item) = self.lookup_named_item_in_module(&module_path, member) {
                return ModulePathLookup::Found(item);
            }
        }

        // `Concurrency.Channel`, `Ansi.StyleChain` — homonymous type in leaf module path.
        if let ModulePathLookup::Found(item) = self.lookup_homonymous_module_item(lookup_segments) {
            return ModulePathLookup::Found(item);
        }

        primary
    }

    pub(super) fn lookup_item_in_parent_module(&self, segments: &[String]) -> ModulePathLookup {
        let (module_path, tail) = segments.split_at(segments.len() - 1);
        self.lookup_named_item_in_module(module_path, &tail[0])
    }

    pub(super) fn lookup_named_item_in_module(&self, module_path: &[String], name: &str) -> ModulePathLookup {
        let Some(module_id) = self.module_graph.module_id(module_path) else {
            return ModulePathLookup::ModuleMissing;
        };
        let Some(module) = self.module_graph.module(module_id) else {
            return ModulePathLookup::ModuleMissing;
        };

        let module_path_string = module_path.join(".");
        if let Some(item) = module.scope.get(name).copied() {
            if !module_path.is_empty()
                && !self.module_encloses_current(module_id)
                && self.items.get(item.0).is_some_and(|info| info.visibility == Visibility::Private)
            {
                ModulePathLookup::NotVisible { module_path: module_path_string, name: name.to_string() }
            } else {
                ModulePathLookup::Found(item)
            }
        } else {
            // Not declared or imported by the module itself: reach it through the module's
            // public re-exports under the same rule `use Hub;` applies (`reexported_item_route`).
            match self.reexported_item_route(module_id, name) {
                ReexportRoute::Found(item) => ModulePathLookup::Found(item),
                ReexportRoute::Ambiguous(candidates) => {
                    ModulePathLookup::Ambiguous { module_path: module_path_string, name: name.to_string(), candidates }
                }
                ReexportRoute::Missing => {
                    ModulePathLookup::NameMissing { module_path: module_path_string, name: name.to_string() }
                }
            }
        }
    }

    /// A qualified path into the current module or one of its ancestors reaches the same private
    /// items the unqualified lookup (`resolve_item_in_scope`) reaches, so `mod A { contract R {}
    /// ... where T: A.R }` is not a cross-module access.
    fn module_encloses_current(&self, module_id: super::super::ids::ModuleId) -> bool {
        let mut current = Some(self.current_module);
        while let Some(candidate) = current {
            if candidate == module_id {
                return true;
            }
            current = self.module_graph.module(candidate).and_then(|module| module.parent);
        }
        false
    }

    /// When `Foo.Bar` names module `Foo.Bar` and public item `Bar` inside it.
    pub(super) fn lookup_homonymous_module_item(&self, segments: &[String]) -> ModulePathLookup {
        if segments.len() < 2 {
            return ModulePathLookup::ModuleMissing;
        }
        let item_name = segments[segments.len() - 1].clone();
        self.lookup_named_item_in_module(segments, &item_name)
    }

    /// The nominal item a method receiver names. `resolve_type` records a complex type under its
    /// path span, not the outer type span: a type-body method's receiver is re-spanned to the
    /// method (see `MethodDefinition::parse_with_receiver`) while its path keeps the definition
    /// span, so only the path span carries the resolution.
    pub(super) fn receiver_item_id_for_type(&self, receiver_type: &Spanned<Type>) -> Option<ItemId> {
        let Type::Complex(path) = &receiver_type.node else {
            return None;
        };
        match self.tables.resolved_types.get(&path.span) {
            Some(ResolvedType::Item(item_id)) => Some(*item_id),
            _ => None,
        }
    }

    /// The enclosing type-body method's unique sibling method named `name`, if any. Mirrors
    /// `unqualified_enclosing_method_call` in `beskid_queries`: an unqualified call inside a
    /// method declared in a `type X { }` body binds to the sibling method before any lexical or
    /// module lookup, and only when exactly one sibling carries the name.
    pub(super) fn is_unique_sibling_method(&self, name: &str) -> bool {
        self.current_type_sibling_methods
            .as_ref()
            .and_then(|methods| methods.get(name))
            .is_some_and(|count| *count == 1)
    }

    pub(super) fn receiver_has_field(&self, field_name: &str) -> bool {
        let Some(receiver_item_id) = self.current_receiver_item_id else {
            return false;
        };
        let Some(receiver) = self.items.get(receiver_item_id.0) else {
            return false;
        };
        let member_name = format!("{}::{}", receiver.name, field_name);
        self.items.iter().any(|info| info.kind == ItemKind::Field && info.name == member_name)
    }
}
