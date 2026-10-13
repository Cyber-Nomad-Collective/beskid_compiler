//! Checked body counterparts of fallible Glue exports.
//!
//! A Glue export whose body reaches a `[RustOwner(Fallible:true)]` import is fallible. Its checked
//! invocation enters a runtime checked scope and calls the counterpart emitted here instead of the
//! ordinary body. The counterpart is the export's checked effect closure: each member requires the
//! active scope and returns as soon as the scope holds a failure, so a foreign status recorded by
//! a fallible import adapter ends the body before the adapter's zeroed output is used.
use crate::{CodegenContext, CodegenInput};
use beskid_queries::AstNodeKey;
use cranelift_codegen::{ir::InstBuilder, isa::TargetIsa};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module, ModuleError, ModuleResult};
use std::collections::HashMap;

/// Emit `symbol`, the checked counterpart of the fallible export `entry`, into the consumer body
/// module after its sole descriptor pass. `input` must be the Glue body input of the current packet
/// ([`super::glue_body_input`]) so that the closure admits the packet's import adapters. A closure
/// that the effect proof rejects fails the build: a fallible export is never linked without its
/// counterpart.
pub fn emit_checked_export<'db, M: Module>(
    module: &mut M,
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    entry: AstNodeKey,
    literals: &mut CodegenContext,
    runtime: &HashMap<String, FuncId>,
    symbol: &str,
) -> ModuleResult<FuncId> {
    let error = |message: String| ModuleError::Backend(anyhow::anyhow!(message));
    let proof = input.checked_effect_closure(isa, entry, None).map_err(|failure| {
        let site = beskid_queries::format_ast_node_key(input.database(), failure.site);
        error(format!("fallible Glue export requires a checked body: {} at {site}", failure.reason))
    })?;
    if proof.glue_imports().next().is_none() {
        return Err(error("fallible Glue export closure calls no Rust Glue import".into()));
    }
    crate::checked_callback::declare_closure_literals(module, input, isa, &proof, literals)?;
    let mut interner =
        crate::module_emission::imports::ArtifactStringInterner { context: literals, pointer_type: isa.pointer_type() };
    let closure =
        crate::checked_effect::emit_checked_effect_closure(module, input, isa, &proof, &mut interner, runtime, symbol)?;
    let signature = module.declarations().get_function_decl(closure.entry()).signature.clone();
    let function = module.declare_function(symbol, Linkage::Export, &signature)?;
    let mut context = module.make_context();
    context.func.signature = signature;
    let body = module.declare_func_in_func(closure.entry(), &mut context.func);
    let mut frontend = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        builder.seal_block(block);
        let arguments = builder.block_params(block).to_vec();
        let call = builder.ins().call(body, &arguments);
        let results = builder.inst_results(call).to_vec();
        builder.ins().return_(&results);
    }
    module.define_function(function, &mut context)?;
    Ok(function)
}
