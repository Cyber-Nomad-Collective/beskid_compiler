//! Pest grammar for documentation comment bodies (`beskid_doc.pest`): `@ref`, `@arg`, etc.

use pest_derive::Parser;

/// Parser for structured lines inside `///` blocks (not the main Beskid grammar).
#[derive(Parser)]
#[grammar = "beskid_doc.pest"]
pub struct DocSyntaxParser;
