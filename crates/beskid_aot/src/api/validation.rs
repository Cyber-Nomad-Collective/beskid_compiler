use std::collections::HashSet;
#[cfg(test)]
use std::path::PathBuf;

use beskid_abi::generated::abi_v5_contract::{
    ABI_V5_CORE_ARGS_ENTRY_ADAPTERS, ABI_V5_CORELIB_SERVICE_BINDINGS, GeneratedCoreArgsEntryAdapter,
};
use beskid_codegen::CodegenArtifact;

use crate::error::{AotError, AotResult};

use super::model::{AotBuildRequest, BuildOutputKind, ExportPolicy, ProjectTargetKind};
#[cfg(test)]
use super::model::{BuildProfile, LinkMode};

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
    if beskid_entrypoint == DEFAULT_ENTRYPOINT { "main" } else { beskid_entrypoint }
}

/// Normalize CLI-style entrypoint: non-empty string or default [`DEFAULT_ENTRYPOINT`].
pub fn resolve_entrypoint(entrypoint: Option<String>) -> AotResult<String> {
    if let Some(entrypoint) = entrypoint {
        if entrypoint.trim().is_empty() {
            return Err(AotError::InvalidRequest { message: "entrypoint must not be empty".to_owned() });
        }
        return Ok(entrypoint);
    }

    Ok(DEFAULT_ENTRYPOINT.to_owned())
}

pub(super) fn validate_request(req: &AotBuildRequest) -> AotResult<()> {
    validate_extern_libraries(&req.artifact, &req.external_libraries)?;
    if artifact_uses_core_args(&req.artifact) && req.output_kind != BuildOutputKind::Exe {
        return Err(AotError::InvalidRequest { message: "Core.Args requires executable arguments".to_owned() });
    }

    if req.artifact.functions.is_empty() && requires_lowered_functions(req.output_kind) {
        return Err(AotError::InvalidRequest {
            message: "codegen artifact has no lowered functions for executable build".to_owned(),
        });
    }
    if requires_entrypoint(req.output_kind) && req.entrypoint.trim().is_empty() {
        return Err(AotError::InvalidRequest { message: "entrypoint must not be empty".to_owned() });
    }
    if req.output_kind != BuildOutputKind::ObjectOnly && req.runtime.is_none() {
        return Err(AotError::InvalidRequest {
            message: "linked output requires an exact installed ABI-v5 runtime kit".to_owned(),
        });
    }
    Ok(())
}

fn artifact_uses_core_args(artifact: &CodegenArtifact) -> bool {
    artifact.extern_imports.iter().any(|import| {
        ABI_V5_CORELIB_SERVICE_BINDINGS
            .iter()
            .any(|binding| binding.service.starts_with("__args_") && binding.adapter == import.symbol)
    })
}

pub(super) fn core_args_entry_adapter<'a>(
    artifact: &CodegenArtifact,
    target: &str,
) -> AotResult<Option<&'a GeneratedCoreArgsEntryAdapter>> {
    if !artifact_uses_core_args(artifact) {
        return Ok(None);
    }
    ABI_V5_CORE_ARGS_ENTRY_ADAPTERS.iter().find(|adapter| adapter.target == target).map(Some).ok_or_else(|| {
        AotError::InvalidRequest { message: format!("Core.Args has no generated entry adapter for target `{target}`") }
    })
}

fn requires_lowered_functions(output_kind: BuildOutputKind) -> bool {
    output_kind == BuildOutputKind::Exe
}

pub(super) fn requires_entrypoint(output_kind: BuildOutputKind) -> bool {
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

pub(super) fn validate_extern_libraries(artifact: &CodegenArtifact, external_libraries: &[String]) -> AotResult<()> {
    if artifact.extern_imports.is_empty() {
        return Ok(());
    }
    let available: HashSet<String> = external_libraries.iter().map(|name| canonical_logical_name(name)).collect();
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

pub(super) fn apply_export_policy(symbols: Vec<String>, policy: &ExportPolicy) -> Vec<String> {
    match policy {
        ExportPolicy::AllDefined => symbols,
        ExportPolicy::PublicOnly => symbols.into_iter().filter(|name| !name.starts_with("__")).collect(),
        ExportPolicy::Explicit(expected) => {
            symbols.into_iter().filter(|name| expected.iter().any(|wanted| wanted == name)).collect()
        }
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
        let req = AotBuildRequest {
            artifact: CodegenArtifact::default(),
            output_kind: BuildOutputKind::StaticLib,
            output_path: PathBuf::from("/tmp/out.a"),
            object_path: None,
            target_triple: None,
            profile: BuildProfile::Debug,
            entrypoint: "Main".to_owned(),
            export_policy: ExportPolicy::PublicOnly,
            link_mode: LinkMode::Auto,
            runtime: None,
            verbose_link: false,
            external_libraries: Vec::new(),
            library_search_paths: Vec::new(),
            pipeline: None,
        };

        let error = validate_request(&req).expect_err("linked output must require a runtime kit");
        assert!(matches!(error, AotError::InvalidRequest { .. }));
    }

    #[test]
    fn core_args_is_rejected_for_non_executable_outputs() {
        let mut artifact = CodegenArtifact::default();
        artifact.extern_imports.push(beskid_codegen::ExternImport {
            symbol: "beskid_rt_v5_args_count".into(),
            abi: Some("C".into()),
            library: None,
        });
        for output_kind in [BuildOutputKind::StaticLib, BuildOutputKind::SharedLib, BuildOutputKind::ObjectOnly] {
            let req = AotBuildRequest {
                artifact: artifact.clone(),
                output_kind,
                output_path: PathBuf::from("/tmp/out"),
                object_path: None,
                target_triple: None,
                profile: BuildProfile::Debug,
                entrypoint: "Main".into(),
                export_policy: ExportPolicy::PublicOnly,
                link_mode: LinkMode::Auto,
                runtime: None,
                verbose_link: false,
                external_libraries: Vec::new(),
                library_search_paths: Vec::new(),
                pipeline: None,
            };
            let error = validate_request(&req).expect_err("Core.Args must have an executable entry adapter");
            assert!(
                matches!(error, AotError::InvalidRequest { message } if message == "Core.Args requires executable arguments")
            );
        }
    }

    #[test]
    fn core_args_windows_link_entry_is_owned_by_the_manifest_adapter() {
        let mut artifact = CodegenArtifact::default();
        artifact.extern_imports.push(beskid_codegen::ExternImport {
            symbol: "beskid_rt_v5_args_count".into(),
            abi: Some("C".into()),
            library: None,
        });
        let adapter = core_args_entry_adapter(&artifact, "x86_64-pc-windows-msvc")
            .expect("generated adapter lookup")
            .expect("Core.Args adapter");
        assert_eq!(adapter.executable_entry, "wmain");
    }
}
