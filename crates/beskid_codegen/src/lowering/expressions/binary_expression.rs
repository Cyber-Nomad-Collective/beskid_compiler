use crate::errors::CodegenError;
use crate::lowering::cast_intent::ensure_type_compatibility;
use crate::lowering::descriptor::enum_payload_start;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::{map_type_id_to_clif, pointer_type};
use beskid_analysis::hir::{HirBinaryExpression, HirBinaryOp, HirPrimitiveType};
use beskid_analysis::syntax::{SpanInfo, Spanned};
use beskid_analysis::types::{TypeId, TypeInfo};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::types as clif_types;
use cranelift_codegen::ir::{AbiParam, ExternalName, InstBuilder, MemFlags, Signature, Value};
use cranelift_codegen::isa::CallConv;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirBinaryExpression {
    type Output = Option<Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, crate::errors::CodegenError> {
        let mut left = lower_node(&node.node.left, ctx)?.ok_or(CodegenError::UnsupportedNode {
            span: node.node.left.span,
            node: "unit-valued binary operand",
        })?;
        let mut right =
            lower_node(&node.node.right, ctx)?.ok_or(CodegenError::UnsupportedNode {
                span: node.node.right.span,
                node: "unit-valued binary operand",
            })?;

        let left_type = ctx.require_expr_type_for_node(&node.node.left)?;
        let right_type = ctx.require_expr_type_for_node(&node.node.right)?;

        if node.node.op.node == HirBinaryOp::Add {
            let left_is_string = is_string_type(ctx, left_type);
            let right_is_string = is_string_type(ctx, right_type);
            if left_is_string || right_is_string {
                if left_is_string && !right_is_string {
                    right = coerce_operand_to_string(
                        node.node.right.span,
                        right,
                        right_type,
                        left_type,
                        ctx,
                    )?;
                } else if right_is_string && !left_is_string {
                    left = coerce_operand_to_string(
                        node.node.left.span,
                        left,
                        left_type,
                        right_type,
                        ctx,
                    )?;
                } else if !left_is_string {
                    return Err(CodegenError::TypeMismatch {
                        span: node.span,
                        expected: left_type,
                        actual: right_type,
                    });
                }
                return lower_string_concat(node, left, right, ctx);
            }
        }

        let operand_type = if left_type == right_type {
            left_type
        } else if matches!(node.node.op.node, HirBinaryOp::Eq | HirBinaryOp::NotEq)
            && (is_string_type(ctx, left_type) || is_string_type(ctx, right_type))
        {
            let string_type = if is_string_type(ctx, left_type) {
                left_type
            } else {
                right_type
            };
            if !is_string_type(ctx, left_type) {
                left = coerce_operand_to_string(
                    node.node.left.span,
                    left,
                    left_type,
                    string_type,
                    ctx,
                )?;
            }
            if !is_string_type(ctx, right_type) {
                right = coerce_operand_to_string(
                    node.node.right.span,
                    right,
                    right_type,
                    string_type,
                    ctx,
                )?;
            }
            string_type
        } else if is_numeric_type(ctx.type_result.types.get(left_type))
            && is_numeric_type(ctx.type_result.types.get(right_type))
        {
            let target = preferred_numeric_type_id(ctx, left_type, right_type);
            left = ensure_type_compatibility(
                node.node.left.span,
                target,
                left_type,
                ctx.type_result,
                ctx.resolution,
                ctx.builder,
                left,
            )?;
            right = ensure_type_compatibility(
                node.node.right.span,
                target,
                right_type,
                ctx.type_result,
                ctx.resolution,
                ctx.builder,
                right,
            )?;
            target
        } else {
            return Err(CodegenError::TypeMismatch {
                span: node.span,
                expected: left_type,
                actual: right_type,
            });
        };
        let operand_info = ctx.type_result.types.get(operand_type);
        let operand_clif_ty = map_type_id_to_clif(ctx.type_result, operand_type).ok_or(
            CodegenError::UnsupportedNode {
                span: node.span,
                node: "binary operand type",
            },
        )?;

        let value = match node.node.op.node {
            HirBinaryOp::Add => {
                if matches!(
                    operand_info,
                    Some(TypeInfo::Primitive(HirPrimitiveType::String))
                ) {
                    return lower_string_concat(node, left, right, ctx);
                }
                if operand_clif_ty.is_float() {
                    ctx.builder.ins().fadd(left, right)
                } else if operand_clif_ty.is_int() {
                    ctx.builder.ins().iadd(left, right)
                } else {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary add type",
                    });
                }
            }
            HirBinaryOp::Sub => {
                if operand_clif_ty.is_float() {
                    ctx.builder.ins().fsub(left, right)
                } else if operand_clif_ty.is_int() {
                    ctx.builder.ins().isub(left, right)
                } else {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary sub type",
                    });
                }
            }
            HirBinaryOp::Mul => {
                if operand_clif_ty.is_float() {
                    ctx.builder.ins().fmul(left, right)
                } else if operand_clif_ty.is_int() {
                    ctx.builder.ins().imul(left, right)
                } else {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary mul type",
                    });
                }
            }
            HirBinaryOp::Div => {
                if operand_clif_ty.is_float() {
                    ctx.builder.ins().fdiv(left, right)
                } else if operand_clif_ty.is_int() {
                    ctx.builder.ins().sdiv(left, right)
                } else {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary div type",
                    });
                }
            }
            HirBinaryOp::Mod => {
                if operand_clif_ty.is_float() {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary mod on float",
                    });
                }
                if operand_clif_ty.is_int() {
                    ctx.builder.ins().srem(left, right)
                } else {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary mod type",
                    });
                }
            }
            HirBinaryOp::And | HirBinaryOp::Or => {
                let is_bool = matches!(
                    operand_info,
                    Some(TypeInfo::Primitive(HirPrimitiveType::Bool))
                );
                if !is_bool {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary logical type",
                    });
                }
                match node.node.op.node {
                    HirBinaryOp::And => ctx.builder.ins().band(left, right),
                    HirBinaryOp::Or => ctx.builder.ins().bor(left, right),
                    _ => unreachable!("checked operator"),
                }
            }
            HirBinaryOp::IdentityEq | HirBinaryOp::IdentityNotEq => {
                let enum_item_id = match operand_info {
                    Some(TypeInfo::Named(id)) => ctx
                        .type_result
                        .enum_variants_ordered
                        .contains_key(id)
                        .then_some(*id),
                    Some(TypeInfo::Applied { base, .. }) => ctx
                        .type_result
                        .enum_variants_ordered
                        .contains_key(base)
                        .then_some(*base),
                    _ => None,
                };
                if let Some(item_id) = enum_item_id {
                    let payload_start = enum_payload_start(ctx.type_result, item_id).ok_or(
                        CodegenError::UnsupportedNode {
                            span: node.span,
                            node: "enum payload start",
                        },
                    )?;
                    let tag_offset = ctx
                        .builder
                        .ins()
                        .iconst(pointer_type(), payload_start as i64);
                    let left_tag_addr = ctx.builder.ins().iadd(left, tag_offset);
                    let right_tag_addr = ctx.builder.ins().iadd(right, tag_offset);
                    let left_tag = ctx.builder.ins().load(
                        cranelift_codegen::ir::types::I32,
                        MemFlags::new(),
                        left_tag_addr,
                        0,
                    );
                    let right_tag = ctx.builder.ins().load(
                        cranelift_codegen::ir::types::I32,
                        MemFlags::new(),
                        right_tag_addr,
                        0,
                    );
                    let cond = match node.node.op.node {
                        HirBinaryOp::IdentityEq => IntCC::Equal,
                        HirBinaryOp::IdentityNotEq => IntCC::NotEqual,
                        _ => unreachable!("checked operator"),
                    };
                    ctx.builder.ins().icmp(cond, left_tag, right_tag)
                } else if !(operand_clif_ty.is_int() || operand_clif_ty.is_float()) {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary identity comparison type",
                    });
                } else if operand_clif_ty.is_float() {
                    let cond = match node.node.op.node {
                        HirBinaryOp::IdentityEq => FloatCC::Equal,
                        HirBinaryOp::IdentityNotEq => FloatCC::NotEqual,
                        _ => unreachable!("checked operator"),
                    };
                    ctx.builder.ins().fcmp(cond, left, right)
                } else {
                    let cond = match node.node.op.node {
                        HirBinaryOp::IdentityEq => IntCC::Equal,
                        HirBinaryOp::IdentityNotEq => IntCC::NotEqual,
                        _ => unreachable!("checked operator"),
                    };
                    ctx.builder.ins().icmp(cond, left, right)
                }
            }
            HirBinaryOp::Eq
            | HirBinaryOp::NotEq
            | HirBinaryOp::Lt
            | HirBinaryOp::Lte
            | HirBinaryOp::Gt
            | HirBinaryOp::Gte => {
                let is_bool = matches!(
                    operand_info,
                    Some(TypeInfo::Primitive(HirPrimitiveType::Bool))
                );
                if is_bool && !matches!(node.node.op.node, HirBinaryOp::Eq | HirBinaryOp::NotEq) {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary comparison type",
                    });
                }

                if matches!(
                    operand_info,
                    Some(TypeInfo::Primitive(HirPrimitiveType::String))
                ) && matches!(node.node.op.node, HirBinaryOp::Eq | HirBinaryOp::NotEq)
                {
                    return lower_string_eq(node, left, right, ctx);
                }

                if operand_clif_ty.is_float() {
                    let cond = match node.node.op.node {
                        HirBinaryOp::Eq => FloatCC::Equal,
                        HirBinaryOp::NotEq => FloatCC::NotEqual,
                        HirBinaryOp::Lt => FloatCC::LessThan,
                        HirBinaryOp::Lte => FloatCC::LessThanOrEqual,
                        HirBinaryOp::Gt => FloatCC::GreaterThan,
                        HirBinaryOp::Gte => FloatCC::GreaterThanOrEqual,
                        _ => unreachable!("checked operator"),
                    };
                    ctx.builder.ins().fcmp(cond, left, right)
                } else if operand_clif_ty.is_int() {
                    let cond = match node.node.op.node {
                        HirBinaryOp::Eq => IntCC::Equal,
                        HirBinaryOp::NotEq => IntCC::NotEqual,
                        HirBinaryOp::Lt => IntCC::SignedLessThan,
                        HirBinaryOp::Lte => IntCC::SignedLessThanOrEqual,
                        HirBinaryOp::Gt => IntCC::SignedGreaterThan,
                        HirBinaryOp::Gte => IntCC::SignedGreaterThanOrEqual,
                        _ => unreachable!("checked operator"),
                    };
                    ctx.builder.ins().icmp(cond, left, right)
                } else {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "binary comparison type",
                    });
                }
            }
        };

        Ok(Some(value))
    }
}

