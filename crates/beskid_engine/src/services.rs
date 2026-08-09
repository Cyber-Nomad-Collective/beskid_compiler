mod entrypoint_execution;
mod jit_preparation;
mod syntax_queries;

use beskid_codegen::CodegenArtifact;
use beskid_queries::SemanticTypeId;

pub use entrypoint_execution::{
    run_entrypoint, run_entrypoint_from_front_end_with_engine, run_entrypoint_from_front_end_with_pipeline,
    run_entrypoint_with_pipeline,
};
pub use jit_preparation::{
    PreparedJitEntrypoint, lower_prepared_syntax_entrypoint, lower_syntax_assembly_entrypoint, prepare_jit_entrypoint,
    prepare_jit_module, prepare_syntax_front_end,
};
pub use syntax_queries::{
    SyntaxTestItem, syntax_entrypoint_return_type_from_front_end, syntax_test_items_from_front_end,
};

/// Fully syntax-backed entrypoint authority handed from the prepared frontend to the JIT.
///
/// `symbol` and `return_type` are derived from the same generation-safe item key used to emit
/// CLIF. No HIR node, resolution item id, or legacy codegen entrypoint participates.
struct SyntaxEntrypointArtifact {
    artifact: CodegenArtifact,
    symbol: String,
    return_type: SemanticTypeId,
}
