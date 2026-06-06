//! Cranelift-based code generation for Beskid.

#![allow(clippy::drop_non_drop, clippy::question_mark, clippy::result_large_err)]
//!
//! Pipeline: [`services::lower_source_with_pipeline`] reports `parse`, optional semantic phases,
//! [`beskid_pipeline::phases::LOWER_READY`] (instant boundary before HIR work), then `lower` and
//! `codegen_clif`. Mod orchestration ids (`mod.load` … `mod.rewrite`,
//! [`beskid_pipeline::phases::SYNTAX_GENERATION`]) are **not** emitted here; they are reserved for
//! hosts that run the mod SDK and should call [`beskid_pipeline::observe_phase`] around real mod
//! work so observers match [`beskid_pipeline::phases::JIT_RUN_PHASE_ORDER`] when mods are active.

pub mod cranelift_host;
pub mod diagnostics;
pub mod errors;
pub mod linking;
pub mod lowering;
pub mod module_emission;
pub mod services;

pub use diagnostics::{codegen_error_to_diagnostic, codegen_errors_to_diagnostics};
pub use errors::CodegenError;
pub use linking::{
    FunctionDefIndex, LinkPlan, LinkSymbol, MissingSymbol, referenced_extern_imports,
    validate_artifact,
};
pub use lowering::{
    CodegenArtifact, CodegenContext, CodegenResult, DYNAMIC_TYPE_NAME, ExportEntry, ExternImport,
    Lowerable, LoweredFunction, dynamic_clif_type, is_dynamic_type_id, lower_node, lower_program,
    map_type_id_to_clif_with_dynamic, mapping_pair_eligible, pointer_type,
    require_mapping_eligible, shape_id_for_item,
};
pub use module_emission::{DescriptorHandles, emit_string_literals, emit_type_descriptors};
pub use services::{
    LoweredProgram, entrypoint_artifact_from_front_end, lower_from_front_end,
    lower_resolved_entrypoint_with_pipeline, lower_resolved_input_with_pipeline, lower_source,
    lower_source_with_pipeline, render_clif,
};
