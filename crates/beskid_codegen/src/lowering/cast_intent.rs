use crate::errors::CodegenError;
use crate::lowering::context::CodegenResult;
use crate::lowering::descriptor::get_or_compute_layout;
use crate::lowering::type_surface::{contract_method_order, contract_signatures};
use crate::lowering::expressions::mapping::lower_aot_object_mapping;
use crate::lowering::expressions::serialize::{is_serializable_struct, mapping_pair_eligible};
use crate::lowering::function::mangle_method_name;
use crate::lowering::dispatch::emit_str_from_i64_dispatch;
use crate::lowering::types::{map_type_id_to_clif, pointer_type};
use beskid_analysis::hir::HirPrimitiveType;
use beskid_analysis::resolve::{ItemId, ItemKind, Resolution, canonical_item_id};
use beskid_analysis::syntax::SpanInfo;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};
use cranelift_codegen::ir::{
    AbiParam, ExternalName, InstBuilder, MemFlags, Signature, StackSlotData, StackSlotKind, Value,
};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use std::collections::{HashMap, HashSet};

pub(crate) fn ensure_type_compatibility(
    span: SpanInfo,
    expected: TypeId,
    actual: TypeId,
    type_result: &TypeResult,
    resolution: &Resolution,
    builder: &mut FunctionBuilder,
    mut value: Value,
) -> CodegenResult<Value> {
    let value_clif = builder.func.dfg.value_type(value);
    if expected == actual || types_structurally_equal(type_result, resolution, expected, actual) {
        if let Some(expected_clif) = map_type_id_to_clif(type_result, expected) {
            if expected_clif == value_clif {
                return Ok(value);
            }
            if expected_clif.is_int() && value_clif.is_int() {
                return Ok(coerce_int_clif(builder, value, value_clif, expected_clif));
            }
        }
        return Ok(value);
    }

    let expected_info = type_result.types.get(expected);
    let actual_info = type_result.types.get(actual);

    if let Some(contract_value) = lower_contract_compatibility(
        span,
        expected,
        actual,
        value,
        type_result,
        resolution,
        builder,
    )? {
        return Ok(contract_value);
    }

    if is_numeric_type(expected_info)
        && is_numeric_type(actual_info)
        && let (Some(TypeInfo::Primitive(expected_prim)), Some(TypeInfo::Primitive(actual_prim))) =
            (expected_info, actual_info)
    {
        let expected_width = expected_prim.bit_width();
        let actual_width = actual_prim.bit_width();
        let target_ty = crate::lowering::types::map_primitive_to_clif(*expected_prim)
            .expect("expected clif type for numeric cast");

        let value_ty = builder.func.dfg.value_type(value);
        if expected_width > actual_width && value_ty != target_ty {
            value = builder.ins().sextend(target_ty, value);
        } else if expected_width < actual_width && value_ty != target_ty {
            value = builder.ins().ireduce(target_ty, value);
        }
        return Ok(value);
    }

    if is_string_primitive(expected_info) && is_numeric_type(actual_info) {
        return coerce_numeric_to_string(span, value, actual_info, builder);
    }

    if let Some(mapped) = try_lower_struct_object_mapping(
        span,
        expected,
        actual,
        value,
        type_result,
        resolution,
        builder,
    )? {
        return Ok(mapped);
    }

    Err(CodegenError::TypeMismatch {
        span,
        expected,
        actual,
    })
}

