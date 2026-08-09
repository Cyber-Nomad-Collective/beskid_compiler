use crate::hir::{HirType, HirVisibility};
use crate::syntax::Spanned;

use super::super::ids::ItemId;
use super::super::items::ItemKind;
use super::super::resolver::Resolver;
use super::super::tables::ResolvedType;

pub(super) enum ModulePathLookup {
    Found(ItemId),
    ModuleMissing,
    NameMissing { module_path: String, name: String },
    NotVisible { module_path: String, name: String },
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

    pub(super) fn expand_import_alias(&self, segments: &[String]) -> Vec<String> {
        if segments.len() < 2 {
            return segments.to_vec();
        }
        let Some(module_path) = self.module_imports.get(&segments[0]) else {
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
            && let Some(base_module) = self.module_imports.get(&original_segments[0])
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

        let module_path_string = module_path.join("::");
        if let Some(item) = module.scope.get(name).copied() {
            if !module_path.is_empty()
                && self.items.get(item.0).is_some_and(|info| info.visibility == HirVisibility::Private)
            {
                ModulePathLookup::NotVisible { module_path: module_path_string, name: name.to_string() }
            } else {
                ModulePathLookup::Found(item)
            }
        } else {
            ModulePathLookup::NameMissing { module_path: module_path_string, name: name.to_string() }
        }
    }

    /// When `Foo.Bar` names module `Foo.Bar` and public item `Bar` inside it.
    pub(super) fn lookup_homonymous_module_item(&self, segments: &[String]) -> ModulePathLookup {
        if segments.len() < 2 {
            return ModulePathLookup::ModuleMissing;
        }
        let item_name = segments[segments.len() - 1].clone();
        self.lookup_named_item_in_module(segments, &item_name)
    }

    pub(super) fn receiver_item_id_for_type(&self, receiver_type: &Spanned<HirType>) -> Option<ItemId> {
        match self.tables.resolved_types.get(&receiver_type.span) {
            Some(ResolvedType::Item(item_id)) => Some(*item_id),
            _ => None,
        }
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
