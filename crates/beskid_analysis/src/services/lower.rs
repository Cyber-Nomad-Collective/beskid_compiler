use crate::hir::{
    AstProgram, HirNormalizeError, HirProgram, lower_program as lower_hir_program,
    normalize_program_with_resolution,
};
use crate::projects::assembly::ProgramAssembly;
use crate::resolve::{Resolution, ResolveError, Resolver};
use crate::syntax::{Program, Spanned};
use crate::types::{TypeContext, TypeError, TypeResult};

/// Lower AST → HIR, resolve, normalize, re-resolve, and type-check (IDE / tests / shared spine).
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
    let hir = lower_hir_program(&ast);
    typed_hir_from_lowered_with_assembly(hir, assembly)
}

/// Continue typed-HIR spine after pass-1 resolution (single-unit analyze without assembly).
pub fn typed_hir_from_lowered_after_resolution(
    mut hir: Spanned<HirProgram>,
    resolution_pass1: &Resolution,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    normalize_program_with_resolution(&mut hir, Some(resolution_pass1), &[])
        .map_err(LowerResolveTypeError::Normalize)?;
    let resolution = Resolver::new()
        .resolve_program(&hir)
        .map_err(LowerResolveTypeError::Resolve)?;
    let (typed, type_errors) =
        TypeContext::new(&resolution).type_program_with_errors_and_dependencies(&hir, &[]);
    if !type_errors.is_empty() {
        return Err(LowerResolveTypeError::Type(type_errors));
    }
    Ok((hir, resolution, typed))
}

/// Typed HIR spine using a prefetched [`crate::projects::assembly::ModuleIndex`] (analyze / IDE).
pub fn typed_hir_from_lowered_with_module_index(
    mut hir: Spanned<HirProgram>,
    module_index: &crate::projects::assembly::ModuleIndex,
    entry_source_path: Option<std::path::PathBuf>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let resolution_pass1 = module_index
        .resolve_entry_hir(&hir, entry_source_path.as_ref())
        .map_err(LowerResolveTypeError::Resolve)?;
    normalize_program_with_resolution(&mut hir, Some(&resolution_pass1), &[])
        .map_err(LowerResolveTypeError::Normalize)?;
    let resolution = module_index
        .resolve_entry_hir(&hir, entry_source_path.as_ref())
        .map_err(LowerResolveTypeError::Resolve)?;
    let (typed, type_errors) =
        TypeContext::new(&resolution).type_program_with_errors_and_dependencies(&hir, &[]);
    if !type_errors.is_empty() {
        return Err(LowerResolveTypeError::Type(type_errors));
    }
    Ok((hir, resolution, typed))
}

/// Typed HIR spine for an already-lowered entry program (semantic rules / document analysis).
pub fn typed_hir_from_lowered_with_assembly(
    mut hir: Spanned<HirProgram>,
    assembly: Option<&ProgramAssembly>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let dependency_hirs = dependency_hirs_from_assembly(assembly);
    let resolution_pass1 = resolve_entry_hir(&hir, assembly, None)?;
    normalize_program_with_resolution(&mut hir, Some(&resolution_pass1), &dependency_hirs)
        .map_err(LowerResolveTypeError::Normalize)?;
    let entry_path = assembly.map(|a| a.entry_unit().path.clone());
    let resolution = resolve_entry_hir(&hir, assembly, entry_path.as_ref())?;
    let (typed, type_errors) = TypeContext::new(&resolution)
        .type_program_with_errors_and_dependencies(&hir, &dependency_hirs);
    if !type_errors.is_empty() {
        return Err(LowerResolveTypeError::Type(type_errors));
    }
    Ok((hir, resolution, typed))
}

fn dependency_hirs_from_assembly(assembly: Option<&ProgramAssembly>) -> Vec<Spanned<HirProgram>> {
    assembly
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
        .unwrap_or_default()
}

fn resolve_entry_hir(
    hir: &Spanned<HirProgram>,
    assembly: Option<&ProgramAssembly>,
    entry_source_path: Option<&std::path::PathBuf>,
) -> Result<Resolution, LowerResolveTypeError> {
    if let Some(assembly) = assembly {
        assembly
            .module_index
            .resolve_entry_hir(hir, entry_source_path)
            .map_err(LowerResolveTypeError::Resolve)
    } else {
        Resolver::new()
            .resolve_program(hir)
            .map_err(LowerResolveTypeError::Resolve)
    }
}

/// Resolves with a [`ModuleIndex`] only (no dependency signature seeding). Prefer [`lower_normalize_resolve_type_spanned_with_assembly`].
///
/// Currently delegates to [`lower_normalize_resolve_type_spanned_with_assembly`] without assembly;
/// the `module_index` parameter is reserved for future support of module-index-driven resolution.
#[allow(unused_variables)]
pub fn lower_normalize_resolve_type_spanned_with_index(
    program: &Spanned<Program>,
    module_index: Option<&crate::projects::assembly::ModuleIndex>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    lower_normalize_resolve_type_spanned_with_assembly(program, None)
}

/// Which pipeline stage failed when running [`lower_normalize_resolve_type_spanned`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum LowerResolveTypeError {
    #[error("Normalization failed\n{}", format_errors(.0))]
    Normalize(Vec<HirNormalizeError>),
    #[error("Resolution failed\n{}", format_errors(.0))]
    Resolve(Vec<ResolveError>),
    #[error("Type checking failed\n{}", format_errors(.0))]
    Type(Vec<TypeError>),
}

fn format_errors<E: std::fmt::Display>(errors: &[E]) -> String {
    errors.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n")
}
