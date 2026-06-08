use crate::errors::CodegenError;
use crate::lowering::context::CodegenContext;
use crate::lowering::function::FunctionLoweringState;
use crate::lowering::locals::{
    expr_type_at, infer_expr_type, require_expr_type, resolved_value_at,
};
use beskid_analysis::hir::{HirBinaryOp, HirExpressionNode, HirFunctionDefinition, HirPrimitiveType};
use beskid_analysis::resolve::Resolution;
use beskid_analysis::resolve::{ItemId, ResolvedValue};
use beskid_analysis::syntax::{SpanInfo, Spanned};
use beskid_analysis::types::{TypeId, TypeResult, resolve_path_base_local};
use cranelift_frontend::FunctionBuilder;
use std::collections::HashMap;

pub(crate) struct NodeLoweringContext<'a, 'b> {
    pub(crate) resolution: &'a Resolution,
    pub(crate) type_result: &'a TypeResult,
    pub(crate) codegen: &'a mut CodegenContext,
    pub(crate) function_defs: &'a HashMap<ItemId, &'a Spanned<HirFunctionDefinition>>,
    pub(crate) builder: &'a mut FunctionBuilder<'b>,
    pub(crate) state: &'a mut FunctionLoweringState,
    pub(crate) expected_return_type: Option<TypeId>,
    pub(crate) receiver_type: Option<TypeId>,
}

impl NodeLoweringContext<'_, '_> {
    pub(crate) fn expr_type(&self, span: SpanInfo) -> Option<TypeId> {
        expr_type_at(
            self.type_result,
            span,
            self.codegen.current_source_path.as_ref(),
        )
    }

    pub(crate) fn require_expr_type(&self, span: SpanInfo) -> Result<TypeId, CodegenError> {
        if let Some(ResolvedValue::Local(local_id)) = resolved_value_at(
            self.resolution,
            span,
            self.codegen.current_source_path.as_ref(),
        ) {
            if let Some(type_id) = self.state.local_type_overrides.get(&local_id) {
                return Ok(*type_id);
            }
            if let Some(type_id) = self.type_result.local_types.get(&local_id) {
                return Ok(*type_id);
            }
        }
        if let Some(type_id) = self.expr_type(span) {
            return Ok(type_id);
        }
        require_expr_type(
            self.resolution,
            self.type_result,
            span,
            self.codegen.current_source_path.as_ref(),
            None,
        )
    }

    pub(crate) fn require_expr_type_for_node(
        &self,
        node: &Spanned<HirExpressionNode>,
    ) -> Result<TypeId, CodegenError> {
        if let HirExpressionNode::BinaryExpression(binary) = &node.node
            && binary.node.op.node == HirBinaryOp::Add
            && let Some(type_id) = primitive_type_id(self.type_result, HirPrimitiveType::String)
        {
            return Ok(type_id);
        }
        if let HirExpressionNode::PathExpression(path) = &node.node {
            let segments = &path.node.path.node.segments;
            if segments.len() == 1 {
                let name = segments[0].node.name.node.name.as_str();
                if let Some(local_id) = resolve_path_base_local(
                    self.resolution,
                    path.node.path.span,
                    name,
                    self.codegen.current_source_path.as_ref(),
                ) {
                    if let Some(type_id) = self.state.local_type_overrides.get(&local_id) {
                        return Ok(*type_id);
                    }
                    if let Some(type_id) = self.type_result.local_types.get(&local_id) {
                        return Ok(*type_id);
                    }
                }
            }
        }
        if let Some(type_id) = infer_expr_type(
            self.resolution,
            self.type_result,
            node,
            self.codegen.current_source_path.as_ref(),
            self.receiver_type,
        ) {
            return Ok(type_id);
        }
        if !matches!(
            node.node,
            HirExpressionNode::MemberExpression(_) | HirExpressionNode::PathExpression(_)
        ) && let Some(type_id) = self.expr_type(node.span)
        {
            return Ok(type_id);
        }
        Err(CodegenError::MissingExpressionType { span: node.span })
    }
}

fn primitive_type_id(
    type_result: &TypeResult,
    primitive: HirPrimitiveType,
) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if matches!(info, beskid_analysis::types::TypeInfo::Primitive(found) if *found == primitive) {
            return Some(type_id);
        }
        index += 1;
    }
}
