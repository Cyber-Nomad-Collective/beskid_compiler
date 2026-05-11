//! Structural type interning ([`TypeTable`]) and HIR type checking ([`context::TypeContext`]) against a [`crate::resolve::Resolution`].

pub mod context;
pub mod table;

pub use context::context::{
    CallLoweringKind, MethodReceiverSource, TypeContext, TypeError, TypeResult, type_program,
    type_program_with_errors,
};
pub use table::{TypeId, TypeInfo, TypeTable};
