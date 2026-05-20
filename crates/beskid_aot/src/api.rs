//! Public AOT API: build requests, output kinds, and the [`build`] orchestration entry point.

use std::path::PathBuf;

use beskid_codegen::CodegenArtifact;
use beskid_pipeline::{
    SharedPipelineObserver, observe_phase_result,
    phases::{AOT_EMIT_OBJECT, AOT_LINK, AOT_RUNTIME},
};

use crate::error::{AotError, AotResult};
use crate::linker::{LinkRequest, link};
use crate::object_module::BeskidObjectModule;
use crate::runtime::{RuntimeBuildRequest, prepare_runtime};
use crate::target::detect_target;

/// Final linked artifact shape (or object-only emission without linking).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildOutputKind {
    Exe,
    StaticLib,
    SharedLib,
    ObjectOnly,
}

/// Beskid project classification used when choosing a default [`BuildOutputKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTargetKind {
    App,
    Lib,
    Test,
}

/// Optimization profile for selecting a matching prebuilt runtime archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

/// Hint for shared-library link lines (`-Wl,-Bstatic` / `-Wl,-Bdynamic`); ignored for other kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMode {
    Auto,
    PreferStatic,
    PreferDynamic,
}

/// How the AOT pipeline obtains a runtime static library to link against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeStrategy {
    UsePrebuilt { path: PathBuf, abi_version: u32 },
    Standalone,
}

/// Which symbols from the object file participate in export lists / entrypoint checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportPolicy {
    PublicOnly,
    Explicit(Vec<String>),
    AllDefined,
}

/// Inputs and options for a single AOT build (object emit, optional runtime, link).
#[derive(Clone)]
pub struct AotBuildRequest {
    pub artifact: CodegenArtifact,
    pub output_kind: BuildOutputKind,
    pub output_path: PathBuf,
    pub object_path: Option<PathBuf>,
    pub target_triple: Option<String>,
    pub profile: BuildProfile,
    pub entrypoint: String,
    pub export_policy: ExportPolicy,
    pub link_mode: LinkMode,
    pub runtime: RuntimeStrategy,
    pub verbose_link: bool,
    /// Optional compilation pipeline observer (e.g. CLI progress).
    pub pipeline: Option<SharedPipelineObserver>,
}

impl std::fmt::Debug for AotBuildRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AotBuildRequest")
            .field("output_kind", &self.output_kind)
            .field("output_path", &self.output_path)
            .field("object_path", &self.object_path)
            .field("target_triple", &self.target_triple)
            .field("profile", &self.profile)
            .field("entrypoint", &self.entrypoint)
            .field("export_policy", &self.export_policy)
            .field("link_mode", &self.link_mode)
            .field("runtime", &self.runtime)
            .field("verbose_link", &self.verbose_link)
            .field("pipeline", &self.pipeline.is_some())
            .finish_non_exhaustive()
    }
}

impl AotBuildRequest {
    /// Build request with defaults shared by integration tests and ad hoc tooling runs.
    ///
    /// Sets [`BuildProfile::Debug`], [`ExportPolicy::PublicOnly`], [`LinkMode::Auto`],
    /// bundled prebuilt [`RuntimeStrategy::UsePrebuilt`], no pipeline observer, and no explicit
    /// target triple or secondary object path. Override any field with struct update syntax, for
    /// example `AotBuildRequest { runtime: RuntimeStrategy::Standalone, ..AotBuildRequest::with_defaults(...) }`.
    pub fn with_defaults(
        artifact: CodegenArtifact,
        output_kind: BuildOutputKind,
        output_path: PathBuf,
        entrypoint: impl Into<String>,
    ) -> Self {
        let profile = BuildProfile::Debug;
        let runtime = crate::bundled::default_runtime_strategy(profile, None).unwrap_or_else(
            |err| {
                panic!(
                    "with_defaults requires a prebuilt runtime archive (build beskid_runtime_bridge): {err}"
                )
            },
        );
        Self {
            artifact,
            output_kind,
            output_path,
            object_path: None,
            target_triple: None,
            profile,
            entrypoint: entrypoint.into(),
            export_policy: ExportPolicy::PublicOnly,
            link_mode: LinkMode::Auto,
            runtime,
            verbose_link: false,
            pipeline: None,
        }
    }
}

/// Paths and metadata produced by [`build`] or [`emit_object_only`].
#[derive(Debug, Clone)]
pub struct AotBuildResult {
    pub object_path: PathBuf,
    pub final_path: Option<PathBuf>,
    pub exported_symbols: Vec<String>,
    pub linker_invocation: Option<String>,
}

#[derive(Debug, Clone)]
struct ObjectStageResult {
    object_path: PathBuf,
    exported_symbols: Vec<String>,
}

/// Emit a single object file; fails unless `req.output_kind` is [`BuildOutputKind::ObjectOnly`].
pub fn emit_object_only(req: AotBuildRequest) -> AotResult<AotBuildResult> {
    if req.output_kind != BuildOutputKind::ObjectOnly {
        return Err(AotError::InvalidRequest {
            message: "emit_object_only requires BuildOutputKind::ObjectOnly".to_owned(),
        });
    }
    build(req)
}

/// Default artifact kind for a project target (`Lib` → shared library, else executable).
pub fn default_output_kind(target_kind: Option<ProjectTargetKind>) -> BuildOutputKind {
    match target_kind {
        Some(ProjectTargetKind::Lib) => BuildOutputKind::SharedLib,
        Some(ProjectTargetKind::App) | Some(ProjectTargetKind::Test) | None => BuildOutputKind::Exe,
    }
}

