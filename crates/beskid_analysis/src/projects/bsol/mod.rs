//! **Bsol** (Beskid Structured Object Language): pest grammar, AST, and document builder for
//! `.bproj` / `.bws` manifest files.

mod ast;
mod build;
mod parser;

pub use ast::{
    BsolAssignment, BsolBlock, BsolBlockHeader, BsolBodyItem, BsolBracketList, BsolDocument,
    BsolListItem, BsolNestedBlock, BsolNestedBlockKind, BsolQuotedString, BsolReservedBlockKind,
    BsolSpan, BsolValue,
};
pub use build::parse_bsol_document;
pub use parser::{BsolParser, Rule};
