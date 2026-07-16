//! Public AOT API: build requests, output kinds, and the [`build`] orchestration entry point.

use std::path::PathBuf;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_codegen::CodegenArtifact;
use beskid_pipeline::{
    SharedPipelineObserver, observe_phase_result,
    phases::{AOT_EMIT_OBJECT, AOT_LINK, AOT_RUNTIME},
};

use std::collections::HashSet;

use crate::error::{AotError, AotResult};
use crate::export_table::ExportTable;
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

/// Optimization profile for selecting a matching installed runtime kit.
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

/// Identity of the one exact ABI-v5 runtime kit linked into a final artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeKitRequest {
    pub prefix: PathBuf,
    pub target: TargetMetadata,
    pub profile: RuntimeKitProfile,
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
    /// Required for linked output; object-only emission deliberately has no runtime dependency.
    pub runtime: Option<RuntimeKitRequest>,
    pub verbose_link: bool,
    /// Logical library names (for example `"c"`, `"m"`) passed as `-l<name>` to the host linker.
    pub external_libraries: Vec<String>,
    /// Extra `-L` search paths for the host linker.
    pub library_search_paths: Vec<PathBuf>,
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
    /// exact installed ABI-v5 kit for linked outputs, no pipeline observer, and no explicit
    /// target triple or secondary object path. Object-only output has no runtime dependency.
    pub fn with_defaults(
        artifact: CodegenArtifact,
        output_kind: BuildOutputKind,
        output_path: PathBuf,
        entrypoint: impl Into<String>,
    ) -> Self {
        let profile = BuildProfile::Debug;
        let runtime = (output_kind != BuildOutputKind::ObjectOnly).then(|| {
            crate::bundled::default_runtime_strategy(profile, None).unwrap_or_else(|err| {
                panic!("with_defaults requires an exact installed ABI-v5 runtime kit: {err}")
            })
        });
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
            external_libraries: Vec::new(),
            library_search_paths: Vec::new(),
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

/// Native static/shared library inputs suitable for higher-level runtime-kit publication.
#[derive(Debug, Clone)]
pub struct NativeLibraryPair {
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub provenance_symbols: Vec<String>,
}

/// Emit native library artifacts from an existing codegen artifact without requiring a runtime
/// kit. This is intentionally a library-only primitive: executable entrypoint and runtime-kit
/// validation remain enforced by [`build`].
pub fn emit_library_pair(
    artifact: CodegenArtifact,
    output_dir: PathBuf,
    name: &str,
    target_triple: Option<String>,
    exported_symbols: Vec<String>,
) -> AotResult<NativeLibraryPair> {
    let target = detect_target(target_triple.as_deref())?;
    std::fs::create_dir_all(&output_dir).map_err(|err| AotError::Io { path: output_dir.clone(), message: err.to_string() })?;
    let object_path = output_dir.join(format!("{name}.{}", target.object_ext));
    let request = AotBuildRequest {
        artifact,
        output_kind: BuildOutputKind::ObjectOnly,
        output_path: object_path.clone(),
        object_path: Some(object_path),
        target_triple: target_triple.clone(),
        profile: BuildProfile::Debug,
        entrypoint: String::new(),
        export_policy: ExportPolicy::Explicit(exported_symbols),
        link_mode: LinkMode::Auto,
        runtime: None,
        verbose_link: false,
        external_libraries: Vec::new(),
        library_search_paths: Vec::new(),
        pipeline: None,
    };
    validate_extern_libraries(&request.artifact, &request.external_libraries)?;
    let object = emit_object_stage(&request)?;
    let static_library = output_dir.join(crate::target::output_filename(name, BuildOutputKind::StaticLib, &target));
    let shared_library = output_dir.join(crate::target::output_filename(name, BuildOutputKind::SharedLib, &target));
    for (output_kind, output_path) in [(BuildOutputKind::StaticLib, &static_library), (BuildOutputKind::SharedLib, &shared_library)] {
        link(&LinkRequest {
            target_triple: target_triple.clone(), output_kind, output_path: output_path.clone(),
            object_path: object.object_path.clone(), runtime_staticlib: None, host_staticlib: None,
            entrypoint_symbol: String::new(), exported_symbols: object.exported_symbols.clone(),
            link_mode: LinkMode::Auto, verbose: false, external_libraries: Vec::new(), library_search_paths: Vec::new(),
        })?;
    }
    Ok(NativeLibraryPair { static_library, shared_library, provenance_symbols: object.exported_symbols })
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

/// Normalize CLI-style entrypoint: non-empty string or default [`DEFAULT_ENTRYPOINT`].
pub const DEFAULT_ENTRYPOINT: &str = "Main";

/// Beskid entrypoint name mapped to the native C link symbol for executable output.
pub fn native_link_entrypoint(beskid_entrypoint: &str) -> &str {
    if beskid_entrypoint == DEFAULT_ENTRYPOINT {
        "main"
    } else {
        beskid_entrypoint
    }
}

/// Normalize CLI-style entrypoint: non-empty string or default [`DEFAULT_ENTRYPOINT`].
pub fn resolve_entrypoint(entrypoint: Option<String>) -> AotResult<String> {
    if let Some(entrypoint) = entrypoint {
        if entrypoint.trim().is_empty() {
            return Err(AotError::InvalidRequest {
                message: "entrypoint must not be empty".to_owned(),
            });
        }
        return Ok(entrypoint);
    }

    Ok(DEFAULT_ENTRYPOINT.to_owned())
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
    let link_result = link_stage(&req, &object_stage, &runtime)?;

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
    let export_table = ExportTable::from_artifact(&req.artifact);
    let export_policy = export_table.resolve_export_policy(&req.export_policy);
    let exported_symbols = apply_export_policy(all_symbols, &export_policy);

    object_module.finalize_to_path(&object_path)?;

    Ok(ObjectStageResult {
        object_path,
        exported_symbols,
    })
}

fn ensure_entrypoint_exported(req: &AotBuildRequest, exported_symbols: &[String]) -> AotResult<()> {
    let native = native_link_entrypoint(&req.entrypoint);
    if exported_symbols
        .iter()
        .any(|sym| symbol_matches_entrypoint(sym, &req.entrypoint, native))
    {
        return Ok(());
    }

    Err(AotError::MissingEntrypoint {
        symbol: req.entrypoint.clone(),
    })
}

fn symbol_matches_entrypoint(symbol: &str, entrypoint: &str, native: &str) -> bool {
    symbol == entrypoint
        || symbol == native
        || symbol
            .strip_prefix(entrypoint)
            .is_some_and(|suffix| suffix.starts_with('#'))
}

fn prepare_runtime_stage(req: &AotBuildRequest) -> AotResult<crate::runtime::RuntimeArtifact> {
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_RUNTIME, || {
        prepare_runtime(&RuntimeBuildRequest {
            kit: req
                .runtime
                .clone()
                .expect("validated linked output runtime kit"),
        })
    })
}

fn link_stage(
    req: &AotBuildRequest,
    object_stage: &ObjectStageResult,
    runtime: &crate::runtime::RuntimeArtifact,
) -> AotResult<crate::linker::LinkResult> {
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_LINK, || {
        link(&LinkRequest {
            target_triple: req.target_triple.clone(),
            output_kind: req.output_kind,
            output_path: req.output_path.clone(),
            object_path: object_stage.object_path.clone(),
            runtime_staticlib: Some(runtime.staticlib_path.clone()),
            host_staticlib: None,
            entrypoint_symbol: native_link_entrypoint(&req.entrypoint).to_owned(),
            exported_symbols: object_stage.exported_symbols.clone(),
            link_mode: req.link_mode,
            verbose: req.verbose_link,
            external_libraries: req.external_libraries.clone(),
            library_search_paths: req.library_search_paths.clone(),
        })
    })
}

