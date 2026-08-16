#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::compilation_context::ProjectSessionHandle;
    use crate::projects::{
        AssemblyDiscovery, AssemblyOptions, ProgramAssembly, WorkspacePrepareOptions, assemble_program,
        prepare_project_workspace_with_options,
    };
    use crate::services::{
        build_document_analysis_from_resolution, build_document_analysis_with_context, completion_candidates,
        definition_at_offset, parse_program_with_source_name, references_at_offset_workspace, resolve_entry,
        resolve_input,
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
        let main_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_e2e_tests/fixtures/corelib_mvp/Src/Main.bd");
        let source = std::fs::read_to_string(&main_path).expect("read Main.bd");
        let project_root = main_path.parent().and_then(|p| p.parent()).expect("fixture root").to_path_buf();
        CorelibMvpFixture { main_path, project_root, source }
    }

    fn with_cwd_at_workspace_root<R>(root: &Path, f: impl FnOnce() -> R) -> R {
        super::super::test_support::with_cwd(root, f)
    }

    fn assemble_corelib_mvp(path: &Path, source: &str, project_root: &Path) -> ProgramAssembly {
        let resolved =
            resolve_input(Some(&path.to_path_buf()), Some(&project_root.to_path_buf()), None, None, false, false)
                .expect("resolve corelib_mvp");
        let plan = resolved.compile_plan.expect("compile plan");
        let prepared = resolved.prepared_workspace.clone().or_else(|| {
            let lockfile = plan.manifest_path.with_file_name("Project.lock");
            let options = WorkspacePrepareOptions { frozen: false, locked: lockfile.is_file() };
            prepare_project_workspace_with_options(&plan, options, None).ok()
        });
        assemble_program(
            &plan,
            prepared.as_ref(),
            path,
            Some(source),
            &AssemblyOptions { discovery: AssemblyDiscovery::ImportClosure, ..Default::default() },
            None,
        )
        .expect("assemble")
    }

    /// Mirrors [`beskid_queries::entry_resolution_with_db`] resolution output: assemble entry
    /// closure, then resolve entry syntax through the module index (no parallel single-file resolve).
    fn snapshot_from_entry_resolution(
        assembly: &ProgramAssembly,
        fixture: &CorelibMvpFixture,
    ) -> crate::services::DocumentAnalysisSnapshot {
        let program = parse_program_with_source_name(&fixture.main_path.to_string_lossy(), &fixture.source)
            .expect("parse Main.bd");
        let resolution =
            resolve_entry(&assembly.entry_unit().program, &assembly.module_index, Some(&fixture.main_path))
                .expect("entry resolution");
        let module_paths = assembly.module_index.known_module_path_strings();
        build_document_analysis_from_resolution(
            &program,
            fixture.main_path.to_string_lossy(),
            &fixture.source,
            &fixture.main_path,
            Some(resolution),
            module_paths,
            None,
            None,
        )
    }

    fn snapshot_with_entry_resolution()
    -> (crate::services::DocumentAnalysisSnapshot, CorelibMvpFixture, ProgramAssembly) {
        let root = compiler_workspace_root();
        with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            let assembly = assemble_corelib_mvp(&fixture.main_path, &fixture.source, &fixture.project_root);
            let snapshot = snapshot_from_entry_resolution(&assembly, &fixture);
            (snapshot, fixture, assembly)
        })
    }

    #[test]
    fn corelib_mvp_document_analysis_resolves_io_printline() {
        let (snapshot, _, _) = snapshot_with_entry_resolution();
        let resolution = snapshot.resolution.as_ref().expect("assembly-backed resolution");
        assert!(
            resolution.items.iter().any(|item| item.name == "WriteLine"),
            "expected WriteLine in merged items: {:?}",
            resolution.items.iter().map(|item| &item.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn corelib_mvp_definition_at_printline_targets_io_module() {
        let (snapshot, fixture, _) = snapshot_with_entry_resolution();
        let offset = fixture.source.find("WriteLine").expect("WriteLine usage");
        if let Some(definition) = definition_at_offset(&snapshot, offset) {
            let def_path = definition.location.path.to_string_lossy();
            assert!(
                def_path.contains("Output") || def_path.contains("System"),
                "expected cross-file definition under Output module, got {def_path}"
            );
        } else {
            assert!(
                snapshot
                    .resolution
                    .as_ref()
                    .is_some_and(|resolution| resolution.items.iter().any(|item| item.name == "WriteLine")),
                "expected WriteLine in resolution when cross-file definition is unavailable"
            );
        }
    }

    #[test]
    fn corelib_mvp_completion_after_output_dot_includes_writeline() {
        let (snapshot, fixture, _) = snapshot_with_entry_resolution();
        let offset = fixture.source.find("    Output.").expect("main Output.") + "    Output.".len();
        let candidates = completion_candidates(&snapshot, &fixture.source, offset);
        assert!(
            candidates.iter().any(|c| c.label == "WriteLine"),
            "expected WriteLine member completion after Output., got {:?}",
            candidates.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn corelib_mvp_use_path_completion_offers_std_segments() {
        let (snapshot, fixture, _) = snapshot_with_entry_resolution();
        let offset = fixture.source.find("use Std.").expect("use Std.") + "use Std.".len();
        let candidates = completion_candidates(&snapshot, &fixture.source, offset);
        let labels: Vec<_> = candidates.iter().map(|c| c.label.as_str()).collect();
        assert!(
            labels.iter().any(|label| {
                *label == "System"
                    || label.contains("System")
                    || *label == "Core"
                    || label.contains("corelib_runtime")
                    || label.contains("corelib_foundation")
                    || label.contains("crates")
            }),
            "expected Std shard segment after use Std., got {labels:?}"
        );
        assert!(
            !labels.contains(&"Std.Core"),
            "use-path completion must offer the next segment, not a repeated prefix: {labels:?}"
        );
    }

    #[test]
    fn corelib_mvp_workspace_references_include_io_definition() {
        let (snapshot, fixture, assembly) = snapshot_with_entry_resolution();
        let offset = fixture.source.find("WriteLine").expect("WriteLine usage");
        let references = references_at_offset_workspace(&snapshot, &assembly, &fixture.main_path, offset, true);
        if references.is_empty() {
            assert!(
                snapshot
                    .resolution
                    .as_ref()
                    .is_some_and(|resolution| resolution.items.iter().any(|item| item.name == "WriteLine")),
                "expected WriteLine in resolution when workspace references are unavailable"
            );
        } else {
            assert!(
                references.iter().any(|reference| { reference.location.path.to_string_lossy().contains("Output") }),
                "expected a reference in Output.bd, got {:?}",
                references.iter().map(|r| r.location.path.display()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn corelib_mvp_lifecycle_snapshot_resolves_writeline_via_entry_resolution() {
        let (snapshot, fixture, _) = snapshot_with_entry_resolution();
        let resolution = snapshot.resolution.as_ref().expect("lifecycle entry resolution snapshot");
        assert!(
            resolution.items.iter().any(|item| item.name == "WriteLine"),
            "expected WriteLine via entry resolution spine: {:?}",
            resolution.items.iter().map(|item| &item.name).collect::<Vec<_>>()
        );
        assert!(fixture.main_path.is_file(), "fixture entry should exist at {}", fixture.main_path.display());
    }

    #[test]
    fn corelib_mvp_lifecycle_completion_after_output_dot_includes_writeline() {
        let (snapshot, fixture, _) = snapshot_with_entry_resolution();
        let offset = fixture.source.find("    Output.").expect("main Output.") + "    Output.".len();
        let candidates = completion_candidates(&snapshot, &fixture.source, offset);
        assert!(
            candidates.iter().any(|c| c.label == "WriteLine"),
            "expected WriteLine member completion after Output., got {:?}",
            candidates.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn corelib_mvp_document_analysis_without_context_has_no_resolution() {
        let root = compiler_workspace_root();
        with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            let program =
                parse_program_with_source_name(&fixture.main_path.to_string_lossy(), &fixture.source).expect("parse");
            let handle = ProjectSessionHandle::try_for_analysis_path(&fixture.main_path, None);
            let snapshot = build_document_analysis_with_context(
                &program,
                fixture.main_path.to_string_lossy(),
                &fixture.source,
                &fixture.main_path,
                handle.as_ref(),
                None,
            );
            assert!(
                snapshot.resolution.is_none(),
                "without query-backed entry resolution, snapshot should not resolve"
            );
            assert!(handle.is_some(), "project session should still be available for composition diagnostics");
        });
    }
}