fn try_lower_struct_object_mapping(
    span: SpanInfo,
    expected: TypeId,
    actual: TypeId,
    value: Value,
    type_result: &TypeResult,
    resolution: &Resolution,
    builder: &mut FunctionBuilder,
) -> CodegenResult<Option<Value>> {
    let Some(expected_item) = named_item_id(type_result, expected) else {
        return Ok(None);
    };
    let Some(actual_item) = named_item_id(type_result, actual) else {
        return Ok(None);
    };
    if canonical_item_id(resolution, expected_item) == canonical_item_id(resolution, actual_item) {
        return Ok(None);
    }
    if !matches!(type_result.types.get(expected), Some(TypeInfo::Named(_)))
        || !matches!(type_result.types.get(actual), Some(TypeInfo::Named(_)))
    {
        return Ok(None);
    }

    if mapping_pair_eligible(resolution, type_result, actual_item, expected_item) {
        let mut layouts = HashMap::new();
        let layout = get_or_compute_layout(&mut layouts, type_result, expected).ok_or(
            CodegenError::UnsupportedNode {
                span,
                node: "struct mapping destination layout",
            },
        )?;
        let slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            layout.size as u32,
            3,
        ));
        let dst_out = builder.ins().stack_addr(pointer_type(), slot, 0);
        let _status = lower_aot_object_mapping(
            builder,
            resolution,
            type_result,
            span,
            actual_item,
            expected_item,
            value,
            dst_out,
        )?;
        return Ok(Some(dst_out));
    }

    if is_serializable_struct(resolution, type_result, actual_item)
        && is_serializable_struct(resolution, type_result, expected_item)
    {
        let src_name = item_display_name(resolution, actual_item);
        let dst_name = item_display_name(resolution, expected_item);
        return Err(CodegenError::IneligibleSerializeMapping {
            span,
            src_name,
            dst_name,
        });
    }

    Ok(None)
}

fn item_display_name(resolution: &Resolution, item_id: ItemId) -> String {
    resolution
        .items
        .iter()
        .find(|item| item.id == item_id)
        .map(|item| item.name.clone())
        .unwrap_or_else(|| "<unknown>".to_string())
}

/// Like [`ensure_type_compatibility`], but when span-keyed expression types collide across
/// linked units, fall back to the expected type and rely on the lowered CLIF value.
pub(crate) fn ensure_type_compatibility_or_expected(
    span: SpanInfo,
    expected: TypeId,
    actual: TypeId,
    type_result: &TypeResult,
    resolution: &Resolution,
    builder: &mut FunctionBuilder,
    value: Value,
) -> CodegenResult<Value> {
    match ensure_type_compatibility(
        span, expected, actual, type_result, resolution, builder, value,
    ) {
        Ok(value) => Ok(value),
        Err(CodegenError::TypeMismatch { span, expected, actual }) => {
            if expected != actual {
                ensure_type_compatibility(
                    span,
                    actual,
                    actual,
                    type_result,
                    resolution,
                    builder,
                    value,
                )
            } else {
                Err(CodegenError::TypeMismatch {
                    span,
                    expected,
                    actual,
                })
            }
        }
        Err(err) => Err(err),
    }
}

fn is_string_primitive(info: Option<&TypeInfo>) -> bool {
    matches!(info, Some(TypeInfo::Primitive(HirPrimitiveType::String)))
}

fn coerce_numeric_to_string(
    span: SpanInfo,
    value: Value,
    actual_info: Option<&TypeInfo>,
    builder: &mut FunctionBuilder,
) -> CodegenResult<Value> {
    let i64_ty =
        crate::lowering::types::map_primitive_to_clif(HirPrimitiveType::I64).expect("i64 clif");
    let value = match actual_info {
        Some(TypeInfo::Primitive(HirPrimitiveType::I64)) => value,
        Some(TypeInfo::Primitive(HirPrimitiveType::I32)) | None => {
            let value_ty = builder.func.dfg.value_type(value);
            if value_ty.is_int() && value_ty.bits() < i64_ty.bits() {
                builder.ins().sextend(i64_ty, value)
            } else {
                value
            }
        }
        Some(TypeInfo::Primitive(HirPrimitiveType::U8)) => builder.ins().uextend(i64_ty, value),
        _ => {
            return Err(CodegenError::UnsupportedNode {
                span,
                node: "numeric to string coercion",
            });
        }
    };

    emit_str_from_i64_dispatch(builder, value).map_err(|node| CodegenError::UnsupportedNode {
        span,
        node,
    })
}

