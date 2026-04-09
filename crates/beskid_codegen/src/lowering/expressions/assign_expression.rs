use crate::errors::CodegenError;
use crate::lowering::cast_intent::ensure_type_compatibility;
use crate::lowering::descriptor::struct_field_offsets;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::pointer_type;
use beskid_analysis::hir::{HirAssignExpression, HirAssignOp, HirExpressionNode};
use beskid_analysis::resolve::ResolvedValue;
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo};
use cranelift_codegen::ir::Value;
use cranelift_codegen::ir::{AbiParam, ExternalName, InstBuilder, MemFlags, Signature};
use cranelift_codegen::isa::CallConv;

const DEFAULT_EVENT_CAPACITY: i64 = 8;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirAssignExpression {
    type Output = Option<Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        let target = resolve_assign_target(node, ctx)?;

        let value = lower_node(&node.node.value, ctx)?.ok_or(CodegenError::UnsupportedNode {
            span: node.node.value.span,
            node: "unit-valued assignment",
        })?;

        let expected_type = target.expected_type;
        let actual_type = ctx
            .type_result
            .expr_types
            .get(&node.node.value.span)
            .copied()
            .ok_or(CodegenError::MissingExpressionType {
                span: node.node.value.span,
            })?;
        let value = ensure_type_compatibility(
            node.node.value.span,
            expected_type,
            actual_type,
            ctx.type_result,
            ctx.resolution,
            ctx.builder,
            value,
        )?;

        let assigned = match node.node.op.node {
            HirAssignOp::Assign => match target.kind {
                AssignTargetKind::Local { .. } => value,
                AssignTargetKind::EventMember { field_addr, .. } => {
                    ctx.builder
                        .ins()
                        .store(MemFlags::new(), value, field_addr, 0);
                    value
                }
            },
            HirAssignOp::AddAssign | HirAssignOp::SubAssign => {
                if let AssignTargetKind::EventMember {
                    field_addr,
                    capacity,
                } = target.kind
                {
                    match node.node.op.node {
                        HirAssignOp::AddAssign => {
                            let cap_value = ctx
                                .builder
                                .ins()
                                .iconst(pointer_type(), capacity.unwrap_or(DEFAULT_EVENT_CAPACITY));
                            call_event_subscribe(ctx, field_addr, value, cap_value);
                            return Ok(Some(value));
                        }
                        HirAssignOp::SubAssign => {
                            call_event_unsubscribe(ctx, field_addr, value);
                            return Ok(Some(value));
                        }
                        HirAssignOp::Assign => unreachable!("handled above"),
                    }
                }

                let var = match target.kind {
                    AssignTargetKind::Local { var } => var,
                    AssignTargetKind::EventMember { .. } => unreachable!("handled above"),
                };
                let current = ctx.builder.use_var(var);
                let is_string = matches!(
                    ctx.type_result.types.get(expected_type),
                    Some(TypeInfo::Primitive(
                        beskid_analysis::hir::HirPrimitiveType::String
                    ))
                );
                let is_float = matches!(
                    ctx.type_result.types.get(expected_type),
                    Some(TypeInfo::Primitive(
                        beskid_analysis::hir::HirPrimitiveType::F64
                    ))
                );
                let is_numeric = matches!(
                    ctx.type_result.types.get(expected_type),
                    Some(TypeInfo::Primitive(
                        beskid_analysis::hir::HirPrimitiveType::I32
                            | beskid_analysis::hir::HirPrimitiveType::I64
                            | beskid_analysis::hir::HirPrimitiveType::U8
                            | beskid_analysis::hir::HirPrimitiveType::F64
                    ))
                );

                if node.node.op.node == HirAssignOp::AddAssign && is_string {
                    lower_string_concat(current, value, ctx, node.span)?
                } else if !is_numeric {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.span,
                        node: "compound assignment type",
                    });
                } else if is_float {
                    match node.node.op.node {
                        HirAssignOp::AddAssign => ctx.builder.ins().fadd(current, value),
                        HirAssignOp::SubAssign => ctx.builder.ins().fsub(current, value),
                        HirAssignOp::Assign => unreachable!("handled above"),
                    }
                } else {
                    match node.node.op.node {
                        HirAssignOp::AddAssign => ctx.builder.ins().iadd(current, value),
                        HirAssignOp::SubAssign => ctx.builder.ins().isub(current, value),
                        HirAssignOp::Assign => unreachable!("handled above"),
                    }
                }
            }
        };

        if let AssignTargetKind::Local { var } = target.kind {
            ctx.builder.def_var(var, assigned);
        }
        Ok(Some(assigned))
    }
}

#[derive(Clone, Copy)]
enum AssignTargetKind {
    Local {
        var: cranelift_frontend::Variable,
    },
    EventMember {
        field_addr: Value,
        capacity: Option<i64>,
    },
}

#[derive(Clone, Copy)]
struct AssignTarget {
    kind: AssignTargetKind,
    expected_type: TypeId,
}

