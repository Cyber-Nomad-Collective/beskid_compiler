use std::sync::Arc;

use anyhow::Result;
use beskid_analysis::syntax::{AstNodeId, SyntaxGenerationId};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SemanticTypeId, build_typed_program, child_nodes, item_name, item_signature,
    project_session_for_syntax_assembly, test_item, with_db,
};

/// Syntax-backed test metadata consumed by `beskid test`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTestItem {
    pub name: String,
    pub qualified_name: String,
    pub tags: Vec<String>,
    pub group: Option<String>,
    pub skip_condition: Option<bool>,
    pub skip_reason: Option<String>,
    pub selection_span: beskid_analysis::syntax::SpanInfo,
}

/// Discover current-generation test items from a prepared frontend snapshot.
///
/// This deliberately registers and queries the post-expansion syntax assembly rather than
/// traversing the legacy HIR-backed `ProgramAssembly` retained for compatibility consumers.
pub fn syntax_test_items_from_front_end(
    front: &beskid_analysis::services::FrontEndTypedResult,
) -> Result<Vec<SyntaxTestItem>> {
    let assembly = Arc::new(front.syntax_assembly());
    with_db(|db| syntax_test_items_from_assembly(db, assembly))
}

/// Return the syntax-derived result type of one prepared no-argument entrypoint.
///
/// REPL type inspection uses this authority directly instead of reading the legacy typed-HIR
/// result retained in the frontend compatibility bundle.
pub fn syntax_entrypoint_return_type_from_front_end(
    front: &beskid_analysis::services::FrontEndTypedResult,
    entrypoint: &str,
) -> Result<SemanticTypeId> {
    let assembly = Arc::new(front.syntax_assembly());
    with_db(|db| {
        let entry_path = assembly.entry_unit().path.clone();
        let project = project_session_for_syntax_assembly(db, &assembly, "syntax-repl", "prepared-frontend")
            .map_err(|error| anyhow::anyhow!("syntax REPL session preparation failed: {error}"))?;
        let generation = assembly.generation;
        build_typed_program(db, project, generation, Arc::clone(&assembly))
            .map_err(|error| anyhow::anyhow!("syntax REPL preparation failed: {error}"))?;
        let root =
            AstNodeKey { unit: beskid_queries::SourceUnitId::new(db, entry_path), generation, node: AstNodeId(0) };
        let entry = find_syntax_item(db, root, entrypoint)
            .ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;
        let signature = item_signature(db, entry)
            .map_err(|error| anyhow::anyhow!("entrypoint signature query failed: {error}"))?
            .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;
        if !signature.parameters.is_empty() {
            anyhow::bail!("Entrypoint `{entrypoint}` must take no parameters");
        }
        Ok(signature.result)
    })
}

fn syntax_test_items_from_assembly(
    db: &mut BeskidDatabase,
    assembly: Arc<beskid_analysis::projects::ProgramAssembly>,
) -> Result<Vec<SyntaxTestItem>> {
    let entry_path = assembly.entry_unit().path.clone();
    let project = project_session_for_syntax_assembly(db, &assembly, "syntax-tests", "prepared-frontend")
        .map_err(|error| anyhow::anyhow!("syntax test session preparation failed: {error}"))?;
    let generation = assembly.generation;
    build_typed_program(db, project, generation, assembly)
        .map_err(|error| anyhow::anyhow!("syntax test preparation failed: {error}"))?;
    let root = AstNodeKey { unit: beskid_queries::SourceUnitId::new(db, entry_path), generation, node: AstNodeId(0) };
    collect_syntax_test_items(db, root)
}

fn collect_syntax_test_items(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Result<Vec<SyntaxTestItem>> {
    let mut out = Vec::new();
    if let Some(facts) = test_item(db, key).map_err(|error| anyhow::anyhow!("syntax test query failed: {error}"))? {
        out.push(SyntaxTestItem {
            name: facts.name.to_string(),
            qualified_name: facts.qualified_name.to_string(),
            tags: facts.tags.iter().map(ToString::to_string).collect(),
            group: facts.group.map(|group| group.to_string()),
            skip_condition: facts.skip_condition,
            skip_reason: facts.skip_reason.map(|reason| reason.to_string()),
            selection_span: facts.selection_span,
        });
    }
    for child in child_nodes(db, key)
        .map_err(|error| anyhow::anyhow!("syntax test traversal failed: {error}"))?
        .unwrap_or_default()
        .iter()
        .copied()
    {
        out.extend(collect_syntax_test_items(db, child)?);
    }
    Ok(out)
}

pub(super) fn find_syntax_item(db: &dyn beskid_queries::Db, key: AstNodeKey, entrypoint: &str) -> Option<AstNodeKey> {
    if syntax_item_name(db, key).as_deref() == Some(entrypoint) {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_syntax_item(db, child, entrypoint))
}

pub(super) fn syntax_item_name(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<String> {
    item_name(db, key).ok().flatten().map(|name| name.as_ref().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use beskid_analysis::services::parse_program_with_source_name;

    #[test]
    fn syntax_test_discovery_preserves_nested_metadata() {
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let path = directory.join("Main.bd");
        let source = r#"mod Checks { test Smoke {
            meta { group = "fast"; tags = "unit, smoke"; }
            skip { condition = true; reason = "not on this host"; }
            return;
        } }"#;
        let program = parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse");
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit { logical_name: "Main".into(), path, source: source.into(), program }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            SyntaxGenerationId(0),
        ));

        let tests = syntax_test_items_from_assembly(&mut db, assembly).expect("syntax tests");
        assert_eq!(tests.len(), 1);
        let test = &tests[0];
        assert_eq!(test.name, "Smoke");
        assert_eq!(test.qualified_name, "Checks::Smoke");
        assert_eq!(test.tags, ["unit", "smoke"]);
        assert_eq!(test.group.as_deref(), Some("fast"));
        assert_eq!(test.skip_condition, Some(true));
        assert_eq!(test.skip_reason.as_deref(), Some("not on this host"));
        assert!(test.selection_span.start < test.selection_span.end);
    }

    #[test]
    fn syntax_test_discovery_reuses_the_registered_assembly_session() {
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let path = directory.join("Main.bd");
        let source = "test Smoke { return; }";
        let program = parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse");
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit { logical_name: "Main".into(), path, source: source.into(), program }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            SyntaxGenerationId(0),
        ));

        syntax_test_items_from_assembly(&mut db, Arc::clone(&assembly)).expect("initial syntax test discovery");
        let tests = syntax_test_items_from_assembly(&mut db, assembly)
            .expect("repeated syntax test discovery must retain source ownership");

        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "Smoke");
    }
}
