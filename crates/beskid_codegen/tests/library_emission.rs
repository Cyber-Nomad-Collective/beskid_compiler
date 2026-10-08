//! Library outputs own every root unit of an entry-less library: all of their items are emitted,
//! their `[Export]` declarations form the export set, and dependency items are emitted only when an
//! owned item reaches them (once, however many owned units share them).

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::projects::{
    AssemblyRootSet, CompilePlan, ResolvedDependencyProject, Target, TargetKind, plan_entry_path,
};
use beskid_analysis::services::{FrontEndOptions, FrontEndTypedResult, resolved_input_from_plan};
use beskid_codegen::{CodegenArtifact, lower_prepared_syntax_library};
use beskid_queries::{compile_front_end_from_resolved_input, with_db};
use cranelift_codegen::{isa, settings};

const ALPHA: &str = "use Shared;\n\
    [Export(Abi:\"C\", Symbol:\"alpha_answer\")]\n\
    pub i32 AlphaAnswer() { return Shared.Help(); }\n";
const BETA: &str = "use Shared;\n\
    [Export(Abi:\"C\", Symbol:\"beta_answer\")]\n\
    pub i32 BetaAnswer() { return Shared.Help(); }\n\
    pub i32 BetaLocal() { return 2; }\n";
const SHARED: &str = "pub i32 Help() { return 41; }\n\
    pub i32 Unused() { return 0; }\n\
    [Export(Abi:\"C\", Symbol:\"dependency_export\")]\n\
    pub i32 DependencyExport() { return 1; }\n";

fn entry_less_library_front(root: &std::path::Path) -> FrontEndTypedResult {
    let source_root = root.join("src");
    let dependency_root = root.join("deps").join("shared");
    std::fs::create_dir_all(&source_root).expect("host source root");
    std::fs::create_dir_all(dependency_root.join("src")).expect("dependency source root");
    std::fs::write(source_root.join("Alpha.bd"), ALPHA).expect("Alpha");
    std::fs::write(source_root.join("Beta.bd"), BETA).expect("Beta");
    std::fs::write(dependency_root.join("src").join("Shared.bd"), SHARED).expect("Shared");
    std::fs::write(
        dependency_root.join("shared.bproj"),
        "shared {\n  name = \"shared\"\n  version = \"0.1.0\"\n  root = \"src\"\n}\n\n\
         target \"SharedLib\" {\n  kind = Lib\n}\n",
    )
    .expect("shared manifest");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: root.to_path_buf(),
        manifest_path: root.join("fixture.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Library".to_string(), kind: TargetKind::Lib, entry: None },
        dependency_projects: vec![ResolvedDependencyProject {
            dependency_name: "shared".to_string(),
            manifest_path: dependency_root.join("shared.bproj"),
            project_root: dependency_root.clone(),
            project_name: "shared".to_string(),
            source_root: dependency_root.join("src"),
        }],
        unresolved_dependencies: Vec::new(),
        has_core_dependency: false,
    };
    let entry_path = plan_entry_path(&plan, &source_root);
    let resolved = resolved_input_from_plan(entry_path, String::new(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .expect("prepare entry-less library frontend");
    assert!(
        matches!(front.assembly.root_set, AssemblyRootSet::OwnUnits(ref paths) if paths.len() == 2),
        "both own units are roots: {:?}",
        front.assembly.root_set
    );
    front
}

fn lower_library(front: &FrontEndTypedResult) -> CodegenArtifact {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str().starts_with("x86_64-"))
        .expect("x86_64 ABI target");
    let isa = isa::lookup_by_name("x86_64")
        .expect("x86 ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("finish ISA");
    with_db(|db| lower_prepared_syntax_library(db, front, target, isa.as_ref())).expect("library lowering")
}

fn emitted_with_prefix(artifact: &CodegenArtifact, prefix: &str) -> usize {
    artifact.functions.iter().filter(|function| function.name.starts_with(prefix)).count()
}

#[test]
fn entry_less_library_exports_every_own_unit_and_emits_shared_dependencies_once() {
    let root = std::env::temp_dir().join(format!("beskid_codegen_library_emission_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let front = entry_less_library_front(&root);
    let artifact = lower_library(&front);
    let names = artifact.functions.iter().map(|function| function.name.clone()).collect::<Vec<_>>();

    for owned in ["AlphaAnswer#", "BetaAnswer#", "BetaLocal#"] {
        assert_eq!(emitted_with_prefix(&artifact, owned), 1, "own item `{owned}` emitted once: {names:?}");
    }
    assert_eq!(emitted_with_prefix(&artifact, "Help#"), 1, "shared dependency emitted once: {names:?}");
    for unreached in ["Unused#", "DependencyExport#"] {
        assert_eq!(emitted_with_prefix(&artifact, unreached), 0, "unreached dependency item `{unreached}`: {names:?}");
    }
    let exports = artifact.exports.iter().map(|entry| entry.exported_symbol.as_str()).collect::<Vec<_>>();
    assert_eq!(exports, ["alpha_answer", "beta_answer"], "own exports in assembly order, no dependency export");

    let again = lower_library(&front);
    assert_eq!(
        again.functions.iter().map(|function| function.name.clone()).collect::<Vec<_>>(),
        names,
        "library emission order is deterministic"
    );
    assert_eq!(again.exports, artifact.exports);
    beskid_analysis::services::invalidate_entry_sessions_for_project(&root);
    let _ = std::fs::remove_dir_all(&root);
}
