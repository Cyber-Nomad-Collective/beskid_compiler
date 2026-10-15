//! Shared syntax primitives: identifiers, spans, and visibility keywords.

pub mod identifier;
pub mod span;
pub mod type_parameter;
pub mod visibility;

pub use identifier::Identifier;
pub use span::{HasSpan, SpanInfo, Spanned};
pub use type_parameter::TypeParameter;
pub use visibility::Visibility;
