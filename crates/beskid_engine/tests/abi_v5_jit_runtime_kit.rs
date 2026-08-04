#![cfg(unix)]

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, canonical_source_hash};
use beskid_abi::runtime_kit::{BuildProfile, RuntimeKitBuildRequest, build_runtime_kit};
use beskid_abi::runtime_source::canonical_runtime_sources;
use beskid_codegen::{CodegenArtifact, ExternImport, LoweredFunction};
use beskid_engine::{BeskidJitModule, Engine, JitRuntimeKit};
use cranelift_codegen::ir::{AbiParam, ExternalName, Function, InstBuilder, Signature, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
// `BESKID_RUNTIME_PREFIX` is process-global. These integration tests run concurrently in the
// same test binary, so every temporary installed-prefix context must hold this lock from the
// environment update through the call that resolves the exact kit.
static RUNTIME_PREFIX_LOCK: Mutex<()> = Mutex::new(());

struct RuntimePrefixContext {
    previous: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl RuntimePrefixContext {
    fn install(prefix: &Path) -> Self {
        let lock = RUNTIME_PREFIX_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os("BESKID_RUNTIME_PREFIX");
        // SAFETY: the process-wide lock prevents concurrent reads or writes in this integration
        // target, and Drop restores the exact pre-test value before releasing that lock.
        unsafe { std::env::set_var("BESKID_RUNTIME_PREFIX", prefix) };
        Self { previous, _lock: lock }
    }
}

impl Drop for RuntimePrefixContext {
    fn drop(&mut self) {
        // SAFETY: `RuntimePrefixContext::install` holds the process-wide lock for this mutation.
        unsafe {
            if let Some(value) = &self.previous {
                std::env::set_var("BESKID_RUNTIME_PREFIX", value);
            } else {
                std::env::remove_var("BESKID_RUNTIME_PREFIX");
            }
        }
    }
}

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
    TargetMetadata::supported().into_iter().find(|target| target.triple.as_str() == triple)
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
    let output = command.arg(&source_path).arg("-o").arg(path).output().expect("invoke host C compiler");
    assert!(output.status.success(), "shared runtime compile failed: {}", String::from_utf8_lossy(&output.stderr));
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
    let shared_library = inputs.join(if cfg!(target_os = "macos") { "runtime.dylib" } else { "runtime.so" });
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let symbols = if complete_exports {
        manifest
            .exports
            .iter()
            .map(|entry| entry.symbol.clone())
            .chain(manifest.assembly_exports.iter().map(|entry| entry.symbol.as_str().to_owned()))
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
fn loader_requires_only_metadata_loader_exports_and_retains_library() {
    let Some(target) = host_target() else {
        return;
    };
    let temp = TestDir::new();
    let shared = install_kit(temp.path(), &target, true, true, canonical_hash());
    let runtime = JitRuntimeKit::load(temp.path(), &target, BuildProfile::Debug).unwrap();
    let actual = runtime.symbol_names().collect::<BTreeSet<_>>();
    let expected = runtime.metadata().loader_required_exports.iter().map(String::as_str).collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert!(!actual.contains("beskid_rt_v5_intrinsic_system_allocate"));
    assert!(!actual.contains("attacker_unapproved_export"));
    assert_eq!(runtime.shared_library_path(), shared);
    assert!(runtime.symbols().iter().all(|(_, address)| !address.is_null()));

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
    let fake = [("beskid_rt_v5_abi_version".to_owned(), std::ptr::dangling::<u8>())];
    assert!(BeskidJitModule::new_with_runtime_kit(temp.path(), &target, BuildProfile::Debug, &fake,).is_err());
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
    let signature = function.import_signature(Signature::new(cranelift_codegen::isa::CallConv::SystemV));
    // `getpid` is a real process symbol (resolvable via dlsym) that is neither a kit export nor a
    // soft builtin (`BUILTIN_SPECS` / dispatch). Exact-kit JIT validation must reject it before any
    // process-symbol fallback can satisfy it.
    function.import_function(cranelift_codegen::ir::ExtFuncData {
        name: ExternalName::testcase("getpid".as_bytes()),
        signature,
        colocated: false,
        patchable: false,
    });
    let artifact =
        CodegenArtifact { functions: vec![LoweredFunction { name: "Main".into(), function }], ..Default::default() };
    let mut jit = BeskidJitModule::new_with_runtime_kit(temp.path(), &target, BuildProfile::Debug, &[]).unwrap();
    let error = jit.compile(&artifact).expect_err("unapproved process symbol must be rejected before dlsym fallback");
    assert!(error.to_string().contains("not approved"));
}

#[test]
fn corelib_syscall_write_links_from_the_process_builtin_registry() {
    let Some(target) = host_target() else {
        return;
    };
    let temp = TestDir::new();
    install_kit(temp.path(), &target, true, false, canonical_hash());

    let mut function = Function::new();
    let mut syscall_signature = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
    syscall_signature.params.push(AbiParam::new(types::I64));
    syscall_signature.params.push(AbiParam::new(types::I64));
    syscall_signature.returns.push(AbiParam::new(types::I64));
    let signature = function.import_signature(syscall_signature);
    let syscall = function.import_function(cranelift_codegen::ir::ExtFuncData {
        name: ExternalName::testcase("syscall_write".as_bytes()),
        signature,
        colocated: false,
        patchable: false,
    });
    let mut builder_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
    let block = builder.create_block();
    builder.switch_to_block(block);
    let fd = builder.ins().iconst(types::I64, 1);
    let value = builder.ins().iconst(types::I64, 0);
    builder.ins().call(syscall, &[fd, value]);
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize();

    let artifact = CodegenArtifact {
        functions: vec![LoweredFunction { name: "Main".into(), function }],
        extern_imports: vec![ExternImport { symbol: "syscall_write".into(), abi: Some("C".into()), library: None }],
        ..Default::default()
    };
    let mut jit = BeskidJitModule::new_with_runtime_kit(temp.path(), &target, BuildProfile::Debug, &[])
        .expect("exact ABI-v5 runtime kit loads before process builtin registration");
    jit.compile(&artifact).expect("Corelib syscall_write must link through the process builtin registry");
}

#[test]
fn engine_uses_only_the_configured_exact_runtime_kit() {
    let Some(target) = host_target() else {
        return;
    };
    let temp = TestDir::new();
    install_kit(temp.path(), &target, true, false, canonical_hash());
    let mut engine =
        Engine::with_runtime_kit(temp.path(), target, BuildProfile::Debug).expect("construct exact-kit Engine");

    let mut function = Function::new();
    let signature = function.import_signature(Signature::new(cranelift_codegen::isa::CallConv::SystemV));
    // `getpid` resolves in-process but is not part of the exact ABI-v5 kit nor a soft builtin, so the
    // Engine must reject it rather than satisfy it from the surrounding process symbols.
    function.import_function(cranelift_codegen::ir::ExtFuncData {
        name: ExternalName::testcase("getpid".as_bytes()),
        signature,
        colocated: false,
        patchable: false,
    });
    let artifact =
        CodegenArtifact { functions: vec![LoweredFunction { name: "Main".into(), function }], ..Default::default() };

    let error = engine
        .compile_artifact(&artifact)
        .expect_err("Engine must not satisfy an unapproved reference from process symbols");
    // The Engine fails closed on the unapproved reference regardless of build profile: release
    // builds reach the exact-kit validator ("not approved"), while debug builds trip the
    // `#[cfg(debug_assertions)]` artifact validator first, which rejects the same reference as an
    // undefined callee. Either way it is rejected rather than satisfied from process symbols.
    let message = error.to_string();
    assert!(
        message.contains("not approved") || message.contains("undefined callees"),
        "Engine must fail closed on an unapproved reference rather than satisfy it from process symbols; got: {message}"
    );
}

#[test]
fn engine_try_new_fails_closed_when_exact_debug_manifest_is_missing() {
    let Some(target) = host_target() else {
        return;
    };
    let empty = TestDir::new();
    let _runtime_prefix = RuntimePrefixContext::install(empty.path());
    let error = match Engine::try_new() {
        Ok(_) => panic!("missing exact kit must fail closed"),
        Err(error) => error,
    };
    let message = error.to_string();
    let expected =
        empty.path().join("lib/beskid-runtime/abi-5").join(target.triple.as_str()).join("debug").join("abi.json");
    assert!(
        message.contains(&expected.display().to_string()) || message.contains("MetadataRead"),
        "expected missing-manifest fail-closed diagnostic mentioning {}, got {message}",
        expected.display()
    );
}

#[test]
fn codegen_input_route_fails_closed_when_exact_kit_manifest_is_missing() {
    let Some(target) = host_target() else {
        return;
    };
    let empty = TestDir::new();
    let _runtime_prefix = RuntimePrefixContext::install(empty.path());
    let error =
        beskid_engine::services::run_entrypoint(Path::new("missing-kit.bd"), "i64 Main() { return 1; }", "Main")
            .expect_err("CodegenInput JIT route must fail closed without an exact kit");
    let message = error.to_string();
    let expected =
        empty.path().join("lib/beskid-runtime/abi-5").join(target.triple.as_str()).join("debug").join("abi.json");
    assert!(
        message.contains(&expected.display().to_string())
            || message.contains("MetadataRead")
            || message.contains("runtime kit"),
        "expected missing-kit fail-closed diagnostic mentioning {}, got {message}",
        expected.display()
    );
}

#[test]
fn codegen_input_route_fails_closed_when_exact_kit_is_tampered() {
    let Some(target) = host_target() else {
        return;
    };
    let tampered = TestDir::new();
    let shared = install_kit(tampered.path(), &target, true, false, canonical_hash());
    fs::write(shared, b"tampered shared runtime").unwrap();

    let _runtime_prefix = RuntimePrefixContext::install(tampered.path());
    let error =
        beskid_engine::services::run_entrypoint(Path::new("tampered-kit.bd"), "i64 Main() { return 1; }", "Main")
            .expect_err("CodegenInput JIT route must reject a tampered exact kit");
    let message = error.to_string();
    assert!(
        message.contains("runtime kit")
            || message.contains("hash")
            || message.contains("validation")
            || message.contains("ArtifactHash"),
        "expected tampered-kit fail-closed diagnostic, got {message}"
    );
}

#[test]
fn prepare_jit_entrypoint_uses_codegen_input_symbols_only() {
    let Some(_target) = host_target() else {
        return;
    };
    let prepared = beskid_engine::services::prepare_jit_entrypoint(
        Path::new("codegen-input-route.bd"),
        "i64 Echo(i64 value) { return value; } i64 Main() { return Echo(41); }",
        "Main",
    )
    .expect("CodegenInput preparation");
    assert!(prepared.symbol.starts_with("Main#syntax_"));
    assert_eq!(prepared.artifact.functions.len(), 2);
    assert!(prepared.artifact.functions.iter().any(|function| function.name.starts_with("Echo#syntax_")));
}
