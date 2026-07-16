use beskid_queries::{BeskidDatabase, CompletionContext, completion_candidates};
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionResponse, CompletionTextEdit, TextEdit, Uri,
};

use crate::features::project_manifest::api as project_manifest;
use crate::position::offset_range_to_lsp;
use crate::session::store::Document;

/// Completion items at `offset`, including manifest-aware suggestions for `.bproj`/`.bws` buffers.
pub fn handle_completion(
    db: &BeskidDatabase,
    uri: &Uri,
    doc: &Document,
    offset: usize,
) -> CompletionResponse {
    let prefix = project_manifest::completion_prefix_at_offset(&doc.text, offset).to_lowercase();

    if project_manifest::is_manifest_uri(uri) {
        if let Some(mut items) = project_manifest::manifest_enum_completion_items(&doc.text, offset)
        {
            items.sort_by(|left, right| left.label.cmp(&right.label));
            return CompletionResponse::Array(items);
        }

        let mut items: Vec<CompletionItem> = project_manifest::manifest_keyword_completions(uri)
            .iter()
            .filter(|(label, _, _)| {
                prefix.is_empty() || label.to_lowercase().starts_with(prefix.as_str())
            })
            .map(|(label, kind, detail)| CompletionItem {
                label: (*label).to_string(),
                kind: Some(*kind),
                detail: Some((*detail).to_string()),
                ..CompletionItem::default()
            })
            .collect();
        items.sort_by(|left, right| left.label.cmp(&right.label));
        return CompletionResponse::Array(items);
    }

    let context = completion_context(&doc.text, offset);
    let mut items: Vec<CompletionItem> = doc
        .syntax_completion
        .and_then(|completion| {
            context.and_then(|context| {
                completion_candidates(db, completion.anchor, context)
                    .ok()
                    .flatten()
                    .map(|candidates| (context, candidates))
            })
        })
        .map(|(_context, candidates)| {
            candidates
                .iter()
                .filter(|candidate| {
                    prefix.is_empty() || candidate.label.to_lowercase().starts_with(prefix.as_str())
                })
                .map(|candidate| CompletionItem {
                    label: candidate.label.to_string(),
                    kind: Some(syntax_completion_kind_to_lsp(candidate.kind)),
                    detail: candidate.detail.as_ref().map(ToString::to_string),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: offset_range_to_lsp(
                            &doc.text,
                            candidate.replacement_start,
                            candidate.replacement_end,
                        ),
                        new_text: candidate.label.to_string(),
                    })),
                    ..CompletionItem::default()
                })
                .collect()
        })
        .unwrap_or_default();

    items.sort_by(|left, right| left.label.cmp(&right.label));
    items.dedup_by(|left, right| left.label == right.label && left.kind == right.kind);
    items.truncate(200);
    CompletionResponse::Array(items)
}

fn completion_context(text: &str, offset: usize) -> Option<CompletionContext> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let is_ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mut replacement_start = offset;
    while replacement_start > 0 {
        let ch = text[..replacement_start].chars().next_back()?;
        if !is_ident(ch) {
            break;
        }
        replacement_start -= ch.len_utf8();
    }
    let mut replacement_end = offset;
    while replacement_end < text.len() {
        let ch = text[replacement_end..].chars().next()?;
        if !is_ident(ch) {
            break;
        }
        replacement_end += ch.len_utf8();
    }
    Some(CompletionContext {
        cursor: offset,
        replacement_start,
        replacement_end,
    })
}

