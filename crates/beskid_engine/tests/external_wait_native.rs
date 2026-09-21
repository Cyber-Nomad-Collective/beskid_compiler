#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

#[path = "support/native_fixture.rs"]
mod native_fixture;

use beskid_abi::runtime_kit::{BuildProfile, host_runtime_target, resolve_installed_runtime_kit};
use beskid_engine::JitRuntimeKit;
#[cfg(windows)]
use beskid_tests_support::native_harness::place_shared_runtime;
use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded, shared_library_name};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};

const CHILD_MODE: &str = "BESKID_EXTERNAL_WAIT_CHILD_MODE";
const CHILD_PREFIX: &str = "BESKID_EXTERNAL_WAIT_CHILD_PREFIX";
const CHILD_FIXTURE: &str = "BESKID_EXTERNAL_WAIT_CHILD_FIXTURE";
const ROUTE_LIMIT: Duration = Duration::from_secs(60);
type TryComplete = unsafe extern "C" fn(usize, usize, usize) -> u8;

fn compile_standalone(root: &Path, output: &Path, library: &Path) -> Output {
    let mut command = native_c_compiler();
    command
        .args(["-DEXTERNAL_WAIT_STANDALONE", "-I"])
        .arg(root.join("../beskid_abi/include"))
        .arg(root.join("tests/fixtures/external_wait.c"))
        .arg(library);
    #[cfg(unix)]
    command.args(["-lpthread", "-lm"]);
    command.arg("-o").arg(output).current_dir(output.parent().unwrap());
    run_bounded("compile standalone", &mut command, ROUTE_LIMIT)
}

fn compile_fixture_library(root: &Path, output: &Path, import_library: &Path) -> Output {
    let mut command = native_c_compiler();
    #[cfg(target_os = "macos")]
    command.arg("-dynamiclib");
    #[cfg(target_os = "linux")]
    command.args(["-shared", "-fPIC"]);
    #[cfg(windows)]
    command.arg("-shared");
    command
        .arg("-I")
        .arg(root.join("../beskid_abi/include"))
        .arg(root.join("tests/fixtures/external_wait.c"))
        .arg(import_library);
    #[cfg(unix)]
    command.arg("-lpthread");
    command.arg("-o").arg(output).current_dir(output.parent().unwrap());
    run_bounded("compile fixture library", &mut command, ROUTE_LIMIT)
}

fn child_command(mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["owner_routed_waits_and_deadlines_have_one_winner", "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_MODE, mode);
    command
}

#[test]
fn owner_routed_waits_and_deadlines_have_one_winner() {
    if let Some(mode) = std::env::var_os(CHILD_MODE) {
        match mode.to_str() {
            Some("jit") => run_jit_fixture(),
            _ => panic!("unsupported external-wait child mode: {}", mode.to_string_lossy()),
        }
        return;
    }
    let unsupported = run_bounded("unsupported child marker", &mut child_command("unsupported-probe"), ROUTE_LIMIT);
    assert_eq!(unsupported.status.code(), Some(101), "{unsupported:?}");
    assert!(
        String::from_utf8_lossy(&unsupported.stderr)
            .contains("unsupported external-wait child mode: unsupported-probe"),
        "{unsupported:?}",
    );
    let prefix = tempfile::tempdir().unwrap();
    let prefix_path = prefix.path().canonicalize().unwrap();
    let kit = build_native_host(prefix_path.clone(), RuntimeKitProfile::Debug).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    #[cfg(unix)]
    let shared_link_library = &kit.shared_library;
    #[cfg(windows)]
    let shared_link_library =
        kit.shared_import_library.as_ref().expect("Windows ABI-v5 kit must contain a COFF import library");
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", shared_link_library)] {
        let directory = prefix_path.join(mode);
        std::fs::create_dir(&directory).unwrap();
        let executable = directory.join(executable_name("external-wait"));
        let output = compile_standalone(root, &executable, library);
        assert!(output.status.success(), "{mode}: {output:?}");
        #[cfg(windows)]
        if mode == "native-kit" {
            place_shared_runtime(&directory, &kit.shared_library);
        }
        let mut command = Command::new(&executable);
        #[cfg(unix)]
        if mode == "native-kit" {
            command.env(
                if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                kit.shared_library.parent().unwrap(),
            );
        }
        let output = run_bounded(&format!("{mode} success"), &mut command, ROUTE_LIMIT);
        assert!(output.status.success(), "{mode}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("descriptor-close-rebind=preserved"),
            "{mode} fixture did not prove that an admitted worker duplicate survives caller close/rebind: {output:?}",
        );
        let deadlock = run_bounded(&format!("{mode} deadlock"), command.arg("deadlock"), ROUTE_LIMIT);
        assert_eq!(deadlock.status.code(), Some(101), "{mode}: {deadlock:?}");
        assert!(String::from_utf8_lossy(&deadlock.stderr).contains("beskid runtime trap v5"), "{mode}: {deadlock:?}");
        eprintln!("{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
    // The fixture lives beside the exact kit DLL; Windows dependency search is constrained to this directory.
    let fixture_library = kit.shared_library.parent().unwrap().join(shared_library_name("wait-fixture"));
    let compiled = compile_fixture_library(root, &fixture_library, shared_link_library);
    assert!(compiled.status.success(), "{compiled:?}");
    let output = run_bounded(
        "jit callback",
        child_command("jit").env(CHILD_PREFIX, &prefix_path).env(CHILD_FIXTURE, &fixture_library),
        ROUTE_LIMIT,
    );
    assert!(output.status.success(), "jit callback: {output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("descriptor-close-rebind=preserved"),
        "jit callback fixture did not prove that an admitted worker duplicate survives caller close/rebind: {output:?}",
    );
    eprintln!("jit callback: {}", String::from_utf8_lossy(&output.stderr));
}

fn run_jit_fixture() {
    let prefix = PathBuf::from(std::env::var_os(CHILD_PREFIX).expect("JIT child requires its exact kit prefix"));
    let fixture_library = PathBuf::from(std::env::var_os(CHILD_FIXTURE).expect("JIT child requires its fixture path"));
    assert!(prefix.is_absolute() && fixture_library.is_absolute(), "JIT child paths must be absolute");
    let target = host_runtime_target().unwrap();
    let kit = resolve_installed_runtime_kit(&prefix, &target, BuildProfile::Debug).unwrap();
    let runtime = JitRuntimeKit::load(&prefix, &kit.metadata.target, BuildProfile::Debug).unwrap();
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
        function.finalize(module.isa().frontend_config());
    }
    module.define_function(entry, &mut context).unwrap();
    module.finalize_definitions().unwrap();
    // SAFETY: the fixture and JIT entry use these exact C signatures; the
    // runtime, fixture library and JIT module remain live through the call.
    unsafe {
        let fixture = native_fixture::NativeFixture::<unsafe extern "C" fn(TryComplete, i32) -> i32>::load(
            &fixture_library,
            c"RunExternalWaitFixture",
        );
        let complete: TryComplete = std::mem::transmute(module.get_finalized_function(entry));
        let result = (fixture.entry)(complete, 0);
        assert_eq!(result, 0);
    }
}
