use crate::errors::CodegenError;
use crate::lowering::context::CodegenContext;
use crate::lowering::context::CodegenResult;
use crate::lowering::dispatch::emit_dispatch_call;
use crate::lowering::locals::node_expr_type;
use crate::lowering::types::{map_type_id_to_clif, pointer_type};
use beskid_abi::{DispatchReturnGroup, DispatchRoute, TAG_STR_NEW};
use beskid_analysis::hir::{HirLiteral, HirPrimitiveType, integer_literal_magnitude, integer_literal_primitive_type};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};
use cranelift_codegen::ir::{ExternalName, GlobalValueData, InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;

pub(crate) fn lower_literal(
    literal: &Spanned<HirLiteral>,
    expression_id: beskid_analysis::resolve::HirNodeId,
    type_result: &TypeResult,
    codegen: &mut CodegenContext,
    builder: &mut FunctionBuilder,
) -> CodegenResult<Value> {
    let type_id = node_expr_type(type_result, expression_id)
        .or_else(|| match &literal.node {
            HirLiteral::Integer(text) => find_literal_type(type_result, integer_literal_primitive_type(text)),
            HirLiteral::Float(_) => find_literal_type(type_result, HirPrimitiveType::F64),
            HirLiteral::Bool(_) => find_literal_type(type_result, HirPrimitiveType::Bool),
            HirLiteral::String(_) => find_literal_type(type_result, HirPrimitiveType::String),
            HirLiteral::Char(_) => find_literal_type(type_result, HirPrimitiveType::Char),
        })
        .ok_or(CodegenError::UnsupportedNode { span: literal.span, node: "literal type" })?;
    let clif_ty = map_type_id_to_clif(type_result, type_id)
        .ok_or(CodegenError::UnsupportedNode { span: literal.span, node: "literal type" })?;

    match &literal.node {
        HirLiteral::Integer(value) => {
            let parsed = integer_literal_magnitude(value).parse::<i64>().map_err(|_| {
                CodegenError::UnsupportedNode { span: literal.span, node: "non-integer literal for kickoff" }
            })?;
            Ok(builder.ins().iconst(clif_ty, parsed))
        }
        HirLiteral::Bool(value) => {
            let numeric = if *value { 1 } else { 0 };
            Ok(builder.ins().iconst(clif_ty, numeric))
        }
        HirLiteral::Char(value) => {
            let trimmed = value.trim_matches('"').trim_matches('\'');
            let mut chars = trimmed.chars();
            let ch =
                chars.next().ok_or(CodegenError::UnsupportedNode { span: literal.span, node: "empty char literal" })?;
            Ok(builder.ins().iconst(clif_ty, ch as i64))
        }
        HirLiteral::String(value) => {
            let trimmed = value.trim_matches('"');
            let bytes = trimmed.as_bytes();
            let len = bytes.len();
            let symbol = codegen.intern_string_literal(bytes);
            let string_gv = builder.func.create_global_value(GlobalValueData::Symbol {
                name: ExternalName::testcase(symbol),
                offset: 0.into(),
                colocated: true,
                tls: false,
            });
            let str_ptr = builder.ins().global_value(pointer_type(), string_gv);
            let len_val = builder.ins().iconst(pointer_type(), len as i64);
            let result = emit_dispatch_call(
                builder,
                DispatchRoute { tag: TAG_STR_NEW, group: DispatchReturnGroup::Ptr },
                &[str_ptr, len_val],
                true,
            )
            .map_err(|node| CodegenError::UnsupportedNode { span: literal.span, node })?
            .ok_or(CodegenError::UnsupportedNode { span: literal.span, node: "string literal result" })?;
            Ok(result)
        }
        _ => Err(CodegenError::UnsupportedNode { span: literal.span, node: "literal kind" }),
    }
}

fn find_literal_type(type_result: &TypeResult, primitive: HirPrimitiveType) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if matches!(info, TypeInfo::Primitive(found) if *found == primitive) {
            return Some(type_id);
        }
        index += 1;
    }
}
