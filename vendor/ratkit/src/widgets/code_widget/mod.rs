//! Read-only source code viewer widget.

pub mod foundation;
pub mod parser;
pub mod state;
pub mod widget;

pub use foundation::{CodeEvent, CodeLanguage, CodeLine, CodeOutlineItem};
pub use parser::{detect_language, extract_symbol_outline, highlight_code_lines};
pub use state::CodeState;
pub use widget::CodeWidget;
