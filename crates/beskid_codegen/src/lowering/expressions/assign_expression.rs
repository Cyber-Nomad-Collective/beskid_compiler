use crate::errors::CodegenError;
use crate::lowering::cast_intent::ensure_type_compatibility;
use crate::lowering::descriptor::{struct_field_offsets, struct_item_id};
use crate::lowering::locals::resolved_value_at;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::{map_type_id_to_clif, pointer_type};
use beskid_analysis::hir::{HirAssignExpression, HirAssignOp, HirExpressionNode, HirPrimitiveType};
use beskid_analysis::resolve::ResolvedValue;
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo};
use cranelift_codegen::ir::Value;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{AbiParam, ExternalName, InstBuilder, MemFlags, Signature, TrapCode};
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
        let actual_type = ctx.require_expr_type(node.node.value.span)?;
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
                AssignTargetKind::IndexElement {
                    array_handle,
                    index,
                    elem_type,
                } => {
                    store_at_index(node.span, array_handle, index, elem_type, value, ctx)?;
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
                            call_event_subscribe(ctx, node.span, field_addr, value, cap_value);
                            return Ok(Some(value));
                        }
                        HirAssignOp::SubAssign => {
                            call_event_unsubscribe(ctx, node.span, field_addr, value);
                            return Ok(Some(value));
                        }
                        HirAssignOp::Assign => unreachable!("handled above"),
                    }
                }

                // Compound assignment for IndexElement: load current, apply op, store back
                if let AssignTargetKind::IndexElement {
                    array_handle,
                    index,
                    elem_type,
                } = target.kind
                {
                    let current = load_at_index(node.span, array_handle, index, elem_type, ctx)?;
                    let current_type = elem_type;
                    let is_string = matches!(
                        ctx.type_result.types.get(current_type),
                        Some(TypeInfo::Primitive(HirPrimitiveType::String))
                    );
                    let is_float = matches!(
                        ctx.type_result.types.get(current_type),
                        Some(TypeInfo::Primitive(HirPrimitiveType::F64))
                    );
                    let is_numeric = matches!(
                        ctx.type_result.types.get(current_type),
                        Some(TypeInfo::Primitive(
                            HirPrimitiveType::I32
                                | HirPrimitiveType::I64
                                | HirPrimitiveType::U8
                                | HirPrimitiveType::F64
                        ))
                    );

                    let new_value = if node.node.op.node == HirAssignOp::AddAssign && is_string {
                        lower_string_concat(current, value, ctx, node.span)?
                    } else if !is_numeric {
                        return Err(CodegenError::UnsupportedNode {
                            span: node.span,
                            node: "compound assignment type for array index",
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
                    };

                    store_at_index(node.span, array_handle, index, elem_type, new_value, ctx)?;
                    return Ok(Some(new_value));
                }

                let var = match target.kind {
                    AssignTargetKind::Local { var } => var,
                    AssignTargetKind::EventMember { .. }
                    | AssignTargetKind::IndexElement { .. } => {
                        unreachable!("handled above")
                    }
                };
                let current = ctx.builder.use_var(var);
                let is_string = matches!(
                    ctx.type_result.types.get(expected_type),
                    Some(TypeInfo::Primitive(HirPrimitiveType::String))
                );
                let is_float = matches!(
                    ctx.type_result.types.get(expected_type),
                    Some(TypeInfo::Primitive(HirPrimitiveType::F64))
                );
                let is_numeric = matches!(
                    ctx.type_result.types.get(expected_type),
                    Some(TypeInfo::Primitive(
                        HirPrimitiveType::I32
                            | HirPrimitiveType::I64
                            | HirPrimitiveType::U8
                            | HirPrimitiveType::F64
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
    IndexElement {
        array_handle: Value,
        index: Value,
        elem_type: TypeId,
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
            let resolved = resolved_value_at(
                ctx.resolution,
                path_expr.node.path.span,
                ctx.codegen.current_source_path.as_ref(),
            )
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
                let var = ctx.state.locals.get(&local_id).copied().ok_or(
                    CodegenError::InvalidLocalBinding {
                        span: path_expr.node.path.span,
                    },
                )?;
                let expected_type = ctx.type_result.local_types.get(&local_id).copied().ok_or(
                    CodegenError::MissingLocalType {
                        span: path_expr.node.path.span,
                    },
                )?;
                return Ok(AssignTarget {
                    kind: AssignTargetKind::Local { var },
                    expected_type,
                });
            }

            if segments.len() >= 2 {
                let receiver_var = ctx.state.locals.get(&local_id).copied().ok_or(
                    CodegenError::InvalidLocalBinding {
                        span: path_expr.node.path.span,
                    },
                )?;
                let receiver_type = ctx.type_result.local_types.get(&local_id).copied().ok_or(
                    CodegenError::MissingLocalType {
                        span: path_expr.node.path.span,
                    },
                )?;
                let field_name = segments
                    .last()
                    .ok_or(CodegenError::UnsupportedNode {
                        span: node.node.target.span,
                        node: "empty assignment target path",
                    })?
                    .node
                    .name
                    .node
                    .name
                    .as_str();
                let middle = &segments[1..segments.len() - 1];
                let receiver_ptr = ctx.builder.use_var(receiver_var);
                let (receiver_ptr, receiver_type) = if middle.is_empty() {
                    (receiver_ptr, receiver_type)
                } else {
                    load_path_field_chain(ctx, receiver_ptr, receiver_type, middle)?
                };
                return resolve_event_member_target(
                    node.span,
                    receiver_ptr,
                    receiver_type,
                    field_name,
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
            let receiver_type = ctx.require_expr_type(member_expr.node.target.span)?;
            resolve_event_member_target(
                node.span,
                receiver_ptr,
                receiver_type,
                member_expr.node.member.node.name.as_str(),
                ctx,
            )
        }
        HirExpressionNode::IndexExpression(index_expr) => {
            // arr[i] = value  →  resolve the array handle, index, and element type
            let array_handle =
                lower_node(&index_expr.node.target, ctx)?.ok_or(CodegenError::UnsupportedNode {
                    span: index_expr.node.target.span,
                    node: "unit-valued index target",
                })?;
            let index =
                lower_node(&index_expr.node.index, ctx)?.ok_or(CodegenError::UnsupportedNode {
                    span: index_expr.node.index.span,
                    node: "unit-valued index",
                })?;
            let target_type = ctx.require_expr_type(index_expr.node.target.span)?;
            let elem_type = match ctx.type_result.types.get(target_type) {
                Some(TypeInfo::Array(elem)) => *elem,
                Some(TypeInfo::Primitive(HirPrimitiveType::String)) => {
                    // String byte write: element type is U8
                    // Find the U8 type id
                    let mut t = 0usize;
                    loop {
                        let tid = TypeId(t);
                        let Some(info) = ctx.type_result.types.get(tid) else {
                            return Err(CodegenError::UnsupportedNode {
                                span: node.span,
                                node: "U8 type not found for string byte write",
                            });
                        };
                        if matches!(info, TypeInfo::Primitive(HirPrimitiveType::U8)) {
                            break tid;
                        }
                        t += 1;
                    }
                }
                _ => {
                    return Err(CodegenError::UnsupportedNode {
                        span: node.node.target.span,
                        node: "assignment index target type (expected array or string)",
                    });
                }
            };

            Ok(AssignTarget {
                kind: AssignTargetKind::IndexElement {
                    array_handle,
                    index,
                    elem_type,
                },
                expected_type: elem_type,
            })
        }
        _ => Err(CodegenError::UnsupportedNode {
            span: node.node.target.span,
            node: "unsupported assignment target",
        }),
    }
}

fn load_path_field_chain(
    ctx: &mut NodeLoweringContext<'_, '_>,
    mut value: Value,
    mut current_type: TypeId,
    segments: &[Spanned<beskid_analysis::hir::HirPathSegment>],
) -> Result<(Value, TypeId), CodegenError> {
    for segment in segments {
        let item_id =
            struct_item_id(ctx.type_result, current_type).ok_or(CodegenError::UnsupportedNode {
                span: segment.span,
                node: "member target type",
            })?;
        let offsets = struct_field_offsets(ctx.type_result, item_id).ok_or(
            CodegenError::UnsupportedNode {
                span: segment.span,
                node: "member offsets",
            },
        )?;
        let field_name = segment.node.name.node.name.as_str();
        let offset = offsets
            .get(field_name)
            .copied()
            .ok_or(CodegenError::UnsupportedNode {
                span: segment.span,
                node: "member offset",
            })?;
        let field_type = ctx
            .type_result
            .struct_fields_ordered
            .get(&item_id)
            .and_then(|fields| fields.iter().find(|(name, _)| name == field_name))
            .map(|(_, ty)| *ty)
            .ok_or(CodegenError::UnsupportedNode {
                span: segment.span,
                node: "member field type",
            })?;
        let clif_ty = map_type_id_to_clif(ctx.type_result, field_type).ok_or(
            CodegenError::UnsupportedNode {
                span: segment.span,
                node: "member field clif type",
            },
        )?;
        let offset_val = ctx.builder.ins().iconst(pointer_type(), offset as i64);
        let addr = ctx.builder.ins().iadd(value, offset_val);
        value = ctx.builder.ins().load(clif_ty, MemFlags::new(), addr, 0);
        current_type = field_type;
    }
    Ok((value, current_type))
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

/// Load a value from an array at the given index (with bounds check), used for both read and
/// compound-assignment current-value reading.
fn load_at_index(
    span: beskid_analysis::syntax::SpanInfo,
    array_handle: Value,
    index: Value,
    elem_type: TypeId,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Value, CodegenError> {
    let ptr = ctx
        .builder
        .ins()
        .load(pointer_type(), MemFlags::new(), array_handle, 0);
    let len = ctx
        .builder
        .ins()
        .load(pointer_type(), MemFlags::new(), array_handle, 8);

    // Bounds check
    let out_of_bounds = ctx
        .builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, len);
    ctx.builder
        .ins()
        .trapnz(out_of_bounds, TrapCode::unwrap_user(2));

    // Compute element size
    let layout = ctx.codegen.type_layout(ctx.type_result, elem_type).ok_or(
        CodegenError::UnsupportedNode {
            span,
            node: "array element layout for index write",
        },
    )?;
    let elem_size_val = ctx.builder.ins().iconst(pointer_type(), layout.size as i64);

    let offset = ctx.builder.ins().imul(index, elem_size_val);
    let addr = ctx.builder.ins().iadd(ptr, offset);

    let clif_ty =
        map_type_id_to_clif(ctx.type_result, elem_type).ok_or(CodegenError::UnsupportedNode {
            span,
            node: "array element clif type for index write",
        })?;
    let value = ctx.builder.ins().load(clif_ty, MemFlags::new(), addr, 0);

    Ok(value)
}

/// Store a value into an array at the given index (with bounds check).
fn store_at_index(
    span: beskid_analysis::syntax::SpanInfo,
    array_handle: Value,
    index: Value,
    elem_type: TypeId,
    value: Value,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<(), CodegenError> {
    let ptr = ctx
        .builder
        .ins()
        .load(pointer_type(), MemFlags::new(), array_handle, 0);
    let len = ctx
        .builder
        .ins()
        .load(pointer_type(), MemFlags::new(), array_handle, 8);

    // Bounds check
    let out_of_bounds = ctx
        .builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, len);
    ctx.builder
        .ins()
        .trapnz(out_of_bounds, TrapCode::unwrap_user(2));

    // Compute element size
    let layout = ctx.codegen.type_layout(ctx.type_result, elem_type).ok_or(
        CodegenError::UnsupportedNode {
            span,
            node: "array element layout for index store",
        },
    )?;
    let elem_size_val = ctx.builder.ins().iconst(pointer_type(), layout.size as i64);

    let offset = ctx.builder.ins().imul(index, elem_size_val);
    let addr = ctx.builder.ins().iadd(ptr, offset);

    // GC write barrier for pointer-like element types
    if crate::lowering::descriptor::is_pointer_like_type(ctx.type_result, elem_type) {
        call_write_barrier(ctx, array_handle, value);
    }

    ctx.builder.ins().store(MemFlags::new(), value, addr, 0);

    Ok(())
}

fn call_event_subscribe(
    ctx: &mut NodeLoweringContext<'_, '_>,
    span: beskid_analysis::syntax::SpanInfo,
    field_addr: Value,
    handler: Value,
    capacity: Value,
) {
    let _ = crate::lowering::dispatch::lower_dispatch_builtin_call(
        span,
        beskid_abi::DispatchRoute {
            tag: beskid_abi::TAG_EVENT_SUBSCRIBE,
            group: beskid_abi::DispatchReturnGroup::I64,
        },
        &[field_addr, handler, capacity],
        false,
        ctx,
    );
}

fn call_event_unsubscribe(
    ctx: &mut NodeLoweringContext<'_, '_>,
    span: beskid_analysis::syntax::SpanInfo,
    field_addr: Value,
    handler: Value,
) {
    let _ = crate::lowering::dispatch::lower_dispatch_builtin_call(
        span,
        beskid_abi::DispatchRoute {
            tag: beskid_abi::TAG_EVENT_UNSUBSCRIBE_FIRST,
            group: beskid_abi::DispatchReturnGroup::I64,
        },
        &[field_addr, handler],
        false,
        ctx,
    );
}

/// Emit a GC write barrier call for array index stores with pointer-like elements.
fn call_write_barrier(ctx: &mut NodeLoweringContext<'_, '_>, dst_obj: Value, value_ptr: Value) {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx
        .builder
        .func
        .import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("gc_write_barrier"),
            signature: sig_ref,
            colocated: false,
            patchable: false,
        });
    ctx.builder.ins().call(func_ref, &[dst_obj, value_ptr]);
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
            name: ExternalName::testcase("str_concat"),
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
