pub(super) const CORE_ARGS_SERVICES: &str = r#"
corelib_service "__args_count" {
  adapter = "beskid_rt_v5_args_count"
  params = []
  returns = i64
  target_bindings = [
    { target = "x86_64-unknown-linux-gnu", implementation = "beskid_rt_v5_args_count", os_imports = [] },
    { target = "aarch64-apple-darwin", implementation = "beskid_rt_v5_args_count", os_imports = [] },
    { target = "x86_64-pc-windows-msvc", implementation = "beskid_rt_v5_args_count", os_imports = [] }
  ]
}
corelib_service "__args_get" {
  adapter = "beskid_rt_v5_args_get"
  params = [{ name = index, type = i64 }]
  returns = string
  target_bindings = [
    { target = "x86_64-unknown-linux-gnu", implementation = "beskid_rt_v5_args_get", os_imports = [] },
    { target = "aarch64-apple-darwin", implementation = "beskid_rt_v5_args_get", os_imports = [] },
    { target = "x86_64-pc-windows-msvc", implementation = "beskid_rt_v5_args_get", os_imports = [] }
  ]
}
"#;

#[allow(dead_code)]
const MANIFEST: &str = r#"
manifest {
  abi_version = 5
  schema_version = 1
  runtime_publisher = "beskid-lang.org"
  runtime_package = "beskid-runtime-native"
  trap_exit_status = 101
  trap_diagnostic = "beskid runtime trap v5"
}
target "x86_64-unknown-linux-gnu" {
  endianness = little
  pointer_width = 64
  calling_convention = system_v
  object_format = elf
  symbol_prefix = ""
}
target "aarch64-apple-darwin" {
  endianness = little
  pointer_width = 64
  calling_convention = apple_aarch64
  object_format = macho
  symbol_prefix = "_"
}
target "x86_64-pc-windows-msvc" {
  endianness = little
  pointer_width = 64
  calling_convention = windows_x64
  object_format = coff
  symbol_prefix = ""
}
export "beskid_rt_v5_trap" {
  params = [{ name = code, type = u8 }, { name = message, type = pointer }, { name = message_len, type = usize }]
  returns = never
}
intrinsic "pointer_add" {
  symbol = "beskid_rt_v5_intrinsic_pointer_add"
  capability = "runtime.bootstrap.pointer_add"
  params = [{ name = base, type = pointer }, { name = offset, type = usize }]
  returns = pointer
}
trap "null_reference" { code = 1 }
trap "bounds" { code = 2 }
trap "overflow" { code = 3 }
trap "utf8" { code = 4 }
trap "oom" { code = 5 }
trap "handle" { code = 6 }
trap "deadlock" { code = 7 }
trap "abi" { code = 8 }
trap "unreachable" { code = 9 }
trap "corruption" { code = 10 }
assembly "beskid_arch_v5_context_switch" {
  params = [{ name = from, type = pointer }, { name = to, type = pointer }]
  returns = void
  x86_64_unknown_linux_gnu_preserved = [rbx, rbp, r12, r13, r14, r15]
  x86_64_unknown_linux_gnu_locations = [rdi, rsi]
  aarch64_apple_darwin_preserved = [x19, x20, x21, x22, x23, x24, x25, x26, x27, x28, x29, v8, v9, v10, v11, v12, v13, v14, v15]
  aarch64_apple_darwin_locations = [x0, x1]
  x86_64_pc_windows_msvc_preserved = [rbx, rbp, rdi, rsi, r12, r13, r14, r15, xmm6, xmm7, xmm8, xmm9, xmm10, xmm11, xmm12, xmm13, xmm14, xmm15]
  x86_64_pc_windows_msvc_locations = [rcx, rdx]
}
assembly "beskid_arch_v5_context_init" {
  params = [{ name = context, type = pointer }, { name = stack_top, type = pointer }, { name = entry, type = pointer }, { name = argument, type = pointer }, { name = return_trampoline, type = pointer }]
  returns = void
  x86_64_unknown_linux_gnu_preserved = [rbx, rbp, r12, r13, r14, r15]
  x86_64_unknown_linux_gnu_locations = [rdi, rsi, rdx, rcx, r8]
  aarch64_apple_darwin_preserved = [x19, x20, x21, x22, x23, x24, x25, x26, x27, x28, x29, v8, v9, v10, v11, v12, v13, v14, v15]
  aarch64_apple_darwin_locations = [x0, x1, x2, x3, x4]
  x86_64_pc_windows_msvc_preserved = [rbx, rbp, rdi, rsi, r12, r13, r14, r15, xmm6, xmm7, xmm8, xmm9, xmm10, xmm11, xmm12, xmm13, xmm14, xmm15]
  x86_64_pc_windows_msvc_locations = [rcx, rdx, r8, r9, "stack+40"]
}
audit {
  forbidden_symbol_families = [rust, _rust, __rust, "core::panicking", "std::panicking", "alloc::alloc", panic, _Unwind, __Unwind, eh_personality, gcc_personality, abfall, corosensei]
}
"#;
