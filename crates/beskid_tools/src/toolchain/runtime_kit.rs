//! Hermetic production of installed ABI-v5 runtime kits.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_kit::{BuildProfile, ResolvedRuntimeKit, RuntimeKitBuildRequest};
use beskid_abi::runtime_provenance::{parse_symbol_list, RuntimeProvenanceAudit};
use beskid_abi::runtime_source::{
    build_canonical_runtime_kit, canonical_runtime_source_hash, resolve_canonical_runtime_kit,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKitProfile {
    Debug,
    Release,
}

impl RuntimeKitProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

impl From<RuntimeKitProfile> for BuildProfile {
    fn from(value: RuntimeKitProfile) -> Self {
        match value {
            RuntimeKitProfile::Debug => Self::Debug,
            RuntimeKitProfile::Release => Self::Release,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeKitBuildOptions {
    pub prefix: PathBuf,
    pub target: String,
    pub profile: RuntimeKitProfile,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
}

/// Build and atomically publish the compiler-owned canonical runtime for this native host.
/// The caller supplies only its empty destination and profile; runtime source and native library
/// paths are constructed here, so a bridge or arbitrary archive cannot enter the ABI-v5 layout.
pub fn build_native_host(prefix: PathBuf, profile: RuntimeKitProfile) -> Result<ResolvedRuntimeKit> {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => target.triple.as_str() == "aarch64-apple-darwin",
            ("linux", "x86_64") => target.triple.as_str() == "x86_64-unknown-linux-gnu",
            ("windows", "x86_64") => target.triple.as_str() == "x86_64-pc-windows-msvc",
            _ => false,
        })
        .ok_or_else(|| anyhow!("unsupported ABI-v5 native host"))?;
    let staging = std::env::temp_dir().join(format!(
        "beskid-native-runtime-{}-{}",
        std::process::id(),
        profile.as_str()
    ));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|error| anyhow!("remove stale runtime staging {}: {error}", staging.display()))?;
    }
    let artifact = beskid_aot::lower_canonical_runtime_prepared_syntax(target.clone())
        .map_err(|error| anyhow!("lower canonical native runtime: {error}"))?;
    let pair = beskid_aot::emit_host_platform_library_pair(artifact, staging.clone(), "beskid_runtime")
        .map_err(|error| anyhow!("link canonical native runtime: {error}"))?;
    let result = build(RuntimeKitBuildOptions {
        prefix,
        target: target.triple.as_str().to_owned(),
        profile,
        static_library: pair.static_library,
        shared_library: pair.shared_library,
        shared_import_library: None,
    });
    let _ = std::fs::remove_dir_all(staging);
    result
}

/// The pair of native artifacts required for one optimization profile.
///
/// This intentionally carries only native files emitted by the runtime build.  The publisher
/// derives the source identity from the compiler-embedded canonical corpus; callers cannot use
/// this matrix API to label an arbitrary bridge or host runtime as ABI-v5.
#[derive(Debug, Clone)]
pub struct RuntimeKitProfileArtifacts {
    pub profile: RuntimeKitProfile,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
    /// Platform-adapter output for the exact runtime binary's defined/undefined symbols.
    /// It must satisfy the target's manifest-derived ABI-v5 audit policy before publication.
    pub provenance_symbol_list: PathBuf,
}

/// One target's complete debug/release publication request.
///
/// Both profiles are staged and published as one immutable target subtree under the same prefix.
/// The layout is target-neutral, so CI can validate Darwin and Windows publication paths on a
/// Linux host while only executing native JIT/link smokes on the matching host.
#[derive(Debug, Clone)]
pub struct RuntimeKitMatrixBuildOptions {
    pub prefix: PathBuf,
    pub target: String,
    pub profiles: Vec<RuntimeKitProfileArtifacts>,
}

