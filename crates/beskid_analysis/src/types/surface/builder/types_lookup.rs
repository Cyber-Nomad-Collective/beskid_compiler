use crate::resolve::{ItemId, ResolvedType};
use crate::resolve::resolver::path_segments;
use crate::syntax::{Path, PrimitiveType, SpanInfo, Spanned, Type};
use crate::types::{TypeId, TypeInfo};

use super::state::{ModuleImport, TypeSurfaceBuilder};

impl<'a> TypeSurfaceBuilder<'a> {
    pub(super) fn type_references_ambiguous_module_import(&self, ty: &Spanned<Type>) -> bool {
        match &ty.node {
            Type::Primitive(_) => false,
            Type::Complex(path) => self.path_references_ambiguous_module_import(path),
            Type::Associated { contract, .. } => self.path_references_ambiguous_module_import(contract),
            Type::Array(inner) => self.type_references_ambiguous_module_import(inner),
            Type::Function { return_type, parameters } => {
                self.type_references_ambiguous_module_import(return_type)
                    || parameters.iter().any(|parameter| self.type_references_ambiguous_module_import(parameter))
            }
        }
    }

    pub(super) fn path_references_ambiguous_module_import(&self, path: &Spanned<Path>) -> bool {
        let segments = path_segments(path);
        if segments.len() >= 2 {
            for scope in self.module_import_scopes.iter().rev() {
                match scope.get(&segments[0]) {
                    Some(ModuleImport::Ambiguous) => return true,
                    Some(ModuleImport::Unique(_)) => break,
                    None => {}
                }
            }
        }
        path.node
            .segments
            .iter()
            .flat_map(|segment| &segment.node.type_args)
            .any(|argument| self.type_references_ambiguous_module_import(argument))
    }

    pub(super) fn type_id_for_type(&mut self, ty: &Spanned<Type>) -> Option<TypeId> {
        if let Type::Complex(path) = &ty.node
            && path.node.segments.len() == 1
            && path.node.segments[0].node.type_args.is_empty()
            && let Some(type_id) = self.generic_params.get(&path.node.segments[0].node.name.node.name)
        {
            return Some(*type_id);
        }
        match &ty.node {
            Type::Primitive(primitive) => self.primitive_type_id(primitive.node),
            Type::Complex(path) => self.type_id_for_path_with_args(path),
            Type::Associated { .. } => None,
            Type::Array(inner) => {
                let inner_id = self.type_id_for_type(inner)?;
                Some(self.types.find_array_of(inner_id).unwrap_or_else(|| self.types.intern(TypeInfo::Array(inner_id))))
            }
            Type::Function { return_type, parameters } => {
                let return_type = self.type_id_for_type(return_type)?;
                let mut params = Vec::with_capacity(parameters.len());
                for parameter in parameters {
                    params.push(self.type_id_for_type(parameter)?);
                }
                Some(self.types.intern(TypeInfo::Function { params, return_type }))
            }
        }
    }

    pub(super) fn type_id_for_path_with_args(&mut self, path: &Spanned<Path>) -> Option<TypeId> {
        let item_id = self.item_id_for_type_path(path)?;
        let base = self.named_type_id(item_id)?;
        let last = path.node.segments.last()?;
        if last.node.type_args.is_empty() {
            return Some(base);
        }
        let mut args = Vec::with_capacity(last.node.type_args.len());
        for arg in &last.node.type_args {
            args.push(self.type_id_for_type(arg)?);
        }
        Some(self.types.intern(TypeInfo::Applied { base: item_id, args }))
    }

    pub(super) fn item_id_for_type_path(&self, path: &Spanned<Path>) -> Option<ItemId> {
        if let Some(ResolvedType::Item(item_id)) = self.resolved_type_at(path.span) {
            return Some(item_id);
        }
        let mut segments: Vec<String> =
            path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect();
        if segments.len() >= 2 {
            for scope in self.module_import_scopes.iter().rev() {
                match scope.get(&segments[0]) {
                    Some(ModuleImport::Unique(imported)) => {
                        let mut expanded = imported.clone();
                        expanded.extend_from_slice(&segments[1..]);
                        segments = expanded;
                        break;
                    }
                    Some(ModuleImport::Ambiguous) => return None,
                    None => {}
                }
            }
        }
        if segments.len() >= 2 {
            let (module_path, tail) = segments.split_at(segments.len() - 1);
            if let Some(module_id) = self.resolution.module_graph.module_id(module_path)
                && let Some(module) = self.resolution.module_graph.module(module_id)
                && let Some(item_id) = module.scope.get(&tail[0])
            {
                return Some(*item_id);
            }
            if let Some(item_name) = segments.last()
                && let Some(module_id) = self.resolution.module_graph.module_id(&segments)
                && let Some(module) = self.resolution.module_graph.module(module_id)
                && let Some(item_id) = module.scope.get(item_name)
            {
                return Some(*item_id);
            }
        }
        if segments.len() == 1 {
            let name = &segments[0];
            return self.item_id_for_name(name, crate::resolve::ItemKind::Enum)
                .or_else(|| self.item_id_for_name(name, crate::resolve::ItemKind::Type));
        }
        None
    }

    pub(super) fn primitive_type_id(&self, primitive: PrimitiveType) -> Option<TypeId> {
        self.primitive_types.get(&primitive).copied()
    }

    pub(super) fn named_type_id(&self, item_id: ItemId) -> Option<TypeId> {
        self.named_types.get(&item_id).copied()
    }

    pub(super) fn named_item_id(&self, type_id: TypeId) -> Option<ItemId> {
        match self.types.get(type_id) {
            Some(TypeInfo::Named(item_id)) => Some(*item_id),
            Some(TypeInfo::Applied { base, .. }) => Some(*base),
            _ => None,
        }
    }

    pub(super) fn resolved_type_at(&self, span: SpanInfo) -> Option<ResolvedType> {
        // A unit surface must never consume the entry unit's unscoped span table. Different
        // source files reuse byte offsets, so an exact offset collision can otherwise turn a
        // dependency declaration such as `Stack<T>` into an unrelated entry type. Only an
        // explicitly source-scoped fact belongs to this surface; declaration-path lookup below
        // remains the authority when dependency bodies were not resolved.
        let mut resolved = None;
        for (source, types) in &self.resolution.tables.scoped_resolved_types {
            if !crate::paths::same_file(source, &self.source_path) {
                continue;
            }
            let Some(candidate) = types.get(&span).cloned() else {
                continue;
            };
            if resolved.as_ref().is_some_and(|existing| existing != &candidate) {
                return None;
            }
            resolved = Some(candidate);
        }
        resolved
    }
}
