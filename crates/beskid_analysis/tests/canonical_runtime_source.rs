use beskid_abi::runtime_source::canonical_runtime_sources;
use beskid_analysis::services::parse_program_with_source_name;

#[test]
fn canonical_runtime_sources_enter_the_expanded_ast_pipeline_without_a_host_stub() {
    for unit in canonical_runtime_sources() {
        let program = parse_program_with_source_name(&unit.logical_path, &unit.source)
            .expect("canonical runtime source must parse through the compiler AST frontend");
        assert!(!program.node.items.is_empty());
    }
}
