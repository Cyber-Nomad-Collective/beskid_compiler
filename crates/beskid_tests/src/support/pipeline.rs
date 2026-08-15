//! Surface parsing helper for integration tests.

use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::syntax::{Program, Spanned};
use beskid_analysis::Rule;

use crate::surface::util::parse_pair;

pub fn parse_program(input: &str) -> Spanned<Program> {
    let pair = parse_pair(Rule::Program, input);
    Program::parse(pair).expect("expected program AST")
}
