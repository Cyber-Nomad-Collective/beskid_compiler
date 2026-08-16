use std::path::PathBuf;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_provenance::{RuntimeProvenanceAudit, SymbolList};
use beskid_abi::runtime_source::{canonical_runtime_sources, prove_canonical_runtime_corpus};
use beskid_codegen::CodegenArtifact;

use crate::error::{AotError, AotResult};
use crate::linker::{LinkRequest, link};
use crate::object_symbols::extract_symbol_inventory;
use crate::target::detect_target;

use super::model::{
    AotBuildRequest, BuildOutputKind, BuildProfile, CanonicalHostEmitAuthority, ExportPolicy, LinkMode,
    NativeLibraryPair, NativeSymbolInventory,
};
use super::object_stage::emit_object_stage;
use super::platform_objects::{compile_context_assembly, compile_platform_objects};
use super::validation::validate_extern_libraries;

/// Mint host-emit authority from the exact compiler-embedded ABI-v5 runtime corpus.
///
/// Fail closed when the embedded corpus cannot prove canonical runtime identity for the
/// current host target. There is no prebuilt, standalone, or ambient fallback mint path.
pub fn require_canonical_host_emit_authority() -> AotResult<CanonicalHostEmitAuthority> {
    let target = host_runtime_target()?;
    let manifest = AbiManifestV5::canonical_runtime(target);
    prove_canonical_runtime_corpus(&canonical_runtime_sources(), &manifest).map_err(|error| {
        AotError::InvalidRequest {
            message: format!("canonical host emit authority requires the embedded ABI-v5 runtime corpus: {error:?}"),
        }
    })?;
    Ok(CanonicalHostEmitAuthority { _private: () })
}
/// Emit a native library pair that includes the current host target's canonical context
/// assembly object. The assembly source and generated ABI include are the same ones verified by
/// `beskid_abi`'s target assembly tests; no inline assembly or synthetic context shim is used.
///
/// Callers must present [`CanonicalHostEmitAuthority`]; arbitrary codegen artifacts cannot enter
/// this publication path.
pub fn emit_host_context_library_pair(
    _authority: &CanonicalHostEmitAuthority,
    output_dir: PathBuf,
    name: &str,
    profile: BuildProfile,
) -> AotResult<NativeLibraryPair> {
    let target = host_runtime_target()?;
    std::fs::create_dir_all(&output_dir)
        .map_err(|err| AotError::Io { path: output_dir.clone(), message: err.to_string() })?;
    let context_object = compile_context_assembly(&target, &output_dir, name)?;
    let target_triple = target.triple.as_str().to_owned();
    let context_symbols = AbiManifestV5::canonical_runtime(target)
        .assembly_exports
        .into_iter()
        .map(|entry| entry.symbol.as_str().to_owned())
        .collect();
    emit_library_pair_with_objects(
        CodegenArtifact::default(),
        output_dir,
        name,
        profile,
        Some(target_triple),
        context_symbols,
        vec![context_object],
        ProvenancePolicy::Exact,
    )
}

/// Emit native library artifacts that include the current host target's context-switch assembly,
/// its minimal platform boundary, and the compiler-embedded canonical runtime object code.
///
/// The platform object deliberately owns only raw native allocation/free and the ABI-v5 trap
/// path; portable memory operations remain compiler intrinsics. Callers cannot supply an
/// alternate [`CodegenArtifact`] — Bootstrap is always lowered through the exact CodegenInput path.
pub fn emit_host_platform_library_pair(
    _authority: &CanonicalHostEmitAuthority,
    output_dir: PathBuf,
    name: &str,
    profile: BuildProfile,
) -> AotResult<NativeLibraryPair> {
    let target = host_runtime_target()?;
    let artifact = crate::prepared_syntax::lower_canonical_runtime_prepared_syntax(target.clone())?;
    std::fs::create_dir_all(&output_dir)
        .map_err(|err| AotError::Io { path: output_dir.clone(), message: err.to_string() })?;
    let context_object = compile_context_assembly(&target, &output_dir, name)?;
    let platform_objects = compile_platform_objects(&target, &output_dir, name)?;
    let target_triple = target.triple.as_str().to_owned();
    let mut required_symbols = AbiManifestV5::canonical_runtime(target.clone())
        .assembly_exports
        .into_iter()
        .map(|entry| entry.symbol.as_str().to_owned())
        .collect::<Vec<_>>();
    required_symbols.extend([
        "beskid_rt_v5_intrinsic_clock_monotonic_nanos".to_owned(),
        "beskid_rt_v5_intrinsic_clock_realtime_nanos".to_owned(),
        "beskid_rt_v5_intrinsic_process_exit".to_owned(),
        "beskid_rt_v5_intrinsic_process_getpid".to_owned(),
        "beskid_rt_v5_intrinsic_system_allocate".to_owned(),
        "beskid_rt_v5_intrinsic_system_free".to_owned(),
        "beskid_rt_v5_intrinsic_guarded_stack_allocate".to_owned(),
        "beskid_rt_v5_intrinsic_guarded_stack_grow".to_owned(),
        "beskid_rt_v5_intrinsic_guarded_stack_free".to_owned(),
        "beskid_rt_v5_intrinsic_tls_get".to_owned(),
        "beskid_rt_v5_intrinsic_tls_set".to_owned(),
    ]);
    emit_library_pair_with_objects(
        artifact,
        output_dir,
        name,
        profile,
        Some(target_triple),
        required_symbols,
        std::iter::once(context_object).chain(platform_objects).collect(),
        ProvenancePolicy::CanonicalRuntime(target),
    )
}

