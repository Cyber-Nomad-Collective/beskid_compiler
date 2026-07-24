//! Shared CLI authority for production syntax-to-ISLE code generation.

use anyhow::Result;
use beskid_abi::abi_v5::TargetMetadata;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::CODEGEN_CLIF};

/// Lower a prepared frontend snapshot through the HIR-free codegen boundary.
///
/// Commands perform their semantic gate before calling this function. Keeping the prepared
/// snapshot intact preserves post-`mod` syntax rewrites and the generation-safe identities used
/// by `TypedProgram`, `CodegenInput`, and ISLE emission.
pub(super) fn lower_prepared_entrypoint(
    front: &beskid_analysis::services::FrontEndTypedResult,
    entrypoint: &str,
    target_triple: Option<&str>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_codegen::CodegenArtifact> {
    let target = resolve_abi_target(target_triple)?;
    observe_phase_result(pipeline, CODEGEN_CLIF, || {
        beskid_engine::services::lower_prepared_syntax_entrypoint(front, entrypoint, target)
    })
}

/// Lower every executable item in a prepared frontend snapshot through the AOT target ISA.
pub(super) fn lower_prepared_module(
    front: &beskid_analysis::services::FrontEndTypedResult,
    target_triple: Option<&str>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<beskid_codegen::CodegenArtifact> {
    let target = resolve_abi_target(target_triple)?;
    observe_phase_result(pipeline, CODEGEN_CLIF, || {
        beskid_aot::lower_prepared_syntax_module(front, target).map_err(anyhow::Error::from)
    })
}

fn resolve_abi_target(target_triple: Option<&str>) -> Result<TargetMetadata> {
    let requested = match target_triple {
        Some(triple) => triple,
        None => {
            return beskid_engine::host_runtime_target()
                .map_err(|error| anyhow::anyhow!("native ABI-v5 target unavailable: {error}"));
        }
    };
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == requested)
        .ok_or_else(|| anyhow::anyhow!("unsupported ABI-v5 target `{requested}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::services::{
        FrontEndOptions, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
    };
    use beskid_queries::compile_front_end_from_resolved_input;

    #[test]
    fn emits_reachable_syntax_items_without_hir_lowering() {
        let directory = std::env::temp_dir().join(format!(
            "beskid_cli_syntax_codegen_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("system clock").as_nanos(),
        ));
        std::fs::create_dir_all(&directory).expect("create test project");
        let path = directory.join("Main.bd");
        let source = "i32 Echo(i32 value) { return value; } i32 Main() { return Echo(41); }";
        std::fs::write(&path, source).expect("write source");
        let plan = synthetic_compile_plan_for_source(&path);
        let resolved: ResolvedInput = resolved_input_from_plan(path.clone(), source.into(), plan, None, None);
        let front = compile_front_end_from_resolved_input(
            &resolved,
            FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
            None,
        )
        .expect("prepare frontend");

        let artifact = lower_prepared_entrypoint(&front, "Main", None, None).expect("syntax entrypoint lowering");

        assert_eq!(artifact.functions.len(), 2);
        assert!(
            artifact.functions.iter().any(|function| function.name.starts_with("Echo#syntax_")),
            "emitted functions: {:?}",
            artifact.functions.iter().map(|function| &function.name).collect::<Vec<_>>(),
        );
        std::fs::remove_dir_all(directory).expect("remove test project");
    }

    #[test]
    fn accepts_each_exact_abi_v5_target_name() {
        for target in TargetMetadata::supported() {
            let resolved = resolve_abi_target(Some(target.triple.as_str())).expect("target");
            assert_eq!(resolved, target);
        }
    }
}
