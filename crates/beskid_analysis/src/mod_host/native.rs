//! Native mod artifact dispatch (dlopen + ABI marshaling).
//!
//! Mod AOT artifacts currently emit relocatable object files (`.o`), not shared
//! libraries. This invoker attempts to load each object file as a shared library:
//! relocatable `.o` / `.obj` files are first linked into a temporary `.dylib` /
//! `.so` via the system linker (`cc -shared`); other files are opened directly
//! with [`libloading`].
//!
//! Once a library is loaded, the invoker looks up the registration's
//! `entry_symbol` and calls it with the marshaled C-layout request. Results are
//! unmarshaled back into Rust outcomes for `Collector`, `Analyzer`, and
//! `Rewriter`. `Generator` results carry opaque host-owned node handles that
//! require emit-bridge materialization not wired in this slice, so generator
//! dispatch falls back to the inner stub.
//!
//! The dlopen path is the production target. It gracefully falls back to
//! [`StubContractInvoker`] dispatch whenever:
//! 1. the object file is missing on disk,
//! 2. a relocatable `.o` cannot be linked into a shared library,
//! 3. the `entry_symbol` is not exported by any loaded library,
//! 4. the native call returns a null pointer.
//!
//! This keeps scheduling and descriptor wiring testable today while the AOT
//! pipeline learns to emit loadable dylibs.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use libloading::Library;
use tracing::debug;

use beskid_abi::{
    BeskidStr, ModAnalysisRequest, ModCollectRequest, ModCollectTargetSet, ModGenerationRequest, ModStrSlice,
    mod_contract::{
        ModAnalysisResult, ModAnalyzerEntryFn, ModCollectorEntryFn, ModDiagnostic, ModDiagnosticSlice, ModEdit,
        ModEditSlice, ModQuickFix, ModQuickFixSlice, ModRewriteResult, ModRewriterEntryFn,
    },
};

use super::invoker::{
    AnalyzerDiagnostic, AnalyzerFix, AnalyzerOutcome, AnalyzerSeverity, CollectorOutcome, ContractInvocationError,
    ContractInvoker, GeneratorOutcome, RewriteEdit, RewriterOutcome, StubContractInvoker,
};
use super::types::ContractRegistration;

#[derive(Debug)]
pub struct NativeContractInvoker {
    pub object_paths: Vec<PathBuf>,
    inner: StubContractInvoker,
    /// Successfully loaded shared libraries, kept alive for the invoker's lifetime.
    libraries: Mutex<Vec<Library>>,
    /// Object paths we have already attempted to load (avoids re-attempting per call).
    attempted: Mutex<Vec<PathBuf>>,
}

impl NativeContractInvoker {
    pub fn new(object_paths: Vec<PathBuf>) -> Self {
        Self {
            object_paths,
            inner: StubContractInvoker::new(),
            libraries: Mutex::new(Vec::new()),
            attempted: Mutex::new(Vec::new()),
        }
    }

    pub fn invocations(&self) -> Vec<super::invoker::InvocationKind> {
        self.inner.invocations()
    }

    /// Load every object path we have not yet attempted. Successfully loaded
    /// libraries are retained in `libraries` for symbol lookup during dispatch.
    fn ensure_loaded(&self) {
        let mut attempted = self.attempted.lock().expect("native invoker attempted lock");
        let mut libraries = self.libraries.lock().expect("native invoker library lock");
        for path in &self.object_paths {
            if attempted.iter().any(|seen| seen == path) {
                continue;
            }
            attempted.push(path.clone());
            if !path.is_file() {
                debug!(object = %path.display(), "mod native invoker: object file missing; stub dispatch");
                continue;
            }
            match load_library(path) {
                Ok(library) => {
                    debug!(object = %path.display(), "mod native invoker: loaded shared library");
                    libraries.push(library);
                }
                Err(err) => {
                    debug!(object = %path.display(), error = %err, "mod native invoker: load failed; stub dispatch");
                }
            }
        }
    }

    fn libraries(&self) -> std::sync::MutexGuard<'_, Vec<Library>> {
        self.libraries.lock().expect("native invoker library lock")
    }
}

