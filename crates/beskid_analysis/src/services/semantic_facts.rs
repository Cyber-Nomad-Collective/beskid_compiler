//! Generation-bound resolution and type facts over expanded syntax.

use std::path::PathBuf;

use beskid_pipeline::{observe_phase_result, phases, PipelineObserver};

use crate::projects::assembly::{ModuleIndex, ProgramAssembly};
use crate::resolve::{Resolution, ResolveError, Resolver};
use crate::syntax::{Program, Spanned};
use crate::types::{TypeChecker, TypeError, TypeResult};

/// Controls whether dependency unit bodies are checked or only their public surfaces are loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyTypingPolicy {
    EntryOnly,
    FullClosure,
}

impl DependencyTypingPolicy {
    fn type_dependency_bodies(self) -> bool {
        matches!(self, Self::FullClosure)
    }
}

/// Source used to resolve one expanded syntax program.
pub enum ProgramResolutionSource<'a> {
    Assembly(Option<&'a ProgramAssembly>),
    ModuleIndex { module_index: &'a ModuleIndex, entry_source_path: Option<PathBuf> },
    Existing(&'a Resolution),
}

/// Resolve and type-check one standalone expanded syntax program.
pub fn resolve_and_type_program(
    program: &Spanned<Program>,
) -> Result<(Spanned<Program>, Resolution, TypeResult), SemanticFactsError> {
    resolve_and_type_program_with_assembly(program, None, None, DependencyTypingPolicy::FullClosure)
}

/// Resolve and type-check expanded syntax against an optional syntax-only assembly.
pub fn resolve_and_type_program_with_assembly(
    program: &Spanned<Program>,
    assembly: Option<&ProgramAssembly>,
    pipeline: Option<&dyn PipelineObserver>,
    policy: DependencyTypingPolicy,
) -> Result<(Spanned<Program>, Resolution, TypeResult), SemanticFactsError> {
    type_resolved_program(program.clone(), ProgramResolutionSource::Assembly(assembly), pipeline, policy)
}

/// Produce semantic facts without constructing or normalizing a second tree.
pub fn type_resolved_program(
    mut program: Spanned<Program>,
    source: ProgramResolutionSource<'_>,
    pipeline: Option<&dyn PipelineObserver>,
    policy: DependencyTypingPolicy,
) -> Result<(Spanned<Program>, Resolution, TypeResult), SemanticFactsError> {
    let (resolution, assembly, module_index, entry_path) = match source {
        ProgramResolutionSource::Assembly(assembly) => {
            let entry_path = assembly.map(|value| value.entry_unit().path.clone());
            let resolution = observe_phase_result(pipeline, phases::LOWER_RESOLVE, || {
                if let Some(assembly) = assembly {
                    assembly
                        .module_index
                        .resolve_entry_program(&program, entry_path.as_ref())
                        .map_err(SemanticFactsError::Resolve)
                } else {
                    Resolver::new().resolve_program(&program).map_err(SemanticFactsError::Resolve)
                }
            })?;
            (resolution, assembly, assembly.map(|value| value.module_index.as_ref()), entry_path)
        }
        ProgramResolutionSource::ModuleIndex { module_index, entry_source_path } => {
            let resolution = module_index
                .resolve_entry_program(&program, entry_source_path.as_ref())
                .map_err(SemanticFactsError::Resolve)?;
            (resolution, None, Some(module_index), entry_source_path)
        }
        ProgramResolutionSource::Existing(resolution) => (resolution.clone(), None, None, None),
    };

    let dependency_programs: Vec<&Spanned<Program>> = assembly
        .map(|value| {
            value
                .units
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != value.entry_index)
                .map(|(_, unit)| &unit.program)
                .collect()
        })
        .unwrap_or_default();
    let dependency_paths: Vec<PathBuf> = assembly
        .map(|value| {
            value
                .units
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != value.entry_index)
                .map(|(_, unit)| unit.path.clone())
                .collect()
        })
        .unwrap_or_default();

    let (typed, errors) = observe_phase_result(pipeline, phases::LOWER_TYPE_CHECK, || {
        Ok::<_, SemanticFactsError>(TypeChecker::check_entry(
            &mut program,
            &resolution,
            &dependency_programs,
            (!dependency_paths.is_empty()).then_some(dependency_paths.as_slice()),
            entry_path,
            policy.type_dependency_bodies(),
            module_index,
            assembly,
            None,
            pipeline.map(|observer| (observer, phases::LOWER_TYPE_CHECK)),
        ))
    })?;

    if errors.is_empty() {
        Ok((program, resolution, typed))
    } else {
        Err(SemanticFactsError::Type { errors, typed: Box::new(typed) })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SemanticFactsError {
    #[error("name resolution failed")]
    Resolve(Vec<ResolveError>),
    #[error("type checking failed")]
    Type { errors: Vec<TypeError>, typed: Box<TypeResult> },
}
