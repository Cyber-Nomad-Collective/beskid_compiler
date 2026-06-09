//! Viewport renderer for normalized documents.

pub mod document_viewer_widget;
pub mod handle_key;
pub mod handle_mouse;
pub mod render_document_lines;

pub use document_viewer_widget::DocumentViewerWidget;
pub use handle_key::handle_viewer_key;
pub use handle_mouse::handle_viewer_mouse;
pub use render_document_lines::render_document_lines;