impl ContractInvoker for NativeContractInvoker {
    fn invoke_collector(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
    ) -> Result<CollectorOutcome, ContractInvocationError> {
        self.ensure_loaded();
        let libraries = self.libraries();
        let request_ptr = request as *const ModCollectRequest;
        for library in libraries.iter() {
            let symbol = unsafe { library.get::<ModCollectorEntryFn>(registration.entry_symbol.as_bytes()) };
            let Ok(entry) = symbol else { continue };
            let result_ptr = unsafe { (*entry)(request_ptr) };
            if result_ptr.is_null() {
                debug!(
                    symbol = %registration.entry_symbol,
                    "mod native invoker: collector entry returned null; stub dispatch"
                );
                continue;
            }
            let target_set: &ModCollectTargetSet = unsafe { &*result_ptr };
            let narrowed_targets = unmarshal_str_slice(&target_set.target_ids);
            return Ok(CollectorOutcome { type_id: registration.type_id.clone(), narrowed_targets });
        }
        drop(libraries);
        self.inner.invoke_collector(registration, request)
    }

    fn invoke_generator(
        &self,
        registration: &ContractRegistration,
        request: &ModGenerationRequest,
    ) -> Result<GeneratorOutcome, ContractInvocationError> {
        // Generator results carry opaque `ModSyntaxNodeHandle`s that require
        // emit-bridge materialization not wired in this slice. We still ensure the
        // library loads (so descriptor wiring stays testable) but delegate the
        // outcome to the stub until node unmarshaling lands.
        self.ensure_loaded();
        self.inner.invoke_generator(registration, request)
    }

    fn invoke_analyzer(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
        snapshot: Option<&crate::services::SemanticSnapshot>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError> {
        self.ensure_loaded();
        let libraries = self.libraries();
        let analysis_request =
            ModAnalysisRequest { context: *request, semantic: beskid_abi::ModSemanticHandle::null() };
        let request_ptr = &analysis_request as *const ModAnalysisRequest;
        for library in libraries.iter() {
            let symbol = unsafe { library.get::<ModAnalyzerEntryFn>(registration.entry_symbol.as_bytes()) };
            let Ok(entry) = symbol else { continue };
            let result_ptr = unsafe { (*entry)(request_ptr) };
            if result_ptr.is_null() {
                debug!(
                    symbol = %registration.entry_symbol,
                    "mod native invoker: analyzer entry returned null; stub dispatch"
                );
                continue;
            }
            let result: &ModAnalysisResult = unsafe { &*result_ptr };
            let diagnostics = unmarshal_diagnostics(&result.diagnostics);
            let fixes = unmarshal_fixes(&result.fixes, diagnostics.len());
            return Ok(AnalyzerOutcome { type_id: registration.type_id.clone(), diagnostics, fixes });
        }
        drop(libraries);
        self.inner.invoke_analyzer(registration, request, snapshot)
    }

    fn invoke_rewriter(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
    ) -> Result<RewriterOutcome, ContractInvocationError> {
        self.ensure_loaded();
        let libraries = self.libraries();
        let request_ptr = request as *const ModCollectRequest;
        for library in libraries.iter() {
            let symbol = unsafe { library.get::<ModRewriterEntryFn>(registration.entry_symbol.as_bytes()) };
            let Ok(entry) = symbol else { continue };
            let result_ptr = unsafe { (*entry)(request_ptr) };
            if result_ptr.is_null() {
                debug!(
                    symbol = %registration.entry_symbol,
                    "mod native invoker: rewriter entry returned null; stub dispatch"
                );
                continue;
            }
            let result: &ModRewriteResult = unsafe { &*result_ptr };
            let edits = unmarshal_edits(&result.edits);
            let applied_fix_count = edits.len().min(u32::MAX as usize) as u32;
            return Ok(RewriterOutcome { type_id: registration.type_id.clone(), applied_fix_count, edits });
        }
        drop(libraries);
        self.inner.invoke_rewriter(registration, request)
    }
}

/// Load an object file as a shared library. Relocatable `.o` / `.obj` files are
/// first linked into a temporary `.dylib` (macOS) / `.so` (other platforms) via
/// the system linker. Returns the loaded [`Library`] or an error describing the
/// failure.
fn load_library(path: &Path) -> Result<Library, String> {
    if is_relocatable_object(path) {
        let shared = link_relocatable_to_shared(path)?;
        unsafe { Library::new(&shared) }.map_err(|err| format!("dlopen `{}` failed: {err}", shared.display()))
    } else {
        unsafe { Library::new(path) }.map_err(|err| format!("dlopen `{}` failed: {err}", path.display()))
    }
}

/// Link a relocatable object file into a temporary shared library using
/// `cc -shared`. Returns the path to the temporary shared library.
fn link_relocatable_to_shared(object: &Path) -> Result<PathBuf, String> {
    let suffix = if cfg!(target_os = "macos") { "dylib" } else { "so" };
    let id = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let out = std::env::temp_dir().join(format!("beskid_mod_native_{id}.{suffix}"));
    let status = std::process::Command::new("cc")
        .arg("-shared")
        .arg("-o")
        .arg(&out)
        .arg(object)
        .status()
        .map_err(|err| format!("failed to spawn linker for `{}`: {err}", object.display()))?;
    if !status.success() {
        return Err(format!("linker exited {status} for `{}`", object.display()));
    }
    Ok(out)
}

fn is_relocatable_object(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| matches!(ext, "o" | "obj"))
}

