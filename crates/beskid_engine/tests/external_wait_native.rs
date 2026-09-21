#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

use beskid_abi::runtime_kit::{BuildProfile, host_runtime_target, resolve_installed_runtime_kit};
use beskid_engine::JitRuntimeKit;
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const CHILD_MODE: &str = "BESKID_EXTERNAL_WAIT_CHILD_MODE";
const CHILD_PREFIX: &str = "BESKID_EXTERNAL_WAIT_CHILD_PREFIX";
const CHILD_FIXTURE: &str = "BESKID_EXTERNAL_WAIT_CHILD_FIXTURE";
const ROUTE_LIMIT: Duration = Duration::from_secs(60);
type TryComplete = unsafe extern "C" fn(usize, usize, usize) -> u8;

fn fixture_suffix() -> &'static str {
    if cfg!(windows) {
        ".dll"
    } else if cfg!(target_os = "macos") {
        ".dylib"
    } else {
        ".so"
    }
}

fn executable_suffix() -> &'static str {
    if cfg!(windows) { ".exe" } else { "" }
}

fn fixture_compiler() -> Command {
    #[cfg(unix)]
    let command = Command::new("cc");
    #[cfg(windows)]
    let command = {
        let mut command = Command::new("clang");
        command.arg("--target=x86_64-pc-windows-msvc");
        command
    };
    command
}

fn compile_standalone(root: &Path, output: &Path, library: &Path) -> Output {
    let mut command = fixture_compiler();
    command
        .args(["-std=c11", "-DEXTERNAL_WAIT_STANDALONE", "-I"])
        .arg(root.join("../beskid_abi/include"))
        .arg(root.join("tests/fixtures/external_wait.c"))
        .arg(library);
    #[cfg(unix)]
    command.args(["-lpthread", "-lm"]);
    command.arg("-o").arg(output).current_dir(output.parent().unwrap());
    run_bounded("compile standalone", &mut command, ROUTE_LIMIT)
}

fn compile_fixture_library(root: &Path, output: &Path, import_library: &Path) -> Output {
    let mut command = fixture_compiler();
    command.arg("-std=c11");
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

fn run_bounded(label: &str, command: &mut Command, limit: Duration) -> Output {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{label}: spawn failed: {error}"));
    // Drain both pipes while polling so a full output pipe cannot hide completion.
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stdout = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    let failure = loop {
        match child.try_wait() {
            Ok(Some(_)) => break None,
            Ok(None) if started.elapsed() < limit => thread::sleep(Duration::from_millis(10)),
            Ok(None) => break Some(format!("deadline exceeded ({limit:?})")),
            Err(error) => break Some(format!("try_wait failed: {error}")),
        }
    };
    let kill_error = failure.as_ref().and_then(|_| child.kill().err());
    let status = child.wait();
    let stdout = stdout.join().expect("stdout reader panicked").expect("stdout read failed");
    let stderr = stderr.join().expect("stderr reader panicked").expect("stderr read failed");
    if let Some(failure) = failure {
        panic!(
            "{label}: {failure}; limit={limit:?}; status={status:?}; kill_error={kill_error:?}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
        );
    }
    Output { status: status.unwrap_or_else(|error| panic!("{label}: wait failed: {error}")), stdout, stderr }
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
        let executable = directory.join(format!("external-wait{}", executable_suffix()));
        let output = compile_standalone(root, &executable, library);
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        #[cfg(windows)]
        if mode == "native-kit" {
            std::fs::copy(&kit.shared_library, directory.join(kit.shared_library.file_name().unwrap())).unwrap();
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
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        let deadlock = run_bounded(&format!("{mode} deadlock"), command.arg("deadlock"), ROUTE_LIMIT);
        assert_eq!(deadlock.status.code(), Some(101), "{mode}: {deadlock:?}");
        assert!(String::from_utf8_lossy(&deadlock.stderr).contains("beskid runtime trap v5"), "{mode}: {deadlock:?}");
        eprintln!("{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
    // The fixture lives beside the exact kit DLL; Windows dependency search is constrained to this directory.
    let fixture_library = kit.shared_library.parent().unwrap().join(format!("wait-fixture{}", fixture_suffix()));
    let compiled = compile_fixture_library(root, &fixture_library, shared_link_library);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_bounded(
        "jit callback",
        child_command("jit").env(CHILD_PREFIX, &prefix_path).env(CHILD_FIXTURE, &fixture_library),
        ROUTE_LIMIT,
    );
    assert!(output.status.success(), "jit callback: {}", String::from_utf8_lossy(&output.stderr));
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
        function.finalize();
    }
    module.define_function(entry, &mut context).unwrap();
    module.finalize_definitions().unwrap();
    // SAFETY: the fixture and JIT entry use these exact C signatures; the
    // runtime, fixture library and JIT module remain live through the call.
    unsafe {
        let (library, run) = fixture_loader::load_fixture(&fixture_library);
        let complete: TryComplete = std::mem::transmute(module.get_finalized_function(entry));
        let result = run(complete, 0);
        fixture_loader::close_fixture(library);
        assert_eq!(result, 0);
    }
}

#[cfg(unix)]
mod fixture_loader {
    use super::{Path, TryComplete};
    use std::{
        ffi::{CStr, CString, c_void},
        os::unix::ffi::OsStrExt,
    };

    pub unsafe fn load_fixture(path: &Path) -> (*mut c_void, unsafe extern "C" fn(TryComplete, i32) -> i32) {
        let encoded = CString::new(path.as_os_str().as_bytes()).unwrap();
        unsafe {
            let library = libc::dlopen(encoded.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if library.is_null() {
                panic!("dlopen {}: {}", path.display(), CStr::from_ptr(libc::dlerror()).to_string_lossy());
            }
            let symbol = libc::dlsym(library, c"RunExternalWaitFixture".as_ptr());
            if symbol.is_null() {
                libc::dlclose(library);
                panic!("RunExternalWaitFixture missing from {}", path.display());
            }
            (library, std::mem::transmute::<*mut c_void, unsafe extern "C" fn(TryComplete, i32) -> i32>(symbol))
        }
    }

    pub unsafe fn close_fixture(library: *mut c_void) {
        unsafe {
            libc::dlclose(library);
        }
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
mod fixture_loader {
    use super::{Path, TryComplete};
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};

    type HMODULE = *mut c_void;
    const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: u32 = 0x00000100;
    const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: u32 = 0x00001000;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> HMODULE;
        fn GetProcAddress(module: HMODULE, name: *const u8) -> *mut c_void;
        fn GetLastError() -> u32;
        fn FreeLibrary(module: HMODULE) -> i32;
    }

    pub unsafe fn load_fixture(path: &Path) -> (HMODULE, unsafe extern "system" fn(TryComplete, i32) -> i32) {
        assert!(path.is_absolute(), "fixture DLL path must be absolute: {}", path.display());
        let wide = path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        unsafe {
            let library = LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            );
            assert!(!library.is_null(), "LoadLibraryExW {} failed: GetLastError={}", path.display(), GetLastError());
            let symbol = GetProcAddress(library, b"RunExternalWaitFixture\0".as_ptr());
            if symbol.is_null() {
                let error = GetLastError();
                FreeLibrary(library);
                panic!("GetProcAddress RunExternalWaitFixture in {} failed: GetLastError={error}", path.display());
            }
            (library, std::mem::transmute::<*mut c_void, unsafe extern "system" fn(TryComplete, i32) -> i32>(symbol))
        }
    }

    pub unsafe fn close_fixture(library: HMODULE) {
        unsafe {
            FreeLibrary(library);
        }
    }
}
