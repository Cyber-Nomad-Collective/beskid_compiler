use std::path::Path;

use crate::hir::{
    AstProgram, HirNormalizeError, HirProgram, index_program_from_base, lower_program as lower_hir_program,
    max_hir_node_id, normalize_program_with_resolution,
};
use crate::projects::assembly::ModuleIndex;
use crate::projects::{AssemblyDiscovery, assembly::ProgramAssembly};
use crate::resolve::resolver::{ResolveTraceContext, enter_resolve_span, resolve_program_traced};
use crate::resolve::{Resolution, ResolveError, Resolver};
use crate::syntax::{Program, Spanned};
use crate::types::{TypeChecker, TypeError, TypeResult};
use beskid_pipeline::{PipelineObserver, observe_phase_result, observe_phase_value, phases};

fn lower_trace_context(entry_source_path: Option<&std::path::PathBuf>) -> ResolveTraceContext<'_> {
    ResolveTraceContext {
        entry_path: entry_source_path.map(|path| path.as_path()),
        session_fingerprint: None,
        syntax_generation_id: None,
    }
}

fn index_hir_program(hir: &mut Spanned<HirProgram>, assembly: Option<&ProgramAssembly>) {
    let base = assembly
        .map(|assembly| assembly.hir_units.iter().map(|unit| max_hir_node_id(&unit.hir)).max().unwrap_or(0))
        .unwrap_or(0);
    let _ = index_program_from_base(hir, base);
}

fn enter_lower_span(entry_path: Option<&Path>) -> tracing::span::EnteredSpan {
    tracing::info_span!(
        target: "beskid.analysis",
        "beskid.analysis.lower",
        entry = tracing::field::display(
            entry_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string())
        ),
        session_fingerprint = tracing::field::display("<none>"),
        syntax_generation_id = 0,
    )
    .entered()
}

/// Controls whether dependency unit bodies are fully type-checked or only signatures prefetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyTypingPolicy {
    /// Type entry only; prefetch dependency signatures without typing dependency bodies.
    EntryOnly,
    /// Type entry and full dependency closure bodies.
    FullClosure,
}

impl DependencyTypingPolicy {
    fn type_dependency_bodies(self) -> bool {
        matches!(self, Self::FullClosure)
    }
}