fn lower_string_eq(
    node: &Spanned<HirBinaryExpression>,
    left: Value,
    right: Value,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    signature.returns.push(AbiParam::new(clif_types::I64));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx
        .builder
        .func
        .import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("str_eq"),
            signature: sig_ref,
            colocated: false,
            patchable: false,
        });

    let call = ctx.builder.ins().call(func_ref, &[left, right]);
    let eq_flag = *ctx
        .builder
        .inst_results(call)
        .first()
        .ok_or(CodegenError::UnsupportedNode {
            span: node.span,
            node: "string eq result",
        })?;
    let zero = ctx.builder.ins().iconst(clif_types::I64, 0);
    let value = match node.node.op.node {
        HirBinaryOp::Eq => ctx.builder.ins().icmp(IntCC::NotEqual, eq_flag, zero),
        HirBinaryOp::NotEq => ctx.builder.ins().icmp(IntCC::Equal, eq_flag, zero),
        _ => unreachable!("checked operator"),
    };
    Ok(Some(value))
}

fn lower_string_concat(
    node: &Spanned<HirBinaryExpression>,
    left: Value,
    right: Value,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    signature.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx
        .builder
        .func
        .import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("str_concat"),
            signature: sig_ref,
            colocated: false,
            patchable: false,
        });

    let call = ctx.builder.ins().call(func_ref, &[left, right]);
    let result = *ctx
        .builder
        .inst_results(call)
        .first()
        .ok_or(CodegenError::UnsupportedNode {
            span: node.span,
            node: "string concat result",
        })?;
    Ok(Some(result))
}