/// Normalize CLI-style entrypoint: non-empty string or default `"main"`.
pub fn resolve_entrypoint(entrypoint: Option<String>) -> AotResult<String> {
    if let Some(entrypoint) = entrypoint {
        if entrypoint.trim().is_empty() {
            return Err(AotError::InvalidRequest {
                message: "entrypoint must not be empty".to_owned(),
            });
        }
        return Ok(entrypoint);
    }

    Ok("main".to_owned())
}

/// Run object emission, optional runtime preparation, and linking per `req.output_kind`.
pub fn build(req: AotBuildRequest) -> AotResult<AotBuildResult> {
    validate_request(&req)?;

    let object_stage = emit_object_stage(&req)?;

    if req.output_kind == BuildOutputKind::ObjectOnly {
        return Ok(AotBuildResult {
            object_path: object_stage.object_path,
            final_path: None,
            exported_symbols: object_stage.exported_symbols,
            linker_invocation: None,
        });
    }

    if requires_entrypoint(req.output_kind) {
        ensure_entrypoint_exported(&req, &object_stage.exported_symbols)?;
    }
    let runtime = prepare_runtime_stage(&req)?;
    let link_result = link_stage(&req, &object_stage, runtime.staticlib_path)?;

    Ok(AotBuildResult {
        object_path: object_stage.object_path,
        final_path: Some(link_result.output_path),
        exported_symbols: link_result.exported_symbols,
        linker_invocation: Some(link_result.command_line),
    })
}

fn emit_object_stage(req: &AotBuildRequest) -> AotResult<ObjectStageResult> {
    let target = detect_target(req.target_triple.as_deref())?;
    let object_path = req
        .object_path
        .clone()
        .unwrap_or_else(|| req.output_path.with_extension(target.object_ext));

    let mut object_module = BeskidObjectModule::new(req.target_triple.as_deref())?;
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_EMIT_OBJECT, || {
        object_module.compile_artifact(&req.artifact, obs)
    })?;

    let all_symbols = object_module.declared_symbols();
    let exported_symbols = apply_export_policy(all_symbols, &req.export_policy);

    object_module.finalize_to_path(&object_path)?;

    Ok(ObjectStageResult {
        object_path,
        exported_symbols,
    })
}

fn ensure_entrypoint_exported(req: &AotBuildRequest, exported_symbols: &[String]) -> AotResult<()> {
    if exported_symbols.iter().any(|sym| sym == &req.entrypoint) {
        return Ok(());
    }

    Err(AotError::MissingEntrypoint {
        symbol: req.entrypoint.clone(),
    })
}

fn prepare_runtime_stage(req: &AotBuildRequest) -> AotResult<crate::runtime::RuntimeArtifact> {
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_RUNTIME, || {
        prepare_runtime(&RuntimeBuildRequest {
            strategy: req.runtime.clone(),
        })
    })
}

fn link_stage(
    req: &AotBuildRequest,
    object_stage: &ObjectStageResult,
    runtime_staticlib: Option<PathBuf>,
) -> AotResult<crate::linker::LinkResult> {
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_LINK, || {
        link(&LinkRequest {
            target_triple: req.target_triple.clone(),
            output_kind: req.output_kind,
            output_path: req.output_path.clone(),
            object_path: object_stage.object_path.clone(),
            runtime_staticlib,
            entrypoint_symbol: req.entrypoint.clone(),
            exported_symbols: object_stage.exported_symbols.clone(),
            link_mode: req.link_mode,
            verbose: req.verbose_link,
        })
    })
}

fn validate_request(req: &AotBuildRequest) -> AotResult<()> {
    if req.artifact.functions.is_empty() && requires_lowered_functions(req.output_kind) {
        return Err(AotError::InvalidRequest {
            message: "codegen artifact has no lowered functions for executable build".to_owned(),
        });
    }
    if requires_entrypoint(req.output_kind) && req.entrypoint.trim().is_empty() {
        return Err(AotError::InvalidRequest {
            message: "entrypoint must not be empty".to_owned(),
        });
    }
    Ok(())
}

fn requires_lowered_functions(output_kind: BuildOutputKind) -> bool {
    output_kind == BuildOutputKind::Exe
}

fn requires_entrypoint(output_kind: BuildOutputKind) -> bool {
    output_kind == BuildOutputKind::Exe
}

fn apply_export_policy(symbols: Vec<String>, policy: &ExportPolicy) -> Vec<String> {
    match policy {
        ExportPolicy::AllDefined => symbols,
        ExportPolicy::PublicOnly => symbols
            .into_iter()
            .filter(|name| !name.starts_with("__"))
            .collect(),
        ExportPolicy::Explicit(expected) => symbols
            .into_iter()
            .filter(|name| expected.iter().any(|wanted| wanted == name))
            .collect(),
    }
}

#[cfg(test)]
mod with_defaults_tests {
    use super::*;

    #[test]
    fn with_defaults_matches_expected_shared_fields() {
        let req = AotBuildRequest::with_defaults(
            CodegenArtifact::default(),
            BuildOutputKind::ObjectOnly,
            PathBuf::from("/tmp/out.o"),
            "main",
        );
        assert_eq!(req.object_path, None);
        assert_eq!(req.target_triple, None);
        assert_eq!(req.profile, BuildProfile::Debug);
        assert_eq!(req.entrypoint, "main");
        assert_eq!(req.export_policy, ExportPolicy::PublicOnly);
        assert_eq!(req.link_mode, LinkMode::Auto);
        assert!(matches!(req.runtime, RuntimeStrategy::UsePrebuilt { .. }));
        assert!(!req.verbose_link);
        assert!(req.pipeline.is_none());
    }
}