fn lower_contract_compatibility(
    span: SpanInfo,
    expected: TypeId,
    actual: TypeId,
    value: Value,
    type_result: &TypeResult,
    resolution: &Resolution,
    builder: &mut FunctionBuilder,
) -> CodegenResult<Option<Value>> {
    let Some(expected_item_id) = named_item_id(type_result, expected) else {
        return Ok(None);
    };
    let Some(actual_item_id) = named_item_id(type_result, actual) else {
        return Ok(None);
    };
    let Some(expected_item) = resolution.items.get(expected_item_id.0) else {
        return Ok(None);
    };
    if expected_item.kind != ItemKind::Contract {
        return Ok(None);
    }
    let conforms = resolution
        .tables
        .type_conformances
        .get(&actual_item_id)
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|(contract_item, _)| *contract_item == expected_item_id)
        });
    if !conforms {
        return Ok(None);
    }

    let methods = contract_method_order(type_result)
        .get(&expected_item_id)
        .cloned()
        .unwrap_or_default();
    let wrapper_ptr = emit_contract_wrapper_alloc(builder, methods.len());
    builder.ins().store(MemFlags::new(), value, wrapper_ptr, 0);

    let receiver_name = resolution
        .items
        .get(actual_item_id.0)
        .map(|item| item.name.clone())
        .ok_or(CodegenError::MissingSymbol("contract receiver item"))?;
    for (index, method_name) in methods.iter().enumerate() {
        let contract_sigs = contract_signatures(type_result);
        let signature = contract_sigs
            .get(&(expected_item_id, method_name.clone()))
            .ok_or(CodegenError::MissingSymbol("contract method signature"))?;

        let mut signature_ir = Signature::new(CallConv::SystemV);
        let receiver_clif_ty =
            map_type_id_to_clif(type_result, actual).ok_or(CodegenError::UnsupportedNode {
                span,
                node: "contract receiver type",
            })?;
        signature_ir.params.push(AbiParam::new(receiver_clif_ty));
        for param in &signature.params {
            let clif_ty =
                map_type_id_to_clif(type_result, *param).ok_or(CodegenError::UnsupportedNode {
                    span,
                    node: "contract parameter type",
                })?;
            signature_ir.params.push(AbiParam::new(clif_ty));
        }
        if !matches!(
            type_result.types.get(signature.return_type),
            Some(TypeInfo::Primitive(HirPrimitiveType::Unit))
        ) {
            let return_clif = map_type_id_to_clif(type_result, signature.return_type).ok_or(
                CodegenError::UnsupportedNode {
                    span,
                    node: "contract return type",
                },
            )?;
            signature_ir.returns.push(AbiParam::new(return_clif));
        }

        let symbol = mangle_method_name(&receiver_name, method_name);
        let sig_ref = builder.func.import_signature(signature_ir);
        let func_ref = builder
            .func
            .import_function(cranelift_codegen::ir::ExtFuncData {
                name: ExternalName::testcase(symbol),
                signature: sig_ref,
                colocated: true,
                patchable: false,
            });
        let func_addr = builder.ins().func_addr(pointer_type(), func_ref);
        let offset = ((index + 1) * std::mem::size_of::<u64>()) as i32;
        builder
            .ins()
            .store(MemFlags::new(), func_addr, wrapper_ptr, offset);
    }

    Ok(Some(wrapper_ptr))
}

fn emit_contract_wrapper_alloc(builder: &mut FunctionBuilder, method_count: usize) -> Value {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    signature.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = builder.func.import_signature(signature);
    let func_ref = builder
        .func
        .import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("alloc"),
            signature: sig_ref,
            colocated: false,
            patchable: false,
        });
    let wrapper_size = ((method_count + 1) * std::mem::size_of::<u64>()) as i64;
    let size_val = builder.ins().iconst(pointer_type(), wrapper_size);
    let null_desc = builder.ins().iconst(pointer_type(), 0);
    let call = builder.ins().call(func_ref, &[size_val, null_desc]);
    builder
        .inst_results(call)
        .first()
        .copied()
        .expect("alloc must return pointer")
}

