use crate::resolve::{ItemId, ItemKind, ResolvedType, ResolvedValue, canonical_item_id};
use crate::syntax::{Expression, PrimitiveType};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::path_value::PathTypeEnv;
use crate::types::{TypeId, TypeInfo};

use super::TypeChecker;
use crate::types::result::TypeError;
use std::collections::HashMap;

impl<'a> TypeChecker<'a> {
    pub(super) fn seed_types(&mut self) {
        for primitive in [
            PrimitiveType::Bool,
            PrimitiveType::I32,
            PrimitiveType::I64,
            PrimitiveType::U8,
            PrimitiveType::F64,
            PrimitiveType::Char,
            PrimitiveType::String,
            PrimitiveType::Unit,
            PrimitiveType::Never,
        ] {
            let id = self
                .type_table
                .find_primitive(primitive)
                .unwrap_or_else(|| self.type_table.intern(TypeInfo::Primitive(primitive)));
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

    pub(super) fn u8_array_type_id(&mut self) -> Option<TypeId> {
        let u8_id = self.primitive_type_id(PrimitiveType::U8)?;
        Some(self.type_table.find_array_of(u8_id).unwrap_or_else(|| self.type_table.intern(TypeInfo::Array(u8_id))))
    }

    pub(super) fn path_env(&self) -> PathTypeEnv<'_> {
        PathTypeEnv {
            types: &self.type_table,
            local_types: &self.local_types,
            struct_fields_ordered: &self.struct_fields_ordered,
            generic_items: &self.generic_items,
        }
    }

    pub(super) fn resolved_value_at(&self, span: SpanInfo) -> Option<ResolvedValue> {
        // A merged SpanIndex has no source identity: every unit's resolutions share one
        // `(start, end)` keyspace, so a dependency path at the same byte range can preempt
        // this unit's own value resolution. When type-checking a known unit the
        // source-scoped resolution table is authoritative; the index stays as the fallback
        // for spans it does not cover. Mirrors `resolved_type_at`.
        let value = self
            .resolution
            .tables
            .resolved_value_at(span, self.current_source_path.as_ref())
            .or_else(|| self.resolution.span_index.lookup_value(span))?;
        Some(match value {
            ResolvedValue::Item(item_id) => ResolvedValue::Item(canonical_item_id(self.resolution, item_id)),
            other => other,
        })
    }

    pub(super) fn resolved_type_at(&self, span: SpanInfo) -> Option<ResolvedType> {
        // A merged SpanIndex has no source identity. When type-checking a known unit, the
        // source-scoped resolution table is authoritative; otherwise a same-offset dependency
        // type can preempt the current unit before its scoped fact is considered.
        if let Some(resolved_type) = self.resolution.tables.resolved_type_at(span, self.current_source_path.as_ref()) {
            return Some(resolved_type);
        }
        if let Some(resolved_type) = self.resolution.span_index.lookup_type(span) {
            return Some(resolved_type);
        }
        None
    }

    pub(super) fn infer_generic_args_from_qualified_type_path(
        &mut self,
        segments: &[Spanned<crate::syntax::PathSegment>],
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
    ) -> Option<crate::types::result::FunctionSignature> {
        let signature = self
            .method_function_signatures
            .get(&method_item_id)
            .or_else(|| self.function_signatures.get(&method_item_id))?
            .clone();
        let mapping = self.generic_mapping_for_type_id(receiver_type);
        if mapping.is_empty() {
            return Some(signature);
        }
        Some(crate::types::result::FunctionSignature {
            params: signature.params.iter().map(|param| self.substitute_type_id(*param, &mapping)).collect(),
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
        node_id: crate::syntax::AstNodeId,
        kind: crate::types::result::CallLoweringKind,
    ) {
        if node_id.is_valid() {
            self.call_kinds.insert(node_id, kind);
        }
    }
    pub(super) fn record_node_type(&mut self, node_id: crate::syntax::AstNodeId, type_id: crate::types::TypeId) {
        if node_id.is_valid() {
            self.node_types.insert(node_id, type_id);
        }
    }
    pub(super) fn flush_scoped_type_maps_for_current_path(&mut self) {}
    pub(super) fn insert_local_type(&mut self, span: SpanInfo, type_id: TypeId) {
        if let Some(local_id) = self.local_id_for_span(span) {
            self.local_types.insert(local_id, type_id);
        } else {
            self.errors.push(TypeError::UnknownValueType { span });
        }
    }

    pub(super) fn local_id_for_span(&self, span: SpanInfo) -> Option<crate::resolve::LocalId> {
        self.resolution.tables.local_id_for_span(span, self.current_source_path.as_ref())
    }

    pub(super) fn item_id_for_span(&self, span: SpanInfo) -> Option<crate::resolve::ItemId> {
        if let Some(path) = &self.current_source_path
            && let Some(info) = self.resolution.items.iter().find(|info| {
                info.span == span
                    && info.source_path.as_ref().is_some_and(|source| crate::paths::same_file(source, path))
            })
        {
            return Some(info.id);
        }

        let matches: Vec<_> = self.resolution.items.iter().filter(|info| info.span == span).collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            _ => None,
        }
    }

    pub(super) fn item_id_for_name(&self, name: &str, kind: ItemKind) -> Option<ItemId> {
        let matches: Vec<_> =
            self.resolution.items.iter().filter(|info| info.name == name && info.kind == kind).collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            many => {
                if let Some(path) = &self.current_source_path
                    && let Some(info) = many.iter().rev().find(|info| {
                        info.source_path.as_ref().is_some_and(|source| crate::paths::same_file(source, path))
                    })
                {
                    return Some(info.id);
                }
                // Entry-unit symbols are collected after dependency prefetch; prefer the last match.
                many.last().map(|info| info.id)
            }
        }
    }

    pub(crate) fn named_item_id(&self, type_id: TypeId) -> Option<ItemId> {
        match self.type_table.get(type_id) {
            Some(TypeInfo::Named(item_id)) => Some(*item_id),
            Some(TypeInfo::Applied { base, .. }) => Some(*base),
            _ => None,
        }
    }

    pub(crate) fn ok_variant_name(&self, enum_item_id: ItemId, variant: &str) -> Option<String> {
        self.enum_variants_ordered
            .get(&enum_item_id)?
            .iter()
            .find(|(name, _)| name == variant)
            .map(|(name, _)| name.clone())
    }

    pub(super) fn method_item_for_receiver(&self, receiver_type: TypeId, method_name: &str) -> Option<ItemId> {
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

    pub(super) fn substitute_type_id(&mut self, type_id: TypeId, mapping: &HashMap<String, TypeId>) -> TypeId {
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
                if changed { self.type_table.intern(TypeInfo::Applied { base, args: new_args }) } else { type_id }
            }
            Some(TypeInfo::Array(element)) => {
                let substituted = self.substitute_type_id(element, mapping);
                if substituted != element { self.type_table.intern(TypeInfo::Array(substituted)) } else { type_id }
            }
            _ => type_id,
        }
    }

