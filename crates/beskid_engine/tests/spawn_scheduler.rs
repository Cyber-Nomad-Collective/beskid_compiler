mod support;

use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::services::{prepare_jit_entrypoint, run_entrypoint};
use beskid_engine::{Engine, host_runtime_target};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use support::runtime_prefix::RuntimePrefixContext;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn append_probe_hex(buffer: &mut [u8], cursor: &mut usize, value: u64) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for shift in (0..16).rev() {
        buffer[*cursor] = HEX[((value >> (shift * 4)) & 0xf) as usize];
        *cursor += 1;
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe extern "C" fn scheduler_probe_sigsegv(
    _signal: libc::c_int,
    _info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    let registers = unsafe { &(*(context.cast::<libc::ucontext_t>())).uc_mcontext.gregs };
    let mut buffer = [0_u8; 256];
    let mut cursor = 0;
    for (label, register) in [
        (b"rip=0x".as_slice(), libc::REG_RIP),
        (b" rsp=0x".as_slice(), libc::REG_RSP),
        (b" rbp=0x".as_slice(), libc::REG_RBP),
        (b" rdi=0x".as_slice(), libc::REG_RDI),
        (b" rsi=0x".as_slice(), libc::REG_RSI),
    ] {
        buffer[cursor..cursor + label.len()].copy_from_slice(label);
        cursor += label.len();
        append_probe_hex(&mut buffer, &mut cursor, registers[register as usize] as u64);
    }
    buffer[cursor] = b'\n';
    cursor += 1;
    let _ = unsafe { libc::write(libc::STDERR_FILENO, buffer.as_ptr().cast(), cursor) };
    unsafe { libc::_exit(139) };
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn install_scheduler_probe() {
    let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = scheduler_probe_sigsegv as usize;
    action.sa_flags = libc::SA_SIGINFO;
    unsafe {
        libc::sigemptyset(&mut action.sa_mask);
        assert_eq!(libc::sigaction(libc::SIGSEGV, &action, std::ptr::null_mut()), 0);
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn install_scheduler_probe() {}

#[test]
fn jit_runs_zero_capture_lambda_spawn_under_fiber_scheduler() {
    let prefix = tempfile::tempdir().expect("exact runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).expect("publish exact native runtime kit");
    let target = host_runtime_target().expect("supported native host target");
    let mut engine =
        Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).expect("load exact native runtime kit");

    let source = "i64 Main() { spawn (() => 42_i64); return 5; }";
    let prepared = prepare_jit_entrypoint(Path::new("spawn_lambda.bd"), source, "Main")
        .expect("syntax-owned lambda spawn must prepare for JIT");
    engine.compile_artifact(&prepared.artifact).expect("syntax-owned lambda spawn must compile in the JIT");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let mut functions = prepared
            .artifact
            .functions
            .iter()
            .map(|function| {
                let pointer = unsafe { engine.entrypoint_ptr(&function.name) }.expect("probe function pointer");
                (pointer as usize, function.name.as_str())
            })
            .collect::<Vec<_>>();
        functions.sort_unstable_by_key(|(address, _)| *address);
        for (index, (start, name)) in functions.iter().enumerate() {
            let end = functions.get(index + 1).map_or(*start, |(address, _)| *address);
            eprintln!("beskid-linux-probe jit=[0x{start:016x},0x{end:016x}) {name}");
        }
        let maps = std::fs::read_to_string("/proc/self/maps").expect("Linux process mappings");
        for mapping in maps.lines().filter(|line| line.contains("r-xp") || line.contains("rwxp")) {
            eprintln!("beskid-linux-probe map {mapping}");
        }
    }
    install_scheduler_probe();
    let pointer = unsafe { engine.entrypoint_ptr(&prepared.symbol) }.expect("Main pointer");
    let main: extern "C" fn() -> i64 = unsafe { std::mem::transmute(pointer) };
    eprintln!("beskid-linux-probe before-main");
    let result = main();
    eprintln!("beskid-linux-probe after-main result={result}");
    assert_eq!(result, 5, "spawned lambda must not corrupt the caller result");
    eprintln!("beskid-linux-probe before-engine-drop");
    drop(engine);
    eprintln!("beskid-linux-probe after-engine-drop");
}

#[test]
fn jit_child_value_returns_42() {
    let prefix = tempfile::tempdir().expect("exact runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).expect("publish exact native runtime kit");
    let _runtime_prefix = RuntimePrefixContext::install(prefix.path());

    let source = "i64 child_value() { return 42; } i64 Main() { return child_value(); }";
    let output = run_entrypoint(Path::new("child_value.bd"), source, "Main").expect("main should run");
    assert_eq!(output, "42", "expected child_value return to round-trip through JIT");
}
