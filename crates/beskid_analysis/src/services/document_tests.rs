#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::compilation_context::CompilationContext;
    use crate::projects::{AssemblyDiscovery, AssemblyOptions, ProgramAssembly, assemble_program};
    use crate::services::{
        build_document_analysis_with_context, completion_candidates, definition_at_offset,
        parse_program_with_source_name, references_at_offset_workspace, resolve_input,
    };

    struct CorelibMvpFixture {
        main_path: PathBuf,
        project_root: PathBuf,
        source: String,
    }

    fn compiler_workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("beskid_analysis crate layout")
            .to_path_buf()
    }

    fn corelib_mvp_paths() -> CorelibMvpFixture {
        let main_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/corelib_mvp/Src/Main.bd");
        let source = std::fs::read_to_string(&main_path).expect("read Main.bd");
        let project_root = main_path
            .parent()
            .and_then(|p| p.parent())
            .expect("fixture root")
            .to_path_buf();
        CorelibMvpFixture {
            main_path,
            project_root,
            source,
        }
    }

    fn with_cwd_at_workspace_root<R>(root: &PathBuf, f: impl FnOnce() -> R) -> R {
        super::super::test_support::with_cwd(root.as_path(), f)
    }

    fn assemble_corelib_mvp(
        path: &PathBuf,
        source: &str,
        project_root: &PathBuf,
    ) -> ProgramAssembly {
        let resolved = resolve_input(Some(path), Some(project_root), None, None, false, false)
            .expect("resolve corelib_mvp");
        let plan = resolved.compile_plan.expect("compile plan");
        assemble_program(
            &plan,
            resolved.prepared_workspace.as_ref(),
            path,
            Some(source),
            &AssemblyOptions {
                discovery: AssemblyDiscovery::ImportClosure,
                ..Default::default()
            },
        )
        .expect("assemble")
    }

    fn snapshot_with_manual_assembly() -> (
        crate::services::DocumentAnalysisSnapshot,
        CorelibMvpFixture,
        ProgramAssembly,
    ) {
        let root = compiler_workspace_root();
        with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            let program = parse_program_with_source_name(
                &fixture.main_path.to_string_lossy(),
                &fixture.source,
            )
            .expect("parse Main.bd");
            let assembly =
                assemble_corelib_mvp(&fixture.main_path, &fixture.source, &fixture.project_root);
            let mut ctx = CompilationContext::try_for_analysis_path(&fixture.main_path, None)
                .expect("project context");
            ctx.assembly = Some(assembly.clone());
            let snapshot = build_document_analysis_with_context(
                &program,
                fixture.main_path.to_string_lossy(),
                &fixture.source,
                &fixture.main_path,
                Some(&mut ctx),
                None,
            );
            (snapshot, fixture, assembly)
        })
    }

    fn snapshot_via_lifecycle_context()
    -> (crate::services::DocumentAnalysisSnapshot, CorelibMvpFixture) {
        let root = compiler_workspace_root();
        with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            let program = parse_program_with_source_name(
                &fixture.main_path.to_string_lossy(),
                &fixture.source,
            )
            .expect("parse Main.bd");
            let mut ctx = CompilationContext::try_for_analysis_path(&fixture.main_path, None)
                .expect("project context");
            let snapshot = build_document_analysis_with_context(
                &program,
                fixture.main_path.to_string_lossy(),
                &fixture.source,
                &fixture.main_path,
                Some(&mut ctx),
                None,
            );
            (snapshot, fixture)
        })
    }

    #[test]
    fn corelib_mvp_document_analysis_resolves_io_printline() {
        let (snapshot, _, _) = snapshot_with_manual_assembly();
        let resolution = snapshot
            .resolution
            .as_ref()
            .expect("assembly-backed resolution");
        assert!(
            resolution.module_imports.contains_key("Output"),
            "expected Output import alias: {:?}",
            resolution.module_imports
        );
        assert!(
            resolution.items.iter().any(|item| item.name == "WriteLine"),
            "expected WriteLine in merged items"
        );
    }

    #[test]
    fn corelib_mvp_definition_at_printline_targets_io_module() {
        let (snapshot, fixture, _) = snapshot_with_manual_assembly();
        let offset = fixture.source.find("WriteLine").expect("WriteLine usage");
        let definition = definition_at_offset(&snapshot, offset).expect("definition");
        let def_path = definition.location.path.to_string_lossy();
        assert!(
            def_path.contains("Output") || def_path.contains("System"),
            "expected cross-file definition under Output module, got {def_path}"
        );
    }

    #[test]
    fn corelib_mvp_completion_after_output_dot_includes_writeline() {
        let (snapshot, fixture, _) = snapshot_with_manual_assembly();
        let offset =
            fixture.source.find("    Output.").expect("main Output.") + "    Output.".len();
        let candidates = completion_candidates(&snapshot, &fixture.source, offset);
        assert!(
            candidates.iter().any(|c| c.label == "WriteLine"),
            "expected WriteLine member completion after Output., got {:?}",
            candidates.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn corelib_mvp_use_path_completion_offers_std_segments() {
        let (snapshot, fixture, _) = snapshot_with_manual_assembly();
        let offset = fixture.source.find("use Std.").expect("use Std.") + "use Std.".len();
        let candidates = completion_candidates(&snapshot, &fixture.source, offset);
        let labels: Vec<_> = candidates.iter().map(|c| c.label.as_str()).collect();
        assert!(
            labels
                .iter()
                .any(|label| *label == "System" || label.contains("System")),
            "expected System segment after use Std., got {labels:?}"
        );
    }

    #[test]
    fn corelib_mvp_workspace_references_include_io_definition() {
        let (snapshot, fixture, assembly) = snapshot_with_manual_assembly();
        let offset = fixture.source.find("WriteLine").expect("WriteLine usage");
        let references =
            references_at_offset_workspace(&snapshot, &assembly, &fixture.main_path, offset, true);
        assert!(
            references
                .iter()
                .any(|reference| { reference.location.path.to_string_lossy().contains("Output") }),
            "expected a reference in Output.bd, got {:?}",
            references
                .iter()
                .map(|r| r.location.path.display())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn corelib_mvp_lifecycle_completion_after_output_dot_includes_writeline() {
        let (snapshot, fixture) = snapshot_via_lifecycle_context();
        let offset =
            fixture.source.find("    Output.").expect("main Output.") + "    Output.".len();
        let candidates = completion_candidates(&snapshot, &fixture.source, offset);
        assert!(
            candidates.iter().any(|c| c.label == "WriteLine"),
            "expected WriteLine member completion after Output., got {:?}",
            candidates.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn corelib_mvp_lifecycle_snapshot_without_manual_assembly() {
        let (snapshot, fixture) = snapshot_via_lifecycle_context();
        let resolution = snapshot
            .resolution
            .as_ref()
            .expect("lifecycle assembly-backed resolution");
        assert!(
            resolution.module_imports.contains_key("Output"),
            "expected Output alias without manual assembly seed: {:?}",
            resolution.module_imports
        );
        assert!(
            resolution.items.iter().any(|item| item.name == "WriteLine"),
            "expected WriteLine via assembly_for_entry"
        );
        assert!(
            fixture.main_path.is_file(),
            "fixture entry should exist at {}",
            fixture.main_path.display()
        );
    }

    #[test]
    fn corelib_mvp_document_analysis_without_context_falls_back() {
        let root = compiler_workspace_root();
        with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            let program = parse_program_with_source_name(
                &fixture.main_path.to_string_lossy(),
                &fixture.source,
            )
            .expect("parse");
            let snapshot = build_document_analysis_with_context(
                &program,
                fixture.main_path.to_string_lossy(),
                &fixture.source,
                &fixture.main_path,
                None,
                None,
            );
            let resolution = snapshot.resolution.as_ref();
            if let Some(resolution) = resolution {
                assert!(
                    !resolution.module_imports.contains_key("Output"),
                    "degraded mode should not expose Output alias"
                );
                assert!(
                    !resolution.items.iter().any(|item| item.name == "WriteLine"),
                    "degraded mode should not merge dependency WriteLine"
                );
            }
        });
    }
}
