use std::path::PathBuf;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::BuildProfile as RuntimeKitProfile;
use beskid_codegen::CodegenArtifact;
use beskid_pipeline::SharedPipelineObserver;

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
            crate::bundled::default_runtime_strategy(profile, None)
                .unwrap_or_else(|err| panic!("with_defaults requires an exact installed ABI-v5 runtime kit: {err}"))
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
    pub(super) _private: (),
}
