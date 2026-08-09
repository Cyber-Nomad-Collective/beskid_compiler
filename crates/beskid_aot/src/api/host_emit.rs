use std::path::PathBuf;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{canonical_runtime_sources, prove_canonical_runtime_corpus};
use beskid_codegen::CodegenArtifact;

use crate::error::{AotError, AotResult};
use crate::linker::{LinkRequest, link};
use crate::target::detect_target;

use super::model::{
    AotBuildRequest, BuildOutputKind, BuildProfile, CanonicalHostEmitAuthority, ExportPolicy, LinkMode,
    NativeLibraryPair,
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
        Some(target_triple),
        context_symbols,
        vec![context_object],
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
) -> AotResult<NativeLibraryPair> {
    let target = host_runtime_target()?;
    let artifact = crate::prepared_syntax::lower_canonical_runtime_prepared_syntax(target.clone())?;
    std::fs::create_dir_all(&output_dir)
        .map_err(|err| AotError::Io { path: output_dir.clone(), message: err.to_string() })?;
    let context_object = compile_context_assembly(&target, &output_dir, name)?;
    let platform_objects = compile_platform_objects(&target, &output_dir, name)?;
    let target_triple = target.triple.as_str().to_owned();
    let mut provenance_symbols = AbiManifestV5::canonical_runtime(target)
        .assembly_exports
        .into_iter()
        .map(|entry| entry.symbol.as_str().to_owned())
        .collect::<Vec<_>>();
    provenance_symbols.extend([
        "beskid_rt_v5_intrinsic_clock_monotonic_nanos".to_owned(),
        "beskid_rt_v5_intrinsic_clock_realtime_nanos".to_owned(),
        "beskid_rt_v5_intrinsic_process_exit".to_owned(),
        "beskid_rt_v5_intrinsic_process_getpid".to_owned(),
        "beskid_rt_v5_intrinsic_system_allocate".to_owned(),
        "beskid_rt_v5_intrinsic_system_free".to_owned(),
        "beskid_rt_v5_intrinsic_guarded_stack_allocate".to_owned(),
        "beskid_rt_v5_intrinsic_guarded_stack_free".to_owned(),
        "beskid_rt_v5_intrinsic_tls_get".to_owned(),
        "beskid_rt_v5_intrinsic_tls_set".to_owned(),
    ]);
    emit_library_pair_with_objects(
        artifact,
        output_dir,
        name,
        Some(target_triple),
        provenance_symbols,
        std::iter::once(context_object).chain(platform_objects).collect(),
    )
}

fn emit_library_pair_with_objects(
    artifact: CodegenArtifact,
    output_dir: PathBuf,
    name: &str,
    target_triple: Option<String>,
    exported_symbols: Vec<String>,
    additional_object_paths: Vec<PathBuf>,
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
        profile: BuildProfile::Debug,
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
    let mut provenance_symbols = object.exported_symbols;
    provenance_symbols.extend(exported_symbols);
    provenance_symbols.sort();
    provenance_symbols.dedup();
    Ok(NativeLibraryPair {
        static_library,
        shared_import_library: target.triple.contains("windows").then(|| output_dir.join(format!("{name}_import.lib"))),
        shared_library,
        provenance_symbols,
    })
}

fn host_runtime_target() -> AotResult<TargetMetadata> {
    beskid_abi::runtime_kit::host_runtime_target().map_err(|error| AotError::UnsupportedLinkerStrategy {
        target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        message: format!("canonical context assembly is only available for supported native ABI-v5 hosts ({error})"),
    })
}
