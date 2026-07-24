use crate::errors::CodegenError;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use beskid_analysis::hir::{HirElseBranch, HirIfStatement, HirPrimitiveType};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::TypeInfo;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirIfStatement {
    type Output = ();

    fn lower(node: &Spanned<Self>, ctx: &mut NodeLoweringContext<'_, '_>) -> Result<Self::Output, CodegenError> {
        lower_if_statement(node, ctx)
    }
}

fn lower_if_statement(
    node: &Spanned<HirIfStatement>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<(), CodegenError> {
    let mut condition = lower_node(&node.node.condition, ctx)?
        .ok_or(CodegenError::UnsupportedNode { span: node.node.condition.span, node: "unit-valued if condition" })?;
    let condition_type = ctx.require_expr_type_for_node(&node.node.condition)?;
    let value_ty = ctx.builder.func.dfg.value_type(condition);
    let analysis_bool =
        matches!(ctx.type_result.types.get(condition_type), Some(TypeInfo::Primitive(HirPrimitiveType::Bool)));
    if analysis_bool || value_ty == types::I8 {
        // Comparison and logical lowering produce i8 booleans; analysis may lag.
    } else if value_ty.is_int() {
        let zero = ctx.builder.ins().iconst(value_ty, 0);
        condition = ctx.builder.ins().icmp(IntCC::NotEqual, condition, zero);
    } else {
        return Err(CodegenError::UnsupportedNode { span: node.node.condition.span, node: "non-bool if condition" });
    }

    let then_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();
    let else_block = node.node.else_branch.as_ref().map(|_| ctx.builder.create_block());

    if let Some(else_block) = else_block {
        ctx.builder.ins().brif(condition, then_block, &[], else_block, &[]);
    } else {
        ctx.builder.ins().brif(condition, then_block, &[], merge_block, &[]);
    }

    ctx.builder.switch_to_block(then_block);
    ctx.builder.seal_block(then_block);
    ctx.state.block_terminated = false;
    ctx.state.return_emitted = false;
    for statement in &node.node.then_block.node.statements {
        lower_node(statement, ctx)?;
        if ctx.state.block_terminated {
            break;
        }
    }
    let then_returned = ctx.state.return_emitted;
    let then_terminated = ctx.state.block_terminated;
    if !then_terminated {
        ctx.builder.ins().jump(merge_block, &[]);
    }

    if let (Some(else_clif_block), Some(else_branch)) = (else_block, &node.node.else_branch) {
        ctx.state.block_terminated = false;
        ctx.state.return_emitted = false;
        ctx.builder.switch_to_block(else_clif_block);
        ctx.builder.seal_block(else_clif_block);
        match &else_branch.node {
            HirElseBranch::Block(block) => {
                for statement in &block.node.statements {
                    lower_node(statement, ctx)?;
                    if ctx.state.block_terminated {
                        break;
                    }
                }
            }
            HirElseBranch::If(nested) => {
                lower_if_statement(nested, ctx)?;
            }
        }
        let else_returned = ctx.state.return_emitted;
        let else_terminated = ctx.state.block_terminated;
        if !else_terminated {
            ctx.builder.ins().jump(merge_block, &[]);
        }
        ctx.state.return_emitted = then_returned && else_returned;
        ctx.state.block_terminated = then_terminated && else_terminated;
    } else {
        ctx.state.return_emitted = false;
        ctx.state.block_terminated = false;
    }

    ctx.builder.seal_block(merge_block);
    ctx.builder.switch_to_block(merge_block);
    Ok(())
}
