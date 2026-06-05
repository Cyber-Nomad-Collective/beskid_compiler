use crate::errors::CodegenError;
use crate::lowering::context::CodegenResult;
use crate::lowering::function::mangle_method_name;
use crate::lowering::types::{map_type_id_to_clif, pointer_type};
use beskid_analysis::hir::HirPrimitiveType;
use beskid_analysis::resolve::{canonical_item_id, ItemKind, Resolution};
use beskid_analysis::syntax::SpanInfo;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};
use cranelift_codegen::ir::{AbiParam, ExternalName, InstBuilder, MemFlags, Signature, Value};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use std::collections::HashSet;

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
                return Ok(coerce_int_clif(
                    builder,
                    value,
                    value_clif,
                    expected_clif,
                ));
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

        if expected_width > actual_width {
            value = builder.ins().sextend(target_ty, value);
        } else if expected_width < actual_width {
            value = builder.ins().ireduce(target_ty, value);
        }
        return Ok(value);
    }

    Err(CodegenError::TypeMismatch {
        span,
        expected,
        actual,
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

    let methods = type_result
        .contract_method_order
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
        let signature = type_result
            .contract_signatures
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
                && expected_args.iter().zip(actual_args.iter()).all(|(left, right)| {
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

    for intent in &type_result.cast_intents {
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
