use super::{
    RuntimeKitBuildOptions, RuntimeKitMatrixBuildOptions, RuntimeKitProfile, RuntimeKitProfileArtifacts, build,
    build_matrix, build_native_host,
};
use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_provenance::{RuntimeProvenanceAudit, parse_symbol_list};
use beskid_abi::runtime_source::canonical_runtime_source_hash;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn native_host_builder_maps_runtime_kit_profiles_to_aot_profiles() {
    assert_eq!(super::native_host::aot_profile(RuntimeKitProfile::Debug), beskid_aot::BuildProfile::Debug);
    assert_eq!(super::native_host::aot_profile(RuntimeKitProfile::Release), beskid_aot::BuildProfile::Release);
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native_host_builder_publishes_the_canonical_runtime_to_an_empty_prefix() {
    let prefix = TempDir::new("native-host-runtime-prefix");
    let built = build_native_host(prefix.0.clone(), RuntimeKitProfile::Debug).expect("publish native host runtime kit");
    assert!(built.static_library.is_file());
    assert!(built.shared_library.is_file());
    assert!(built.shared_import_library.is_none(), "Mach-O native kits must not publish a COFF import library");
    let output = std::process::Command::new("nm")
        .args(["-g", "--defined-only", "-j"])
        .arg(&built.static_library)
        .output()
        .expect("inspect staged static runtime archive");
    assert!(output.status.success(), "nm failed for staged static runtime archive");
    let symbols = String::from_utf8(output.stdout).expect("utf-8 nm output");
    assert!(
        !symbols.lines().map(|symbol| symbol.trim_start_matches('_')).any(|symbol| symbol == "panic"),
        "staged static runtime archive leaked forbidden non-ABI panic symbol: {symbols}"
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn tool_output(program: &str, args: &[&str], path: &std::path::Path) -> String {
    let output = Command::new(program).args(args).arg(path).output().unwrap_or_else(|error| panic!("run {program}: {error}"));
    // codesign reports on stderr, otool on stdout.
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

/// File offset of `__TEXT,__text`, read from the Mach-O load commands.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn text_section_offset(path: &std::path::Path) -> usize {
    let listing = tool_output("otool", &["-l"], path);
    let mut lines = listing.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line == "sectname __text" {
            for following in lines.by_ref() {
                if let Some(offset) = following.strip_prefix("offset ") {
                    return offset.trim().parse().expect("numeric __text offset");
                }
            }
        }
    }
    panic!("no __text section in {}", path.display());
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native_host_shared_runtime_is_relocatable_and_raw_byte_bound() {
    use beskid_abi::runtime_kit::{RuntimeKitResolutionError, host_runtime_target, resolve_installed_runtime_kit};

    for profile in [RuntimeKitProfile::Debug, RuntimeKitProfile::Release] {
        let prefix = TempDir::new(&format!("relocatable-{}", profile.as_str()));
        let built = build_native_host(prefix.0.clone(), profile).expect("publish native host runtime kit");
        let dylib = built.shared_library.clone();
        let name = dylib.file_name().and_then(|name| name.to_str()).expect("kit dylib name").to_owned();
        let target = host_runtime_target().expect("native ABI-v5 target");
        let resolve = || resolve_installed_runtime_kit(&prefix.0, &target, profile.into());
        let label = profile.as_str();

        // Relocatable identity: `@rpath/<basename>`, no run path, system dependencies only.
        let id_output = tool_output("otool", &["-D"], &dylib);
        let id = id_output.lines().map(str::trim).filter(|line| !line.is_empty()).last().expect("otool -D ID").to_owned();
        assert_eq!(id, format!("@rpath/{name}"), "{label} kit dylib ID");
        let load_commands = tool_output("otool", &["-l"], &dylib);
        assert!(!load_commands.contains("LC_RPATH"), "{label} kit dylib must not carry LC_RPATH");
        let mut dependencies: Vec<String> = tool_output("otool", &["-L"], &dylib)
            .lines()
            .skip(1)
            .filter_map(|line| line.trim().split(" (").next().map(str::to_owned))
            .filter(|line| !line.is_empty())
            .collect();
        let own_id = dependencies.iter().position(|dependency| *dependency == id);
        if let Some(index) = own_id {
            dependencies.remove(index);
        }
        assert!(!dependencies.is_empty(), "{label} kit dylib lists no dependencies");
        for dependency in &dependencies {
            assert!(
                dependency.starts_with("/usr/lib/") || dependency.starts_with("/System/Library/"),
                "{label} kit dylib has non-system dependency {dependency}"
            );
        }

        // No staging or temp-directory bytes survive in the published artifact.
        let original = fs::read(&dylib).expect("read published kit dylib");
        let temp = std::env::temp_dir();
        let mut forbidden: Vec<Vec<u8>> = vec![b"beskid-native-runtime-".to_vec(), temp.to_string_lossy().as_bytes().to_vec()];
        if let Ok(canonical) = temp.canonicalize() {
            forbidden.push(canonical.to_string_lossy().as_bytes().to_vec());
        }
        for needle in forbidden.iter().filter(|needle| needle.len() > 1) {
            assert!(
                !original.windows(needle.len()).any(|window| window == needle.as_slice()),
                "{label} kit dylib embeds {}",
                String::from_utf8_lossy(needle)
            );
        }

        let signature = tool_output("codesign", &["-dv"], &dylib);
        assert!(signature.contains("linker-signed"), "{label} kit dylib is not linker-signed: {signature}");
        resolve().expect("relocatable kit must resolve");

        // One flipped byte in __text is a hash mismatch for exactly this path; restoring it recovers.
        let offset = text_section_offset(&dylib) + 16;
        let mut tampered = original.clone();
        tampered[offset] ^= 0xFF;
        fs::write(&dylib, &tampered).expect("write tampered kit dylib");
        match resolve() {
            Err(RuntimeKitResolutionError::ArtifactHashMismatch { path, .. }) => assert_eq!(path, dylib),
            other => panic!("{label} tampered __text must fail with ArtifactHashMismatch, got {other:?}"),
        }
        fs::write(&dylib, &original).expect("restore kit dylib");
        resolve().expect("restored kit must resolve");

        // A Homebrew-style relocation (new ID plus ad-hoc re-sign) changes raw bytes and is rejected.
        let relocated = format!("/opt/homebrew/opt/beskid/libexec/lib/{name}");
        let status = Command::new("install_name_tool").args(["-id", &relocated]).arg(&dylib).status().expect("install_name_tool");
        assert!(status.success(), "install_name_tool failed");
        let status = Command::new("codesign").args(["--force", "-s", "-"]).arg(&dylib).status().expect("codesign");
        assert!(status.success(), "codesign failed");
        match resolve() {
            Err(RuntimeKitResolutionError::ArtifactHashMismatch { path, .. }) => assert_eq!(path, dylib),
            other => panic!("{label} relocated kit must fail with ArtifactHashMismatch, got {other:?}"),
        }
    }
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn native_host_builder_publishes_coff_import_library_for_windows_kits() {
    for profile in [RuntimeKitProfile::Debug, RuntimeKitProfile::Release] {
        let prefix = TempDir::new(profile.as_str());
        let built = build_native_host(prefix.0.clone(), profile).expect("publish Windows native host runtime kit");
        let import =
            built.shared_import_library.as_ref().expect("Windows ABI-v5 kits must publish a COFF import library");
        assert!(import.is_file(), "missing COFF import library: {}", import.display());
        assert_eq!(import.file_name().and_then(|name| name.to_str()), Some("beskid_runtime_import.lib"));
        assert!(built.static_library.is_file());
        assert!(built.shared_library.is_file());
    }
}

#[test]
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
fn native_host_builder_publishes_closure_exports_with_manifest_provenance_for_each_profile() {
    let triple = if cfg!(target_os = "macos") { "aarch64-apple-darwin" } else { "x86_64-unknown-linux-gnu" };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == triple)
        .expect("supported native ABI-v5 target");
    let expected = [
        "beskid_rt_v5_managed_object_allocate",
        "beskid_rt_v5_closure_environment_allocate",
        "beskid_rt_v5_closure_capture_store",
        "beskid_rt_v5_closure_environment_root",
        "beskid_rt_v5_closure_environment_root_current",
        "str_concat",
        "gc_collect",
    ];

    for profile in [RuntimeKitProfile::Debug, RuntimeKitProfile::Release] {
        let prefix = TempDir::new(profile.as_str());
        let built = build_native_host(prefix.0.clone(), profile).expect("publish native host runtime kit");
        let defined = Command::new("nm")
            .args(if cfg!(target_os = "macos") { vec!["-gU", "-j"] } else { vec!["-g", "--defined-only", "-j"] })
            .arg(&built.static_library)
            .output()
            .expect("inspect staged static runtime archive");
        assert!(defined.status.success(), "nm failed for static runtime archive");
        let defined = String::from_utf8(defined.stdout).expect("UTF-8 symbol list");
        for symbol in expected {
            assert!(
                defined.lines().any(|actual| actual.trim_start_matches('_') == symbol),
                "{} kit omitted {symbol}: {defined}",
                profile.as_str(),
            );
        }

        let undefined = Command::new("nm")
            .args(["-u", "-j"])
            .arg(&built.static_library)
            .output()
            .expect("inspect staged static runtime archive imports");
        assert!(undefined.status.success(), "nm failed for static runtime archive imports");
        let symbol_list = format!(
            "target={}\n{}{}",
            target.triple.as_str(),
            defined
                .lines()
                .filter(|symbol| !symbol.is_empty() && !symbol.ends_with(':'))
                .map(|symbol| format!("defined={symbol}\n"))
                .collect::<String>(),
            String::from_utf8(undefined.stdout)
                .expect("UTF-8 import list")
                .lines()
                .filter(|symbol| !symbol.is_empty() && !symbol.ends_with(':'))
                .map(|symbol| format!("undefined={symbol}\n"))
                .collect::<String>(),
        );
        // Static archives still carry GD TLS `__tls_get_addr` until the shared
        // loader boundary is linked; use the archive-scoped audit, not the
        // final-image verify().
        RuntimeProvenanceAudit::canonical(target.clone())
            .expect("canonical provenance audit")
            .verify_static_archive(&parse_symbol_list(&symbol_list).expect("parse symbol list"))
            .expect("native runtime kit must satisfy manifest provenance");
    }
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
    let artifacts = profile_artifacts(&inputs, RuntimeKitProfile::Debug, "x86_64-unknown-linux-gnu");
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

fn profile_artifacts(inputs: &TempDir, profile: RuntimeKitProfile, target: &str) -> RuntimeKitProfileArtifacts {
    let suffix = profile.as_str();
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == target)
        .expect("supported matrix target");
    let static_extension = if target.object_format.as_str() == "coff" { "lib" } else { "a" };
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
    let static_provenance_symbol_list = inputs.0.join(format!("{suffix}.static.symbols"));
    let shared_provenance_symbol_list = inputs.0.join(format!("{suffix}.shared.symbols"));
    let audit = RuntimeProvenanceAudit::canonical(target).expect("canonical audit");
    let fixture = audit.fixture_symbol_list().expect("fixture symbols");
    let mut symbols = format!("target={}\n", fixture.target);
    for symbol in fixture.defined {
        symbols.push_str(&format!("defined={symbol}\n"));
    }
    for symbol in fixture.undefined {
        symbols.push_str(&format!("undefined={symbol}\n"));
    }
    fs::write(&static_provenance_symbol_list, &symbols).unwrap();
    fs::write(&shared_provenance_symbol_list, symbols).unwrap();
    RuntimeKitProfileArtifacts {
        profile,
        static_library,
        shared_library,
        shared_import_library,
        static_provenance_symbol_list,
        shared_provenance_symbol_list,
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
            profile_artifacts(&inputs, RuntimeKitProfile::Debug, "x86_64-unknown-linux-gnu"),
            profile_artifacts(&inputs, RuntimeKitProfile::Release, "x86_64-unknown-linux-gnu"),
        ],
    })
    .expect("publish Linux runtime-kit matrix");
    assert_eq!(built.len(), 2);
    for profile in ["debug", "release"] {
        let root = prefix.0.join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu").join(profile);
        assert!(root.join("abi.json").is_file());
        assert!(root.join("static/libbeskid_runtime.a").is_file());
        assert!(root.join("shared/libbeskid_runtime.so").is_file());
    }
}

#[test]
fn linux_matrix_accepts_documented_shared_loader_imports() {
    let prefix = TempDir::new("linux-loader-prefix");
    let inputs = TempDir::new("linux-loader-inputs");
    let profiles = [RuntimeKitProfile::Debug, RuntimeKitProfile::Release]
        .into_iter()
        .map(|profile| {
            let artifacts = profile_artifacts(&inputs, profile, "x86_64-unknown-linux-gnu");
            let mut symbols = fs::read_to_string(&artifacts.shared_provenance_symbol_list).unwrap();
            for import in [
                "_ITM_deregisterTMCloneTable",
                "_ITM_registerTMCloneTable",
                "__cxa_finalize",
                "__gmon_start__",
                "__tls_get_addr",
                // GNU ld's weak aliases of glibc's `__environ` data import.
                "_environ",
                "environ",
            ] {
                symbols.push_str(&format!("undefined={import}\n"));
            }
            fs::write(&artifacts.shared_provenance_symbol_list, symbols).unwrap();
            artifacts
        })
        .collect();

    let built = build_matrix(RuntimeKitMatrixBuildOptions {
        prefix: prefix.0.clone(),
        target: "x86_64-unknown-linux-gnu".into(),
        profiles,
    })
    .expect("documented shared-loader imports must pass the shared-artifact policy");

    assert_eq!(built.len(), 2);
}

#[test]
fn matrix_rejects_an_export_missing_from_only_one_artifact() {
    let prefix = TempDir::new("missing-static-export-prefix");
    let inputs = TempDir::new("missing-static-export-inputs");
    let debug = profile_artifacts(&inputs, RuntimeKitProfile::Debug, "x86_64-unknown-linux-gnu");
    let release = profile_artifacts(&inputs, RuntimeKitProfile::Release, "x86_64-unknown-linux-gnu");
    let symbols = fs::read_to_string(&release.static_provenance_symbol_list)
        .unwrap()
        .lines()
        .filter(|line| *line != "defined=beskid_rt_v5_abi_version")
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&release.static_provenance_symbol_list, format!("{symbols}\n")).unwrap();

    let error = build_matrix(RuntimeKitMatrixBuildOptions {
        prefix: prefix.0.clone(),
        target: "x86_64-unknown-linux-gnu".into(),
        profiles: vec![debug, release],
    })
    .unwrap_err()
    .to_string();

    assert!(error.contains("defined symbol table is missing"));
    assert!(error.contains("beskid_rt_v5_abi_version"));
    assert!(!prefix.0.join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu").exists());
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
    assert!(built.iter().all(|kit| { kit.shared_library.ends_with("shared/libbeskid_runtime.dylib") }));
    assert!(built.iter().all(|kit| kit.static_library.ends_with("static/libbeskid_runtime.a")));
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
        let root = prefix.0.join("lib/beskid-runtime/abi-5").join(target).join(profile);
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
        profiles: vec![profile_artifacts(&inputs, RuntimeKitProfile::Debug, "x86_64-unknown-linux-gnu")],
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("exactly debug and release"));

    let error = build_matrix(RuntimeKitMatrixBuildOptions {
        prefix: prefix.0.clone(),
        target: "x86_64-unknown-linux-gnu".into(),
        profiles: vec![
            profile_artifacts(&inputs, RuntimeKitProfile::Debug, "x86_64-unknown-linux-gnu"),
            profile_artifacts(&inputs, RuntimeKitProfile::Debug, "x86_64-unknown-linux-gnu"),
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
    let debug = profile_artifacts(&inputs, RuntimeKitProfile::Debug, "x86_64-unknown-linux-gnu");
    let release = profile_artifacts(&inputs, RuntimeKitProfile::Release, "x86_64-unknown-linux-gnu");
    fs::write(
        &release.static_provenance_symbol_list,
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
    assert!(!prefix.0.join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu/debug").exists());
}

#[test]
fn failed_artifact_copy_publishes_no_profile_from_the_matrix() {
    let prefix = TempDir::new("artifact-prefix");
    let inputs = TempDir::new("artifact-inputs");
    let debug = profile_artifacts(&inputs, RuntimeKitProfile::Debug, "x86_64-unknown-linux-gnu");
    let mut release = profile_artifacts(&inputs, RuntimeKitProfile::Release, "x86_64-unknown-linux-gnu");
    release.shared_library = inputs.0.join("missing-release.so");

    assert!(
        build_matrix(RuntimeKitMatrixBuildOptions {
            prefix: prefix.0.clone(),
            target: "x86_64-unknown-linux-gnu".into(),
            profiles: vec![debug, release],
        })
        .is_err()
    );
    assert!(!prefix.0.join("lib/beskid-runtime/abi-5/x86_64-unknown-linux-gnu").exists());
}
