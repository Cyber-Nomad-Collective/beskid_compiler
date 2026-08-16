use crate::syntax::Spanned;
use crate::syntax::{EnumPath, Path, Type};

use super::super::errors::ResolveError;
use super::super::items::ItemKind;
use super::super::resolver::{self, Resolver};
use super::super::tables::{ResolvedType, ResolvedValue};
use super::lookup::ModulePathLookup;

impl Resolver {
    pub(super) fn resolve_type(&mut self, ty: &Spanned<Type>) {
        match &ty.node {
            Type::Primitive(_) => {}
            Type::Complex(path) => {
                for segment in &path.node.segments {
                    for type_arg in &segment.node.type_args {
                        self.resolve_type(type_arg);
                    }
                }
                self.resolve_type_path(path);
            }
            Type::Array(inner) => self.resolve_type(inner),
            Type::Function { return_type, parameters } => {
                self.resolve_type(return_type);
                for parameter in parameters {
                    self.resolve_type(parameter);
                }
            }
        }
    }

    pub(super) fn resolve_value_path(&mut self, path: &Spanned<Path>) {
        let segments = resolver::path_segments(path);
        if segments.is_empty() {
            self.errors.push(ResolveError::UnknownValue { name: "<unnamed>".to_string(), span: path.span });
            return;
        }
        if segments.len() == 1 {
            let name = &segments[0];
            if let Some(local) = self.resolve_local(name) {
                self.tables.insert_value(path.span, ResolvedValue::Local(local));
                return;
            }
            if self.receiver_has_field(name)
                && let Some(this_local) = self.resolve_local("this")
            {
                self.tables.insert_value(path.span, ResolvedValue::Local(this_local));
                return;
            }
            if let Some(item) = self.resolve_item_in_scope(name) {
                self.tables.insert_value(path.span, ResolvedValue::Item(item));
                return;
            }
            self.errors.push(ResolveError::UnknownValue { name: (*name).clone(), span: path.span });
            return;
        }
        if segments.len() >= 2 {
            if let Some(local) = self.resolve_local(&segments[0]) {
                self.tables.insert_value(path.span, ResolvedValue::Local(local));
                return;
            }
            if self.resolve_item_in_scope(&segments[0]).is_none()
                && self.module_graph.module_id(std::slice::from_ref(&segments[0])).is_none()
                && !self.module_imports.contains_key(&segments[0])
            {
                self.errors.push(ResolveError::UnknownValue { name: segments[0].clone(), span: path.span });
                return;
            }
        }
        let lookup_segments = self.expand_import_alias(&segments);
        match self.resolve_item_in_module_path(&segments, &lookup_segments) {
            ModulePathLookup::Found(item) => {
                self.tables.insert_value(path.span, ResolvedValue::Item(item));
            }
            ModulePathLookup::ModuleMissing => {
                if let Some(local) = self.resolve_local(&segments[0]) {
                    self.tables.insert_value(path.span, ResolvedValue::Local(local));
                } else if let Some(item) = self.resolve_item_in_scope(&segments[0])
                    && self.items.get(item.0).is_some_and(|info| info.kind == ItemKind::Contract)
                {
                    self.tables.insert_value(path.span, ResolvedValue::Item(item));
                } else {
                    self.errors.push(ResolveError::UnknownModulePath {
                        path: segments[..segments.len() - 1].join("::"),
                        span: path.span,
                    });
                }
            }
            ModulePathLookup::NameMissing { module_path, name } => {
                self.errors.push(ResolveError::UnknownValueInModule { module_path, name, span: path.span });
            }
            ModulePathLookup::NotVisible { module_path, name } => {
                self.errors.push(ResolveError::PrivateItemInModule { module_path, name, span: path.span });
            }
        }
    }

    pub(super) fn resolve_type_path(&mut self, path: &Spanned<Path>) {
        let segments = resolver::path_segments(path);
        if segments.is_empty() {
            self.errors.push(ResolveError::UnknownType { name: "<unnamed>".to_string(), span: path.span });
            return;
        }
        if segments.len() == 1 {
            let name = &segments[0];
            if self.is_generic(name) {
                self.tables.insert_type(path.span, ResolvedType::Generic(name.clone()));
                return;
            }
            if let Some(item) = self.resolve_item_in_scope(name) {
                self.tables.insert_type(path.span, ResolvedType::Item(item));
                return;
            }
            self.errors.push(ResolveError::UnknownType { name: (*name).clone(), span: path.span });
            return;
        }
        let lookup_segments = self.expand_import_alias(&segments);
        match self.resolve_item_in_module_path(&segments, &lookup_segments) {
            ModulePathLookup::Found(item) => {
                self.tables.insert_type(path.span, ResolvedType::Item(item));
            }
            ModulePathLookup::ModuleMissing => {
                self.errors.push(ResolveError::UnknownModulePath {
                    path: segments[..segments.len() - 1].join("::"),
                    span: path.span,
                });
            }
            ModulePathLookup::NameMissing { module_path, name } => {
                self.errors.push(ResolveError::UnknownTypeInModule { module_path, name, span: path.span });
            }
            ModulePathLookup::NotVisible { module_path, name } => {
                self.errors.push(ResolveError::PrivateItemInModule { module_path, name, span: path.span });
            }
        }
    }

    pub(super) fn resolve_enum_path(&mut self, path: &Spanned<EnumPath>) {
        self.resolve_type_path(&path.node.type_path);
        if let Some(resolved) = self.tables.resolved_types.get(&path.node.type_path.span).cloned() {
            self.tables.insert_type(path.span, resolved);
        }
    }
}
