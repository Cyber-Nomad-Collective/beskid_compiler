//! Shared state for read-only document viewers.

pub mod cache;
pub mod display_settings;
pub mod scroll;
pub mod selection;
pub mod source;
pub mod vim;

pub use cache::CacheState;
pub use display_settings::DisplaySettings;
pub use scroll::ScrollState;
pub use selection::SelectionState;
pub use source::SourceState;
pub use vim::VimState;
