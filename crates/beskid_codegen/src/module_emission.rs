mod contracts;
mod data;
mod imports;
mod items;
mod orchestration;
mod specialization;
mod trace;
mod trampolines;

pub use contracts::SyntaxModuleEmissionError;
pub use data::{DescriptorHandles, emit_closure_static_plans, emit_string_literals, emit_type_descriptors};
pub use items::SyntaxModuleItem;
pub use orchestration::{
    ModuleEmissionSession, emit_syntax_program, emit_syntax_program_in_session, lower_syntax_program,
};

#[allow(unused_imports)] // Keep the pre-split crate-internal facade path.
pub(crate) use data::{descriptor_offsets_symbol_name, descriptor_symbol_name};
