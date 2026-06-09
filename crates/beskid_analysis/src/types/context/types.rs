use std::collections::HashMap;

use crate::hir::{HirPath, HirType};
use crate::resolve::{ItemKind, ResolvedType};
use crate::syntax::Spanned;
use crate::types::{TypeId, TypeInfo};

use super::context::{TypeContext, TypeError};

fn type_display_name(ty: &Spanned<HirType>) -> String {
    match &ty.node {
        HirType::Primitive(primitive) => format!("{:?}", primitive.node),
        HirType::Complex(path) => path_display_name(path),
        HirType::Array(inner) => format!("{}[]", type_display_name(inner)),
        HirType::Function {
            return_type,
            parameters,
        } => {
            let params = parameters
                .iter()
                .map(type_display_name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({params})", type_display_name(return_type))
        }
    }
}

fn path_display_name(path: &Spanned<HirPath>) -> String {
    let segments = &path.node.segments;
    if segments.is_empty() {
        return "<unnamed>".to_string();
    }
    let head = segments
        .iter()
        .take(segments.len().saturating_sub(1))
        .map(|segment| segment.node.name.node.name.as_str())
        .collect::<Vec<_>>()
        .join(".");
    let last = segments.last().expect("non-empty segments");
    let tail = last.node.name.node.name.as_str();
    let mut name = if head.is_empty() {
        tail.to_string()
    } else {
        format!("{head}.{tail}")
    };
    if !last.node.type_args.is_empty() {
        let args = last
            .node
            .type_args
            .iter()
            .map(type_display_name)
            .collect::<Vec<_>>()
            .join(", ");
        name.push('<');
        name.push_str(&args);
        name.push('>');
    }
    name
}

impl<'a> TypeContext<'a> {
    /// Resolve a type AST node while generic parameters from the enclosing item are in scope.
    pub(super) fn type_id_for_type_in_generic_scope(
        &mut self,
        ty: &Spanned<HirType>,
    ) -> Option<TypeId> {
        if let HirType::Complex(path) = &ty.node
            && path.node.segments.len() == 1
            && path.node.segments[0].node.type_args.is_empty()
            && let Some(type_id) = self
                .generic_params
                .get(&path.node.segments[0].node.name.node.name)
        {
            return Some(*type_id);
        }
        self.type_id_for_type(ty)
    }

    pub(super) fn type_id_for_type(&mut self, ty: &Spanned<HirType>) -> Option<TypeId> {
        match &ty.node {
            HirType::Primitive(primitive) => {
                let mapped = self.map_primitive(primitive.node);
                self.primitive_type_id(mapped)
            }
            HirType::Complex(path) => self.type_id_for_path_with_args(path),
            HirType::Array(inner) => {
                let inner_id = self.type_id_for_type(inner)?;
                if let Some(existing) = self.type_table.find_array_of(inner_id) {
                    Some(existing)
                } else {
                    Some(self.type_table.intern(TypeInfo::Array(inner_id)))
                }
            }
            HirType::Function {
                return_type,
                parameters,
            } => {
                let return_type = self.type_id_for_type(return_type)?;
                let mut params = Vec::with_capacity(parameters.len());
                for parameter in parameters {
                    params.push(self.type_id_for_type(parameter)?);
                }
                Some(self.type_table.intern(TypeInfo::Function {
                    params,
                    return_type,
                }))
            }
        }
    }

    /// Build a generic-parameter substitution map from explicit or inferred type arguments.
    pub(super) fn generic_substitution_mapping(
        &self,
        item_id: crate::resolve::ItemId,
        substitution: &[TypeId],
    ) -> HashMap<String, TypeId> {
        let Some(names) = self.generic_items.get(&item_id) else {
            return HashMap::new();
        };
        if names.len() != substitution.len() {
            return HashMap::new();
        }
        names
            .iter()
            .zip(substitution.iter())
            .map(|(name, type_id)| (name.clone(), *type_id))
            .collect()
    }

    fn item_id_for_type_path(&self, path: &Spanned<HirPath>) -> Option<crate::resolve::ItemId> {
        if let Some(ResolvedType::Item(item_id)) = self.resolved_type_at(path.span) {
            return Some(item_id);
        }
        let segments: Vec<String> = path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect();
        if segments.len() >= 2 {
            let (module_path, tail) = segments.split_at(segments.len() - 1);
            if let Some(module_id) = self.resolution.module_graph.module_id(module_path)
                && let Some(module) = self.resolution.module_graph.module(module_id)
                && let Some(item_id) = module.scope.get(&tail[0])
            {
                return Some(*item_id);
            }
            // Homonymous leaf module (`System.Syscall.SyscallError`, `Concurrency.Channel`, …).
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
            return self
                .item_id_for_name(name, ItemKind::Enum)
                .or_else(|| self.item_id_for_name(name, ItemKind::Type));
        }
        None
    }

    fn base_item_id_for_applied_path(
        &self,
        path: &Spanned<HirPath>,
    ) -> Option<crate::resolve::ItemId> {
        self.item_id_for_type_path(path).or_else(|| {
            let last_segment = path.node.segments.last()?;
            let name = last_segment.node.name.node.name.as_str();
            self.item_id_for_name(name, ItemKind::Enum)
                .or_else(|| self.item_id_for_name(name, ItemKind::Type))
        })
    }

    pub(super) fn intern_foreign_applied_type(&mut self, path: &Spanned<HirPath>) -> Option<TypeId> {
        let last_segment = path.node.segments.last()?;
        if last_segment.node.type_args.is_empty() {
            return None;
        }
        let base = self.base_item_id_for_applied_path(path).or_else(|| {
            self.foreign_applied_base_item_id(path)
        });
        let base = base?;
        let mut args = Vec::with_capacity(last_segment.node.type_args.len());
        for arg in &last_segment.node.type_args {
            let errors_before = self.errors.len();
            let type_id = self
                .type_id_for_type(arg)
                .or_else(|| self.foreign_type_arg_id(arg));
            if type_id.is_none() {
                self.errors.truncate(errors_before);
                return None;
            }
            args.push(type_id?);
        }
        Some(self.type_table.intern(crate::types::TypeInfo::Applied { base, args }))
    }

    fn foreign_applied_base_item_id(&self, path: &Spanned<HirPath>) -> Option<crate::resolve::ItemId> {
        let segments: Vec<String> = path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect();
        if segments.is_empty() {
            return None;
        }
        let qualified = segments.join("::");
        let leaf = segments.last().map(String::as_str).unwrap_or("");
        self.resolution
            .items
            .iter()
            .find(|info| {
                matches!(info.kind, ItemKind::Enum | ItemKind::Type)
                    && (info.name.as_str() == qualified.as_str()
                        || info.name.ends_with(&format!("::{leaf}")))
            })
            .map(|info| info.id)
    }

    fn foreign_type_arg_id(&self, ty: &Spanned<HirType>) -> Option<TypeId> {
        match &ty.node {
            HirType::Primitive(primitive) => {
                self.primitive_type_id(self.map_primitive(primitive.node))
            }
            HirType::Complex(path) => {
                let name = path.node.segments.last()?.node.name.node.name.as_str();
                let item_id = self.resolution.items.iter().find(|info| {
                    matches!(info.kind, ItemKind::Enum | ItemKind::Type)
                        && (info.name.as_str() == name || info.name.ends_with(&format!("::{name}")))
                })?;
                self.named_types.get(&item_id.id).copied()
            }
            _ => None,
        }
    }

    pub(super) fn type_id_for_path_with_args(&mut self, path: &Spanned<HirPath>) -> Option<TypeId> {
        if let Some(last_segment) = path.node.segments.last()
            && !last_segment.node.type_args.is_empty()
        {
            let Some(base) = self.base_item_id_for_applied_path(path) else {
                self.errors.push(TypeError::UnknownType {
                    span: path.span,
                    name: path_display_name(path),
                });
                return None;
            };
            if let Some(expected) = self.generic_items.get(&base)
                && expected.len() != last_segment.node.type_args.len()
            {
                self.errors.push(TypeError::GenericArgumentMismatch {
                    span: path.span,
                    expected: expected.len(),
                    actual: last_segment.node.type_args.len(),
                });
                return None;
            }
            let mut args = Vec::with_capacity(last_segment.node.type_args.len());
            for arg in &last_segment.node.type_args {
                let type_id = self.type_id_for_type(arg)?;
                args.push(type_id);
            }
            return Some(self.type_table.intern(TypeInfo::Applied { base, args }));
        }
        self.type_id_for_type_path(path)
    }

    pub(super) fn type_id_for_type_path(&mut self, path: &Spanned<HirPath>) -> Option<TypeId> {
        match self.resolved_type_at(path.span) {
            Some(ResolvedType::Item(item)) => {
                if let Some(expected) = self.generic_items.get(&item)
                    && !expected.is_empty()
                {
                    self.errors
                        .push(TypeError::MissingTypeArguments { span: path.span });
                    return None;
                }
                self.named_types.get(&item).copied()
            }
            Some(ResolvedType::Generic(name)) => self.generic_params.get(&name).copied(),
            None => {
                if path.node.segments.len() == 1
                    && path.node.segments[0].node.type_args.is_empty()
                    && let Some(type_id) = self
                        .generic_params
                        .get(&path.node.segments[0].node.name.node.name)
                {
                    return Some(*type_id);
                }
                if let Some(item_id) = self.item_id_for_type_path(path) {
                    if let Some(expected) = self.generic_items.get(&item_id)
                        && !expected.is_empty()
                    {
                        self.errors
                            .push(TypeError::MissingTypeArguments { span: path.span });
                        return None;
                    }
                    return self.named_types.get(&item_id).copied();
                }
                self.errors.push(TypeError::UnknownType {
                    span: path.span,
                    name: path_display_name(path),
                });
                None
            }
        }
    }
}
