//! Production Mod build must produce an executable contract, not a source/object surrogate.
use std::{fs, path::PathBuf, process::Command};

#[test]
fn v06_real_compiler_sdk_collector_generator_builds_executable_descriptor() {
    let root = tempfile::tempdir().unwrap();
    let compiler = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let project = root.path().join("mod");
    fs::create_dir_all(project.join("Src")).unwrap();
    fs::copy(compiler.join("crates/beskid_tests_mods/fixtures/mods/native_sdk/Src/Mod.bd"), project.join("Src/Mod.bd"))
        .unwrap();
    let manifest = project.join("NativeMod.bproj");
    // The compiler SDK ships in the implicit Std Corelib closure; a second explicit path dependency on the
    // checkout package would duplicate `corelib_compiler_sdk` and `corelib_foundation` in the lock.
    fs::write(&manifest, "NativeMod { name = \"NativeMod\" version = \"0.6.0\" type = Mod root = \"Src\" mod { capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot] } }\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["dev", "mod", "rebuild"])
        .arg(&manifest)
        .args(["--offline", "--plain"])
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual SDK build prerequisite failed before executable qualification:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let descriptors: Vec<_> = walkdir::WalkDir::new(project.join(".beskid/obj/mods"))
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name() == "mod.descriptor.json")
        .collect();
    assert_eq!(descriptors.len(), 1, "one authoritative executable artifact descriptor required");
    let descriptor: serde_json::Value = serde_json::from_slice(&fs::read(descriptors[0].path()).unwrap()).unwrap();
    assert_eq!(descriptor["schemaVersion"], 2, "object-only schema1 does not prove executable Mod contracts");
    assert_eq!(descriptor["artifactKind"], "executable-shared-library");
    assert!(descriptor.get("objectFile").is_none(), "relocatable objects cannot be executable authority");
    assert_eq!(descriptor["hostAbiVersion"], 2);
    let registrations = descriptor["registrations"].as_array().expect("exact native registrations");
    assert_eq!(registrations.len(), 2, "actual Collector and Generator must both be exported");
    for family in ["Collector", "Generator"] {
        assert!(
            registrations
                .iter()
                .any(|registration| registration["contractId"].as_str().is_some_and(|id| id.ends_with(family))),
            "missing {family}"
        );
    }
}

#[test]
fn v06_qualified_mod_worker_generates_callable_and_generic_record_for_real_host() {
    let root = tempfile::tempdir().unwrap();
    let compiler = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let project = root.path().join("mod");
    fs::create_dir_all(project.join("Src")).unwrap();
    fs::copy(compiler.join("crates/beskid_tests_mods/fixtures/mods/native_sdk/Src/Mod.bd"), project.join("Src/Mod.bd"))
        .unwrap();
    fs::write(project.join("NativeMod.bproj"), "NativeMod { name = \"NativeMod\" version = \"0.6.0\" type = Mod root = \"Src\" mod { capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot] } }\n").unwrap();
    let host = root.path().join("host");
    fs::create_dir_all(host.join("Src")).unwrap();
    fs::write(host.join("Src/Main.bd"),"use Std.Testing.Assert;\nunit Main() { GeneratedUnit(); SerializedRecord<u32> value = SerializedRecord<u32> { Value: 7 }; Assert.True(value.Value == 7, \"generated generic field\"); }\n").unwrap();
    let manifest = host.join("Host.bproj");
    fs::write(&manifest,format!("Host {{ name = \"Host\" version = \"0.6.0\" root = \"Src\" }}\ndependency \"NativeMod\" {{ source = path path = {:?} }}\n",project.to_string_lossy())).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["run", "--project"])
        .arg(&manifest)
        .args(["--offline", "--plain"])
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "qualified native Mod must execute Collector/Generator and merge typed function/generic field before native host execution:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
