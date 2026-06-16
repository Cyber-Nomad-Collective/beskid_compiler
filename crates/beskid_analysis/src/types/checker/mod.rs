//! Body typing keyed by [`HirNodeId`](crate::resolve::HirNodeId).
mod contracts;
mod entry;
mod expressions;
mod helpers;
mod items;
mod iterable;
pub(crate) mod precheck;
mod spawn;
mod statements;
mod types;

pub use precheck::TryDesugarTarget;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::builtins::{BuiltinType, builtin_specs};
use crate::hir::{HirBlock, HirPrimitiveType, HirProgram};
use crate::resolve::{HirNodeId, ItemId, LocalId, Resolution};
use crate::syntax::Spanned;
use crate::types::inference::{
    ConstraintSet, InferenceResult, TypeEnv, solve_constraints,
};
use crate::types::result::{CallLoweringKind, FunctionSignature, TypeError};
use crate::types::surface::{MergedTypeEnv, UnitTypeSurface};
use crate::types::{TypeId, TypeTable};

pub struct TypeChecker<'a> {
    pub(super) resolution: &'a Resolution,
    pub(super) type_table: TypeTable,
    pub node_types: HashMap<HirNodeId, TypeId>,
    pub local_types: HashMap<LocalId, TypeId>,
    pub constraints: ConstraintSet,
    pub errors: Vec<TypeError>,
    pub(super) primitive_types: HashMap<HirPrimitiveType, TypeId>,
    pub(super) named_types: HashMap<ItemId, TypeId>,
    pub(super) struct_fields: HashMap<ItemId, HashMap<String, TypeId>>,
    pub(super) struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub(super) struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub(super) enum_variants: HashMap<ItemId, HashMap<String, Vec<TypeId>>>,
    pub(super) enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub(super) function_signatures: HashMap<ItemId, FunctionSignature>,
    pub(super) method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub(super) generic_items: HashMap<ItemId, Vec<String>>,
    pub(super) methods_by_receiver: HashMap<(ItemId, String), ItemId>,
    pub(super) contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub(super) call_kinds: HashMap<HirNodeId, CallLoweringKind>,
    pub(super) pending_cast_checks: Vec<(crate::syntax::SpanInfo, TypeId, TypeId)>,
    pub(super) contextual_expected_type: Option<TypeId>,
    pub(super) current_return_type: Option<TypeId>,
    pub(super) generic_params: HashMap<String, TypeId>,
    pub(super) current_receiver_item_id: Option<ItemId>,
    pub(super) current_source_path: Option<PathBuf>,
    pub(super) fiber_scope_stack: Vec<usize>,
    pub(super) fiber_scope_parent: HashMap<usize, usize>,
    pub(super) next_fiber_scope: usize,
    pub(super) fiber_handle_scopes: HashMap<HirNodeId, usize>,
    pub(super) fiber_handle_locals: HashMap<LocalId, usize>,
}

#[derive(Debug)]
pub struct CheckerResult {
    pub types: TypeTable,
    pub node_types: HashMap<HirNodeId, TypeId>,
    pub local_types: HashMap<LocalId, TypeId>,
    pub inference: InferenceResult,
    pub function_signatures: HashMap<ItemId, FunctionSignature>,
    pub method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: HashMap<ItemId, Vec<String>>,
    pub methods_by_receiver: HashMap<(ItemId, String), ItemId>,
    pub contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub named_types: HashMap<ItemId, TypeId>,
    pub errors: Vec<TypeError>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(resolution: &'a Resolution, surface: &UnitTypeSurface) -> Self {
        let mut c = Self {
            resolution, type_table: surface.types.clone(), node_types: HashMap::new(), local_types: HashMap::new(),
            constraints: ConstraintSet::default(), errors: Vec::new(), primitive_types: HashMap::new(),
            named_types: HashMap::new(), struct_fields: HashMap::new(),
            struct_fields_ordered: surface.struct_fields_ordered.clone(),
            struct_event_fields: surface.struct_event_fields.clone(),
            enum_variants: HashMap::new(), enum_variants_ordered: surface.enum_variants_ordered.clone(),
            function_signatures: surface.function_signatures.clone(),
            method_function_signatures: surface.method_function_signatures.clone(),
            generic_items: surface.generic_items.clone(),
            methods_by_receiver: surface.methods_by_receiver.clone(),
            contract_signatures: surface.contract_signatures.clone(),
            call_kinds: HashMap::new(),
            pending_cast_checks: Vec::new(),
            contextual_expected_type: None, current_return_type: None,
            generic_params: HashMap::new(), current_receiver_item_id: None, current_source_path: None,
            fiber_scope_stack: vec![0], fiber_scope_parent: HashMap::from([(0,0)]), next_fiber_scope: 1,
            fiber_handle_scopes: HashMap::new(), fiber_handle_locals: HashMap::new(),
        };
        c.seed_types();
        c.seed_builtin_signatures();
        c.rebuild_struct_field_maps();
        c.rebuild_enum_variant_maps();
        c
    }

    pub(super) fn from_merged(
        resolution: &'a Resolution,
        merged: &MergedTypeEnv,
        types: TypeTable,
    ) -> Self {
        let surface = UnitTypeSurface {
            types,
            function_signatures: merged.function_signatures.clone(),
            method_function_signatures: merged.method_function_signatures.clone(),
            struct_fields_ordered: merged.struct_fields_ordered.clone(),
            enum_variants_ordered: merged.enum_variants_ordered.clone(),
            generic_items: merged.generic_items.clone(),
            struct_event_fields: merged.struct_event_fields.clone(),
            contract_signatures: merged.contract_signatures.clone(),
            contract_method_order: merged.contract_method_order.clone(),
            methods_by_receiver: merged.methods_by_receiver.clone(),
            named_type_names: merged.named_type_names.clone(),
        };
        Self::new(resolution, &surface)
    }
    pub fn with_source_path(mut self, source_path: &Path) -> Self {
        self.current_source_path = Some(crate::paths::unit_path_key(source_path));
        self
    }

