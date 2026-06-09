//! Shared read-only document viewer primitives.

pub mod extensions;
pub mod foundation;
pub mod state;
pub mod widget;

pub use foundation::{
    DocumentLine, DocumentLineKind, DocumentOutlineItem, RenderedDocument, SelectionPos,
};
pub use state::{CacheState, DisplaySettings, ScrollState, SelectionState, SourceState, VimState};
pub use widget::{handle_viewer_key, handle_viewer_mouse, DocumentViewerWidget};