fn resolve_assign_target(
    node: &Spanned<HirAssignExpression>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<AssignTarget, CodegenError> {
    match &node.node.target.node {
        HirExpressionNode::PathExpression(path_expr) => {
            let segments = &path_expr.node.path.node.segments;
            if segments.is_empty() {
                return Err(CodegenError::UnsupportedNode {
                    span: node.node.target.span,
                    node: "empty assignment target path",
                });
            }
            let resolved = ctx
                .resolution
                .tables
                .resolved_values
                .get(&path_expr.node.path.span)
                .ok_or(CodegenError::MissingResolvedValue {
                    span: path_expr.node.path.span,
                })?;
            let ResolvedValue::Local(local_id) = resolved else {
                return Err(CodegenError::UnsupportedNode {
                    span: path_expr.node.path.span,
                    node: "non-local assignment target",
                });
            };

            if segments.len() == 1 {
                let var = ctx.state.locals.get(local_id).copied().ok_or(
                    CodegenError::InvalidLocalBinding {
                        span: path_expr.node.path.span,
                    },
                )?;
                let expected_type = ctx.type_result.local_types.get(local_id).copied().ok_or(
                    CodegenError::MissingLocalType {
                        span: path_expr.node.path.span,
                    },
                )?;
                return Ok(AssignTarget {
                    kind: AssignTargetKind::Local { var },
                    expected_type,
                });
            }

            if segments.len() == 2 {
                let receiver_var = ctx.state.locals.get(local_id).copied().ok_or(
                    CodegenError::InvalidLocalBinding {
                        span: path_expr.node.path.span,
                    },
                )?;
                let receiver_ptr = ctx.builder.use_var(receiver_var);
                let receiver_type = ctx.type_result.local_types.get(local_id).copied().ok_or(
                    CodegenError::MissingLocalType {
                        span: path_expr.node.path.span,
                    },
                )?;
                let field_name = segments[1].node.name.node.name.clone();
                return resolve_event_member_target(
                    node.span,
                    receiver_ptr,
                    receiver_type,
                    field_name.as_str(),
                    ctx,
                );
            }

            Err(CodegenError::UnsupportedNode {
                span: node.node.target.span,
                node: "multi-segment assignment target",
            })
        }
        HirExpressionNode::MemberExpression(member_expr) => {
            let receiver_ptr = lower_node(&member_expr.node.target, ctx)?.ok_or(
                CodegenError::UnsupportedNode {
                    span: member_expr.node.target.span,
                    node: "unit-valued assignment receiver",
                },
            )?;
            let receiver_type = ctx
                .type_result
                .expr_types
                .get(&member_expr.node.target.span)
                .copied()
                .ok_or(CodegenError::MissingExpressionType {
                    span: member_expr.node.target.span,
                })?;
            resolve_event_member_target(
                node.span,
                receiver_ptr,
                receiver_type,
                member_expr.node.member.node.name.as_str(),
                ctx,
            )
        }
        _ => Err(CodegenError::UnsupportedNode {
            span: node.node.target.span,
            node: "unsupported assignment target",
        }),
    }
}

fn resolve_event_member_target(
    span: beskid_analysis::syntax::SpanInfo,
    receiver_ptr: Value,
    receiver_type: TypeId,
    field_name: &str,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<AssignTarget, CodegenError> {
    let item_id = match ctx.type_result.types.get(receiver_type) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        _ => {
            return Err(CodegenError::UnsupportedNode {
                span,
                node: "event assignment receiver type",
            });
        }
    };
    let offsets =
        struct_field_offsets(ctx.type_result, item_id).ok_or(CodegenError::UnsupportedNode {
            span,
            node: "event assignment offsets",
        })?;
    let offset = offsets
        .get(field_name)
        .copied()
        .ok_or(CodegenError::UnsupportedNode {
            span,
            node: "event assignment field offset",
        })?;
    let expected_type = ctx
        .type_result
        .struct_fields_ordered
        .get(&item_id)
        .and_then(|fields| fields.iter().find(|(name, _)| name == field_name))
        .map(|(_, ty)| *ty)
        .ok_or(CodegenError::UnsupportedNode {
            span,
            node: "event assignment field type",
        })?;
    let capacity = ctx
        .type_result
        .struct_event_fields
        .get(&item_id)
        .and_then(|fields| fields.get(field_name));
    let Some(capacity) = capacity else {
        return Err(CodegenError::UnsupportedNode {
            span,
            node: "non-event member assignment target",
        });
    };
    let offset_val = ctx.builder.ins().iconst(pointer_type(), offset as i64);
    let field_addr = ctx.builder.ins().iadd(receiver_ptr, offset_val);

    Ok(AssignTarget {
        kind: AssignTargetKind::EventMember {
            field_addr,
            capacity: capacity.map(|value| value as i64),
        },
        expected_type,
    })
}

fn call_event_subscribe(
    ctx: &mut NodeLoweringContext<'_, '_>,
    field_addr: Value,
    handler: Value,
    capacity: Value,
) {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    signature.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx
        .builder
        .func
        .import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("event_subscribe".to_string()),
            signature: sig_ref,
            colocated: false,
            patchable: false,
        });
    let _ = ctx
        .builder
        .ins()
        .call(func_ref, &[field_addr, handler, capacity]);
}

fn call_event_unsubscribe(
    ctx: &mut NodeLoweringContext<'_, '_>,
    field_addr: Value,
    handler: Value,
) {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    signature.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx
        .builder
        .func
        .import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("event_unsubscribe_first".to_string()),
            signature: sig_ref,
            colocated: false,
            patchable: false,
        });
    let _ = ctx.builder.ins().call(func_ref, &[field_addr, handler]);
}

fn lower_string_concat(
    left: Value,
    right: Value,
    ctx: &mut NodeLoweringContext<'_, '_>,
    span: beskid_analysis::syntax::SpanInfo,
) -> Result<Value, CodegenError> {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    signature.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx
        .builder
        .func
        .import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("str_concat".to_string()),
            signature: sig_ref,
            colocated: false,
            patchable: false,
        });
    let call = ctx.builder.ins().call(func_ref, &[left, right]);
    ctx.builder
        .inst_results(call)
        .first()
        .copied()
        .ok_or(CodegenError::UnsupportedNode {
            span,
            node: "string concat result",
        })
}