/// Read a [`BeskidStr`] into an owned [`String`]. Null/empty views decode to an
/// empty string; invalid UTF-8 is lossily replaced.
fn beskid_str_to_string(view: &BeskidStr) -> String {
    if view.ptr.is_null() || view.len == 0 {
        return String::new();
    }
    unsafe {
        let bytes = std::slice::from_raw_parts(view.ptr, view.len);
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Unmarshal a C string slice into owned Rust strings.
fn unmarshal_str_slice(slice: &ModStrSlice) -> Vec<String> {
    if slice.items.is_null() || slice.len == 0 {
        return Vec::new();
    }
    unsafe {
        let items = std::slice::from_raw_parts(slice.items, slice.len);
        items.iter().map(beskid_str_to_string).collect()
    }
}

/// Unmarshal native analyzer diagnostics into [`AnalyzerDiagnostic`]s.
fn unmarshal_diagnostics(slice: &ModDiagnosticSlice) -> Vec<AnalyzerDiagnostic> {
    if slice.items.is_null() || slice.len == 0 {
        return Vec::new();
    }
    unsafe {
        let items: &[ModDiagnostic] = std::slice::from_raw_parts(slice.items, slice.len);
        items
            .iter()
            .map(|diagnostic| {
                let severity = match diagnostic.severity {
                    0 => AnalyzerSeverity::Error,
                    1 => AnalyzerSeverity::Warning,
                    _ => AnalyzerSeverity::Note,
                };
                // A native span of (0, 0) means "no span"; surface as `None` so the
                // host falls back to a whole-file span.
                let span = if diagnostic.span_start == 0 && diagnostic.span_end == 0 {
                    None
                } else {
                    Some((diagnostic.span_start as usize, diagnostic.span_end as usize))
                };
                AnalyzerDiagnostic {
                    code: beskid_str_to_string(&diagnostic.code),
                    message: beskid_str_to_string(&diagnostic.message),
                    severity,
                    span,
                }
            })
            .collect()
    }
}

/// Unmarshal native analyzer quick-fixes into [`AnalyzerFix`]es.
///
/// `diagnostics_len` is the length of the diagnostics slice returned alongside the fixes.
/// Fixes whose `diagnostic_index` is out of range are dropped (fail-closed: a mod that
/// emits a bad index loses the fix, not the build). Edit unmarshaling reuses
/// [`unmarshal_edits`] since `ModQuickFix.edits` is the same `ModEditSlice` shape.
fn unmarshal_fixes(slice: &ModQuickFixSlice, diagnostics_len: usize) -> Vec<AnalyzerFix> {
    if slice.items.is_null() || slice.len == 0 {
        return Vec::new();
    }
    unsafe {
        let items: &[ModQuickFix] = std::slice::from_raw_parts(slice.items, slice.len);
        items
            .iter()
            .filter_map(|fix| {
                // Bounds-check the diagnostic link; drop out-of-range fixes (fail-closed).
                if (fix.diagnostic_index as usize) >= diagnostics_len {
                    debug!(
                        diagnostic_index = fix.diagnostic_index,
                        diagnostics_len, "mod native invoker: dropping quick-fix with out-of-range diagnostic_index"
                    );
                    return None;
                }
                Some(AnalyzerFix {
                    diagnostic_index: fix.diagnostic_index,
                    title: beskid_str_to_string(&fix.title),
                    edits: unmarshal_edits(&fix.edits),
                })
            })
            .collect()
    }
}

/// Unmarshal native rewriter edits into [`RewriteEdit`]s.
fn unmarshal_edits(slice: &ModEditSlice) -> Vec<RewriteEdit> {
    if slice.items.is_null() || slice.len == 0 {
        return Vec::new();
    }
    unsafe {
        let items: &[ModEdit] = std::slice::from_raw_parts(slice.items, slice.len);
        items
            .iter()
            .map(|edit| {
                let text = beskid_str_to_string(&edit.text);
                match edit.kind {
                    0 => RewriteEdit::Insert { offset: edit.start as usize, text },
                    1 => RewriteEdit::Replace { start: edit.start as usize, end: edit.end as usize, text },
                    _ => RewriteEdit::Delete { start: edit.start as usize, end: edit.end as usize },
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_object_files_fall_back_to_stub() {
        let invoker = NativeContractInvoker::new(vec![PathBuf::from("/nonexistent/beskid_mod.o")]);
        let registration = ContractRegistration {
            contract_id: "Beskid.Compiler.Collect.Collector".to_owned(),
            type_id: "T".to_owned(),
            entry_symbol: "missing".to_owned(),
        };
        let request = ModInvocationContext::empty_collect_request_for_test();
        let outcome = invoker.invoke_collector(&registration, &request).expect("collector fallback");
        assert_eq!(outcome.type_id, "T");
        assert!(outcome.narrowed_targets.is_empty());
        // Stub recorded the invocation.
        assert_eq!(invoker.invocations().len(), 1);
    }

    #[test]
    fn beskid_str_to_string_handles_null_and_empty() {
        assert_eq!(beskid_str_to_string(&BeskidStr { ptr: std::ptr::null(), len: 0 }), "");
        let bytes = b"hello";
        assert_eq!(beskid_str_to_string(&BeskidStr { ptr: bytes.as_ptr(), len: bytes.len() }), "hello");
    }

    #[test]
    fn unmarshal_str_slice_handles_null_and_empty() {
        assert!(unmarshal_str_slice(&ModStrSlice { items: std::ptr::null(), len: 0 }).is_empty());
    }

    #[test]
    fn unmarshal_diagnostics_handles_null_and_empty() {
        assert!(unmarshal_diagnostics(&ModDiagnosticSlice { items: std::ptr::null(), len: 0 }).is_empty());
    }

    #[test]
    fn unmarshal_edits_handles_null_and_empty() {
        assert!(unmarshal_edits(&ModEditSlice { items: std::ptr::null(), len: 0 }).is_empty());
    }

    #[test]
    fn unmarshal_fixes_drops_out_of_range_diagnostic_index() {
        // One fix linking to diagnostic 0 (in range) and one linking to diagnostic 5 (out of range).
        let empty_str = BeskidStr { ptr: c"".as_ptr() as *const u8, len: 0 };
        let edits = ModEditSlice { items: std::ptr::null(), len: 0 };
        let fixes = [
            ModQuickFix { diagnostic_index: 0, title: empty_str, edits },
            ModQuickFix { diagnostic_index: 5, title: empty_str, edits },
        ];
        let slice = ModQuickFixSlice { items: fixes.as_ptr(), len: fixes.len() };
        // diagnostics_len = 1 → only the first fix (index 0) survives.
        let out = unmarshal_fixes(&slice, 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].diagnostic_index, 0);
    }

    #[test]
    fn unmarshal_fixes_handles_null_and_empty() {
        assert!(unmarshal_fixes(&ModQuickFixSlice { items: std::ptr::null(), len: 0 }, 0).is_empty());
    }

    /// Minimal helper to build an empty `ModCollectRequest` for native invoker tests
    /// without depending on `ModInvocationContext::build` (which needs a `ModHostInput`).
    struct ModInvocationContext;
    impl ModInvocationContext {
        fn empty_collect_request_for_test() -> ModCollectRequest {
            use beskid_abi::{ModCatalog, ModCompilation, ModWorkspace};
            let empty_str = BeskidStr { ptr: c"".as_ptr() as *const u8, len: 0 };
            ModCollectRequest {
                compilation: ModCompilation {
                    active_project_name: empty_str,
                    active_project_root: empty_str,
                    target_triple: empty_str,
                    syntax_generation_id: 0,
                    entry_source_path: empty_str,
                    entry_source_name: empty_str,
                    entry_source_text: empty_str,
                },
                workspace: ModWorkspace {
                    root_path: empty_str,
                    members: beskid_abi::ModWorkspaceMemberSlice { items: std::ptr::null(), len: 0 },
                    lock_hash: empty_str,
                },
                mods: ModCatalog { packages: beskid_abi::ModPackageSlice { items: std::ptr::null(), len: 0 } },
            }
        }
    }
}
