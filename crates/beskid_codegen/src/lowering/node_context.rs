use crate::errors::CodegenError;
use crate::lowering::context::CodegenContext;
use crate::lowering::function::FunctionLoweringState;
use crate::lowering::locals::{
    expr_type_for_node, node_expr_type, require_expr_type, resolved_value_at,
};
use beskid_analysis::hir::{
    HirBinaryOp, HirExpressionNode, HirFunctionDefinition, HirPrimitiveType,
};
use beskid_analysis::resolve::{HirNodeId, ItemId, Resolution, ResolvedValue};
use beskid_analysis::syntax::Spanned;
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
    pub(crate) expected_expr_type: Option<TypeId>,
}

impl NodeLoweringContext<'_, '_> {
    pub(crate) fn expr_type(&self, node_id: HirNodeId) -> Option<TypeId> {
        node_expr_type(self.type_result, node_id)
    }

    pub(crate) fn expr_type_for_node(&self, node: &Spanned<HirExpressionNode>) -> Option<TypeId> {
        expr_type_for_node(self.type_result, node)
    }

    pub(crate) fn require_expr_type_for_node(
        &self,
        node: &Spanned<HirExpressionNode>,
    ) -> Result<TypeId, CodegenError> {
        if let HirExpressionNode::BinaryExpression(binary) = &node.node
            && binary.node.op.node == HirBinaryOp::Add
            && let Some(type_id) = self
                .type_result
                .types
                .find_primitive(HirPrimitiveType::String)
        {
            return Ok(type_id);
        }
        if let HirExpressionNode::PathExpression(path) = &node.node {
            let segments = &path.node.path.node.segments;
            if segments.len() == 1 {
                let name = segments[0].node.name.node.name.as_str();
                for source_path in [self.codegen.current_source_path.as_ref(), None] {
                    if let Some(ResolvedValue::Local(local_id)) =
                        resolved_value_at(self.resolution, path.node.path.span, source_path)
                    {
                        if let Some(type_id) = self.state.local_type_overrides.get(&local_id) {
                            return Ok(*type_id);
                        }
                        if let Some(type_id) = self.type_result.local_types.get(&local_id) {
                            return Ok(*type_id);
                        }
                    }
                }
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
        if let Some(type_id) = expr_type_for_node(self.type_result, node) {
            return Ok(type_id);
        }
        if let Some(type_id) = self.expected_expr_type {
            return Ok(type_id);
        }
        require_expr_type(self.type_result, node)
    }

    pub(crate) fn with_expected_expr_type<R>(
        &mut self,
        expected: TypeId,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous = self.expected_expr_type.replace(expected);
        let result = f(self);
        self.expected_expr_type = previous;
        result
    }
}
