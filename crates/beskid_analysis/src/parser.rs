//! Pest-generated parser for the main Beskid surface grammar (`beskid.pest`).

use pest_derive::Parser;

/// Entry type for [`pest::Parser`] over Beskid source (rule [`Rule::Program`] for a full file).
#[derive(Parser)]
#[grammar = "beskid.pest"]
pub struct BeskidParser;
