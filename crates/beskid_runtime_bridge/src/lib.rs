//! Static library bridge exporting stable Beskid runtime symbols for AOT linking.

// Each `as usize` coercion below forces the linker to resolve the symbol.
// Suppress unused-imports at the crate level because the imports serve as
// link-time anchors only — they are not called directly from this crate.
#![allow(unused_imports)]

mod generated;

use generated::link_anchor::anchor_kernel_exports;

#[unsafe(no_mangle)]
pub extern "C" fn beskid_runtime_link_anchor() {
    beskid_runtime::gc::enable_aot_main_bootstrap();
    anchor_kernel_exports();
    #[cfg(feature = "host")]
    {
        let _ = beskid_host::beskid_host_register_all();
    }
    #[cfg(feature = "language_handlers")]
    {
        let _ = beskid_runtime_handlers::beskid_language_register_all();
    }
}
