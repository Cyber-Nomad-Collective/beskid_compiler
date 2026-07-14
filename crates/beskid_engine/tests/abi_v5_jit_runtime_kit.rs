#![cfg(unix)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, canonical_source_hash};
use beskid_abi::runtime_kit::{BuildProfile, RuntimeKitBuildRequest, build_runtime_kit};
use beskid_abi::runtime_source::canonical_runtime_sources;
use beskid_codegen::{CodegenArtifact, LoweredFunction};
use beskid_engine::{BeskidJitModule, JitRuntimeKit};
use cranelift_codegen::ir::{ExternalName, Function, Signature};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "beskid-jit-v5-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn host_target() -> Option<TargetMetadata> {
    let triple = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        _ => return None,
    };
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == triple)
}

fn compile_shared(path: &Path, symbols: &[String], unexpected: bool) {
    let source_path = path.with_extension("c");
    let mut source = String::new();
    for symbol in symbols {
        source.push_str(&format!("void {symbol}(void) {{}}\n"));
    }
    if unexpected {
        source.push_str("void attacker_unapproved_export(void) {}\n");
    }
    fs::write(&source_path, source).unwrap();
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let mut command = Command::new(compiler);
    if cfg!(target_os = "macos") {
        command.arg("-dynamiclib");
    } else {
        command.args(["-shared", "-fPIC"]);
    }
    let output = command
        .arg(&source_path)
        .arg("-o")
        .arg(path)
        .output()
        .expect("invoke host C compiler");
    assert!(
        output.status.success(),
        "shared runtime compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn install_kit(
    prefix: &Path,
    target: &TargetMetadata,
    complete_exports: bool,
    unexpected_export: bool,
    source_hash: String,
) -> PathBuf {
    let inputs = prefix.join("inputs");
    fs::create_dir_all(&inputs).unwrap();
    let static_library = inputs.join("runtime.a");
    fs::write(&static_library, b"static runtime placeholder").unwrap();
    let shared_library = inputs.join(if cfg!(target_os = "macos") {
        "runtime.dylib"
    } else {
        "runtime.so"
    });
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let symbols = if complete_exports {
        manifest
            .exports
            .iter()
            .map(|entry| entry.symbol.clone())
            .chain(
                manifest
                    .assembly_exports
                    .iter()
                    .map(|entry| entry.symbol.as_str().to_owned()),
            )
            .collect::<Vec<_>>()
    } else {
        vec!["beskid_rt_v5_abi_version".to_owned()]
    };
    compile_shared(&shared_library, &symbols, unexpected_export);
    build_runtime_kit(&RuntimeKitBuildRequest {
        prefix: prefix.to_path_buf(),
        target: target.clone(),
        profile: BuildProfile::Debug,
        runtime_source_hash: source_hash,
        static_library,
        shared_library,
        shared_import_library: None,
    })
    .expect("install hermetic JIT kit")
    .shared_library
}

fn canonical_hash() -> String {
    canonical_source_hash(&canonical_runtime_sources()).unwrap()
}

#[test]
fn loader_registers_exactly_metadata_approved_exports_and_retains_library() {
    let Some(target) = host_target() else {
        return;
    };
    let temp = TestDir::new();
    let shared = install_kit(temp.path(), &target, true, true, canonical_hash());
    let runtime = JitRuntimeKit::load(temp.path(), &target, BuildProfile::Debug).unwrap();
    let actual = runtime.symbol_names().collect::<BTreeSet<_>>();
    let expected = runtime
        .metadata()
        .export_allowlist
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert!(!actual.contains("attacker_unapproved_export"));
    assert_eq!(runtime.shared_library_path(), shared);
    assert!(
        runtime
            .symbols()
            .iter()
            .all(|(_, address)| !address.is_null())
    );

    BeskidJitModule::new_with_runtime_kit(temp.path(), &target, BuildProfile::Debug, &[])
        .expect("JIT owns loaded runtime for module lifetime");
}

#[test]
fn runtime_exports_cannot_be_overridden_by_external_symbol_registration() {
    let Some(target) = host_target() else {
        return;
    };
    let temp = TestDir::new();
    install_kit(temp.path(), &target, true, false, canonical_hash());
    let fake = [(
        "beskid_rt_v5_abi_version".to_owned(),
        std::ptr::dangling::<u8>(),
    )];
    assert!(
        BeskidJitModule::new_with_runtime_kit(temp.path(), &target, BuildProfile::Debug, &fake,)
            .is_err()
    );
}

#[test]
fn hash_drift_and_missing_approved_exports_fail_without_rust_fallback() {
    let Some(target) = host_target() else {
        return;
    };
    let tampered = TestDir::new();
    let shared = install_kit(tampered.path(), &target, true, false, canonical_hash());
    fs::write(shared, b"tampered shared runtime").unwrap();
    assert!(JitRuntimeKit::load(tampered.path(), &target, BuildProfile::Debug).is_err());

    let incomplete = TestDir::new();
    install_kit(incomplete.path(), &target, false, false, canonical_hash());
    assert!(JitRuntimeKit::load(incomplete.path(), &target, BuildProfile::Debug).is_err());
}

#[test]
fn internally_valid_kit_for_different_sources_is_rejected_before_loading() {
    let Some(target) = host_target() else {
        return;
    };
    let temp = TestDir::new();
    install_kit(temp.path(), &target, true, false, "b".repeat(64));
    assert!(JitRuntimeKit::load(temp.path(), &target, BuildProfile::Debug).is_err());
}

#[test]
fn unapproved_runtime_reference_is_rejected_before_process_symbol_fallback() {
    let Some(target) = host_target() else {
        return;
    };
    let temp = TestDir::new();
    install_kit(temp.path(), &target, true, false, canonical_hash());
    let mut function = Function::new();
    let signature =
        function.import_signature(Signature::new(cranelift_codegen::isa::CallConv::SystemV));
    function.import_function(cranelift_codegen::ir::ExtFuncData {
        name: ExternalName::testcase(beskid_abi::SYM_ALLOC.as_bytes()),
        signature,
        colocated: false,
        patchable: false,
    });
    let artifact = CodegenArtifact {
        functions: vec![LoweredFunction {
            name: "Main".into(),
            function,
        }],
        ..Default::default()
    };
    let mut jit =
        BeskidJitModule::new_with_runtime_kit(temp.path(), &target, BuildProfile::Debug, &[])
            .unwrap();
    let error = jit
        .compile(&artifact)
        .expect_err("legacy Rust runtime symbol");
    assert!(error.to_string().contains("not approved"));
}
