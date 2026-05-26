use beskid_analysis::hir::{
    AstProgram, HirProgram, lower_program as lower_hir_program,
};
use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::services::typed_hir_from_lowered_after_resolution;
use beskid_analysis::syntax::{Program, Spanned};
use beskid_analysis::{BeskidParser, Rule};
use pest::Parser;

pub fn parse_program_ast(input: &str) -> Spanned<Program> {
    let mut pairs = BeskidParser::parse(Rule::Program, input)
        .unwrap_or_else(|error| panic!("expected parse success: {input}\n{error}"));
    let pair = pairs.next().expect("expected parse pair");
    Program::parse(pair).expect("expected AST program")
}

pub fn lower_resolve_type(
    source: &str,
) -> (
    Spanned<HirProgram>,
    beskid_analysis::resolve::Resolution,
    beskid_analysis::types::TypeResult,
) {
    let program = parse_program_ast(source);

    let ast: Spanned<AstProgram> = program.into();
    let hir = lower_hir_program(&ast);
    let resolution = beskid_analysis::resolve::Resolver::new()
        .resolve_program(&hir)
        .unwrap_or_else(|errors| panic!("expected resolution success: {errors:?}"));
    let (hir, resolution, typed) = typed_hir_from_lowered_after_resolution(hir, &resolution)
        .unwrap_or_else(|err| panic!("expected type success: {err}"));
    (hir, resolution, typed)
}
