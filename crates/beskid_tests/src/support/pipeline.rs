//! Parse → lower → resolve → typecheck helpers for integration tests.

use beskid_analysis::Rule;
use beskid_analysis::hir::{AstProgram, HirProgram, lower_program, normalize_program};
use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::resolve::{Resolution, ResolveError, Resolver};
use beskid_analysis::services::{LowerResolveTypeError, lower_normalize_resolve_type_spanned};
use beskid_analysis::syntax::{Program, Spanned};
use beskid_analysis::types::{TypeError, TypeResult};

use crate::surface::util::parse_pair;

pub fn parse_program(input: &str) -> Spanned<Program> {
    let pair = parse_pair(Rule::Program, input);
    Program::parse(pair).expect("expected program AST")
}

pub fn lower_to_hir(source: &str) -> Spanned<HirProgram> {
    let program = parse_program(source);
    let ast: Spanned<AstProgram> = program.into();
    lower_program(&ast)
}

pub fn resolve(source: &str) -> Result<Resolution, Vec<ResolveError>> {
    let mut hir = lower_to_hir(source);
    normalize_program(&mut hir).expect("normalization failed");
    Resolver::new().resolve_program(&hir)
}

pub fn typecheck(source: &str) -> Result<TypeResult, Vec<TypeError>> {
    let program = parse_program(source);
    match lower_normalize_resolve_type_spanned(&program) {
        Ok((_, _, typed)) => Ok(typed),
        Err(LowerResolveTypeError::Type { errors, .. }) => Err(errors),
        Err(other) => panic!("expected typing spine to reach type-check, got: {other}"),
    }
}

pub fn typecheck_hir(source: &str) -> (Spanned<HirProgram>, Resolution, TypeResult) {
    let program = parse_program(source);
    lower_normalize_resolve_type_spanned(&program).unwrap_or_else(|err| panic!("expected type success: {err}"))
}

pub fn lower_resolve(source: &str) -> (Spanned<HirProgram>, Resolution) {
    let mut hir = lower_to_hir(source);
    normalize_program(&mut hir).expect("normalization failed");
    let resolution = Resolver::new().resolve_program(&hir).expect("expected resolution to succeed");
    (hir, resolution)
}
