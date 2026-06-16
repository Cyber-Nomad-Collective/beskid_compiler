//! HIR lowering to Cranelift IR: [`CodegenContext`], [`Lowerable`], and [`lower_program`].

mod cast_intent;
pub mod composition;
pub mod composition_policy;
mod context;
pub(crate) mod descriptor;
pub(crate) mod dispatch;
pub mod expressions;
pub(crate) mod function;
pub(crate) mod locals;
pub(crate) mod memory;
pub mod lowerable;
mod node_context;
mod statements;
mod type_surface;
pub mod types;

pub use context::{CodegenArtifact, CodegenContext, CodegenResult, ExternImport, LoweredFunction};
pub use expressions::export::{ExportEntry, object_link_symbol};
pub use expressions::mapping::shape_id_for_item;
pub use expressions::serialize::{
    DYNAMIC_TYPE_NAME, mapping_pair_eligible, require_mapping_eligible,
};
pub use lowerable::{
    Lowerable, lower_node, lower_program, lower_program_with_assembly,
    lower_program_with_assembly_for_entrypoint,
};
pub use types::{
    dynamic_clif_type, is_dynamic_type_id, map_type_id_to_clif_with_dynamic, pointer_type,
};
