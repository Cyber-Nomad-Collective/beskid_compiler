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
use cranelift_codegen::{Context, control::ControlPlane, ir::ExternalName, settings, verify_function};
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
    assert_legality_failure, assert_unsupported_closed_failure, lower_verified_entrypoint, parse_production_units,
    x86_64_target_and_isa,
};

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
    assert!(lowered.artifact().functions.len() >= 2, "direct-call closure through syntax ISLE");
}