    /// Widen `i32` to `i64` when paired with `i64` so integer literals compare with syscall counts.
    pub(super) fn promote_binary_numeric_operands(&self, left: TypeId, right: TypeId) -> (TypeId, TypeId) {
        let Some(i64_id) = self.primitive_type_id(PrimitiveType::I64) else {
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
            (Some(PrimitiveType::I64), Some(PrimitiveType::I32)) => (left, i64_id),
            (Some(PrimitiveType::I32), Some(PrimitiveType::I64)) => (i64_id, right),
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
        if self.named_item_id(expected).is_some() && self.named_item_id(expected) == self.named_item_id(actual) {
            return;
        }
        if self.is_contract_compatible(expected, actual) {
            return;
        }
        if self.is_byte_array_ptr_compatible(expected, actual) {
            return;
        }
        if self.is_numeric(expected) && self.is_numeric(actual) {
            return;
        }
        self.errors.push(TypeError::TypeMismatch { span, expected, actual });
    }

    /// `u8[]` and `i64` Ptr handles share the same runtime representation (BYTES-001 ABI).
    fn is_byte_array_ptr_compatible(&mut self, expected: TypeId, actual: TypeId) -> bool {
        let (Some(i64_id), Some(u8_arr)) = (self.primitive_type_id(PrimitiveType::I64), self.u8_array_type_id()) else {
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
        let Some(expected_info) = self.resolution.items.iter().find(|info| info.id == expected_item) else {
            return false;
        };
        if expected_info.kind != ItemKind::Contract {
            return false;
        }
        self.resolution
            .tables
            .type_conformances
            .get(&actual_item)
            .is_some_and(|entries| entries.iter().any(|(contract_item, _)| *contract_item == expected_item))
    }

    pub(super) fn require_bool(&mut self, span: SpanInfo, expression: &crate::syntax::Spanned<Expression>) {
        let type_id = self.type_expression(expression);
        let bool_id = self.primitive_type_id(PrimitiveType::Bool);
        if let (Some(type_id), Some(bool_id)) = (type_id, bool_id)
            && type_id != bool_id
        {
            self.errors.push(TypeError::NonBoolCondition { span });
        }
    }

    pub(super) fn primitive_type_id(&self, primitive: PrimitiveType) -> Option<TypeId> {
        self.primitive_types.get(&primitive).copied()
    }

    pub(super) fn is_numeric(&self, type_id: TypeId) -> bool {
        matches!(
            self.type_table.get(type_id),
            Some(TypeInfo::Primitive(PrimitiveType::I32 | PrimitiveType::I64 | PrimitiveType::U8 | PrimitiveType::F64))
        )
    }

    pub(super) fn is_bool(&self, type_id: TypeId) -> bool {
        matches!(self.type_table.get(type_id), Some(TypeInfo::Primitive(PrimitiveType::Bool)))
    }

    pub(super) fn is_string(&self, type_id: TypeId) -> bool {
        matches!(self.type_table.get(type_id), Some(TypeInfo::Primitive(PrimitiveType::String)))
    }

    pub(super) fn is_never(&self, type_id: TypeId) -> bool {
        matches!(self.type_table.get(type_id), Some(TypeInfo::Primitive(PrimitiveType::Never)))
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
                | Some(TypeInfo::Primitive(PrimitiveType::String))
        )
    }

    pub(super) fn map_primitive(&self, primitive: PrimitiveType) -> PrimitiveType {
        primitive
    }

    pub(super) fn record_generic_call_constraints(
        &mut self,
        callee: ItemId,
        arg_types: &[TypeId],
        generic_param_count: usize,
        span: SpanInfo,
    ) {
        if generic_param_count == 0 {
            return;
        }
        let result_vars = (0..generic_param_count).map(|_| self.constraints.fresh_var()).collect::<Vec<_>>();
        self.constraints.apply_generic(callee, arg_types.to_vec(), result_vars, span);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::*;
    use crate::resolve::{Resolution, ResolvedType, SpanIndex};
    use crate::types::surface::UnitTypeSurface;

    #[test]
    fn source_scoped_value_fact_wins_over_same_offset_span_index_entry() {
        let span = SpanInfo { start: 8, end: 12, ..SpanInfo::default() };
        let entry_path = PathBuf::from("src/entry.bd");
        let dependency_path = PathBuf::from("src/dependency.bd");
        let entry_callee = ItemId(1);
        let dependency_callee = ItemId(2);
        let mut resolution = Resolution::default();

        resolution.tables.resolved_values.insert(span, ResolvedValue::Item(entry_callee));
        resolution
            .tables
            .scoped_resolved_values
            .insert(dependency_path, HashMap::from([(span, ResolvedValue::Item(dependency_callee))]));
        // The merged span index is source-less, so the dependency's call at the same byte
        // range used to preempt the entry unit's own callee and the checker then validated
        // the arguments against the wrong signature.
        resolution.span_index = SpanIndex::build_from_maps(&[(span, ResolvedValue::Item(dependency_callee))], &[]);

        let checker = TypeChecker::new(&resolution, &UnitTypeSurface::default()).with_source_path(&entry_path);

        assert_eq!(checker.resolved_value_at(span), Some(ResolvedValue::Item(entry_callee)));
    }

    #[test]
    fn source_scoped_type_fact_wins_over_same_offset_span_index_entry() {
        let span = SpanInfo { start: 8, end: 12, ..SpanInfo::default() };
        let entry_path = PathBuf::from("src/entry.bd");
        let dependency_path = PathBuf::from("src/dependency.bd");
        let entry_type = ItemId(1);
        let dependency_type = ItemId(2);
        let mut resolution = Resolution::default();

        resolution.tables.resolved_types.insert(span, ResolvedType::Item(entry_type));
        resolution
            .tables
            .scoped_resolved_types
            .insert(dependency_path, HashMap::from([(span, ResolvedType::Item(dependency_type))]));
        // The merged span index is source-less. It represents the same dependency fact that
        // previously preempted the entry unit before TypeChecker reached ResolutionTables.
        resolution.span_index = SpanIndex::build_from_maps(&[], &[(span, ResolvedType::Item(dependency_type))]);

        let checker = TypeChecker::new(&resolution, &UnitTypeSurface::default()).with_source_path(&entry_path);

        assert_eq!(checker.resolved_type_at(span), Some(ResolvedType::Item(entry_type)));
    }
}