fn syntax_completion_kind_to_lsp(
    kind: beskid_queries::CompletionKind,
) -> tower_lsp_server::ls_types::CompletionItemKind {
    match kind {
        beskid_queries::CompletionKind::Function => {
            tower_lsp_server::ls_types::CompletionItemKind::FUNCTION
        }
        beskid_queries::CompletionKind::Module => {
            tower_lsp_server::ls_types::CompletionItemKind::MODULE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::Arc;

    use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
        SyntaxProgramAssembly,
    };
    use beskid_analysis::services::parse_program;
    use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
    use beskid_queries::{
        AstNodeKey, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId,
        build_typed_program,
    };
    use tower_lsp_server::ls_types::Uri;

    use super::handle_completion;
    use crate::session::lifecycle::ANALYSIS_CACHE_VERSION;
    use crate::session::store::{Document, SyntaxCompletion};

    #[test]
    fn syntax_completion_works_without_legacy_analysis() {
        let source = "i32 Zebra() { return 0; } i32 Main() { return Zeb; }";
        let mut db = BeskidDatabase::default();
        let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/completion/Main.bd"));
        let project = ProjectSession::new(
            &db,
            PathBuf::from("/tmp/completion"),
            unit.path(&db).clone(),
            "App".to_string(),
            "lock".to_string(),
        );
        let generation = SyntaxGenerationId(1);
        db.ensure_file_text(unit.path(&db).clone(), source.to_string());
        db.ensure_syntax_unit(project, unit, generation)
            .expect("syntax registration");
        let index = SyntaxIndex::from_program(
            &expand_program(
                parse_program(source).expect("parse"),
                DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
            ),
            generation,
        );
        let anchor = AstNodeKey {
            unit,
            generation,
            node: index
                .ids_of_kind(NodeKind::Program)
                .next()
                .expect("program node"),
        };
        let doc = Document {
            version: 1,
            text: source.to_string(),
            analysis_cache_version: ANALYSIS_CACHE_VERSION,
            analysis: None,
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: Some(SyntaxCompletion { anchor }),
        };
        let offset = source.find("Zeb;").expect("completion prefix") + 3;
        let response = handle_completion(
            &db,
            &Uri::from_str("file:///tmp/completion/Main.bd").expect("uri"),
            &doc,
            offset,
        );
        let tower_lsp_server::ls_types::CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        assert!(items.iter().any(|item| item.label == "Zebra"));
    }

    #[test]
    fn syntax_completion_lists_imported_members_without_legacy_analysis() {
        let root = PathBuf::from("/tmp/completion-import/src");
        let main_path = root.join("Main.bd");
        let tools_path = root.join("Lib/Tools.bd");
        let main_source = "use Lib.Tools;\ni32 Main() { return Tools.Hel; }";
        let tools_source = "i32 Helper() { return 1; }";
        let main_program = expand_program(
            parse_program(main_source).expect("main parses"),
            DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        );
        let tools_program = expand_program(
            parse_program(tools_source).expect("tools parse"),
            DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        );
        let assembly = Arc::new(SyntaxProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry {
                    dependency_name: None,
                    source_root: root.clone(),
                },
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
                    logical_name: tools_path.display().to_string(),
                    path: tools_path.clone(),
                    source: tools_source.to_string(),
                    program: tools_program,
                },
            ]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
        ));
        let mut db = BeskidDatabase::default();
        let main_unit = SourceUnitId::new(&db, main_path.clone());
        let project = ProjectSession::new(
            &db,
            root.parent().expect("project root").to_path_buf(),
            main_path,
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
            node: index
                .ids_of_kind(NodeKind::Program)
                .next()
                .expect("program node"),
        };
        let doc = Document {
            version: 1,
            text: main_source.to_string(),
            analysis_cache_version: ANALYSIS_CACHE_VERSION,
            analysis: None,
            syntax_definitions: Vec::new(),
            syntax_hovers: Vec::new(),
            syntax_symbols: Vec::new(),
            syntax_completion: Some(SyntaxCompletion { anchor }),
        };
        let offset = main_source.find("Hel;").expect("completion prefix") + 3;
        let response = handle_completion(
            &db,
            &Uri::from_str("file:///tmp/completion-import/src/Main.bd").expect("uri"),
            &doc,
            offset,
        );
        let tower_lsp_server::ls_types::CompletionResponse::Array(items) = response else {
            panic!("expected completion array");
        };
        let helper = items
            .iter()
            .find(|item| item.label == "Helper")
            .expect("imported member candidate");
        let Some(tower_lsp_server::ls_types::CompletionTextEdit::Edit(edit)) = &helper.text_edit
        else {
            panic!("expected replacement edit");
        };
        assert_eq!(edit.new_text, "Helper");
        assert_eq!(
            edit.range.start.character,
            (main_source.find("Hel").expect("prefix")
                - main_source.rfind('\n').expect("line break")
                - 1) as u32
        );
    }
}