// 8 params: artifact + output metadata + build knobs; grouping would obscure the
// 1:1 mapping with the AotBuildRequest fields consumed below.
#[allow(clippy::too_many_arguments)]
fn emit_library_pair_with_objects(
    artifact: CodegenArtifact,
    output_dir: PathBuf,
    name: &str,
    profile: BuildProfile,
    target_triple: Option<String>,
    exported_symbols: Vec<String>,
    additional_object_paths: Vec<PathBuf>,
    provenance_policy: ProvenancePolicy,
) -> AotResult<NativeLibraryPair> {
    let target = detect_target(target_triple.as_deref())?;
    std::fs::create_dir_all(&output_dir)
        .map_err(|err| AotError::Io { path: output_dir.clone(), message: err.to_string() })?;
    let object_path = output_dir.join(format!("{name}.{}", target.object_ext));
    let request = AotBuildRequest {
        artifact,
        output_kind: BuildOutputKind::ObjectOnly,
        output_path: object_path.clone(),
        object_path: Some(object_path),
        target_triple: target_triple.clone(),
        profile,
        entrypoint: String::new(),
        export_policy: ExportPolicy::Explicit(exported_symbols.clone()),
        link_mode: LinkMode::Auto,
        runtime: None,
        verbose_link: false,
        external_libraries: Vec::new(),
        library_search_paths: Vec::new(),
        pipeline: None,
    };
    validate_extern_libraries(&request.artifact, &request.external_libraries)?;
    let object = emit_object_stage(&request)?;
    let mut linked_exports = object.exported_symbols.clone();
    linked_exports.extend(exported_symbols.iter().cloned());
    linked_exports.sort();
    linked_exports.dedup();
    let static_library = output_dir.join(crate::target::output_filename(name, BuildOutputKind::StaticLib, &target));
    let shared_library = output_dir.join(crate::target::output_filename(name, BuildOutputKind::SharedLib, &target));
    for (output_kind, output_path) in
        [(BuildOutputKind::StaticLib, &static_library), (BuildOutputKind::SharedLib, &shared_library)]
    {
        link(&LinkRequest {
            target_triple: target_triple.clone(),
            output_kind,
            output_path: output_path.clone(),
            object_path: object.object_path.clone(),
            runtime_staticlib: None,
            host_staticlib: None,
            additional_object_paths: additional_object_paths.clone(),
            entrypoint_symbol: String::new(),
            exported_symbols: linked_exports.clone(),
            link_mode: LinkMode::Auto,
            verbose: false,
            external_libraries: Vec::new(),
            library_search_paths: Vec::new(),
        })?;
    }
    let symbol_prefix = provenance_policy.symbol_prefix();
    let canonical_object_inventory = extract_symbol_inventory(&object.object_path, symbol_prefix)?;
    let additional_object_inventories = additional_object_paths
        .iter()
        .map(|path| extract_symbol_inventory(path, symbol_prefix))
        .collect::<AotResult<Vec<_>>>()?;
    let static_archive_inventory = extract_symbol_inventory(&static_library, symbol_prefix)?;
    let shared_image_inventory = extract_symbol_inventory(&shared_library, symbol_prefix)?;
    provenance_policy.verify(&exported_symbols, &static_archive_inventory, false)?;
    provenance_policy.verify(&exported_symbols, &shared_image_inventory, true)?;
    Ok(NativeLibraryPair {
        static_library,
        shared_import_library: target.triple.contains("windows").then(|| output_dir.join(format!("{name}_import.lib"))),
        shared_library,
        canonical_object_inventory,
        additional_object_inventories,
        static_archive_inventory,
        shared_image_inventory,
    })
}

enum ProvenancePolicy {
    Exact,
    CanonicalRuntime(TargetMetadata),
}

impl ProvenancePolicy {
    fn symbol_prefix(&self) -> &str {
        match self {
            Self::Exact => {
                if cfg!(target_os = "macos") {
                    "_"
                } else {
                    ""
                }
            }
            Self::CanonicalRuntime(target) => &target.symbol_prefix,
        }
    }

    fn verify(&self, required_symbols: &[String], inventory: &NativeSymbolInventory, shared: bool) -> AotResult<()> {
        match self {
            Self::Exact => {
                let mut required = required_symbols.to_vec();
                required.sort();
                required.dedup();
                if inventory.defined != required || !inventory.imported.is_empty() {
                    return Err(AotError::ObjectModule {
                        message: format!(
                            "runtime provenance mismatch for {}: required definitions {required:?}, actual definitions {:?}, actual imports {:?}",
                            inventory.artifact.display(),
                            inventory.defined,
                            inventory.imported
                        ),
                    });
                }
                Ok(())
            }
            Self::CanonicalRuntime(target) => {
                let audit =
                    RuntimeProvenanceAudit::canonical(target.clone()).map_err(|error| AotError::ObjectModule {
                        message: format!("cannot construct canonical runtime provenance policy: {error}"),
                    })?;
                let symbols = SymbolList {
                    target: target.triple.as_str().to_owned(),
                    defined: inventory.defined.clone(),
                    undefined: inventory.imported.clone(),
                };
                let result = if shared { audit.verify_shared(&symbols) } else { audit.verify_static_archive(&symbols) };
                result.map_err(|error| AotError::ObjectModule {
                    message: format!("runtime provenance mismatch for {}: {error}", inventory.artifact.display()),
                })
            }
        }
    }
}

fn host_runtime_target() -> AotResult<TargetMetadata> {
    beskid_abi::runtime_kit::host_runtime_target().map_err(|error| AotError::UnsupportedLinkerStrategy {
        target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        message: format!("canonical context assembly is only available for supported native ABI-v5 hosts ({error})"),
    })
}