pub fn build(options: RuntimeKitBuildOptions) -> Result<ResolvedRuntimeKit> {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == options.target)
        .ok_or_else(|| {
            let supported = TargetMetadata::supported()
                .into_iter()
                .map(|target| target.triple.as_str().to_owned())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow!(
                "unsupported ABI-v5 runtime target `{}`; expected one of: {supported}",
                options.target
            )
        })?;
    let canonical_hash = canonical_runtime_source_hash();
    let request = RuntimeKitBuildRequest {
        prefix: options.prefix,
        target,
        profile: options.profile.into(),
        runtime_source_hash: canonical_hash,
        static_library: options.static_library,
        shared_library: options.shared_library,
        shared_import_library: options.shared_import_library,
    };
    build_canonical_runtime_kit(&request)
        .map_err(|error| anyhow!("failed to build ABI-v5 runtime kit: {error:?}"))
}

/// Publish every requested profile for one ABI-v5 target.
///
/// A matrix must contain exactly one debug and one release artifact set.  Rejecting partial or
/// duplicate matrices keeps release automation from silently shipping a target with only the
/// profile exercised on the build host.
pub fn build_matrix(options: RuntimeKitMatrixBuildOptions) -> Result<Vec<ResolvedRuntimeKit>> {
    if options.profiles.len() != 2 {
        bail!("runtime-kit matrix must contain exactly debug and release profiles");
    }
    let mut saw_debug = false;
    let mut saw_release = false;
    // Validate every profile before publishing either one. In particular, a rejected release
    // provenance report must never leave a debug kit behind in an otherwise empty prefix.
    for artifacts in &options.profiles {
        let seen = match artifacts.profile {
            RuntimeKitProfile::Debug => &mut saw_debug,
            RuntimeKitProfile::Release => &mut saw_release,
        };
        if std::mem::replace(seen, true) {
            bail!(
                "runtime-kit matrix contains duplicate {} profile",
                artifacts.profile.as_str()
            );
        }
        verify_provenance_symbol_list(&options.target, &artifacts.provenance_symbol_list)?;
    }
    if !saw_debug || !saw_release {
        bail!("runtime-kit matrix must contain both debug and release profiles");
    }

    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == options.target)
        .ok_or_else(|| anyhow!("unsupported ABI-v5 runtime target `{}`", options.target))?;
    let final_target_root = options
        .prefix
        .join("lib/beskid-runtime/abi-5")
        .join(target.triple.as_str());
    if final_target_root.exists() {
        bail!(
            "runtime-kit matrix destination already exists: {}",
            final_target_root.display()
        );
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging_prefix = options.prefix.join(format!(
        ".beskid-runtime-kit-matrix-{}-{}-{nonce}",
        std::process::id(),
        target.triple.as_str()
    ));
    let profiles = options
        .profiles
        .iter()
        .map(|artifacts| artifacts.profile)
        .collect::<Vec<_>>();
    let publish = (|| -> Result<Vec<ResolvedRuntimeKit>> {
        for artifacts in options.profiles {
            build(RuntimeKitBuildOptions {
                prefix: staging_prefix.clone(),
                target: options.target.clone(),
                profile: artifacts.profile,
                static_library: artifacts.static_library,
                shared_library: artifacts.shared_library,
                shared_import_library: artifacts.shared_import_library,
            })?;
        }
        let staged_target_root = staging_prefix
            .join("lib/beskid-runtime/abi-5")
            .join(target.triple.as_str());
        let parent = final_target_root
            .parent()
            .expect("ABI-v5 runtime target root has a parent");
        std::fs::create_dir_all(parent).map_err(|error| {
            anyhow!(
                "create ABI-v5 runtime-kit destination parent `{}`: {error}",
                parent.display()
            )
        })?;
        std::fs::rename(&staged_target_root, &final_target_root).map_err(|error| {
            anyhow!(
                "atomically publish ABI-v5 runtime-kit matrix `{}`: {error}",
                final_target_root.display()
            )
        })?;
        profiles
            .into_iter()
            .map(|profile| {
                resolve_canonical_runtime_kit(&options.prefix, &target, profile.into()).map_err(
                    |error| anyhow!("resolve published ABI-v5 runtime-kit matrix: {error:?}"),
                )
            })
            .collect()
    })();
    let _ = std::fs::remove_dir_all(&staging_prefix);
    publish
}

