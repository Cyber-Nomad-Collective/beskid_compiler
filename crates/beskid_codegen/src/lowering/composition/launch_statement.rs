//! Lowering of `launch <Host>` statements to runtime-container ABI calls.
//!
//! The runtime container handles host bootstrap (eager singletons, scope[0] activation) and
//! teardown (reverse dispose). Codegen emits the bracket:
//!
//! ```text
//! let container = composition_container_create();
//! composition_launch(container);
//! // … host body lowering grows here as the HIR exposes its statements …
//! composition_shutdown(container);
//! composition_container_drop(container);
//! ```
//!
//! Registration / plural-binding sites are emitted by analysis-driven companion passes that
//! consume the `BindingPlan` produced by `beskid_analysis::composition`; they call
//! `composition_register` / `composition_bind_plural` before `composition_launch`.

use crate::errors::CodegenError;
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::pointer_type;
use beskid_abi::{
    SYM_COMPOSITION_CONTAINER_CREATE, SYM_COMPOSITION_CONTAINER_DROP, SYM_COMPOSITION_LAUNCH,
    SYM_COMPOSITION_SHUTDOWN,
};
use beskid_analysis::syntax::{LaunchStatement, Spanned};
use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, InstBuilder, Signature, types};
use cranelift_codegen::isa::CallConv;

pub(crate) fn lower_launch_statement(
    node: &Spanned<LaunchStatement>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<(), CodegenError> {
    let _ = node;

    let container = emit_container_create(ctx);
    emit_launch(ctx, container);
    emit_shutdown(ctx, container);
    emit_container_drop(ctx, container);
    Ok(())
}

fn emit_container_create(ctx: &mut NodeLoweringContext<'_, '_>) -> cranelift_codegen::ir::Value {
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(sig);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(SYM_COMPOSITION_CONTAINER_CREATE),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });
    let call = ctx.builder.ins().call(func_ref, &[]);
    ctx.builder.inst_results(call)[0]
}

fn emit_launch(ctx: &mut NodeLoweringContext<'_, '_>, container: cranelift_codegen::ir::Value) {
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(pointer_type()));
    sig.returns.push(AbiParam::new(types::I32));
    let sig_ref = ctx.builder.func.import_signature(sig);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(SYM_COMPOSITION_LAUNCH),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });
    ctx.builder.ins().call(func_ref, &[container]);
}

fn emit_shutdown(ctx: &mut NodeLoweringContext<'_, '_>, container: cranelift_codegen::ir::Value) {
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(pointer_type()));
    sig.returns.push(AbiParam::new(types::I32));
    let sig_ref = ctx.builder.func.import_signature(sig);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(SYM_COMPOSITION_SHUTDOWN),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });
    ctx.builder.ins().call(func_ref, &[container]);
}

fn emit_container_drop(
    ctx: &mut NodeLoweringContext<'_, '_>,
    container: cranelift_codegen::ir::Value,
) {
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(sig);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(SYM_COMPOSITION_CONTAINER_DROP),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });
    ctx.builder.ins().call(func_ref, &[container]);
}
