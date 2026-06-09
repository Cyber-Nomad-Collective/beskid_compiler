//! Parsing helpers for code-widget content.

pub mod detect_language;
pub mod extract_symbol_outline;
pub mod highlight_code_lines;

pub use detect_language::detect_language;
pub use extract_symbol_outline::extract_symbol_outline;
pub use highlight_code_lines::highlight_code_lines;