    pub fn type_block(&mut self, block: &Spanned<HirBlock>) { self.type_block_inner(block); }

    pub(crate) fn infer_expression_type(
        &mut self,
        expression: &Spanned<crate::hir::HirExpressionNode>,
    ) -> Option<TypeId> {
        self.type_expression(expression)
    }

    pub(crate) fn item_for_type_id(&self, type_id: TypeId) -> Option<ItemId> {
        self.named_item_id(type_id)
    }

    pub(crate) fn variant_display_name(
        &self,
        enum_item_id: ItemId,
        variant: &str,
    ) -> Option<String> {
        self.ok_variant_name(enum_item_id, variant)
    }

    pub(crate) fn seed_program_enums(&mut self, program: &Spanned<HirProgram>) {
        self.seed_enum_definitions(program);
    }

    pub fn finish(self) -> CheckerResult {
        let TypeChecker {
            type_table,
            node_types,
            local_types,
            constraints,
            errors,
            generic_items,
            function_signatures,
            method_function_signatures,
            struct_fields_ordered,
            struct_event_fields,
            enum_variants_ordered,
            methods_by_receiver,
            contract_signatures,
            named_types,
            enum_variants,
            ..
        } = self;
        let mut errors = errors;
        let inference = match solve_constraints(
            constraints,
            &TypeEnv::new(&type_table)
                .with_generics(&generic_items, &function_signatures)
                .with_enum_variants(&enum_variants)
                .with_named_types(&named_types),
            crate::syntax::SpanInfo::default(),
        ) {
            Ok(result) => result,
            Err(solver_errors) => {
                errors.extend(solver_errors);
                InferenceResult::default()
            }
        };
        CheckerResult {
            types: type_table,
            node_types,
            local_types,
            inference,
            function_signatures,
            method_function_signatures,
            struct_fields_ordered,
            struct_event_fields,
            enum_variants_ordered,
            generic_items,
            methods_by_receiver,
            contract_signatures,
            named_types,
            errors,
        }
    }
    pub(super) fn infer_local_type_from_expression(&mut self, local_span: crate::syntax::SpanInfo, expression: &Spanned<crate::hir::HirExpressionNode>) -> Option<TypeId> {
        let var = self.constraints.fresh_var(); let prev = self.contextual_expected_type; self.contextual_expected_type = None;
        let actual = self.type_expression(expression)?; self.contextual_expected_type = prev;
        self.record_node_type(expression.id, actual); self.constraints.equal(var, actual, expression.span);
        self.insert_local_type(local_span, actual); Some(actual)
    }
    fn seed_builtin_signatures(&mut self) { for (item_id, index) in &self.resolution.builtin_items {
        let Some(spec) = builtin_specs().get(*index) else { continue; };
        let mut params = Vec::new(); for p in spec.params { if let Some(t)=self.builtin_surface_type_id(spec,*p,false){params.push(t);} }
        let Some(ret) = self.builtin_surface_type_id(spec, spec.returns, true) else { continue; };
        self.function_signatures.insert(*item_id, FunctionSignature { params, return_type: ret });
    }}
    fn rebuild_struct_field_maps(&mut self) { for (id, ord) in &self.struct_fields_ordered {
        self.struct_fields.insert(*id, ord.iter().map(|(n,t)| (n.clone(),*t)).collect());
    }}
    fn rebuild_enum_variant_maps(&mut self) { for (id, ord) in &self.enum_variants_ordered {
        self.enum_variants.insert(*id, ord.iter().map(|(n,f)| (n.clone(),f.clone())).collect());
    }}
    fn builtin_surface_type_id(&mut self, spec: &crate::builtins::BuiltinSpec, b: BuiltinType, ret: bool) -> Option<TypeId> {
        if b == BuiltinType::Ptr { let p = spec.beskid_path;
            if ret && matches!(p, &["__bytes_from_str"]|&["__syscall_read_bytes"]|&["__bytes_set"]|&["__str_new"]|&["__str_slice"]) {
                return if matches!(p,&["__str_new"]|&["__str_slice"]){self.primitive_type_id(HirPrimitiveType::String)} else {self.u8_array_type_id()}; }
            if matches!(p,&["__bytes_copy"]|&["__bytes_get"]|&["__bytes_set"]|&["__bytes_compare"]|&["__syscall_write_bytes"]){return self.u8_array_type_id();}
            return self.primitive_type_id(HirPrimitiveType::I64); }
        match b { BuiltinType::String=>self.primitive_type_id(HirPrimitiveType::String), BuiltinType::Unit=>self.primitive_type_id(HirPrimitiveType::Unit),
            BuiltinType::Never=>self.primitive_type_id(HirPrimitiveType::Never), _=>self.primitive_type_id(HirPrimitiveType::I64) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::HirNodeId;

    #[test]
    fn record_node_type_skips_invalid() {
        let mut node_types = HashMap::new();
        let id = TypeId(1);
        let record = |map: &mut HashMap<HirNodeId, TypeId>, node_id: HirNodeId, type_id: TypeId| {
            if node_id.is_valid() {
                map.insert(node_id, type_id);
            }
        };
        record(&mut node_types, HirNodeId::INVALID, id);
        assert!(node_types.is_empty());
        record(&mut node_types, HirNodeId(1), id);
        assert_eq!(node_types.get(&HirNodeId(1)), Some(&id));
    }
}