fn validate_request(req: &AotBuildRequest) -> AotResult<()> {
    validate_extern_libraries(&req.artifact, &req.external_libraries)?;

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
    if req.output_kind != BuildOutputKind::ObjectOnly && req.runtime.is_none() {
        return Err(AotError::InvalidRequest {
            message: "linked output requires an exact installed ABI-v5 runtime kit".to_owned(),
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

fn canonical_logical_name(logical: &str) -> String {
    let lower = logical.trim().to_ascii_lowercase();
    let stripped_prefix = lower.strip_prefix("lib").unwrap_or(&lower);
    let stripped_suffix = stripped_prefix
        .strip_suffix(".so")
        .or_else(|| stripped_prefix.strip_suffix(".dylib"))
        .or_else(|| stripped_prefix.strip_suffix(".a"))
        .unwrap_or(stripped_prefix);
    stripped_suffix.to_string()
}

fn validate_extern_libraries(
    artifact: &CodegenArtifact,
    external_libraries: &[String],
) -> AotResult<()> {
    if artifact.extern_imports.is_empty() {
        return Ok(());
    }
    let available: HashSet<String> = external_libraries
        .iter()
        .map(|name| canonical_logical_name(name))
        .collect();
    for import in &artifact.extern_imports {
        let Some(library) = import.library.as_deref() else {
            continue;
        };
        let canon = canonical_logical_name(library);
        if !available.contains(&canon) {
            return Err(AotError::UnresolvedExternLibrary {
                library: library.to_owned(),
                symbol: import.symbol.clone(),
            });
        }
    }
    Ok(())
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
            "Main",
        );
        assert_eq!(req.object_path, None);
        assert_eq!(req.target_triple, None);
        assert_eq!(req.profile, BuildProfile::Debug);
        assert_eq!(req.entrypoint, "Main");
        assert_eq!(req.export_policy, ExportPolicy::PublicOnly);
        assert_eq!(req.link_mode, LinkMode::Auto);
        assert!(req.runtime.is_none());
        assert!(!req.verbose_link);
        assert!(req.external_libraries.is_empty());
        assert!(req.library_search_paths.is_empty());
        assert!(req.pipeline.is_none());
    }

    #[test]
    fn object_only_defaults_do_not_require_a_runtime_kit() {
        let req = AotBuildRequest::with_defaults(
            CodegenArtifact::default(),
            BuildOutputKind::ObjectOnly,
            PathBuf::from("/tmp/out.o"),
            "Main",
        );

        assert!(req.runtime.is_none());
    }

    #[test]
    fn linked_artifacts_require_an_exact_runtime_kit() {
        let mut req = AotBuildRequest::with_defaults(
            CodegenArtifact::default(),
            BuildOutputKind::StaticLib,
            PathBuf::from("/tmp/out.a"),
            "Main",
        );
        req.runtime = None;

        let error = validate_request(&req).expect_err("linked output must require a runtime kit");
        assert!(matches!(error, AotError::InvalidRequest { .. }));
    }
}
