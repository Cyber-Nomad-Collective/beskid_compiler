//! Host-neutral prepared-syntax entrypoint lowering.

use std::sync::Arc;

use anyhow::Result;
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_analysis::services::FrontEndTypedResult;
use beskid_isle::AstNodeKey;
use beskid_queries::{
    BeskidDatabase, ProjectSession, SemanticTypeId, SourceUnitId, SyntaxGenerationId,
    build_typed_program, child_nodes, item_name, item_signature, reachable_items,
};
use cranelift_codegen::isa::TargetIsa;

use crate::{CodegenArtifact, CodegenInput, SyntaxModuleItem, lower_syntax_program};

/// Result of lowering one prepared syntax entrypoint through the HIR-free boundary.
pub struct PreparedSyntaxEntrypoint {
    pub artifact: CodegenArtifact,
    pub symbol: String,
    pub return_type: SemanticTypeId,
}

/// Lower a prepared frontend snapshot with the caller's target ISA.
///
/// Hosts retain ISA selection and runtime-kit/link policy; this function owns only the shared
/// prepared-syntax → `TypedProgram` → `CodegenInput` → ISLE transition.
pub fn lower_prepared_syntax_entrypoint(
    db: &mut BeskidDatabase,
    front: &FrontEndTypedResult,
    entrypoint: &str,
    target: TargetMetadata,
    isa: &dyn TargetIsa,
) -> Result<PreparedSyntaxEntrypoint> {
    let assembly = Arc::new(syntax_assembly_from_front_end(front));
    let entry_path = assembly.entry_unit().path.clone();
    let generation = SyntaxGenerationId(1);
    let project = ProjectSession::new(
        db,
        assembly.roots().host.source_root.clone(),
        entry_path.clone(),
        "syntax-codegen".into(),
        "prepared-frontend".into(),
    );
    let typed = build_typed_program(db, project, generation, Arc::clone(&assembly))
        .map_err(|error| anyhow::anyhow!("syntax program preparation failed: {error}"))?;
    let roots = assembly
        .units()
        .iter()
        .map(|unit| AstNodeKey {
            unit: SourceUnitId::new(db, unit.path.clone()),
            generation,
            node: beskid_queries::AstNodeId(0),
        })
        .collect::<Vec<_>>();
    let input = CodegenInput::new(
        db,
        typed,
        Arc::from(roots),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .map_err(|error| anyhow::anyhow!("invalid syntax codegen input: {error}"))?;
    let entry_root = AstNodeKey {
        unit: SourceUnitId::new(db, entry_path),
        generation,
        node: beskid_queries::AstNodeId(0),
    };
    let entry = find_entrypoint(db, &input, entrypoint)
        .ok_or_else(|| anyhow::anyhow!("Missing entrypoint `{entrypoint}`"))?;
    let signature = item_signature(db, entry)
        .map_err(|error| anyhow::anyhow!("entrypoint signature query failed: {error}"))?
        .ok_or_else(|| anyhow::anyhow!("Missing signature for `{entrypoint}`"))?;
    if !signature.parameters.is_empty() {
        anyhow::bail!("Entrypoint `{entrypoint}` must take no parameters");
    }
    let reachable = reachable_items(db, entry_root, entry)
        .map_err(|error| anyhow::anyhow!("entrypoint reachability query failed: {error}"))?
        .ok_or_else(|| anyhow::anyhow!("incomplete direct-call facts for `{entrypoint}`"))?;
    let items = reachable
        .iter()
        .copied()
        .map(|key| syntax_item_symbol(db, &input, key).map(|symbol| SyntaxModuleItem { key, symbol }))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("reachable item is not a syntax function or test"))?;
    let symbol = syntax_item_symbol(db, &input, entry)
        .ok_or_else(|| anyhow::anyhow!("entrypoint `{entrypoint}` is not a syntax function or test"))?;
    let artifact = lower_syntax_program(&input, isa, &items)
        .map_err(|error| anyhow::anyhow!("syntax ISLE lowering failed: {error}"))?;
    Ok(PreparedSyntaxEntrypoint { artifact, symbol, return_type: signature.result })
}

fn syntax_assembly_from_front_end(front: &FrontEndTypedResult) -> beskid_analysis::projects::SyntaxProgramAssembly {
    let assembly = &front.assembly;
    let mut units = assembly.units.as_ref().clone();
    units[assembly.entry_index].program = front.program.clone();
    beskid_analysis::projects::SyntaxProgramAssembly::new(
        assembly.roots.clone(), Arc::new(units), assembly.entry_index, assembly.discovery,
        Arc::clone(&assembly.module_index), assembly.has_std_dependency,
    )
}

fn find_entrypoint(db: &BeskidDatabase, input: &CodegenInput<'_>, entrypoint: &str) -> Option<AstNodeKey> {
    input.roots().iter().copied().find_map(|root| find_item(db, root, entrypoint))
}

fn find_item(db: &BeskidDatabase, key: AstNodeKey, entrypoint: &str) -> Option<AstNodeKey> {
    if item_name(db, key).ok().flatten().as_deref() == Some(entrypoint) { return Some(key); }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_item(db, child, entrypoint))
}

fn syntax_item_symbol(db: &BeskidDatabase, input: &CodegenInput<'_>, key: AstNodeKey) -> Option<String> {
    let name = item_name(db, key).ok().flatten()?;
    let unit = input.typed_program().assembly.units().iter()
        .find(|unit| SourceUnitId::new(db, unit.path.clone()) == key.unit)?;
    let logical = unit.logical_name.chars().map(|character| {
        if character.is_ascii_alphanumeric() { character } else { '_' }
    }).collect::<String>();
    Some(format!("{name}#syntax_{logical}_{}", key.node.0))
}
