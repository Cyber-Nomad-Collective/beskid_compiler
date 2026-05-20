use std::error::Error;
use std::fmt;

use crate::hir::{
    AstProgram, HirNormalizeError, HirProgram, lower_program as lower_hir_program,
    normalize_program,
};
use crate::projects::assembly::ProgramAssembly;
use crate::resolve::{Resolution, ResolveError, Resolver};
use crate::syntax::{Program, Spanned};
use crate::types::{TypeContext, TypeError, TypeResult};

/// Lower AST → HIR, normalize, resolve, and type-check in one pipeline (IDE / tests).
pub fn lower_normalize_resolve_type_spanned(
    program: &Spanned<Program>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    lower_normalize_resolve_type_spanned_with_assembly(program, None)
}

/// Like [`lower_normalize_resolve_type_spanned`], resolving against [`ProgramAssembly`] when provided.
pub fn lower_normalize_resolve_type_spanned_with_assembly(
    program: &Spanned<Program>,
    assembly: Option<&ProgramAssembly>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let ast: Spanned<AstProgram> = program.clone().into();
    let mut hir: Spanned<HirProgram> = lower_hir_program(&ast);
    normalize_program(&mut hir).map_err(LowerResolveTypeError::Normalize)?;
    let resolution = if let Some(assembly) = assembly {
        assembly
            .module_index
            .resolve_entry_hir(&hir)
            .map_err(LowerResolveTypeError::Resolve)?
    } else {
        Resolver::new()
            .resolve_program(&hir)
            .map_err(LowerResolveTypeError::Resolve)?
    };
    let dependency_hirs: Vec<Spanned<HirProgram>> = assembly
        .map(|assembly| {
            assembly
                .units
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != assembly.entry_index)
                .map(|(_, unit)| {
                    let unit_ast: Spanned<AstProgram> = unit.program.clone().into();
                    lower_hir_program(&unit_ast)
                })
                .collect()
        })
        .unwrap_or_default();
    let (typed, type_errors) = TypeContext::new(&resolution)
        .type_program_with_errors_and_dependencies(&hir, &dependency_hirs);
    if !type_errors.is_empty() {
        return Err(LowerResolveTypeError::Type(type_errors));
    }
    Ok((hir, resolution, typed))
}

/// Resolves with a [`ModuleIndex`] only (no dependency signature seeding). Prefer [`lower_normalize_resolve_type_spanned_with_assembly`].
pub fn lower_normalize_resolve_type_spanned_with_index(
    program: &Spanned<Program>,
    module_index: Option<&crate::projects::assembly::ModuleIndex>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let _ = module_index;
    lower_normalize_resolve_type_spanned_with_assembly(program, None)
}

/// Which pipeline stage failed when running [`lower_normalize_resolve_type_spanned`].
#[derive(Debug, Clone)]
pub enum LowerResolveTypeError {
    Normalize(Vec<HirNormalizeError>),
    Resolve(Vec<ResolveError>),
    Type(Vec<TypeError>),
}

impl fmt::Display for LowerResolveTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LowerResolveTypeError::Normalize(errors) => {
                writeln!(f, "Normalization failed")?;
                for err in errors {
                    writeln!(f, "  - {err}")?;
                }
                Ok(())
            }
            LowerResolveTypeError::Resolve(errors) => {
                writeln!(f, "Resolution failed")?;
                for err in errors {
                    writeln!(f, "  - {err}")?;
                }
                Ok(())
            }
            LowerResolveTypeError::Type(errors) => {
                writeln!(f, "Type checking failed")?;
                for err in errors {
                    writeln!(f, "  - {err}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for LowerResolveTypeError {}
