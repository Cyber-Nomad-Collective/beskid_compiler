use crate::errors::CodegenError;
use crate::lowering::expressions::call_expression::lower_lambda_function_value;
use crate::lowering::lowerable::Lowerable;
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::pointer_type;
use beskid_analysis::hir::{HirExpressionNode, HirSpawnExpression};
use beskid_analysis::resolve::ResolvedValue;
use beskid_analysis::syntax::Spanned;
use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, InstBuilder, Signature};
use cranelift_codegen::isa::CallConv;

/// Lower `spawn callee` to `fiber_spawn(entry_trampoline, env)` returning an opaque handle pointer.
impl Lowerable<NodeLoweringContext<'_, '_>> for HirSpawnExpression {
    type Output = Option<cranelift_codegen::ir::Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        lower_spawn_expression(node, ctx)
    }
}

fn lower_spawn_expression(
    spawn: &Spanned<HirSpawnExpression>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<cranelift_codegen::ir::Value>, CodegenError> {
    let entry_ptr = match &spawn.node.callee.node {
        HirExpressionNode::LambdaExpression(lambda) => {
            lower_lambda_function_value(lambda, spawn.span, ctx)?
        }
        HirExpressionNode::PathExpression(path) => {
            if let Some(ResolvedValue::Item(item_id)) = ctx
                .resolution
                .tables
                .resolved_values
                .get(&path.node.path.span)
            {
                let name = ctx.resolution.items.get(item_id.0).map(|i| i.name.clone()).ok_or(
                    CodegenError::MissingSymbol("spawn entry function"),
                )?;
                let sig_ref = ctx.builder.func.import_signature(Signature::new(CallConv::SystemV));
                let func_ref = ctx.builder.func.import_function(ExtFuncData {
                    name: ExternalName::testcase(name),
                    signature: sig_ref,
                    colocated: true,
                    patchable: false,
                });
                ctx.builder.ins().func_addr(pointer_type(), func_ref)
            } else {
                return Err(CodegenError::UnsupportedNode {
                    span: spawn.span,
                    node: "spawn entry path",
                });
            }
        }
        _ => {
            return Err(CodegenError::UnsupportedNode {
                span: spawn.span,
                node: "spawn callee",
            });
        }
    };

    let env = ctx.builder.ins().iconst(pointer_type(), 0);

    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(pointer_type()));
    sig.params.push(AbiParam::new(pointer_type()));
    sig.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(sig);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase("fiber_spawn"),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });
    let call = ctx.builder.ins().call(func_ref, &[entry_ptr, env]);
    let handle = *ctx.builder.inst_results(call).first().ok_or(
        CodegenError::UnsupportedNode {
            span: spawn.span,
            node: "fiber_spawn result",
        },
    )?;
    Ok(Some(handle))
}