fn named_item_id(
    type_result: &TypeResult,
    type_id: TypeId,
) -> Option<beskid_analysis::resolve::ItemId> {
    match type_result.types.get(type_id) {
        Some(TypeInfo::Named(item_id)) => Some(*item_id),
        Some(TypeInfo::Applied { base, .. }) => Some(*base),
        _ => None,
    }
}

fn types_structurally_equal(
    type_result: &TypeResult,
    resolution: &Resolution,
    expected: TypeId,
    actual: TypeId,
) -> bool {
    match (
        type_result.types.get(expected),
        type_result.types.get(actual),
    ) {
        (Some(TypeInfo::Primitive(e)), Some(TypeInfo::Primitive(a))) => e == a,
        (Some(TypeInfo::Named(expected_item)), Some(TypeInfo::Named(actual_item))) => {
            canonical_item_id(resolution, *expected_item)
                == canonical_item_id(resolution, *actual_item)
        }
        (
            Some(TypeInfo::Applied {
                base: expected_base,
                args: expected_args,
            }),
            Some(TypeInfo::Applied {
                base: actual_base,
                args: actual_args,
            }),
        ) => {
            canonical_item_id(resolution, *expected_base)
                == canonical_item_id(resolution, *actual_base)
                && expected_args.len() == actual_args.len()
                && expected_args
                    .iter()
                    .zip(actual_args.iter())
                    .all(|(left, right)| {
                        types_structurally_equal(type_result, resolution, *left, *right)
                    })
        }
        (Some(TypeInfo::Applied { base, .. }), Some(TypeInfo::Named(actual_base))) => {
            canonical_item_id(resolution, *base) == canonical_item_id(resolution, *actual_base)
        }
        (Some(TypeInfo::Named(expected_base)), Some(TypeInfo::Applied { base, .. })) => {
            canonical_item_id(resolution, *expected_base) == canonical_item_id(resolution, *base)
        }
        _ => false,
    }
}

pub(crate) fn validate_cast_intents(type_result: &TypeResult) -> Vec<CodegenError> {
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    let mut reverse_seen = HashSet::new();

    for intent in &type_result.lowering.cast_intents {
        let from_info = type_result.types.get(intent.from);
        let to_info = type_result.types.get(intent.to);

        if !is_numeric_type(from_info) || !is_numeric_type(to_info) {
            errors.push(CodegenError::InvalidCastIntent {
                span: intent.span,
                message: "cast intents must be numeric-to-numeric".to_string(),
            });
        }

        let key = (
            intent.span.start,
            intent.span.end,
            intent.from.0,
            intent.to.0,
        );
        let reverse_key = (
            intent.span.start,
            intent.span.end,
            intent.to.0,
            intent.from.0,
        );
        if !seen.insert(key) {
            errors.push(CodegenError::InvalidCastIntent {
                span: intent.span,
                message: "duplicate cast intent for span".to_string(),
            });
        }
        reverse_seen.insert(reverse_key);
    }

    errors
}

fn coerce_int_clif(
    builder: &mut FunctionBuilder,
    value: Value,
    from: cranelift_codegen::ir::Type,
    to: cranelift_codegen::ir::Type,
) -> Value {
    if from == to {
        return value;
    }
    let from_bits = from.bits();
    let to_bits = to.bits();
    if to_bits > from_bits {
        builder.ins().sextend(to, value)
    } else {
        builder.ins().ireduce(to, value)
    }
}

/// Align a lowered value with the CLIF type used for a `declare_var` binding.
pub(crate) fn ensure_value_clif_type(
    builder: &mut FunctionBuilder,
    value: Value,
    expected_clif: cranelift_codegen::ir::Type,
) -> Value {
    let actual_clif = builder.func.dfg.value_type(value);
    if actual_clif == expected_clif {
        value
    } else if expected_clif.is_int() && actual_clif.is_int() {
        coerce_int_clif(builder, value, actual_clif, expected_clif)
    } else {
        value
    }
}

