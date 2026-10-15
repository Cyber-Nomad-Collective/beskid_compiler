//! Per-contribution unit attribution of Mod-generated serialization groups.
//!
//! The Mod merger appends every generated item to the entry source unit. A group generated
//! for a target declared in unit `U` (its encoder impl, decoder record, free `read` function,
//! decoder impl and factory) is attributed to `U`, as Rust appends derive output to the module
//! of the input item: field visibility (E1211) judges the group in `U`, so a generated adapter
//! reads and constructs the private fields of its own target and nothing else. Name resolution
//! of the generated code stays in the entry unit, where the Mod planned its canonical routes.
//!
//! The table is derived from the issuer's private payload (`CompiledSerializationTarget`,
//! `CompiledSerializationTemplate`) when the generation is registered, never from the
//! descriptive claim. An appended item that no retained payload names exactly once keeps the
//! entry unit, so an unattributed contribution fails closed under the ordinary rule.
use crate::{AstNodeId, AstNodeKey, Db, SemanticError, SourceUnitId};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_analysis::syntax_query::NodeKind;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The source unit `key` is judged in for field visibility: the attributed unit of the
/// retained serialization group whose appended top-level item contains `key`, otherwise
/// `key.unit`.
pub fn contribution_attributed_unit(db: &dyn Db, key: AstNodeKey) -> SourceUnitId {
    let table = {
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        registry.contribution_attributions.get(&(key.unit, key.generation)).cloned()
    };
    let Some(table) = table else { return key.unit };
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else { return key.unit };
    let index = syntax.syntax_index(db);
    // Climb to the direct child of the program root that contains `key`.
    let mut item = key.node;
    loop {
        let Some(parent) = index.metadata().get(item.0 as usize).and_then(|metadata| metadata.parent) else {
            return key.unit;
        };
        if index.kind(parent) == Some(NodeKind::Program) {
            break;
        }
        item = parent;
    }
    table.iter().find(|(node, _)| *node == item).map_or(key.unit, |(_, unit)| *unit)
}

/// Register the attribution table of `assembly`'s entry unit for its generation, derived from
/// every retained serialization payload. A registered generation keeps one table: a second
/// registration must derive the same table or the assembly is rejected.
pub(crate) fn register_contribution_attributions(
    db: &dyn Db,
    assembly: &ProgramAssembly,
) -> Result<(), SemanticError> {
    let entry = assembly.entry_unit();
    let entry_id = SourceUnitId::new(db, entry.path.clone());
    let generation: SyntaxGenerationId = assembly.generation;
    let mut table = BTreeMap::<AstNodeId, SourceUnitId>::new();
    for metadata in &assembly.compiled_mod_metadata {
        let attribution = if let Some(target) = metadata.issuer_payload::<crate::CompiledSerializationTarget>() {
            target.appended_attribution(db, assembly)?
        } else if let Some(template) = metadata.issuer_payload::<crate::CompiledSerializationTemplate>() {
            template.appended_attribution(db, assembly)?
        } else {
            None
        };
        let Some((unit, nodes)) = attribution else { continue };
        for node in nodes {
            if let Some(previous) = table.insert(node, unit)
                && previous != unit
            {
                return Err(SemanticError::new(
                    "serialization contribution is attributed to two source units; the binding is ambiguous",
                ));
            }
        }
    }
    let table: Arc<[(AstNodeId, SourceUnitId)]> = table.into_iter().collect::<Vec<_>>().into();
    let mut registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    match registry.contribution_attributions.get(&(entry_id, generation)) {
        Some(existing) if existing.as_ref() != table.as_ref() => Err(SemanticError::new(
            "serialization contribution attribution differs from the registered project generation",
        )),
        Some(_) => Ok(()),
        None => {
            registry.contribution_attributions.insert((entry_id, generation), table);
            Ok(())
        }
    }
}

/// The top-level entry node of each `items` element that occurs exactly once in the appended
/// tail of `current`'s entry program after `original_count` preserved items, in `items` order.
/// `None` when any item is absent, repeated, or outside the tail.
pub(crate) fn appended_item_nodes(
    current: &ProgramAssembly,
    original_count: usize,
    items: &[beskid_analysis::syntax::Spanned<beskid_analysis::mod_host::ProgramItem>],
) -> Option<Vec<AstNodeId>> {
    let entry = current.entry_unit();
    let program = &entry.program;
    let index = current.entry_syntax_index();
    let root = index.ids_of_kind(NodeKind::Program).next()?;
    let mut nodes = Vec::with_capacity(items.len());
    for item in items {
        let matches = program
            .node
            .items
            .iter()
            .enumerate()
            .filter(|(_, candidate)| crate::mod_semantic_authority::same_retained_syntax(*candidate, item))
            .collect::<Vec<_>>();
        let [(position, appended)] = matches.as_slice() else { return None };
        if *position < original_count {
            return None;
        }
        nodes.push(index.direct_child_id(program, root, beskid_analysis::syntax_query::DynNodeRef::from(*appended))?);
    }
    Some(nodes)
}
