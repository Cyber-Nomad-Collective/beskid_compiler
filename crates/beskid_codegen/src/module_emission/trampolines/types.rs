use beskid_isle::AstNodeKey;
use cranelift_codegen::ir::Signature;

#[derive(Debug, Clone)]
pub(in crate::module_emission) struct SpawnTrampoline {
    pub(in crate::module_emission) spawn: AstNodeKey,
    pub(in crate::module_emission) target_symbol: String,
    pub(in crate::module_emission) target_signature: Signature,
    pub(in crate::module_emission) lambda_body: Option<AstNodeKey>,
    /// Present when the trampoline target is a capturing lambda that reads from the environment.
    pub(in crate::module_emission) closure_captures: Option<Vec<beskid_isle::InlineCaptureField>>,
    pub(in crate::module_emission) symbol: String,
    pub(in crate::module_emission) result_plan: crate::aggregate_static::AggregateStaticPlan,
}

pub(in crate::module_emission) enum SchedulerCompletionTransfer<'a> {
    Return,
    Tail { context_switch_symbol: &'a str },
}

/// One freestanding lambda lowered to its own trampoline function.
#[derive(Debug, Clone)]
pub(in crate::module_emission) struct LambdaTrampoline {
    pub(in crate::module_emission) lambda: AstNodeKey,
    pub(in crate::module_emission) lambda_body: AstNodeKey,
    pub(in crate::module_emission) target_signature: Signature,
    pub(in crate::module_emission) closure_captures: Option<Vec<beskid_isle::InlineCaptureField>>,
    pub(in crate::module_emission) symbol: String,
}
