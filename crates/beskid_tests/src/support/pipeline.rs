//! Parse → lower → resolve → typecheck helpers for integration tests.

use beskid_analysis::Rule;
use beskid_analysis::hir::{AstProgram, HirProgram, lower_program, normalize_program};
use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::resolve::{Resolution, ResolveError, Resolver};
use beskid_analysis::services::typed_hir_from_lowered_after_resolution;
use beskid_analysis::syntax::{Program, Spanned};
use beskid_analysis::types::{TypeError, TypeResult, type_program};

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
    let hir = lower_to_hir(source);
    let resolution =
        Resolver::new()
            .resolve_program(&hir)
            .unwrap_or_else(|errors: Vec<ResolveError>| {
                panic!("expected resolver to succeed, got errors: {errors:?}")
            });
    type_program(&hir, &resolution)
}

pub fn typecheck_hir(source: &str) -> (Spanned<HirProgram>, Resolution, TypeResult) {
    let program = parse_program(source);
    let ast: Spanned<AstProgram> = program.into();
    let hir = lower_program(&ast);
    let resolution = Resolver::new()
        .resolve_program(&hir)
        .unwrap_or_else(|errors| panic!("expected resolution success: {errors:?}"));
    let (hir, resolution, typed) = typed_hir_from_lowered_after_resolution(hir, &resolution)
        .unwrap_or_else(|err| panic!("expected type success: {err}"));
    (hir, resolution, typed)
}

pub fn lower_resolve(source: &str) -> (Spanned<HirProgram>, Resolution) {
    let hir = lower_to_hir(source);
    let resolution = Resolver::new()
        .resolve_program(&hir)
        .expect("expected resolution to succeed");
    (hir, resolution)
}