fn verify_provenance_symbol_list(target: &str, path: &std::path::Path) -> Result<()> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        anyhow!(
            "read ABI-v5 runtime provenance symbol list `{}`: {error}",
            path.display()
        )
    })?;
    let symbols = parse_symbol_list(&source).map_err(|error| {
        anyhow!(
            "parse ABI-v5 runtime provenance symbol list `{}`: {error}",
            path.display()
        )
    })?;
    let target_metadata = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == target)
        .ok_or_else(|| anyhow!("unsupported ABI-v5 runtime target `{target}`"))?;
    RuntimeProvenanceAudit::canonical(target_metadata)
        .map_err(|error| anyhow!("derive ABI-v5 provenance audit for `{target}`: {error:?}"))?
        .verify(&symbols)
        .map_err(|error| {
            anyhow!(
                "ABI-v5 runtime provenance rejected `{}`: {error}",
                path.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn native_host_builder_publishes_the_canonical_runtime_to_an_empty_prefix() {
        let prefix = TempDir::new("native-host-runtime-prefix");
        let built = build_native_host(prefix.0.clone(), RuntimeKitProfile::Debug)
            .expect("publish native host runtime kit");
        assert!(built.static_library.is_file());
        assert!(built.shared_library.is_file());
        let output = std::process::Command::new("nm")
            .args(["-g", "--defined-only", "-j"])
            .arg(&built.static_library)
            .output()
            .expect("inspect staged static runtime archive");
        assert!(
            output.status.success(),
            "nm failed for staged static runtime archive"
        );
        let symbols = String::from_utf8(output.stdout).expect("utf-8 nm output");
        assert!(
            !symbols
                .lines()
                .map(|symbol| symbol.trim_start_matches('_'))
                .any(|symbol| symbol == "panic"),
            "staged static runtime archive leaked forbidden non-ABI panic symbol: {symbols}"
        );
    }

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "beskid-runtime-kit-matrix-{label}-{}-{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn options() -> RuntimeKitBuildOptions {
        RuntimeKitBuildOptions {
            prefix: "/tmp/beskid-runtime-kit-negative".into(),
            target: "not-a-supported-target".into(),
            profile: RuntimeKitProfile::Debug,
            static_library: "missing-static".into(),
            shared_library: "missing-shared".into(),
            shared_import_library: None,
        }
    }

    #[test]
    fn rejects_unsupported_target_before_reading_artifacts() {
        let error = build(options()).unwrap_err().to_string();
        assert!(error.contains("unsupported ABI-v5 runtime target"));
        assert!(error.contains("x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn build_derives_the_canonical_runtime_source_hash() {
        let prefix = TempDir::new("canonical-source-prefix");
        let inputs = TempDir::new("canonical-source-inputs");
        let artifacts = profile_artifacts(
            &inputs,
            RuntimeKitProfile::Debug,
            "x86_64-unknown-linux-gnu",
        );
        let built = build(RuntimeKitBuildOptions {
            prefix: prefix.0.clone(),
            target: "x86_64-unknown-linux-gnu".into(),
            profile: RuntimeKitProfile::Debug,
            static_library: artifacts.static_library,
            shared_library: artifacts.shared_library,
            shared_import_library: None,
        })
        .expect("publish canonical runtime kit");
        assert_eq!(built.metadata.source_hash, canonical_runtime_source_hash());
    }

    fn profile_artifacts(
        inputs: &TempDir,
        profile: RuntimeKitProfile,
        target: &str,
    ) -> RuntimeKitProfileArtifacts {
        let suffix = profile.as_str();
        let target = TargetMetadata::supported()
            .into_iter()
            .find(|candidate| candidate.triple.as_str() == target)
            .expect("supported matrix target");
        let static_extension = if target.object_format.as_str() == "coff" {
            "lib"
        } else {
            "a"
        };
        let shared_extension = match target.object_format.as_str() {
            "elf" => "so",
            "macho" => "dylib",
            "coff" => "dll",
            _ => unreachable!("supported matrix target object format"),
        };
        let static_library = inputs.0.join(format!("{suffix}.{static_extension}"));
        let shared_library = inputs.0.join(format!("{suffix}.{shared_extension}"));
        fs::write(&static_library, format!("static-{suffix}")).unwrap();
        fs::write(&shared_library, format!("shared-{suffix}")).unwrap();
        let shared_import_library = (target.object_format.as_str() == "coff").then(|| {
            let import_library = inputs.0.join(format!("{suffix}.import.lib"));
            fs::write(&import_library, format!("import-{suffix}")).unwrap();
            import_library
        });
        let provenance_symbol_list = inputs.0.join(format!("{suffix}.symbols"));
        let audit = RuntimeProvenanceAudit::canonical(target).expect("canonical audit");
        let fixture = audit.fixture_symbol_list().expect("fixture symbols");
        let mut symbols = format!("target={}\n", fixture.target);
        for symbol in fixture.defined {
            symbols.push_str(&format!("defined={symbol}\n"));
        }
        for symbol in fixture.undefined {
            symbols.push_str(&format!("undefined={symbol}\n"));
        }
        fs::write(&provenance_symbol_list, symbols).unwrap();
        RuntimeKitProfileArtifacts {
            profile,
            static_library,
            shared_library,
            shared_import_library,
            provenance_symbol_list,
        }
    }

    #[test]
    fn publishes_hermetic_linux_debug_and_release_matrix() {
        let prefix = TempDir::new("linux-prefix");
        let inputs = TempDir::new("linux-inputs");
        let built = build_matrix(RuntimeKitMatrixBuildOptions {
            prefix: prefix.0.clone(),
            target: "x86_64-unknown-linux-gnu".into(),
            profiles: vec![
                profile_artifacts(
                    &inputs,
                    RuntimeKitProfile::Debug,
                    "x86_64-unknown-linux-gnu",
                ),
                profile_artifacts(
                    &inputs,
                    RuntimeKitProfile::Release,
                    "x86_64-unknown-linux-gnu",
                ),
            ],
        })
        .expect("publish Linux runtime-kit matrix");
        assert_eq!(built.len(), 2);
        for profile in ["debug", "release"] {
            let root = prefix
                .0
                .join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu")
                .join(profile);
            assert!(root.join("abi.json").is_file());
            assert!(root.join("static/libbeskid_runtime.a").is_file());
            assert!(root.join("shared/libbeskid_runtime.so").is_file());
        }
    }

    #[test]
    fn validates_cross_target_matrix_layout_without_host_toolchains() {
        let prefix = TempDir::new("darwin-prefix");
        let inputs = TempDir::new("darwin-inputs");
        let built = build_matrix(RuntimeKitMatrixBuildOptions {
            prefix: prefix.0.clone(),
            target: "aarch64-apple-darwin".into(),
            profiles: vec![
                profile_artifacts(&inputs, RuntimeKitProfile::Debug, "aarch64-apple-darwin"),
                profile_artifacts(&inputs, RuntimeKitProfile::Release, "aarch64-apple-darwin"),
            ],
        })
        .expect("publish deterministic Darwin layout");
        assert!(built.iter().all(|kit| kit
            .shared_library
            .ends_with("shared/libbeskid_runtime.dylib")));
        assert!(built
            .iter()
            .all(|kit| kit.static_library.ends_with("static/libbeskid_runtime.a")));
    }

    #[test]
    fn validates_windows_x86_64_debug_and_release_matrix_layout_without_host_toolchains() {
        let prefix = TempDir::new("windows-prefix");
        let inputs = TempDir::new("windows-inputs");
        let target = "x86_64-pc-windows-msvc";
        let built = build_matrix(RuntimeKitMatrixBuildOptions {
            prefix: prefix.0.clone(),
            target: target.into(),
            profiles: vec![
                profile_artifacts(&inputs, RuntimeKitProfile::Debug, target),
                profile_artifacts(&inputs, RuntimeKitProfile::Release, target),
            ],
        })
        .expect("publish deterministic Windows runtime-kit matrix");

        assert_eq!(built.len(), 2);
        for profile in ["debug", "release"] {
            let root = prefix
                .0
                .join("lib/beskid-runtime/abi-5")
                .join(target)
                .join(profile);
            assert!(root.join("abi.json").is_file());
            assert!(root.join("static/beskid_runtime.lib").is_file());
            assert!(root.join("shared/beskid_runtime.dll").is_file());
        }
    }

    #[test]
    fn matrix_requires_both_profiles_exactly_once() {
        let prefix = TempDir::new("incomplete-prefix");
        let inputs = TempDir::new("incomplete-inputs");
        let error = build_matrix(RuntimeKitMatrixBuildOptions {
            prefix: prefix.0.clone(),
            target: "x86_64-unknown-linux-gnu".into(),
            profiles: vec![profile_artifacts(
                &inputs,
                RuntimeKitProfile::Debug,
                "x86_64-unknown-linux-gnu",
            )],
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("exactly debug and release"));

        let error = build_matrix(RuntimeKitMatrixBuildOptions {
            prefix: prefix.0.clone(),
            target: "x86_64-unknown-linux-gnu".into(),
            profiles: vec![
                profile_artifacts(
                    &inputs,
                    RuntimeKitProfile::Debug,
                    "x86_64-unknown-linux-gnu",
                ),
                profile_artifacts(
                    &inputs,
                    RuntimeKitProfile::Debug,
                    "x86_64-unknown-linux-gnu",
                ),
            ],
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("duplicate debug"));
    }

    #[test]
    fn rejected_provenance_publishes_no_profile_from_the_matrix() {
        let prefix = TempDir::new("provenance-prefix");
        let inputs = TempDir::new("provenance-inputs");
        let debug = profile_artifacts(
            &inputs,
            RuntimeKitProfile::Debug,
            "x86_64-unknown-linux-gnu",
        );
        let release = profile_artifacts(
            &inputs,
            RuntimeKitProfile::Release,
            "x86_64-unknown-linux-gnu",
        );
        fs::write(
            &release.provenance_symbol_list,
            "target=x86_64-unknown-linux-gnu\ndefined=beskid_runtime_bridge\n",
        )
        .unwrap();

        let error = build_matrix(RuntimeKitMatrixBuildOptions {
            prefix: prefix.0.clone(),
            target: "x86_64-unknown-linux-gnu".into(),
            profiles: vec![debug, release],
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("runtime provenance rejected"));
        assert!(!prefix
            .0
            .join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/debug")
            .exists());
    }

    #[test]
    fn failed_artifact_copy_publishes_no_profile_from_the_matrix() {
        let prefix = TempDir::new("artifact-prefix");
        let inputs = TempDir::new("artifact-inputs");
        let debug = profile_artifacts(
            &inputs,
            RuntimeKitProfile::Debug,
            "x86_64-unknown-linux-gnu",
        );
        let mut release = profile_artifacts(
            &inputs,
            RuntimeKitProfile::Release,
            "x86_64-unknown-linux-gnu",
        );
        release.shared_library = inputs.0.join("missing-release.so");

        assert!(build_matrix(RuntimeKitMatrixBuildOptions {
            prefix: prefix.0.clone(),
            target: "x86_64-unknown-linux-gnu".into(),
            profiles: vec![debug, release],
        })
        .is_err());
        assert!(!prefix
            .0
            .join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu")
            .exists());
    }
}
