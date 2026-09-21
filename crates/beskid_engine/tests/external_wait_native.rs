#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::JitRuntimeKit;
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use std::{ffi::CString, path::Path, process::Command};

#[test]
fn owner_routed_waits_and_deadlines_have_one_winner() {
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", &kit.shared_library)] {
        let executable = prefix.path().join(mode);
        let output = Command::new("cc")
            .args(["-std=c11", "-DEXTERNAL_WAIT_STANDALONE", "-I"])
            .arg(root.join("../beskid_abi/include"))
            .arg(root.join("tests/fixtures/external_wait.c"))
            .arg(library)
            .args(["-lpthread", "-lm", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        let output = Command::new(&executable)
            .env(
                if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                kit.shared_library.parent().unwrap(),
            )
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        let deadlock = Command::new(&executable)
            .arg("deadlock")
            .env(
                if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                kit.shared_library.parent().unwrap(),
            )
            .output()
            .unwrap();
        assert_eq!(deadlock.status.code(), Some(101), "{mode}: {deadlock:?}");
        assert!(String::from_utf8_lossy(&deadlock.stderr).contains("beskid runtime trap v5"));
        eprintln!("{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
    let fixture_library =
        prefix.path().join(if cfg!(target_os = "macos") { "wait-fixture.dylib" } else { "wait-fixture.so" });
    let compiled = Command::new("cc")
        .arg("-std=c11")
        .args(if cfg!(target_os = "macos") { vec!["-dynamiclib"] } else { vec!["-shared", "-fPIC"] })
        .arg("-I")
        .arg(root.join("../beskid_abi/include"))
        .arg(root.join("tests/fixtures/external_wait.c"))
        .arg(&kit.shared_library)
        .args(["-lpthread", "-o"])
        .arg(&fixture_library)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let runtime = JitRuntimeKit::load(prefix.path(), &kit.metadata.target, BuildProfile::Debug).unwrap();
    let mut builder = JITBuilder::new(default_libcall_names()).unwrap();
    for (name, address) in runtime.symbols() {
        builder.symbol(name, *address);
    }
    let mut module = JITModule::new(builder);
    let mut signature = module.make_signature();
    signature.params.extend([AbiParam::new(types::I64); 3]);
    signature.returns.push(AbiParam::new(types::I8));
    let target = module.declare_function("beskid_rt_v5_external_try_complete", Linkage::Import, &signature).unwrap();
    let entry = module.declare_function("jit_try_complete", Linkage::Export, &signature).unwrap();
    let mut context = module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut function = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = function.create_block();
        function.append_block_params_for_function_params(block);
        function.switch_to_block(block);
        function.seal_block(block);
        let callee = module.declare_func_in_func(target, function.func);
        let args = function.block_params(block).to_vec();
        let call = function.ins().call(callee, &args);
        let result = function.inst_results(call)[0];
        function.ins().return_(&[result]);
        function.finalize();
    }
    module.define_function(entry, &mut context).unwrap();
    module.finalize_definitions().unwrap();
    let path = CString::new(fixture_library.to_str().unwrap()).unwrap();
    // SAFETY: the fixture and JIT entry use these exact C signatures; the
    // runtime, fixture library and JIT module remain live through the call.
    unsafe {
        let library = libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        assert!(!library.is_null());
        let symbol = libc::dlsym(library, c"RunExternalWaitFixture".as_ptr());
        assert!(!symbol.is_null());
        type TryComplete = unsafe extern "C" fn(usize, usize, usize) -> u8;
        let run: unsafe extern "C" fn(TryComplete, i32) -> i32 = std::mem::transmute(symbol);
        let complete: TryComplete = std::mem::transmute(module.get_finalized_function(entry));
        assert_eq!(run(complete, 0), 0);
        libc::dlclose(library);
    }
}
