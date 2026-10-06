//! AOT library lowering owns the entry unit and every unit of its own package that it imports:
//! their `[Export]` declarations are exported even when the entry does not call them.

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::projects::{CompilePlan, Target, TargetKind};
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan};
use beskid_aot::lower_prepared_syntax_library;
use beskid_queries::compile_front_end_from_resolved_input;

#[test]
fn library_with_entry_exports_its_own_imported_units() {
    let root = std::env::temp_dir().join(format!("beskid_aot_library_emission_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source_root = root.join("src");
    std::fs::create_dir_all(&source_root).expect("source root");
    let entry_source = "use Extra;\n\
        [Export(Abi:\"C\", Symbol:\"lib_entry\")]\n\
        pub i32 EntryAnswer() { return Extra.Value(); }\n";
    std::fs::write(source_root.join("Lib.bd"), entry_source).expect("Lib");
    std::fs::write(
        source_root.join("Extra.bd"),
        "[Export(Abi:\"C\", Symbol:\"extra_export\")]\n\
         pub i32 ExtraExport() { return 3; }\n\
         pub i32 Value() { return 4; }\n",
    )
    .expect("Extra");
    let plan = CompilePlan {
        source_root: source_root.clone(),
        project_root: root.clone(),
        manifest_path: root.join("fixture.bproj"),
        project_name: "fixture".to_string(),
        target: Target { name: "Library".to_string(), kind: TargetKind::Lib, entry: Some("Lib.bd".to_string()) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let resolved = resolved_input_from_plan(source_root.join("Lib.bd"), entry_source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .expect("prepare library frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str().starts_with("x86_64-"))
        .expect("x86_64 ABI target");

    let artifact = lower_prepared_syntax_library(&front, target).expect("AOT library lowering");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    for owned in ["EntryAnswer#", "ExtraExport#", "Value#"] {
        assert_eq!(
            names.iter().filter(|name| name.starts_with(owned)).count(),
            1,
            "own item `{owned}` emitted once: {names:?}"
        );
    }
    let exports = artifact.exports.iter().map(|entry| entry.exported_symbol.as_str()).collect::<Vec<_>>();
    assert_eq!(exports, ["lib_entry", "extra_export"], "entry unit first, then its own imported unit");
    beskid_analysis::services::invalidate_entry_sessions_for_project(&root);
    let _ = std::fs::remove_dir_all(&root);
}