/// How the typed-HIR spine obtains resolutions before normalize and type-check.
pub enum TypedHirResolution<'a> {
    /// Assembly-backed resolution (import closure or workspace scan).
    Assembly(Option<&'a ProgramAssembly>),
    /// Prefetched module index (analyze / IDE without assembly).
    ModuleIndex { module_index: &'a ModuleIndex, entry_source_path: Option<std::path::PathBuf> },
    /// Pass-1 resolution already computed (single-unit analyze without assembly).
    Pass1(&'a Resolution),
}

/// Lower AST → HIR, resolve, normalize, re-resolve, and type-check (IDE / tests / shared spine).
pub fn lower_normalize_resolve_type_spanned(
    program: &Spanned<Program>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    lower_normalize_resolve_type_spanned_with_assembly(program, None, None, DependencyTypingPolicy::FullClosure)
}

/// Like [`lower_normalize_resolve_type_spanned`], resolving against [`ProgramAssembly`] when provided.
pub fn lower_normalize_resolve_type_spanned_with_assembly(
    program: &Spanned<Program>,
    assembly: Option<&ProgramAssembly>,
    pipeline: Option<&dyn PipelineObserver>,
    policy: DependencyTypingPolicy,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let entry_path = assembly.map(|a| a.entry_unit().path.as_path());
    let _lower_guard = enter_lower_span(entry_path);
    let ast: Spanned<AstProgram> = program.clone().into();
    let hir = observe_phase_value(pipeline, phases::LOWER_AST, || lower_hir_program(&ast));
    typed_hir_from_lowered(hir, TypedHirResolution::Assembly(assembly), pipeline, policy)
}

/// Unified typed-HIR spine: normalize, re-resolve, and type-check from an already-lowered program.
pub fn typed_hir_from_lowered(
    hir: Spanned<HirProgram>,
    resolution: TypedHirResolution<'_>,
    pipeline: Option<&dyn PipelineObserver>,
    policy: DependencyTypingPolicy,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    match resolution {
        TypedHirResolution::Assembly(assembly) => {
            typed_hir_from_lowered_with_assembly_inner(hir, assembly, pipeline, policy)
        }
        TypedHirResolution::ModuleIndex { module_index, entry_source_path } => {
            typed_hir_from_lowered_with_module_index_inner(hir, module_index, entry_source_path, pipeline, policy)
        }
        TypedHirResolution::Pass1(resolution_pass1) => {
            typed_hir_from_lowered_after_resolution_inner(hir, resolution_pass1, pipeline, policy)
        }
    }
}

fn typed_hir_from_lowered_with_assembly_inner(
    mut hir: Spanned<HirProgram>,
    assembly: Option<&ProgramAssembly>,
    pipeline: Option<&dyn PipelineObserver>,
    policy: DependencyTypingPolicy,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let entry_source_path = assembly.map(|a| a.entry_unit().path.clone());
    let _lower_guard = enter_lower_span(entry_source_path.as_deref().map(Path::new));
    let resolve_ctx = lower_trace_context(entry_source_path.as_ref());
    let dependency_hir_refs = assembly.map(|a| a.dependency_hir_refs()).unwrap_or_default();
    let dependency_source_paths: Vec<std::path::PathBuf> = assembly
        .map(|a| {
            a.hir_units
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != a.entry_index)
                .map(|(_, unit)| unit.path.clone())
                .collect()
        })
        .unwrap_or_default();
    let resolution_pass1 = observe_phase_result(pipeline, phases::LOWER_RESOLVE_PASS1, || {
        let _resolve_guard = enter_resolve_span(resolve_ctx);
        resolve_entry_hir(&hir, assembly, entry_source_path.as_ref())
    })?;
    observe_phase_result(pipeline, phases::LOWER_NORMALIZE, || {
        normalize_program_with_resolution(&mut hir, Some(&resolution_pass1), &dependency_hir_refs)
            .map_err(LowerResolveTypeError::Normalize)
    })?;
    index_hir_program(&mut hir, assembly);
    let resolution = observe_phase_result(pipeline, phases::LOWER_RESOLVE, || {
        let _resolve_guard = enter_resolve_span(resolve_ctx);
        if let Some(assembly) = assembly {
            let resolve = if assembly.discovery == AssemblyDiscovery::ImportClosure {
                assembly.module_index.resolve_assembly_closure(&hir, assembly)
            } else {
                assembly.module_index.resolve_for_api_documentation(&hir, assembly)
            };
            resolve.ok_or(LowerResolveTypeError::Resolve(vec![crate::resolve::ResolveError::UnknownModulePath {
                path: "<assembly>".to_string(),
                span: hir.span,
            }]))
        } else {
            resolve_entry_hir(&hir, None, None)
        }
    })?;
    type_check_lowered_hir(
        hir,
        &resolution,
        &dependency_hir_refs,
        Some(&dependency_source_paths),
        entry_source_path,
        policy,
        assembly.map(|a| a.module_index.as_ref()),
        assembly,
        None,
        pipeline,
    )
}

fn typed_hir_from_lowered_with_module_index_inner(
    mut hir: Spanned<HirProgram>,
    module_index: &ModuleIndex,
    entry_source_path: Option<std::path::PathBuf>,
    pipeline: Option<&dyn PipelineObserver>,
    policy: DependencyTypingPolicy,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let _lower_guard = enter_lower_span(entry_source_path.as_deref());
    let resolve_ctx = lower_trace_context(entry_source_path.as_ref());
    let dependency_hir_refs: Vec<&Spanned<HirProgram>> = Vec::new();
    let resolution_pass1 = observe_phase_result(pipeline, phases::LOWER_RESOLVE_PASS1, || {
        let _resolve_guard = enter_resolve_span(resolve_ctx);
        module_index.resolve_entry_hir(&hir, entry_source_path.as_ref()).map_err(LowerResolveTypeError::Resolve)
    })?;
    observe_phase_result(pipeline, phases::LOWER_NORMALIZE, || {
        normalize_program_with_resolution(&mut hir, Some(&resolution_pass1), &dependency_hir_refs)
            .map_err(LowerResolveTypeError::Normalize)
    })?;
    index_hir_program(&mut hir, None);
    let resolution = observe_phase_result(pipeline, phases::LOWER_RESOLVE, || {
        let _resolve_guard = enter_resolve_span(resolve_ctx);
        module_index.resolve_entry_hir(&hir, entry_source_path.as_ref()).map_err(LowerResolveTypeError::Resolve)
    })?;
    type_check_lowered_hir(
        hir,
        &resolution,
        &dependency_hir_refs,
        None,
        entry_source_path,
        policy,
        Some(module_index),
        None,
        None,
        pipeline,
    )
}

fn typed_hir_from_lowered_after_resolution_inner(
    mut hir: Spanned<HirProgram>,
    resolution_pass1: &Resolution,
    pipeline: Option<&dyn PipelineObserver>,
    policy: DependencyTypingPolicy,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    let _lower_guard = enter_lower_span(None);
    let dependency_hir_refs: Vec<&Spanned<HirProgram>> = Vec::new();
    observe_phase_result(pipeline, phases::LOWER_NORMALIZE, || {
        normalize_program_with_resolution(&mut hir, Some(resolution_pass1), &dependency_hir_refs)
            .map_err(LowerResolveTypeError::Normalize)
    })?;
    index_hir_program(&mut hir, None);
    let resolution = observe_phase_result(pipeline, phases::LOWER_RESOLVE, || {
        resolve_program_traced(&hir, lower_trace_context(None)).map_err(LowerResolveTypeError::Resolve)
    })?;
    type_check_lowered_hir(hir, &resolution, &dependency_hir_refs, None, None, policy, None, None, None, pipeline)
}

fn type_check_lowered_hir(
    mut hir: Spanned<HirProgram>,
    resolution: &Resolution,
    dependency_hir_refs: &[&Spanned<HirProgram>],
    dependency_source_paths: Option<&[std::path::PathBuf]>,
    entry_source_path: Option<std::path::PathBuf>,
    policy: DependencyTypingPolicy,
    module_index: Option<&ModuleIndex>,
    assembly: Option<&ProgramAssembly>,
    prefetched_surfaces: Option<
        &std::collections::HashMap<std::path::PathBuf, std::sync::Arc<crate::types::surface::UnitTypeSurface>>,
    >,
    pipeline: Option<&dyn PipelineObserver>,
) -> std::result::Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    observe_phase_result(pipeline, phases::LOWER_TYPE_CHECK, || {
        let progress = pipeline.map(|obs| (obs, phases::LOWER_TYPE_CHECK));
        let (typed, type_errors) = TypeChecker::check_entry(
            &mut hir,
            resolution,
            dependency_hir_refs,
            dependency_source_paths,
            entry_source_path,
            policy.type_dependency_bodies(),
            module_index,
            assembly,
            prefetched_surfaces,
            progress,
        );
        if !type_errors.is_empty() {
            Err(LowerResolveTypeError::Type { errors: type_errors, typed: Box::new(typed) })
        } else {
            Ok((hir, resolution.clone(), typed))
        }
    })
}

fn resolve_entry_hir(
    hir: &Spanned<HirProgram>,
    assembly: Option<&ProgramAssembly>,
    entry_source_path: Option<&std::path::PathBuf>,
) -> Result<Resolution, LowerResolveTypeError> {
    if let Some(assembly) = assembly {
        assembly.module_index.resolve_entry_hir(hir, entry_source_path).map_err(LowerResolveTypeError::Resolve)
    } else {
        Resolver::new().resolve_program(hir).map_err(LowerResolveTypeError::Resolve)
    }
}

/// Which pipeline stage failed when running [`lower_normalize_resolve_type_spanned`].
#[derive(Debug, thiserror::Error)]
pub enum LowerResolveTypeError {
    #[error("Normalization failed\n{}", format_errors(.0))]
    Normalize(Vec<HirNormalizeError>),
    #[error("Resolution failed\n{}", format_errors(.0))]
    Resolve(Vec<ResolveError>),
    #[error("Type checking failed\n{}", format_errors(.errors))]
    Type { errors: Vec<TypeError>, typed: Box<TypeResult> },
}

fn format_errors<E: std::fmt::Display>(errors: &[E]) -> String {
    errors.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n")
}
