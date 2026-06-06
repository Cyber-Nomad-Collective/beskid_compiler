//! Structural type interning ([`TypeTable`]) and HIR type checking ([`context::TypeContext`]) against a [`crate::resolve::Resolution`].

pub mod context;
pub mod display;
pub mod path_value;
pub mod table;

pub use context::context::{
    CallLoweringKind, MethodReceiverSource, TypeContext, TypeError, TypeResult, type_program,
    type_program_with_errors,
};
pub use display::format_type_id;
pub use path_value::{
    field_segments_before_method, field_type_for_value_path, field_type_on_receiver,
    first_field_segment_name, generic_mapping_for_type_id, method_name_from_path_callee,
    named_item_id, receiver_type_for_path_callee, resolve_path_base_local, PathTypeEnv,
};
pub use table::{TypeId, TypeInfo, TypeTable};
pub use context::try_infer::{TryDesugarTarget, try_desugar_target_for_operand};
