#![cfg(unix)]

use std::{ffi::CString, path::Path, process::Command};

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::JitRuntimeKit;
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};

#[test]
fn traced_values_survive_collection_and_transfer_in_native_aot_and_jit() {
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("build canonical runtime with traced ABI-value operations");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("tests/fixtures/traced_abi_value.c");
    let include = root.join("../beskid_abi/include");
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", &kit.shared_library)] {
        let executable = prefix.path().join(mode);
        let output = Command::new("cc")
            .arg("-std=c11")
            .arg("-DABI_VALUE_STANDALONE")
            .arg("-I")
            .arg(&include)
            .arg(&source)
            .arg(library)
            .arg("-lpthread")
            .arg("-lm")
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        for fault in ["receipt-count-overflow", "receipt-count-underflow"] {
            let rejected = Command::new(&executable)
                .arg(fault)
                .env(
                    if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                    kit.shared_library.parent().unwrap(),
                )
                .output()
                .unwrap();
            assert_eq!(rejected.status.code(), Some(101), "{mode}/{fault}: {rejected:?}");
            assert!(String::from_utf8_lossy(&rejected.stderr).contains("beskid runtime trap v5"));
        }
        // The existing kit builder preserves its staging install-name on Darwin. Scope
        // loader lookup to this exact test kit, without changing the published artifact.
        let output = Command::new(&executable)
            .env(
                if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                kit.shared_library.parent().unwrap(),
            )
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        let panic = Command::new(&executable)
            .arg("detached-panic")
            .env(
                if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                kit.shared_library.parent().unwrap(),
            )
            .output()
            .unwrap();
        assert!(!panic.status.success(), "{mode}: detached panic must abort");
        assert!(String::from_utf8_lossy(&panic.stderr).contains("child panic"), "{mode}: {:?}", panic);
    }

    // Build a native fixture entry, then supply a real JIT-generated transfer call.
    let fixture_library = prefix.path().join(if cfg!(target_os = "macos") { "fixture.dylib" } else { "fixture.so" });
    let output = Command::new("cc")
        .arg("-std=c11")
        .args(if cfg!(target_os = "macos") { vec!["-dynamiclib"] } else { vec!["-shared", "-fPIC"] })
        .arg("-I")
        .arg(&include)
        .arg(&source)
        .arg(&kit.shared_library)
        .arg("-o")
        .arg(&fixture_library)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let runtime = JitRuntimeKit::load(prefix.path(), &kit.metadata.target, BuildProfile::Debug).unwrap();
    let mut builder = JITBuilder::new(default_libcall_names()).unwrap();
    for (name, address) in runtime.symbols() {
        builder.symbol(name, *address);
    }
    let mut module = JITModule::new(builder);
    let mut signature = module.make_signature();
    signature.params.extend([AbiParam::new(types::I64), AbiParam::new(types::I64)]);
    signature.returns.push(AbiParam::new(types::I8));
    let import = module.declare_function("beskid_rt_v5_abi_value_move_out", Linkage::Import, &signature).unwrap();
    let entry = module.declare_function("jit_move_value", Linkage::Export, &signature).unwrap();
    let mut context = module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut function = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let block = function.create_block();
        function.append_block_params_for_function_params(block);
        function.switch_to_block(block);
        function.seal_block(block);
        let callee = module.declare_func_in_func(import, function.func);
        let args = function.block_params(block).to_vec();
        let call = function.ins().call(callee, &args);
        let result = function.inst_results(call)[0];
        function.ins().return_(&[result]);
        function.finalize();
    }
    module.define_function(entry, &mut context).unwrap();
    module.finalize_definitions().unwrap();
    let path = CString::new(fixture_library.to_str().unwrap()).unwrap();
    // SAFETY: the fixture exports this exact C signature and every library/module remains live
    // through the call. Runtime slots/descriptors are owned by the fixture's stack frame.
    unsafe {
        let library = libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        assert!(!library.is_null());
        let symbol = libc::dlsym(library, c"RunAbiValueFixture".as_ptr());
        assert!(!symbol.is_null());
        type MoveValue = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> u8;
        let run: unsafe extern "C" fn(MoveValue) -> i32 = std::mem::transmute(symbol);
        let move_value: MoveValue = std::mem::transmute(module.get_finalized_function(entry));
        assert_eq!(run(move_value), 0);
        libc::dlclose(library);
    }
}
