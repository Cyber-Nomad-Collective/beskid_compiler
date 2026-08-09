pub(in super::super) use std::collections::HashMap;
pub(in super::super) use std::sync::Arc;
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
pub(in super::super) use std::sync::atomic::{AtomicUsize, Ordering};

pub(in super::super) use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
pub(in super::super) use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH, CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH,
    CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH, CANONICAL_BOOTSTRAP_SOURCE_PATH, CANONICAL_CORELIB_ARGS_SOURCE_PATH,
    CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    canonical_corelib_service_capability, canonical_corelib_service_source_path, canonical_corelib_service_sources,
    canonical_corelib_syscall_service_capability, canonical_corelib_syscall_sources,
    canonical_runtime_intrinsic_capability, canonical_runtime_sources,
};
pub(in super::super) use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    SyntaxProgramAssembly,
};
pub(in super::super) use beskid_analysis::services::parse_program_with_source_name;
pub(in super::super) use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
pub(in super::super) use beskid_codegen::{
    CodegenInput, ItemModuleImporter, emit_closure_static_data, emit_isle_expression, emit_isle_item,
    emit_isle_item_with_call_importer,
    module_emission::{SyntaxModuleItem, emit_syntax_program, lower_syntax_program},
};
pub(in super::super) use beskid_isle::callee::DirectCallee;
pub(in super::super) use beskid_isle::{FunctionEmitter, NodeFacts};
pub(in super::super) use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, CastIntent, Db, ProjectSession, SourceUnitId, SyntaxGenerationId,
    aggregate_field_access, build_canonical_corelib_syscall_typed_program, build_canonical_runtime_typed_program,
    build_typed_program, build_typed_program_with_corelib_services, call_abi_signature, call_lowering, child_nodes,
    closure_environment, empty_array_literal_element_abi_type, enum_constructor, enum_layout, enum_match,
    format_ast_node_site, item_body, item_name, literal_fact, mutable_local_assignment, node_kind, node_type,
    spawn_target, test_statement_nodes,
};
pub(in super::super) use cranelift_codegen::ir::{UserFuncName, types};
pub(in super::super) use cranelift_codegen::isa;
pub(in super::super) use cranelift_codegen::settings;
pub(in super::super) use cranelift_jit::{JITBuilder, JITModule};
pub(in super::super) use cranelift_module::{Linkage, Module, default_libcall_names};
