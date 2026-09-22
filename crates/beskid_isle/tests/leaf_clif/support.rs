pub(super) use std::path::PathBuf;

pub(super) use beskid_isle::syntax_types::{LiteralKind, OperatorFact};
pub(super) use beskid_isle::{
    AstNodeKey, EnumLayout, EnumVariantLayout, FieldLayout, IsleContext, LoweringErrorKind, NodeFacts, NodeKind,
    lower_expression, lower_statement,
};
pub(super) use beskid_queries::{AstNodeId, BeskidDatabase, SemanticTypeId, SourceUnitId, SyntaxGenerationId};
pub(super) use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, types};
pub(super) use cranelift_codegen::isa::CallConv;
pub(super) use cranelift_codegen::settings;
pub(super) use cranelift_codegen::verify_function;
pub(super) use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
pub(super) use cranelift_jit::{JITBuilder, JITModule};
pub(super) use cranelift_module::{Linkage, Module, default_libcall_names};
pub(super) use target_lexicon::Triple;

pub(super) fn test_isa() -> std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa> {
    cranelift_codegen::isa::lookup("x86_64-unknown-linux-gnu".parse().expect("test target"))
        .expect("x86_64 ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("test flags")
}
