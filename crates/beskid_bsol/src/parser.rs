//! Pest-generated parser for Bsol (`bsol.pest`).

use pest_derive::Parser;

/// Entry type for [`pest::Parser`] over Bsol source ([`Rule::document`]).
#[derive(Parser)]
#[grammar = "bsol.pest"]
pub struct BsolParser;
