//! HIR lowering to Cranelift IR: [`CodegenContext`], [`Lowerable`], and [`lower_program`].

pub mod composition;
pub mod composition_policy;
mod cast_intent;
mod context;
pub(crate) mod descriptor;
mod expressions;
mod function;
pub mod lowerable;
mod node_context;
mod statements;
mod types;

pub use context::{CodegenArtifact, CodegenContext, CodegenResult, ExternImport, LoweredFunction};
pub use expressions::export::ExportEntry;
pub use lowerable::{Lowerable, lower_node, lower_program};
