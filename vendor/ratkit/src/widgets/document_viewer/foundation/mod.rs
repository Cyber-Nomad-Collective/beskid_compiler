//! Normalized document model used by read-only viewers.

pub mod document_line;
pub mod document_line_kind;
pub mod document_outline_item;
pub mod rendered_document;
pub mod selection_pos;

pub use document_line::DocumentLine;
pub use document_line_kind::DocumentLineKind;
pub use document_outline_item::DocumentOutlineItem;
pub use rendered_document::RenderedDocument;
pub use selection_pos::SelectionPos;
