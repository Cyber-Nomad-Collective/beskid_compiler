use beskid_codegen::{ExternImport, artifact::BrowserCoverageFunction};
use std::collections::HashMap;

pub(super) fn unavailable(
    operation: &str,
    caller: &str,
    origins: &HashMap<String, BrowserCoverageFunction>,
    externs: &[ExternImport],
) -> String {
    let location = origins.get(caller).map(|origin| {
        let path = origin.path.display();
        match &origin.span {
            Some(span) => format!(" at {path}:{span}"),
            None => format!(" at {path}"),
        }
    }).unwrap_or_default();
    let caller = caller.split('#').next().unwrap_or(caller);
    let native_library = externs.iter().find(|entry| entry.symbol == operation)
        .and_then(|entry| entry.library.as_deref());
    let reason = match (operation, native_library) {
        ("process_getpid", _) => "the browser does not expose an operating-system process ID".to_owned(),
        ("process_exit", _) => "a Wasm instance cannot terminate the browser process".to_owned(),
        ("beskid_rt_v5_thread_yield" | "pthread_create" | "pthread_join", _) =>
            "the browser profile has no native pthreads; use Beskid's cooperative concurrency API".to_owned(),
        (service, None) if (service.starts_with("isolate_") || service.starts_with("port_"))
            && beskid_abi::runtime_source::canonical_corelib_service_abi_for_adapter(service).is_some() =>
            "Core.Parallel isolates and ports require native workers and cross-heap transit; use Concurrency fibers and channels in this browser profile".to_owned(),
        (_, Some(library)) => format!("a browser cannot load native library {library}"),
        _ => "the browser profile has no binding for this operation".to_owned(),
    };
    format!("browser Wasm target cannot execute {operation} in {caller}{location}: {reason}")
}
