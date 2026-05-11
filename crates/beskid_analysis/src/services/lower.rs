use std::error::Error;
use std::fmt;

use crate::hir::{
    AstProgram, HirNormalizeError, HirProgram, lower_program as lower_hir_program,
    normalize_program,
};
use crate::resolve::{Resolution, ResolveError, Resolver};
use crate::syntax::{Program, Spanned};
use crate::types::{TypeError, TypeResult, type_program};

/// Lower AST → HIR, normalize, resolve, and type-check in one pipeline (IDE / tests).
pub fn lower_normalize_resolve_type_spanned(
    program: &Spanned<Program>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let ast: Spanned<AstProgram> = program.clone().into();
    let mut hir: Spanned<HirProgram> = lower_hir_program(&ast);
    normalize_program(&mut hir).map_err(LowerResolveTypeError::Normalize)?;
    let resolution = Resolver::new()
        .resolve_program(&hir)
        .map_err(LowerResolveTypeError::Resolve)?;
    let typed = type_program(&hir, &resolution).map_err(LowerResolveTypeError::Type)?;
    Ok((hir, resolution, typed))
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
