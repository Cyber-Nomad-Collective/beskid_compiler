use crate::hir::{HirExpressionNode, HirPrimitiveType};
use crate::resolve::{ItemId, ItemKind, ResolvedType, ResolvedValue, canonical_item_id};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::path_value::PathTypeEnv;
use crate::types::{TypeId, TypeInfo};

use super::context::{CastIntent, TypeContext, TypeError};
use std::collections::HashMap;

impl<'a> TypeContext<'a> {
    pub(super) fn path_env(&self) -> PathTypeEnv<'_> {
        PathTypeEnv {
            types: &self.type_table,
            local_types: &self.local_types,
            struct_fields_ordered: &self.struct_fields_ordered,
            generic_items: &self.generic_items,
        }
    }

    pub(super) fn resolved_value_at(&self, span: SpanInfo) -> Option<ResolvedValue> {
        let value = self
            .resolution
            .tables
            .resolved_value_at(span, self.current_source_path.as_ref())?;
        Some(match value {
            ResolvedValue::Item(item_id) => {
                ResolvedValue::Item(canonical_item_id(self.resolution, item_id))
            }
            other => other,
        })
    }

    pub(super) fn resolved_type_at(&self, span: SpanInfo) -> Option<ResolvedType> {
        self.resolution
            .tables
            .resolved_type_at(span, self.current_source_path.as_ref())
    }

    pub(super) fn seed_types(&mut self) {
        for primitive in [
            HirPrimitiveType::Bool,
            HirPrimitiveType::I32,
            HirPrimitiveType::I64,
            HirPrimitiveType::U8,
            HirPrimitiveType::F64,
            HirPrimitiveType::Char,
            HirPrimitiveType::String,
            HirPrimitiveType::Unit,
            HirPrimitiveType::Never,
        ] {
            let id = self.type_table.intern(TypeInfo::Primitive(primitive));
            self.primitive_types.insert(primitive, id);
        }

        for item in &self.resolution.items {
            match item.kind {
                crate::resolve::ItemKind::Type
                | crate::resolve::ItemKind::Enum
                | crate::resolve::ItemKind::Contract => {
                    let id = self.type_table.intern(TypeInfo::Named(item.id));
                    self.named_types.insert(item.id, id);
                }
                _ => {}
            }
        }
    }

    pub(super) fn infer_generic_args_from_call(
        &mut self,
        callee_item_id: Option<crate::resolve::ItemId>,
        args: &[Spanned<crate::hir::HirExpressionNode>],
    ) -> Option<Vec<TypeId>> {
        let item_id = callee_item_id?;
        let generic_names = self.generic_items.get(&item_id)?.clone();
        let expected_len = generic_names.len();
        if expected_len == 0 {
            return Some(Vec::new());
        }

        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.type_expression(arg)?);
        }

        crate::types::generic_inference::infer_generic_args_from_call_types(
            &self.type_table,
            &self.generic_items,
            &self.function_signatures,
            item_id,
            &arg_types,
        )
    }

    pub(super) fn infer_generic_args_from_qualified_type_path(
        &mut self,
        segments: &[Spanned<crate::hir::HirPathSegment>],
    ) -> Option<Vec<TypeId>> {
        if segments.len() < 2 {
            return None;
        }
        let type_segment = &segments[segments.len() - 2];
        if type_segment.node.type_args.is_empty() {
            return None;
        }
        let mut args = Vec::with_capacity(type_segment.node.type_args.len());
        for arg in &type_segment.node.type_args {
            args.push(self.type_id_for_type(arg)?);
        }
        Some(args)
    }

    pub(super) fn method_dispatch_signature(
        &mut self,
        method_item_id: crate::resolve::ItemId,
        receiver_type: TypeId,
    ) -> Option<super::context::FunctionSignature> {
        let signature = self
            .method_function_signatures
            .get(&method_item_id)
            .or_else(|| self.function_signatures.get(&method_item_id))?
            .clone();
        let mapping = self.generic_mapping_for_type_id(receiver_type);
        if mapping.is_empty() {
            return Some(signature);
        }
        Some(super::context::FunctionSignature {
            params: signature
                .params
                .iter()
                .map(|param| self.substitute_type_id(*param, &mapping))
                .collect(),
            return_type: self.substitute_type_id(signature.return_type, &mapping),
        })
    }

    pub(super) fn fiber_handle_type_for_payload(&self, payload: TypeId) -> Option<TypeId> {
        if let Some(expected) = self.contextual_expected_type
            && let Some(TypeInfo::Applied { base, args }) = self.type_table.get(expected)
                && args.len() == 1
                    && args[0] == payload
                    && self
                        .resolution
                        .items
                        .get(base.0)
                        .is_some_and(|info| info.name == "Fiber" || info.name.ends_with("::Fiber"))
                {
                    return Some(expected);
                }
        None
    }

    pub(super) fn record_call_kind(
        &mut self,
        span: SpanInfo,
        kind: super::context::CallLoweringKind,
    ) {
        if let Some(path) = &self.current_source_path {
            let key = crate::paths::unit_path_key(path);
            self.scoped_call_kinds
                .entry(key)
                .or_default()
                .insert(span, kind);
        } else {
            self.call_kinds.insert(span, kind);
        }
    }

    pub(super) fn record_expr_type(&mut self, span: SpanInfo, type_id: TypeId) {
        if let Some(path) = &self.current_source_path {
            let key = crate::paths::unit_path_key(path);
            self.scoped_expr_types
                .entry(key)
                .or_default()
                .insert(span, type_id);
        } else {
            self.expr_types.insert(span, type_id);
        }
    }

    /// Commit flat per-unit maps into scoped storage after best-effort dependency typing.
    pub(super) fn flush_scoped_type_maps_for_current_path(&mut self) {
        let Some(path) = self.current_source_path.clone() else {
            return;
        };
        let key = crate::paths::unit_path_key(&path);
        if !self.expr_types.is_empty() {
            self.scoped_expr_types
                .entry(key.clone())
                .or_default()
                .extend(self.expr_types.drain());
        }
        if !self.call_kinds.is_empty() {
            self.scoped_call_kinds
                .entry(key)
                .or_default()
                .extend(self.call_kinds.drain());
        }
    }

    pub(super) fn insert_local_type(&mut self, span: SpanInfo, type_id: TypeId) {
        if let Some(local_id) = self.local_id_for_span(span) {
            self.local_types.insert(local_id, type_id);
        } else {
            self.errors.push(TypeError::UnknownValueType { span });
        }
    }

    pub(super) fn local_id_for_span(&self, span: SpanInfo) -> Option<crate::resolve::LocalId> {
        self.resolution
            .tables
            .local_id_for_span(span, self.current_source_path.as_ref())
    }

    pub(super) fn item_id_for_span(&self, span: SpanInfo) -> Option<crate::resolve::ItemId> {
        if let Some(path) = &self.current_source_path
            && let Some(info) = self.resolution.items.iter().find(|info| {
                info.span == span
                    && info
                        .source_path
                        .as_ref()
                        .is_some_and(|source| crate::paths::same_file(source, path))
            }) {
                return Some(info.id);
            }

        let matches: Vec<_> = self
            .resolution
            .items
            .iter()
            .filter(|info| info.span == span)
            .collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            _ => None,
        }
    }

    pub(super) fn item_id_for_name(&self, name: &str, kind: ItemKind) -> Option<ItemId> {
        let matches: Vec<_> = self
            .resolution
            .items
            .iter()
            .filter(|info| info.name == name && info.kind == kind)
            .collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            many => {
                if let Some(path) = &self.current_source_path
                    && let Some(info) = many.iter().rev().find(|info| {
                        info.source_path
                            .as_ref()
                            .is_some_and(|source| crate::paths::same_file(source, path))
                    }) {
                        return Some(info.id);
                    }
                // Entry-unit symbols are collected after dependency prefetch; prefer the last match.
                many.last().map(|info| info.id)
            }
        }
    }

    pub(super) fn named_item_id(&self, type_id: TypeId) -> Option<ItemId> {
        match self.type_table.get(type_id) {
            Some(TypeInfo::Named(item_id)) => Some(*item_id),
            Some(TypeInfo::Applied { base, .. }) => Some(*base),
            _ => None,
        }
    }

    pub(super) fn ok_variant_name(&self, enum_item_id: ItemId, variant: &str) -> Option<String> {
        self.enum_variants_ordered
            .get(&enum_item_id)?
            .iter()
            .find(|(name, _)| name == variant)
            .map(|(name, _)| name.clone())
    }

    pub(super) fn method_item_for_receiver(
        &self,
        receiver_type: TypeId,
        method_name: &str,
    ) -> Option<ItemId> {
        let receiver_item = self.named_item_id(receiver_type)?;
        self.methods_by_receiver
            .get(&(receiver_item, method_name.to_string()))
            .copied()
            .map(|item| canonical_item_id(self.resolution, item))
    }

    pub(super) fn generic_mapping_for_type_id(&self, type_id: TypeId) -> HashMap<String, TypeId> {
        let Some(TypeInfo::Applied { base, args }) = self.type_table.get(type_id) else {
            return HashMap::new();
        };
        let Some(names) = self.generic_items.get(base) else {
            return HashMap::new();
        };
        if names.len() != args.len() {
            return HashMap::new();
        }
        names.iter().cloned().zip(args.iter().copied()).collect()
    }

    pub(super) fn substitute_type_id(
        &mut self,
        type_id: TypeId,
        mapping: &HashMap<String, TypeId>,
    ) -> TypeId {
        let info = self.type_table.get(type_id).cloned();
        match info {
            Some(TypeInfo::GenericParam(name)) => mapping.get(&name).copied().unwrap_or(type_id),
            Some(TypeInfo::Applied { base, args }) => {
                let mut changed = false;
                let new_args: Vec<TypeId> = args
                    .iter()
                    .map(|arg| {
                        let substituted = self.substitute_type_id(*arg, mapping);
                        if substituted != *arg {
                            changed = true;
                        }
                        substituted
                    })
                    .collect();
                if changed {
                    self.type_table.intern(TypeInfo::Applied {
                        base,
                        args: new_args,
                    })
                } else {
                    type_id
                }
            }
            Some(TypeInfo::Array(element)) => {
                let substituted = self.substitute_type_id(element, mapping);
                if substituted != element {
                    self.type_table.intern(TypeInfo::Array(substituted))
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    /// Widen `i32` to `i64` when paired with `i64` so integer literals compare with syscall counts.
    pub(super) fn promote_binary_numeric_operands(
        &self,
        left: TypeId,
        right: TypeId,
    ) -> (TypeId, TypeId) {
        let Some(i64_id) = self.primitive_type_id(HirPrimitiveType::I64) else {
            return (left, right);
        };
        let left_prim = self.type_table.get(left).and_then(|info| match info {
            TypeInfo::Primitive(primitive) => Some(*primitive),
            _ => None,
        });
        let right_prim = self.type_table.get(right).and_then(|info| match info {
            TypeInfo::Primitive(primitive) => Some(*primitive),
            _ => None,
        });
        match (left_prim, right_prim) {
            (Some(HirPrimitiveType::I64), Some(HirPrimitiveType::I32)) => (left, i64_id),
            (Some(HirPrimitiveType::I32), Some(HirPrimitiveType::I64)) => (i64_id, right),
            _ => (left, right),
        }
    }

    pub(super) fn require_same_type(&mut self, span: SpanInfo, expected: TypeId, actual: TypeId) {
        if expected == actual {
            return;
        }
        if self.is_never(expected) || self.is_never(actual) {
            return;
        }
        if let (Some(TypeInfo::Primitive(e)), Some(TypeInfo::Primitive(a))) =
            (self.type_table.get(expected), self.type_table.get(actual))
            && e == a
        {
            return;
        }
        if let (Some(TypeInfo::Array(e1)), Some(TypeInfo::Array(e2))) =
            (self.type_table.get(expected), self.type_table.get(actual))
            && e1 == e2
        {
            return;
        }
        if let (Some(TypeInfo::Fiber(p1)), Some(TypeInfo::Fiber(p2))) =
            (self.type_table.get(expected), self.type_table.get(actual))
            && p1 == p2
        {
            return;
        }
        if let Some(TypeInfo::Fiber(payload)) = self.type_table.get(actual)
            && let Some(TypeInfo::Applied { base, args }) = self.type_table.get(expected)
                && args.len() == 1
                    && args[0] == *payload
                    && self
                        .resolution
                        .items
                        .get(base.0)
                        .is_some_and(|info| info.name == "Fiber" || info.name.ends_with("::Fiber"))
                {
                    return;
                }
        if self.named_item_id(expected).is_some()
            && self.named_item_id(expected) == self.named_item_id(actual)
        {
            return;
        }
        if self.is_contract_compatible(expected, actual) {
            return;
        }
        if self.is_byte_array_ptr_compatible(expected, actual) {
            return;
        }
        if self.is_numeric(expected) && self.is_numeric(actual) {
            if self
                .cast_intents
                .iter()
                .any(|intent| intent.span == span && intent.from == actual && intent.to == expected)
            {
                return;
            }
            if self
                .cast_intents
                .iter()
                .any(|intent| intent.span == span && intent.from == expected && intent.to == actual)
            {
                self.errors.push(TypeError::TypeMismatch {
                    span,
                    expected,
                    actual,
                });
                return;
            }
            self.cast_intents.push(CastIntent {
                span,
                from: actual,
                to: expected,
                source_path: self.current_source_path.clone(),
            });
            return;
        }
        self.errors.push(TypeError::TypeMismatch {
            span,
            expected,
            actual,
        });
    }

    /// `u8[]` and `i64` Ptr handles share the same runtime representation (BYTES-001 ABI).
    fn is_byte_array_ptr_compatible(&mut self, expected: TypeId, actual: TypeId) -> bool {
        let (Some(i64_id), Some(u8_arr)) = (
            self.primitive_type_id(HirPrimitiveType::I64),
            self.u8_array_type_id(),
        ) else {
            return false;
        };
        (expected == i64_id && actual == u8_arr) || (expected == u8_arr && actual == i64_id)
    }

    fn is_contract_compatible(&self, expected: TypeId, actual: TypeId) -> bool {
        let Some(expected_item) = self.named_item_id(expected) else {
            return false;
        };
        let Some(actual_item) = self.named_item_id(actual) else {
            return false;
        };
        let Some(expected_info) = self
            .resolution
            .items
            .iter()
            .find(|info| info.id == expected_item)
        else {
            return false;
        };
        if expected_info.kind != ItemKind::Contract {
            return false;
        }
        self.resolution
            .tables
            .type_conformances
            .get(&actual_item)
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|(contract_item, _)| *contract_item == expected_item)
            })
    }

    pub(super) fn require_bool(
        &mut self,
        span: SpanInfo,
        expression: &crate::syntax::Spanned<HirExpressionNode>,
    ) {
        let type_id = self.type_expression(expression);
        let bool_id = self.primitive_type_id(HirPrimitiveType::Bool);
        if let (Some(type_id), Some(bool_id)) = (type_id, bool_id)
            && type_id != bool_id
        {
            self.errors.push(TypeError::NonBoolCondition { span });
        }
    }

    pub(super) fn primitive_type_id(&self, primitive: HirPrimitiveType) -> Option<TypeId> {
        self.primitive_types.get(&primitive).copied()
    }

    pub(super) fn is_numeric(&self, type_id: TypeId) -> bool {
        matches!(
            self.type_table.get(type_id),
            Some(TypeInfo::Primitive(
                HirPrimitiveType::I32
                    | HirPrimitiveType::I64
                    | HirPrimitiveType::U8
                    | HirPrimitiveType::F64
            ))
        )
    }

    pub(super) fn is_bool(&self, type_id: TypeId) -> bool {
        matches!(
            self.type_table.get(type_id),
            Some(TypeInfo::Primitive(HirPrimitiveType::Bool))
        )
    }

    pub(super) fn is_string(&self, type_id: TypeId) -> bool {
        matches!(
            self.type_table.get(type_id),
            Some(TypeInfo::Primitive(HirPrimitiveType::String))
        )
    }

    pub(super) fn is_never(&self, type_id: TypeId) -> bool {
        matches!(
            self.type_table.get(type_id),
            Some(TypeInfo::Primitive(HirPrimitiveType::Never))
        )
    }

    pub(super) fn is_comparable(&self, type_id: TypeId) -> bool {
        self.is_numeric(type_id) || self.is_bool(type_id)
    }

    pub(super) fn is_identity_comparable(&self, type_id: TypeId) -> bool {
        matches!(
            self.type_table.get(type_id),
            Some(TypeInfo::Named(_))
                | Some(TypeInfo::Applied { .. })
                | Some(TypeInfo::GenericParam(_))
                | Some(TypeInfo::Function { .. })
                | Some(TypeInfo::Array(_))
                | Some(TypeInfo::Primitive(HirPrimitiveType::String))
        )
    }

    pub(super) fn map_primitive(&self, primitive: HirPrimitiveType) -> HirPrimitiveType {
        primitive
    }
}
