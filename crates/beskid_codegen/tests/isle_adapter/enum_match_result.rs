//! Enum constructor, match, and Result `?` lowering through the syntax ISLE adapter.

#[path = "enum_match_result/constructors.rs"]
mod constructors;
#[path = "enum_match_result/generic_matches.rs"]
mod generic_matches;
#[path = "enum_match_result/imported.rs"]
mod imported;
#[path = "enum_match_result/match_shapes.rs"]
mod match_shapes;
#[path = "enum_match_result/parsed_programs.rs"]
mod parsed_programs;
#[path = "enum_match_result/result_try.rs"]
mod result_try;

use result_try::assert_imported_result_lowering;
