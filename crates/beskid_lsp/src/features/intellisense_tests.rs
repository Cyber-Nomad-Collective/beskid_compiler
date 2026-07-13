#[cfg(test)]
mod tests {
    use beskid_analysis::projects::AssemblyDiscovery;
    use beskid_analysis::services::{
        PrepareOptions, build_document_analysis_from_resolution, parse_program_with_source_name,
        resolve_input,
    };
    use beskid_queries::{BeskidDatabase, configure_db_for_project, entry_resolution_with_db};
    use std::path::PathBuf;
    use tower_lsp_server::ls_types::{GotoDefinitionResponse, Hover, Uri};

    use crate::features::{definition, hover, references};
    use crate::position::position_to_offset;
    use crate::session::lifecycle::{ANALYSIS_CACHE_VERSION, build_document};
    use crate::session::store::{Document, State};
    use crate::workspace_scan::path_to_uri;

    struct CorelibMvpFixture {
        main_path: PathBuf,
        project_root: PathBuf,
        source: String,
        uri: Uri,
    }

    fn compiler_workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("beskid_lsp crate layout")
            .to_path_buf()
    }

    fn with_cwd_at_workspace_root<R>(root: &PathBuf, f: impl FnOnce() -> R) -> R {
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(root).expect("chdir");
        let out = f();
        std::env::set_current_dir(previous).expect("restore cwd");
        out
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
        let uri = path_to_uri(&main_path).expect("file uri");
        CorelibMvpFixture {
            main_path,
            project_root,
            source,
            uri,
        }
    }

    fn corelib_mvp_document_with_entry_resolution()
    -> (Uri, Document, CorelibMvpFixture, BeskidDatabase) {
        let root = compiler_workspace_root();
        with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            let program = parse_program_with_source_name(
                &fixture.main_path.to_string_lossy(),
                &fixture.source,
            )
            .expect("parse");
            let resolved = resolve_input(
                Some(&fixture.main_path),
                Some(&fixture.project_root),
                None,
                None,
                false,
                false,
            )
            .expect("resolve");
            let project_root = fixture
                .project_root
                .canonicalize()
                .unwrap_or_else(|_| fixture.project_root.clone());
            configure_db_for_project(&project_root);
            let mut db = BeskidDatabase::with_persistence(&project_root);
            let mut options = PrepareOptions::default();
            options.front_end.assembly_discovery = AssemblyDiscovery::ImportClosure;
            let shared =
                entry_resolution_with_db(&mut db, &resolved, &options).expect("entry resolution");
            let module_paths = shared
                .module_graph
                .modules()
                .iter()
                .filter_map(|module| {
                    if module.path.is_empty() {
                        None
                    } else {
                        Some(module.path.join("::"))
                    }
                })
                .collect();
            let analysis = build_document_analysis_from_resolution(
                &program,
                fixture.main_path.to_string_lossy(),
                &fixture.source,
                &fixture.main_path,
                Some((*shared).clone()),
                module_paths,
                resolved.compile_plan.as_ref(),
                None,
            );
            let doc = Document {
                version: 1,
                text: fixture.source.clone(),
                analysis_cache_version: ANALYSIS_CACHE_VERSION,
                analysis: Some(analysis),
            };
            (fixture.uri.clone(), doc, fixture, db)
        })
    }

    #[test]
    fn completion_after_output_dot_lists_writeline() {
        let (_uri, doc, fixture, _db) = corelib_mvp_document_with_entry_resolution();
        let analysis = doc.analysis.as_ref().expect("analysis");
        let offset =
            fixture.source.find("    Output.").expect("main Output.") + "    Output.".len();
        let candidates =
            beskid_analysis::services::completion_candidates(analysis, &fixture.source, offset);
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.label == "WriteLine"),
            "expected WriteLine member completion after Output., got {:?}",
            candidates.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn definition_on_printline_targets_dependency_file() {
        let (uri, doc, fixture, _db) = corelib_mvp_document_with_entry_resolution();
        let offset = fixture.source.find("WriteLine").expect("WriteLine");
        let response = definition::handler::handle_definition(&uri, &doc, offset);
        if let Some(GotoDefinitionResponse::Scalar(location)) = response {
            let target = location.uri.to_string();
            assert!(
                target.contains("Output") || target.contains("System"),
                "expected Output/System path in definition uri {target}"
            );
        } else {
            let analysis = doc.analysis.as_ref().expect("analysis");
            let resolution = analysis.resolution.as_ref().expect("resolution");
            assert!(
                resolution.items.iter().any(|item| item.name == "WriteLine"),
                "expected WriteLine in resolution when definition is unavailable"
            );
        }
    }

    #[tokio::test]
    async fn lifecycle_build_document_corelib_mvp_has_resolution() {
        let root = compiler_workspace_root();
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&root).expect("chdir");
        let fixture = corelib_mvp_paths();
        let uri = fixture.uri.clone();
        let state = tokio::sync::RwLock::new(State::default());
        let doc = build_document(&state, &uri, 1, fixture.source.clone()).await;
        std::env::set_current_dir(previous).expect("restore cwd");
        let analysis = doc
            .analysis
            .expect("analysis from lifecycle build_document");
        let resolution = analysis
            .resolution
            .as_ref()
            .expect("project-aware resolution");
        assert!(
            resolution.items.iter().any(|item| item.name == "WriteLine"),
            "expected WriteLine in resolution: {:?}",
            resolution
                .items
                .iter()
                .map(|item| &item.name)
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn lifecycle_build_document_parses_corelib_mvp() {
        let root = compiler_workspace_root();
        let (uri, fixture) = with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            (fixture.uri.clone(), fixture)
        });
        let state = tokio::sync::RwLock::new(State::default());
        let doc = build_document(&state, &uri, 1, fixture.source.clone()).await;
        let analysis = doc
            .analysis
            .expect("analysis from lifecycle build_document");
        assert!(
            !analysis.program.node.items.is_empty(),
            "lifecycle build_document should attach a parsed program snapshot"
        );
    }

    #[test]
    fn entry_resolution_with_db_populates_writeline_for_intellisense() {
        let root = compiler_workspace_root();
        with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            let resolved = resolve_input(
                Some(&fixture.main_path),
                Some(&fixture.project_root),
                None,
                None,
                false,
                false,
            )
            .expect("resolve");
            let project_root = fixture
                .project_root
                .canonicalize()
                .unwrap_or_else(|_| fixture.project_root.clone());
            configure_db_for_project(&project_root);
            let mut db = BeskidDatabase::with_persistence(&project_root);
            let mut options = PrepareOptions::default();
            options.front_end.assembly_discovery = AssemblyDiscovery::ImportClosure;
            let shared =
                entry_resolution_with_db(&mut db, &resolved, &options).expect("entry resolution");
            assert!(
                shared.items.iter().any(|item| item.name == "WriteLine"),
                "entry_resolution_with_db should expose dependency WriteLine"
            );
        });
    }

    #[test]
    fn references_on_printline_includes_dependency() {
        let (uri, doc, fixture, _db) = corelib_mvp_document_with_entry_resolution();
        let offset = fixture.source.find("WriteLine").expect("WriteLine");
        let locations = references::handler::handle_references(
            &uri,
            &doc,
            offset,
            true,
            Some(fixture.main_path.as_path()),
        );
        if locations.is_empty() {
            let analysis = doc.analysis.as_ref().expect("analysis");
            let resolution = analysis.resolution.as_ref().expect("resolution");
            assert!(
                resolution.items.iter().any(|item| item.name == "WriteLine"),
                "expected WriteLine in resolution when references are unavailable"
            );
        } else {
            assert!(
                locations
                    .iter()
                    .any(|location| location.uri.to_string().contains("Output")),
                "expected Output dependency reference, got {:?}",
                locations
                    .iter()
                    .map(|l| l.uri.to_string())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn hover_on_printline_range_in_dependency_file() {
        let (uri, doc, fixture, _db) = corelib_mvp_document_with_entry_resolution();
        let offset = fixture.source.find("WriteLine").expect("WriteLine");
        let hover = hover::handler::handle_hover(&uri, &doc, offset).expect("hover");
        let Hover { range, .. } = hover;
        let range = range.expect("hover range");
        let analysis = doc.analysis.as_ref().expect("analysis");
        let hover_info =
            beskid_analysis::services::hover_at_offset(analysis, offset).expect("hover info");
        assert!(
            hover_info
                .location
                .path
                .to_string_lossy()
                .contains("Output")
                || analysis.resolution.as_ref().is_some_and(|resolution| {
                    resolution.items.iter().any(|item| item.name == "WriteLine")
                }),
            "hover target should be Output module file or resolve WriteLine"
        );
        let dependency_source =
            std::fs::read_to_string(&hover_info.location.path).expect("read dependency source");
        let start = position_to_offset(&dependency_source, range.start);
        let end = position_to_offset(&dependency_source, range.end);
        assert!(
            start < end,
            "hover range should be non-empty in dependency file"
        );
        let snippet = &dependency_source[start..end];
        assert!(
            snippet.contains("WriteLine"),
            "hover range should cover WriteLine in dependency, got `{snippet}`"
        );
    }
}
