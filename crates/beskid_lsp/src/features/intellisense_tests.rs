#[cfg(test)]
mod tests {
    use beskid_analysis::projects::AssemblyDiscovery;
    use beskid_analysis::services::{PrepareOptions, parse_program_with_source_name, resolve_input};
    use beskid_queries::{BeskidDatabase, configure_db_for_project, entry_resolution_with_db};
    use std::path::PathBuf;
    use std::str::FromStr;
    use tower_lsp_server::ls_types::{GotoDefinitionResponse, Hover, Uri};

    use crate::features::{completion, definition, hover, references, signature_help};
    use crate::position::position_to_offset;
    use crate::session::lifecycle::build_document;
    use crate::session::store::{Document, State, SyntaxCompletion, SyntaxDefinition, SyntaxHover, SyntaxSymbol};
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
        let main_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_e2e_tests/fixtures/corelib_mvp/Src/Main.bd");
        let source = std::fs::read_to_string(&main_path).expect("read Main.bd");
        let project_root = main_path.parent().and_then(|p| p.parent()).expect("fixture root").to_path_buf();
        let uri = path_to_uri(&main_path).expect("file uri");
        CorelibMvpFixture { main_path, project_root, source, uri }
    }

    #[test]
    fn completion_after_import_dot_lists_member_without_legacy_analysis() {
        use std::sync::Arc;

        use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
        use beskid_analysis::projects::{
            AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, ProgramAssembly,
        };
        use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
        use beskid_queries::{AstNodeKey, ProjectSession, SourceUnitId, SyntaxGenerationId, build_typed_program};

        let root = PathBuf::from("/tmp/intellisense-completion/src");
        let main_path = root.join("Main.bd");
        let output_path = root.join("Std/Core/Output.bd");
        let main_source = "use Std.Core.Output;\ni32 Main() { return Output.Write; }";
        let output_source = "pub i32 WriteLine() { return 1; }";
        let main_program = expand_program(
            parse_program_with_source_name(main_path.to_str().unwrap(), main_source).expect("main parses"),
            DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        );
        let output_program = expand_program(
            parse_program_with_source_name(output_path.to_str().unwrap(), output_source).expect("output parses"),
            DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        );
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: root.clone() },
                dependencies: Vec::new(),
            },
            Arc::new(vec![
                SourceUnit {
                    logical_name: main_path.display().to_string(),
                    path: main_path.clone(),
                    source: main_source.to_string(),
                    program: main_program.clone(),
                },
                SourceUnit {
                    logical_name: output_path.display().to_string(),
                    path: output_path.clone(),
                    source: output_source.to_string(),
                    program: output_program,
                },
            ]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false, generation
        ));
        let mut db = BeskidDatabase::default();
        let main_unit = SourceUnitId::new(&db, main_path.clone());
        let project = ProjectSession::new(
            &db,
            root.parent().expect("project root").to_path_buf(),
            main_path.clone(),
            "App".to_string(),
            "lock".to_string(),
        );
        let generation = SyntaxGenerationId(2);
        db.ensure_file_text(main_unit.path(&db).clone(), main_source.to_string());
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
        let index = SyntaxIndex::from_program(&main_program, generation);
        let anchor = AstNodeKey {
            unit: main_unit,
            generation,
            node: index.ids_of_kind(NodeKind::Program).next().expect("program node"),
        };
        // Document carries no legacy analysis snapshot; completion uses syntax_completion only.
        let doc = Document {
            version: 1,
            text: main_source.to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: Some(SyntaxCompletion { anchor }),
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
        };
        let offset = main_source.find("Output.Write").expect("member prefix") + "Output.Write".len();
        let response = completion::handler::handle_completion(
            &db,
            &Uri::from_str("file:///tmp/intellisense-completion/src/Main.bd").expect("uri"),
            &doc,
            offset,
        );
        let tower_lsp_server::ls_types::CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert!(
            items.iter().any(|item| item.label == "WriteLine"),
            "expected WriteLine member completion after Output. without Document.analysis, got {:?}",
            items.iter().map(|item| &item.label).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn definition_on_printline_targets_dependency_file() {
        let root = compiler_workspace_root();
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&root).expect("chdir");
        let fixture = corelib_mvp_paths();
        let uri = fixture.uri.clone();
        let state = tokio::sync::RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        let doc = build_document(&state, &uri, 1, fixture.source.clone()).await;
        std::env::set_current_dir(previous).expect("restore cwd");
        let offset = fixture.source.find("WriteLine").expect("WriteLine");
        let response = definition::handler::handle_definition(&uri, &doc, offset).expect("syntax definition");
        let GotoDefinitionResponse::Scalar(location) = response else {
            panic!("expected scalar definition from syntax facts");
        };
        let target = location.uri.to_string();
        assert!(
            target.contains("Output") || target.contains("System"),
            "expected Output/System path in definition uri {target}"
        );
    }

    #[test]
    fn definition_uses_syntax_fact_without_legacy_analysis() {
        let uri = Uri::from_str("file:///tmp/syntax-fact.bd").expect("uri");
        let declaration_path = PathBuf::from("/tmp/syntax-fact.bd");
        let doc = Document {
            version: 1,
            text: "i32 Main() { return helper(); }\ni32 helper() { return 0; }".to_string(),
            syntax_definitions: vec![SyntaxDefinition {
                reference_start: 20,
                reference_end: 26,
                declaration_path,
                declaration_start: 36,
                declaration_end: 42,
            }],
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
        };
        let response = definition::handler::handle_definition(&uri, &doc, 22).expect("syntax fact definition");
        let GotoDefinitionResponse::Scalar(location) = response else {
            panic!("expected scalar definition");
        };
        assert_eq!(location.uri, uri);
        assert_eq!(location.range.start.line, 1);
        assert_eq!(location.range.start.character, 4);
    }

    #[test]
    fn definition_on_declaration_uses_syntax_symbol_without_legacy_analysis() {
        let uri = Uri::from_str("file:///tmp/syntax-symbol.bd").expect("uri");
        let doc = Document {
            version: 1,
            text: "i32 helper() { return 0; }".to_string(),
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: vec![SyntaxSymbol {
                name: "helper".to_string(),
                kind: beskid_analysis::services::AnalysisSymbolKind::Function,
                start: 4,
                end: 10,
            }],
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
        };

        let response = definition::handler::handle_definition(&uri, &doc, 6).expect("syntax symbol definition");
        let GotoDefinitionResponse::Scalar(location) = response else {
            panic!("expected scalar definition");
        };
        assert_eq!(location.uri, uri);
        assert_eq!(location.range.start.line, 0);
        assert_eq!(location.range.start.character, 4);
        assert_eq!(location.range.end.character, 10);
    }

    #[test]
    fn references_use_syntax_facts_without_legacy_analysis() {
        let uri = Uri::from_str("file:///tmp/syntax-references.bd").expect("uri");
        let declaration_path = PathBuf::from("/tmp/syntax-references.bd");
        let doc = Document {
            version: 1,
            text: "i32 helper() { return helper(); }".to_string(),
            syntax_definitions: vec![SyntaxDefinition {
                reference_start: 22,
                reference_end: 28,
                declaration_path,
                declaration_start: 4,
                declaration_end: 10,
            }],
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
        };
        let locations = references::handler::handle_references(&uri, &doc, 24, true, None);
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn documentation_and_signature_help_use_syntax_hover_without_analysis() {
        let uri = Uri::from_str("file:///tmp/syntax-docs.bd").expect("uri");
        let source = "i32 Main() { return helper(); }".to_string();
        let doc = Document {
            version: 1,
            text: source.clone(),
            syntax_definitions: Vec::new(),
            syntax_hovers: vec![SyntaxHover {
                reference_start: 20,
                reference_end: 26,
                markdown: "**function** `helper`".to_string(),
                location_path: PathBuf::from("/tmp/syntax-docs.bd"),
                location_start: 20,
                location_end: 26,
            }],
            syntax_symbols: Vec::new(),
            syntax_completion: None,
            syntax_inlay_hints: Vec::new(),
            syntax_documentation: Vec::new(),
            syntax_diagnostics: Vec::new(),
        };
        let documentation = crate::commands::symbol_documentation::documentation_uri_for_document(&doc, 22)
            .expect("syntax documentation URL");
        assert!(documentation.contains("helper"));
        let call_offset = source.find("helper(").expect("helper call") + "helper(".len();
        let signature =
            signature_help::handler::handle_signature_help(&uri, &doc, call_offset).expect("syntax signature help");
        assert_eq!(signature.signatures[0].label, "**function** `helper`");
    }

    #[tokio::test]
    async fn lifecycle_build_document_corelib_mvp_has_resolution() {
        let root = compiler_workspace_root();
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&root).expect("chdir");
        let fixture = corelib_mvp_paths();
        let uri = fixture.uri.clone();
        let state = tokio::sync::RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        let doc = build_document(&state, &uri, 1, fixture.source.clone()).await;
        std::env::set_current_dir(previous).expect("restore cwd");
        assert!(
            doc.syntax_hovers.iter().any(|hover| hover.markdown.contains("WriteLine"))
                || doc
                    .syntax_definitions
                    .iter()
                    .any(|definition| { definition.declaration_path.to_string_lossy().contains("Output") }),
            "lifecycle build_document should attach syntax facts for WriteLine"
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
        state.read().await.mark_initial_scan_complete();
        let doc = build_document(&state, &uri, 1, fixture.source.clone()).await;
        assert!(
            !doc.syntax_documentation.is_empty() || !doc.syntax_symbols.is_empty(),
            "lifecycle build_document should attach syntax documentation/symbols for the buffer"
        );
    }

    #[test]
    fn entry_resolution_with_db_populates_writeline_for_intellisense() {
        let root = compiler_workspace_root();
        with_cwd_at_workspace_root(&root, || {
            let fixture = corelib_mvp_paths();
            let resolved =
                resolve_input(Some(&fixture.main_path), Some(&fixture.project_root), None, None, false, false)
                    .expect("resolve");
            let project_root = fixture.project_root.canonicalize().unwrap_or_else(|_| fixture.project_root.clone());
            configure_db_for_project(&project_root);
            let mut db = BeskidDatabase::with_persistence(&project_root);
            let mut options = PrepareOptions::default();
            options.front_end.assembly_discovery = AssemblyDiscovery::ImportClosure;
            let shared = entry_resolution_with_db(&mut db, &resolved, &options).expect("entry resolution");
            assert!(
                shared.items.iter().any(|item| item.name == "WriteLine"),
                "entry_resolution_with_db should expose dependency WriteLine"
            );
        });
    }

    #[tokio::test]
    async fn references_on_printline_includes_dependency() {
        let root = compiler_workspace_root();
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&root).expect("chdir");
        let fixture = corelib_mvp_paths();
        let uri = fixture.uri.clone();
        let state = tokio::sync::RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        let doc = build_document(&state, &uri, 1, fixture.source.clone()).await;
        std::env::set_current_dir(previous).expect("restore cwd");
        let offset = fixture.source.find("WriteLine").expect("WriteLine");
        let locations =
            references::handler::handle_references(&uri, &doc, offset, true, Some(fixture.main_path.as_path()));
        assert!(!locations.is_empty(), "syntax references must resolve WriteLine without Document.analysis");
        assert!(
            locations.iter().any(|location| {
                location.uri.to_string().contains("Output")
                    || location.uri.to_string().contains("System")
                    || location.uri == uri
            }),
            "expected Output/System or entry reference, got {:?}",
            locations.iter().map(|l| l.uri.to_string()).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn hover_on_printline_range_in_dependency_file() {
        let root = compiler_workspace_root();
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&root).expect("chdir");
        let fixture = corelib_mvp_paths();
        let uri = fixture.uri.clone();
        let state = tokio::sync::RwLock::new(State::default());
        state.read().await.mark_initial_scan_complete();
        let doc = build_document(&state, &uri, 1, fixture.source.clone()).await;
        std::env::set_current_dir(previous).expect("restore cwd");
        let offset = fixture.source.find("WriteLine").expect("WriteLine");
        let syntax_hover = doc
            .syntax_hovers
            .iter()
            .find(|hover| hover.reference_start <= offset && offset <= hover.reference_end)
            .expect("syntax hover fact")
            .clone();
        // The hover handler must rely only on generation-safe syntax facts.
        let hover = hover::handler::handle_hover(&uri, &doc, offset).expect("hover");
        let Hover { range, .. } = hover;
        let range = range.expect("hover range");
        assert!(
            syntax_hover.location_path.to_string_lossy().contains("Output")
                || syntax_hover.markdown.contains("WriteLine"),
            "hover target should preserve the resolved WriteLine declaration"
        );
        let dependency_source = std::fs::read_to_string(&syntax_hover.location_path).expect("read dependency source");
        let start = position_to_offset(&dependency_source, range.start);
        let end = position_to_offset(&dependency_source, range.end);
        assert!(start < end, "hover range should be non-empty in dependency file");
        let snippet = &dependency_source[start..end];
        assert!(snippet.contains("WriteLine"), "hover range should cover WriteLine in dependency, got `{snippet}`");
    }
}