fn is_numeric_type(info: Option<&TypeInfo>) -> bool {
    matches!(
        info,
        Some(TypeInfo::Primitive(
            HirPrimitiveType::I32
                | HirPrimitiveType::I64
                | HirPrimitiveType::U8
                | HirPrimitiveType::F64
        ))
    )
}

#[cfg(test)]
mod struct_mapping_clif_tests {
    use super::*;
    use beskid_analysis::hir::{HirPrimitiveType, HirVisibility};
    use beskid_analysis::resolve::{ItemId, ItemInfo, ItemKind, ModuleGraph, Resolution};
    use beskid_analysis::types::{LoweringPrep, TypeId, TypeInfo, TypeResult, TypeTable};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

    fn struct_mapping_type_context() -> (TypeResult, Resolution, TypeId, TypeId) {
        let mut types = TypeTable::new();
        let i64_type = types.intern(TypeInfo::Primitive(HirPrimitiveType::I64));
        let source_item = ItemId(1);
        let target_item = ItemId(2);
        let source_type = types.intern(TypeInfo::Named(source_item));
        let target_type = types.intern(TypeInfo::Named(target_item));

        let mut struct_fields_ordered = HashMap::new();
        struct_fields_ordered.insert(source_item, vec![("id".to_string(), i64_type)]);
        struct_fields_ordered.insert(target_item, vec![("id".to_string(), i64_type)]);

        let type_result = TypeResult {
            types,
            named_type_names: HashMap::new(),
            node_types: HashMap::new(),
            local_types: HashMap::new(),
            unit_surfaces: HashMap::new(),
            function_signatures: HashMap::new(),
            method_function_signatures: HashMap::new(),
            struct_fields_ordered,
            struct_event_fields: HashMap::new(),
            enum_variants_ordered: HashMap::new(),
            generic_items: HashMap::new(),
            lowering: LoweringPrep::default(),
        };
        let resolution = Resolution {
            items: vec![
                ItemInfo {
                    id: source_item,
                    parent_id: None,
                    name: "Source".to_string(),
                    kind: ItemKind::Type,
                    span: SpanInfo {
                        start: 0,
                        end: 1,
                        line_col_start: (1, 1),
                        line_col_end: (1, 2),
                    },
                    source_path: None,
                    visibility: HirVisibility::Public,
                    symbol: None,
                },
                ItemInfo {
                    id: target_item,
                    parent_id: None,
                    name: "Target".to_string(),
                    kind: ItemKind::Type,
                    span: SpanInfo {
                        start: 2,
                        end: 3,
                        line_col_start: (1, 3),
                        line_col_end: (1, 4),
                    },
                    source_path: None,
                    visibility: HirVisibility::Public,
                    symbol: None,
                },
            ],
            module_graph: ModuleGraph::new_root(),
            tables: Default::default(),
            span_index: Default::default(),
            warnings: Vec::new(),
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: Default::default(),
            by_symbol: HashMap::new(),
        };
        (type_result, resolution, source_type, target_type)
    }

    #[test]
    fn ensure_type_compatibility_emits_dynamic_map_aot_for_eligible_structs() {
        let (type_result, resolution, source_type, target_type) = struct_mapping_type_context();
        let span = SpanInfo {
            start: 0,
            end: 1,
            line_col_start: (1, 1),
            line_col_end: (1, 2),
        };

        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(pointer_type()));
        sig.returns.push(AbiParam::new(pointer_type()));
        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            cranelift_codegen::ir::UserFuncName::testcase("struct_mapping_test"),
            sig,
        );
        let mut fn_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fn_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let src_ptr = builder.block_params(entry)[0];
        let mapped = ensure_type_compatibility(
            span,
            target_type,
            source_type,
            &type_result,
            &resolution,
            &mut builder,
            src_ptr,
        )
        .expect("eligible struct mapping should lower");

        builder.ins().return_(&[mapped]);
        builder.finalize();

        let clif = func.to_string();
        assert!(
            clif.contains("dynamic_map_aot"),
            "expected dynamic_map_aot in struct coercion CLIF: {clif}"
        );
    }
}
