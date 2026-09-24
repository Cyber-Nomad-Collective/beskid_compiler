use std::{collections::BTreeSet, sync::Arc};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_codegen::cranelift_host::collect_validated_extern_signatures;
use beskid_codegen::{lower_canonical_runtime_prepared_syntax, lower_syntax_assembly_entrypoint};
use beskid_queries::{
    AstNodeId, AstNodeKey, SourceUnitId, SyntaxGenerationId, child_nodes, closure_environment, node_kind, with_db,
};
use cranelift_codegen::{Context, control::ControlPlane, ir::ExternalName, isa, settings, verify_function};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;

#[path = "parsed_project_isle_harness/control_flow.rs"]
mod control_flow;
#[path = "parsed_project_isle_harness/entrypoints.rs"]
mod entrypoints;
#[path = "parsed_project_isle_harness/fibers.rs"]
mod fibers;
#[path = "parsed_project_isle_harness/lambda_spawn.rs"]
mod lambda_spawn;
#[path = "parsed_project_isle_harness/runtime.rs"]
mod runtime;
#[path = "parsed_project_isle_harness/support.rs"]
mod support;

use support::{
    assert_unsupported_closed_failure, lower_verified_entrypoint, parse_production_units, x86_64_target_and_isa,
};

#[test]
fn retired_public_codegen_facade_is_absent() {
    let public_services = include_str!("../src/services.rs");
    for retired_api in [
        "pub struct LoweredProgram",
        "pub fn lower_source",
        "pub fn lower_source_for_entrypoint",
        "pub fn lower_source_with_pipeline",
        "pub fn lower_resolved_input_with_pipeline",
        "pub fn lower_from_prepared_or_cache",
        "pub fn lower_resolved_entrypoint_with_pipeline",
        "pub fn lower_from_front_end",
    ] {
        assert!(
            !public_services.contains(retired_api),
            "retired public codegen facade must not expose `{retired_api}`"
        );
    }
}

#[test]
fn production_path_accepts_only_syntax_program_assembly() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Helper(i32 value) { return value + 1; }
        i32 Main() {
            if Helper(1) > 0 { return Helper(2); }
            return 0;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    // The production boundary is ProgramAssembly-only.
    assert_eq!(std::any::type_name_of_val(assembly.as_ref()), "beskid_analysis::projects::assembly::ProgramAssembly");
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(Arc::clone(&assembly), target.clone(), isa.as_ref());
    assert!(lowered.artifact.functions.len() >= 2, "direct-call closure through syntax ISLE");

    let public_exports = include_str!("../src/lib.rs");
    assert!(
        public_exports.contains("lower_syntax_assembly_entrypoint"),
        "production codegen must expose the syntax-assembly lowering boundary"
    );
    assert!(
        public_exports.contains("lower_prepared_syntax_entrypoint"),
        "production codegen must expose the prepared-syntax lowering boundary"
    );
}

#[test]
fn public_codegen_surface_names_canonical_syntax_lowering_authority() {
    let public_exports = include_str!("../src/lib.rs");
    let public_prepared_syntax = include_str!("../src/prepared_syntax.rs");
    assert!(
        public_exports.contains("lower_prepared_syntax_module"),
        "module hosts must use the canonical prepared-syntax lowering boundary"
    );
    assert!(
        public_prepared_syntax.contains("CodegenInput::new"),
        "prepared-syntax lowering must construct CodegenInput before emitting ISLE"
    );
    assert!(
        public_prepared_syntax.contains("lower_syntax_program"),
        "prepared-syntax lowering must emit through the syntax ISLE authority"
    );
}
