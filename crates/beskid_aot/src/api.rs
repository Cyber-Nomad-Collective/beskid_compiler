//! Public AOT API: build requests, output kinds, and the [`build`] orchestration entry point.

use std::path::PathBuf;
use std::process::Command;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, render_runtime_asm_include};
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_abi::runtime_source::{canonical_runtime_sources, prove_canonical_runtime_corpus};
use beskid_codegen::CodegenArtifact;
use beskid_pipeline::{
    SharedPipelineObserver, observe_phase_result,
    phases::{AOT_EMIT_OBJECT, AOT_LINK, AOT_RUNTIME},
};
use cargo_cross::config::{Arch, Os, get_target_config};
use cargo_cross::env::sanitize_cargo_env;

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
    /// COFF import library emitted beside a Windows shared runtime DLL.
    pub shared_import_library: Option<PathBuf>,
    pub provenance_symbols: Vec<String>,
}

/// Opaque authority to publish native host runtime library pairs.
///
/// Deliberately has no public constructor and does not accept a caller-supplied
/// [`CodegenArtifact`]. Minting requires the compiler-embedded canonical runtime corpus.
#[derive(Debug)]
pub struct CanonicalHostEmitAuthority {
    _private: (),
}

/// Mint host-emit authority from the exact compiler-embedded ABI-v5 runtime corpus.
///
/// Fail closed when the embedded corpus cannot prove canonical runtime identity for the
/// current host target. There is no prebuilt, standalone, or ambient fallback mint path.
pub fn require_canonical_host_emit_authority() -> AotResult<CanonicalHostEmitAuthority> {
    let target = host_runtime_target()?;
    let manifest = AbiManifestV5::canonical_runtime(target);
    prove_canonical_runtime_corpus(&canonical_runtime_sources(), &manifest).map_err(|error| {
        AotError::InvalidRequest {
            message: format!(
                "canonical host emit authority requires the embedded ABI-v5 runtime corpus: {error:?}"
            ),
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
    std::fs::create_dir_all(&output_dir).map_err(|err| AotError::Io {
        path: output_dir.clone(),
        message: err.to_string(),
    })?;
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
    std::fs::create_dir_all(&output_dir).map_err(|err| AotError::Io {
        path: output_dir.clone(),
        message: err.to_string(),
    })?;
    let context_object = compile_context_assembly(&target, &output_dir, name)?;
    let platform_objects = compile_platform_objects(&target, &output_dir, name)?;
    let target_triple = target.triple.as_str().to_owned();
    let mut provenance_symbols = AbiManifestV5::canonical_runtime(target)
        .assembly_exports
        .into_iter()
        .map(|entry| entry.symbol.as_str().to_owned())
        .collect::<Vec<_>>();
    provenance_symbols.extend([
        "beskid_rt_v5_intrinsic_system_allocate".to_owned(),
        "beskid_rt_v5_intrinsic_system_free".to_owned(),
        "beskid_rt_v5_intrinsic_tls_get".to_owned(),
        "beskid_rt_v5_intrinsic_tls_set".to_owned(),
    ]);
    emit_library_pair_with_objects(
        artifact,
        output_dir,
        name,
        Some(target_triple),
        provenance_symbols,
        std::iter::once(context_object)
            .chain(platform_objects)
            .collect(),
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
    std::fs::create_dir_all(&output_dir).map_err(|err| AotError::Io {
        path: output_dir.clone(),
        message: err.to_string(),
    })?;
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
    let static_library = output_dir.join(crate::target::output_filename(
        name,
        BuildOutputKind::StaticLib,
        &target,
    ));
    let shared_library = output_dir.join(crate::target::output_filename(
        name,
        BuildOutputKind::SharedLib,
        &target,
    ));
    for (output_kind, output_path) in [
        (BuildOutputKind::StaticLib, &static_library),
        (BuildOutputKind::SharedLib, &shared_library),
    ] {
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
        shared_import_library: target
            .triple
            .contains("windows")
            .then(|| output_dir.join(format!("{name}_import.lib"))),
        shared_library,
        provenance_symbols,
    })
}

fn host_runtime_target() -> AotResult<TargetMetadata> {
    beskid_abi::runtime_kit::host_runtime_target().map_err(|error| {
        AotError::UnsupportedLinkerStrategy {
            target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            message: format!(
                "canonical context assembly is only available for supported native ABI-v5 hosts ({error})"
            ),
        }
    })
}

fn compile_context_assembly(
    target: &TargetMetadata,
    output_dir: &std::path::Path,
    name: &str,
) -> AotResult<PathBuf> {
    let assembly_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../beskid_abi/assembly")
        .join(target.triple.as_str());
    let source = assembly_root.join(if target.triple.as_str().contains("windows") {
        "context.asm"
    } else {
        "context.S"
    });
    let include = output_dir.join(format!(
        "beskid_runtime_abi_v5_{}.inc",
        target.triple.as_str().replace('-', "_")
    ));
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let rendered =
        render_runtime_asm_include(&manifest).map_err(|err| AotError::InvalidRequest {
            message: format!("{err:?}"),
        })?;
    std::fs::write(&include, rendered).map_err(|err| AotError::Io {
        path: include.clone(),
        message: err.to_string(),
    })?;
    let object = output_dir.join(format!(
        "{name}.context.{}",
        if target.triple.as_str().contains("windows") {
            "obj"
        } else {
            "o"
        }
    ));

    let mut command = if target.triple.as_str().contains("windows") {
        Command::new("llvm-ml")
    } else {
        Command::new("clang")
    };
    if target.triple.as_str() == "x86_64-unknown-linux-gnu" {
        command.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
        command
            .arg(&source)
            .arg("-I")
            .arg(output_dir)
            .arg("-o")
            .arg(&object);
    } else if target.triple.as_str() == "aarch64-apple-darwin" {
        command.args(["-c", "-arch", "arm64"]);
        command
            .arg(&source)
            .arg("-I")
            .arg(output_dir)
            .arg("-o")
            .arg(&object);
    } else if target.triple.as_str() == "x86_64-pc-windows-msvc" {
        command.args(["--m64", "/c", "/X", "/Fo"]);
        command.arg(&object).arg("/I").arg(output_dir).arg(&source);
    } else {
        return Err(AotError::UnsupportedLinkerStrategy {
            target: target.triple.as_str().to_owned(),
            message: "no canonical context assembly invocation for target".to_owned(),
        });
    }
    let output = command.output().map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!("{:?}", command),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(object)
}

fn compile_platform_objects(
    target: &TargetMetadata,
    output_dir: &std::path::Path,
    name: &str,
) -> AotResult<Vec<PathBuf>> {
    let plan = platform_object_plan(target.triple.as_str())?;
    let assembly_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../beskid_abi/assembly")
        .join(target.triple.as_str());
    let source = assembly_root.join(plan.assembly_source);
    let tls_source = assembly_root.join(plan.tls_source);
    let object = output_dir.join(format!("{name}.platform.{}", plan.object_extension));
    let tls_object = output_dir.join(format!("{name}.platform_tls.{}", plan.object_extension));
    let mut assembly = Command::new(plan.assembly_program);
    assembly.args(&plan.assembly_args);
    if plan.assembly_output_before_source {
        assembly.arg(&object).arg(&source);
    } else {
        assembly.arg(&source).arg("-o").arg(&object);
    }
    let output = assembly.output().map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!(
                "{} {:?} {} -o {}",
                plan.assembly_program,
                plan.assembly_args,
                source.display(),
                object.display()
            ),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let output = Command::new(plan.tls_program)
        .args(&plan.tls_args)
        .arg(&tls_source)
        .arg("-o")
        .arg(&tls_object)
        .output()
        .map_err(|_| AotError::LinkerUnavailable)?;
    if !output.status.success() {
        return Err(AotError::LinkFailed {
            status: output.status.code().unwrap_or(-1),
            command: format!(
                "{} {:?} {} -o {}",
                plan.tls_program,
                plan.tls_args,
                tls_source.display(),
                tls_object.display()
            ),
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(vec![object, tls_object])
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlatformObjectPlan {
    assembly_source: &'static str,
    tls_source: &'static str,
    assembly_program: &'static str,
    assembly_args: Vec<String>,
    assembly_output_before_source: bool,
    tls_program: &'static str,
    tls_args: Vec<String>,
    object_extension: &'static str,
}

fn platform_object_plan(target: &str) -> AotResult<PlatformObjectPlan> {
    // Try cargo_cross config first; fall back to string-based matching for targets
    // not in cargo_cross's database (e.g. msvc variants).
    if let Some(config) = get_target_config(target) {
        return match (&config.arch, &config.os) {
            (Arch::Aarch64, Os::Darwin) => Ok(PlatformObjectPlan {
                assembly_source: "platform.S",
                tls_source: "platform_tls.c",
                assembly_program: "clang",
                assembly_args: vec!["-c".into(), "-arch".into(), "arm64".into()],
                assembly_output_before_source: false,
                tls_program: "clang",
                tls_args: vec![
                    "-std=c11".into(),
                    "-c".into(),
                    "-arch".into(),
                    "arm64".into(),
                ],
                object_extension: "o",
            }),
            (Arch::X86_64, Os::Linux) => Ok(PlatformObjectPlan {
                assembly_source: "platform.S",
                tls_source: "platform_tls.c",
                assembly_program: "clang",
                assembly_args: vec![
                    "-target".into(),
                    target.to_owned(),
                    "-fPIC".into(),
                    "-c".into(),
                ],
                assembly_output_before_source: false,
                tls_program: "clang",
                tls_args: vec![
                    "-target".into(),
                    target.to_owned(),
                    "-std=c11".into(),
                    "-fPIC".into(),
                    "-c".into(),
                ],
                object_extension: "o",
            }),
            (Arch::X86_64, Os::Windows) => Ok(PlatformObjectPlan {
                assembly_source: "platform.asm",
                tls_source: "platform_tls.c",
                assembly_program: "llvm-ml",
                assembly_args: vec!["--m64".into(), "/c".into(), "/X".into(), "/Fo".into()],
                assembly_output_before_source: true,
                tls_program: "clang",
                tls_args: vec![format!("--target={target}"), "-std=c11".into(), "-c".into()],
                object_extension: "obj",
            }),
            _ => Err(AotError::UnsupportedLinkerStrategy {
                target: target.to_owned(),
                message: format!(
                    "native platform shim is not implemented for {}-{}",
                    config.arch.as_str(),
                    config.os.as_str()
                ),
            }),
        };
    }

    // Fallback: string-based target matching for targets not in cargo_cross config DB
    match target {
        "x86_64-pc-windows-msvc" => Ok(PlatformObjectPlan {
            assembly_source: "platform.asm",
            tls_source: "platform_tls.c",
            assembly_program: "llvm-ml",
            assembly_args: vec!["--m64".into(), "/c".into(), "/X".into(), "/Fo".into()],
            assembly_output_before_source: true,
            tls_program: "clang",
            tls_args: vec![
                "--target=x86_64-pc-windows-msvc".into(),
                "-std=c11".into(),
                "-c".into(),
            ],
            object_extension: "obj",
        }),
        _ => Err(AotError::UnsupportedLinkerStrategy {
            target: target.to_owned(),
            message: "native platform shim is not implemented for this host target".to_owned(),
        }),
    }
}

#[cfg(test)]
mod platform_object_tests {
    use super::platform_object_plan;

    #[test]
    fn windows_platform_plan_uses_coff_sources_and_windows_toolchain_arguments() {
        let plan = platform_object_plan("x86_64-pc-windows-msvc").expect("Windows plan");

        assert_eq!(plan.assembly_source, "platform.asm");
        assert_eq!(plan.tls_source, "platform_tls.c");
        assert_eq!(plan.assembly_program, "llvm-ml");
        assert_eq!(plan.assembly_args, vec!["--m64", "/c", "/X", "/Fo"]);
        assert_eq!(plan.tls_program, "clang");
        assert_eq!(
            plan.tls_args,
            vec!["--target=x86_64-pc-windows-msvc", "-std=c11", "-c"]
        );
        assert_eq!(plan.object_extension, "obj");
    }
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
    // Sanitize the cargo environment before building to avoid leaking
    // host toolchain variables into cross-compilation invocations.
    sanitize_cargo_env();
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

    let exports = req.artifact.exports.clone();
    let all_symbols = req
        .artifact
        .functions
        .iter()
        .map(|function| {
            beskid_codegen::lowering::expressions::export::object_link_symbol(
                &function.name,
                &exports,
            )
        })
        .collect::<Vec<_>>();
    let export_table = ExportTable::from_artifact(&req.artifact);
    let export_policy = export_table.resolve_export_policy(&req.export_policy);
    let exported_symbols = apply_export_policy(all_symbols, &export_policy);
    let exported_symbol_set = exported_symbols.iter().cloned().collect::<HashSet<_>>();

    let mut object_module = BeskidObjectModule::new(req.target_triple.as_deref())?;
    let obs = req.pipeline.as_deref();
    observe_phase_result(obs, AOT_EMIT_OBJECT, || {
        object_module.compile_artifact_with_exports(&req.artifact, &exported_symbol_set, obs)
    })?;

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
            additional_object_paths: Vec::new(),
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
