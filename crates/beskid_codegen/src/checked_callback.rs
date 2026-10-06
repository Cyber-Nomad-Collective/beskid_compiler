//! Source-issued active-scope callback counterparts. These have no outer failure
//! publication authority and cannot be substituted by a caller-authored address.
use crate::{CodegenContext, CodegenInput};
use beskid_queries::{AstNodeKey, GenericSpecializationInstance};
use cranelift_codegen::{ir::InstBuilder, isa::TargetIsa};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module, ModuleError, ModuleResult};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct EmittedCheckedCallback {
    function: FuncId,
    entry: AstNodeKey,
    specialization: Option<GenericSpecializationInstance>,
}
impl EmittedCheckedCallback {
    pub fn function(&self) -> FuncId {
        self.function
    }
    pub fn entry(&self) -> AstNodeKey {
        self.entry
    }
    pub fn specialization(&self) -> Option<&GenericSpecializationInstance> {
        self.specialization.as_ref()
    }
}

/// Emit in the same module after its sole descriptor pass. The image producer
/// must retain this exact source correspondence before it admits a counterpart role.
pub fn emit_checked_callback<'db, M: Module>(
    module: &mut M,
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    entry: AstNodeKey,
    specialization: Option<&GenericSpecializationInstance>,
    literals: &mut CodegenContext,
    runtime: &HashMap<String, FuncId>,
    namespace: &str,
    symbol: &str,
) -> ModuleResult<EmittedCheckedCallback> {
    let error = |message: String| ModuleError::Backend(anyhow::anyhow!(message));
    if specialization.is_some_and(|value| value.declaration != entry) {
        return Err(error("checked callback foreign specialization".into()));
    }
    let proof = input
        .checked_effect_closure(isa, entry, specialization.cloned())
        .map_err(|failure| error(format!("checked callback effect unavailable: {failure:?}")))?;
    use beskid_isle::NodeFacts;
    for (_, member) in proof.members() {
        let facts = match member.specialization() {
            Some(instance) => crate::isle_adapter::SyntaxNodeFacts::new_with_item_specialization(
                input,
                isa,
                member.key(),
                instance.clone(),
            ),
            None => crate::isle_adapter::SyntaxNodeFacts::new_with_isa(input, isa),
        };
        let mut pending =
            beskid_queries::item_body(input.database(), member.key()).ok().flatten().into_iter().collect::<Vec<_>>();
        let mut seen = HashSet::new();
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                continue;
            }
            if let Some(value) = facts.string_literal(node) {
                let name = literals.intern_string_literal(value.as_bytes());
                module.declare_data(&name, Linkage::Local, false, false)?;
            }
            pending.extend(
                beskid_queries::child_nodes(input.database(), node)
                    .map_err(|e| error(e.to_string()))?
                    .unwrap_or_default()
                    .iter()
                    .copied(),
            );
        }
    }
    let mut interner =
        crate::module_emission::imports::ArtifactStringInterner { context: literals, pointer_type: isa.pointer_type() };
    let closure = crate::checked_effect::emit_checked_effect_closure(
        module,
        input,
        isa,
        &proof,
        &mut interner,
        runtime,
        namespace,
    )?;
    let signature = module.declarations().get_function_decl(closure.entry()).signature.clone();
    if signature.returns.len() != 1 || signature.returns[0].value_type != isa.pointer_type() {
        return Err(error("checked Dynamic callback requires its exact managed pointer result ABI".into()));
    }
    let mut providers = runtime.clone();
    for name in ["beskid_rt_v5_checked_scope_current", "beskid_rt_v5_checked_scope_failure_reason"] {
        crate::checked_effect::ensure_checked_runtime_import(module, input, &mut providers, name)?;
    }
    let function = module.declare_function(symbol, Linkage::Export, &signature)?;
    let mut context = module.make_context();
    context.func.signature = signature;
    let current = module.declare_func_in_func(providers["beskid_rt_v5_checked_scope_current"], &mut context.func);
    let reason = module.declare_func_in_func(providers["beskid_rt_v5_checked_scope_failure_reason"], &mut context.func);
    let body = module.declare_func_in_func(closure.entry(), &mut context.func);
    let mut frontend = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend);
        let start = builder.create_block();
        let invoke = builder.create_block();
        let failed = builder.create_block();
        builder.append_block_params_for_function_params(start);
        builder.switch_to_block(start);
        builder.seal_block(start);
        let arguments = builder.block_params(start).to_vec();
        let call = builder.ins().call(current, &[]);
        let scope = builder.inst_results(call)[0];
        let call = builder.ins().call(reason, &[]);
        let status = builder.inst_results(call)[0];
        let absent = builder.ins().icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::Equal, scope, 0);
        let sticky = builder.ins().icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::NotEqual, status, 0);
        let reject = builder.ins().bor(absent, sticky);
        builder.ins().brif(reject, failed, &[], invoke, &[]);
        builder.switch_to_block(failed);
        builder.seal_block(failed);
        let zero = builder.ins().iconst(isa.pointer_type(), 0);
        builder.ins().return_(&[zero]);
        builder.switch_to_block(invoke);
        builder.seal_block(invoke);
        let call = builder.ins().call(body, &arguments);
        let value = builder.inst_results(call)[0];
        let call = builder.ins().call(reason, &[]);
        let status = builder.inst_results(call)[0];
        let sticky = builder.ins().icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::NotEqual, status, 0);
        let zero = builder.ins().iconst(isa.pointer_type(), 0);
        let result = builder.ins().select(sticky, zero, value);
        builder.ins().return_(&[result]);
    }
    module.define_function(function, &mut context)?;
    Ok(EmittedCheckedCallback { function, entry, specialization: specialization.cloned() })
}
