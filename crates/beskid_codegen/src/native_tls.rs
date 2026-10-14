//! Per-thread pointer slots for generated native C.
//!
//! Generated C that ends up in a Windows DLL (Mod adapters, Glue wrappers) links `/NOENTRY`
//! against import libraries only. Compiler-implicit thread-local storage (`_Thread_local`,
//! `__declspec(thread)`) would need the static CRT's `_tls_index`, which such a DLL cannot
//! resolve. On Windows a slot therefore lives in one kernel32 TLS index, the CRT-independent
//! pattern the runtime provenance audit requires; other targets keep `_Thread_local`.
//!
//! The kernel32 prototypes match `<windows.h>` exactly (DWORD, LPVOID, BOOL, WINAPI,
//! dllimport), so a translation unit that also includes it still compiles. The index is
//! published with a C11 compare-and-swap; a thread that loses the race frees its own index.

use std::fmt::Write;

/// Emit `static <ty> <name>_get(void)` and `static int <name>_set(<ty>)` for one per-thread
/// pointer slot. `<name>_set` returns 0 when no TLS index can be allocated, so callers fail
/// closed. `name` and `ty` are compiler-chosen C tokens, never user input.
pub(crate) fn emit_thread_slot(name: &str, ty: &str, output: &mut String) {
    debug_assert!(name.bytes().all(|byte| byte == b'_' || byte.is_ascii_alphanumeric()));
    let _ = write!(
        output,
        r#"#if defined(_WIN32)
#include <stdatomic.h>
__declspec(dllimport) unsigned long __stdcall TlsAlloc(void);
__declspec(dllimport) int __stdcall TlsFree(unsigned long);
__declspec(dllimport) void *__stdcall TlsGetValue(unsigned long);
__declspec(dllimport) int __stdcall TlsSetValue(unsigned long,void*);
static _Atomic unsigned long {name}_index=0xFFFFFFFFul; /* TLS_OUT_OF_INDEXES: not yet allocated */
static int {name}_ready(unsigned long *index) {{
 unsigned long current=atomic_load(&{name}_index);
 if(current==0xFFFFFFFFul) {{
  unsigned long fresh=TlsAlloc();
  if(fresh==0xFFFFFFFFul) return 0;
  if(atomic_compare_exchange_strong(&{name}_index,&current,fresh)) current=fresh;
  else TlsFree(fresh);
 }}
 *index=current;
 return 1;
}}
static {ty} {name}_get(void) {{ unsigned long index; return {name}_ready(&index) ? ({ty})TlsGetValue(index) : NULL; }}
static int {name}_set({ty} value) {{ unsigned long index; return {name}_ready(&index) && TlsSetValue(index,(void*)value); }}
#else
static _Thread_local {ty} {name}_value;
static {ty} {name}_get(void) {{ return {name}_value; }}
static int {name}_set({ty} value) {{ {name}_value=value; return 1; }}
#endif
"#
    );
}

#[cfg(test)]
mod tests {
    use super::emit_thread_slot;

    fn slot_source() -> String {
        let mut source = String::from("#include <stddef.h>\ntypedef struct Scope Scope;\n");
        emit_thread_slot("scope_slot", "Scope *", &mut source);
        source.push_str("int probe(Scope *s) { if(scope_slot_get()) return 0; return scope_slot_set(s); }\n");
        source
    }

    #[test]
    fn windows_slot_uses_kernel32_tls_and_never_implicit_tls() {
        let source = slot_source();
        let windows = &source[..source.find("#else").unwrap()];
        assert!(windows.contains("TlsAlloc()") && windows.contains("TlsSetValue(index,(void*)value)"), "{source}");
        assert!(!windows.contains("_Thread_local") && !source.contains("__declspec(thread)"), "{source}");
    }

    #[test]
    fn emitted_slot_is_valid_c11_on_both_branches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slot.c");
        std::fs::write(&path, slot_source()).unwrap();
        for extra in [&[][..], &["-fms-extensions", "-D_WIN32", "-Wno-ignored-attributes"][..]] {
            let output = std::process::Command::new("clang")
                .args(["-std=c11", "-Wall", "-Wextra", "-fsyntax-only"])
                .args(extra)
                .arg(&path)
                .output()
                .expect("clang required to verify generated thread slot");
            assert!(output.status.success(), "{extra:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