fn is_string_type(ctx: &NodeLoweringContext<'_, '_>, type_id: TypeId) -> bool {
    matches!(
        ctx.type_result.types.get(type_id),
        Some(TypeInfo::Primitive(HirPrimitiveType::String))
    )
}

fn coerce_operand_to_string(
    span: SpanInfo,
    value: Value,
    type_id: TypeId,
    string_type: TypeId,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Value, CodegenError> {
    if is_string_type(ctx, type_id) {
        return Ok(value);
    }
    match ctx.type_result.types.get(type_id) {
        Some(TypeInfo::Primitive(HirPrimitiveType::I64)) => lower_str_from_i64(value, span, ctx),
        Some(TypeInfo::Primitive(HirPrimitiveType::I32)) => {
            let value_ty = ctx.builder.func.dfg.value_type(value);
            if value_ty.is_int() && value_ty.bits() < clif_types::I64.bits() {
                let extended = ctx.builder.ins().sextend(clif_types::I64, value);
                lower_str_from_i64(extended, span, ctx)
            } else {
                lower_str_from_i64(value, span, ctx)
            }
        }
        Some(TypeInfo::Primitive(HirPrimitiveType::U8)) => {
            let extended = ctx.builder.ins().uextend(clif_types::I64, value);
            lower_str_from_i64(extended, span, ctx)
        }
        _ => Err(CodegenError::TypeMismatch {
            span,
            expected: string_type,
            actual: type_id,
        }),
    }
}

fn lower_str_from_i64(
    value: Value,
    span: SpanInfo,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Value, CodegenError> {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(clif_types::I64));
    signature.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx
        .builder
        .func
        .import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("str_from_i64"),
            signature: sig_ref,
            colocated: false,
            patchable: false,
        });
    let call = ctx.builder.ins().call(func_ref, &[value]);
    ctx.builder
        .inst_results(call)
        .first()
        .copied()
        .ok_or(CodegenError::UnsupportedNode {
            span,
            node: "str_from_i64 result",
        })
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

fn preferred_numeric_type_id(
    ctx: &NodeLoweringContext<'_, '_>,
    left: TypeId,
    right: TypeId,
) -> TypeId {
    let left_width = numeric_bit_width(ctx.type_result.types.get(left));
    let right_width = numeric_bit_width(ctx.type_result.types.get(right));
    if left_width >= right_width {
        left
    } else {
        right
    }
}

fn numeric_bit_width(info: Option<&TypeInfo>) -> u32 {
    match info {
        Some(TypeInfo::Primitive(primitive)) => primitive.bit_width(),
        _ => 0,
    }
}
