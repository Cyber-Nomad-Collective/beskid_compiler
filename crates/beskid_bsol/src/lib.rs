//! Bsol — Beskid Structured Object Language.
//!
//! Parser, generic AST, declarative schema profiles, and validation for Beskid config documents.

mod ast;
mod build;
mod error;
mod parser;
pub mod schema;
mod validate;

pub use ast::{
    BsolAssignment, BsolBlock, BsolBracketList, BsolDocument, BsolItem, BsolListItem,
    BsolQuotedString, BsolSpan, BsolValue,
};
pub use build::parse_bsol_document;
pub use error::BsolError;
pub use parser::{BsolParser, Rule};
pub use schema::{BlockRule, SchemaProfile, load_profile};
pub use validate::{ValidatedBlock, ValidatedDocument, validate};
