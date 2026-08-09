mod abi;
mod analysis;
mod common;
mod dispatch;
mod host_handlers;
mod language_handlers;
mod symbols;

pub(super) use abi::render_abi_builtins;
pub(super) use analysis::{append_analysis_v5_intrinsics, render_analysis_builtins, render_runtime_handler_specs};
pub(super) use dispatch::render_runtime_dispatch_table;
pub(super) use host_handlers::render_host_handler_table;
pub(super) use language_handlers::render_language_handler_table;
pub(super) use symbols::{
    render_abi_symbols, render_dispatch_lookup, render_dispatch_tags, render_jit_kernel_registration,
    render_link_anchor,
};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::model::DispatchEntry;

    #[test]
    fn legacy_v4_codegen_rejects_the_v5_authority() {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime_manifest.bsol");
        let error = match crate::load_manifest(&manifest_path) {
            Ok(_) => panic!("v5 cannot enter v4 codegen"),
            Err(error) => error,
        };
        assert!(error.contains("unknown field `schema_version`"));
    }

    #[test]
    fn safe_dispatch_returns_do_not_emit_redundant_expression_braces() {
        let entry = DispatchEntry {
            dispatch_key: "fiber_now_millis".to_string(),
            name: "FiberNowMillis".to_string(),
            tag: 0,
            params: Vec::new(),
            returns: "usize".to_string(),
            injected: true,
            beskid_path: Vec::new(),
            owner: "language".to_string(),
            language_handler: false,
        };

        assert_eq!(
            super::dispatch::wrap_dispatch_return(&entry, "usize", "crate::builtins::fiber_now_millis()"),
            "Some(crate::builtins::fiber_now_millis())"
        );
    }

    #[test]
    fn analysis_v5_intrinsics_replace_legacy_generated_section() {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime_manifest.bsol");
        let source = std::fs::read_to_string(manifest_path).expect("read ABI-v5 manifest");
        let runtime = crate::load_v5_manifest_source(&source).expect("load ABI-v5 manifest");
        let base = "define_builtins! {\n// ABI-v5 canonical runtime intrinsic candidates\n    stale\n}\n";

        let generated = super::append_analysis_v5_intrinsics(base, &runtime);

        assert!(!generated.contains("canonical runtime intrinsic candidates"));
        assert_eq!(generated.matches("canonical runtime declarations").count(), 1);
        assert!(!generated.contains("stale"));
    }
}
