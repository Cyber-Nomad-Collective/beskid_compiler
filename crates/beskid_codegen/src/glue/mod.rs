pub mod artifact;
mod native;
pub(crate) mod handles;
mod rust;
pub use artifact::{GeneratedFile, GlueArtifact, GlueArtifactError, GlueBindingManifest, GlueManifest};
pub use rust::emit;
pub use handles::{EmittedHandleTransport,emit_source_handle_transports};

/// The executable closure of a Glue packet: every selected item and every item it reaches
/// through direct calls, plus the scheduler entry set when that closure needs it.
///
/// A Glue consumer is a library of its selected exports and imports, so its native bodies are
/// exactly what those items reach, as for [`crate::lower_prepared_syntax_library`]. The whole
/// registered assembly (Corelib, the compiler SDK and every Mod a dependency activates) is never
/// lowered into a consumer: items outside this closure may carry capabilities a Glue consumer
/// cannot hold (an SDK host callback needs a prepared native Mod invocation capability) and are
/// not part of the published image.
pub fn native_body_items(
    input: &crate::CodegenInput<'_>,
    all_items: &[crate::module_emission::SyntaxModuleItem],
    selected: &[crate::module_emission::SyntaxModuleItem],
) -> Result<Vec<crate::module_emission::SyntaxModuleItem>, crate::backend::BackendError> {
    let db = input.database();
    let mut reachable = std::collections::HashSet::new();
    for item in selected {
        reachable.insert(item.key);
        // Import declarations have no body and therefore no executable item or call facts; only
        // the exports (and the handle bindings' owners) contribute a closure.
        if !all_items.iter().any(|candidate| candidate.key == item.key) {
            continue;
        }
        let root = input
            .roots()
            .iter()
            .copied()
            .find(|root| root.unit == item.key.unit)
            .ok_or_else(|| GlueArtifactError::Invalid(format!("Glue item `{}` has no source root", item.symbol)))?;
        let closure = beskid_queries::reachable_items(db, root, item.key)
            .map_err(|error| crate::backend::BackendError::RustGlueFact { key: item.key, message: error.to_string() })?
            .ok_or_else(|| GlueArtifactError::Invalid(format!("incomplete direct-call facts for Glue item `{}`", item.symbol)))?;
        reachable.extend(closure.iter().copied());
    }
    let mut items = all_items.iter().filter(|item| reachable.contains(&item.key)).cloned().collect::<Vec<_>>();
    crate::prepared_syntax::close_scheduler_entry_reachability(db, input, &mut reachable, &mut items)
        .map_err(|error| GlueArtifactError::Invalid(error.to_string()))?;
    Ok(items)
}

/// Lower native bodies for the same current assembly that issued a Glue packet.
/// Original export symbols are reserved for normalized wrappers; calls between
/// Beskid functions retain their canonical callee keys and use private symbols.
pub fn lower_native_bodies(
    input: &crate::CodegenInput<'_>,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    all_items: &[crate::module_emission::SyntaxModuleItem],
    selected: &[crate::module_emission::SyntaxModuleItem],
    native_library: &str,
    expected: &GlueArtifact,
) -> Result<crate::CodegenArtifact, crate::backend::BackendError> {
    let current = emit(input, selected, native_library)?;
    if &current != expected {
        return Err(GlueArtifactError::Invalid("packet differs from current registered assembly".into()).into());
    }
    let mut items = native_body_items(input, all_items, selected)?;
    let mut imports = std::collections::HashMap::new();
    // Exports are executable items: their bodies are emitted under the packet's body symbol.
    // Imports are contract method signatures, never items; a call to one lowers to the packet's
    // import body symbol (the generated adapter that reaches the owner), so the map is keyed by
    // the declaration itself, not by an item it has none of.
    for selected_item in selected {
        // Selected declarations without a binding fact (handle types) only bind transports.
        let Some(fact) = beskid_queries::glue_binding(input.database(), selected_item.key)
            .map_err(|error| crate::backend::BackendError::RustGlueFact { key: selected_item.key, message: error.to_string() })?
        else {
            continue;
        };
        if fact.direction == beskid_queries::GlueDirection::Export {
            let binding = current
                .manifest
                .bindings
                .iter()
                .find(|binding| binding.direction == "export" && binding.symbol == fact.symbol)
                .ok_or(crate::backend::BackendError::MissingRustGlueBinding(selected_item.key))?;
            let item = items
                .iter_mut()
                .find(|item| item.key == selected_item.key)
                .ok_or(crate::backend::BackendError::MissingRustGlueBinding(selected_item.key))?;
            item.symbol = binding.body_symbol.clone();
        } else {
            let binding = current
                .manifest
                .bindings
                .iter()
                .find(|binding| binding.direction == "import" && binding.symbol == fact.symbol)
                .ok_or(crate::backend::BackendError::MissingRustGlueBinding(selected_item.key))?;
            imports.insert(selected_item.key, binding.body_symbol.clone());
        }
    }
    let input = input.with_glue_imports(imports).with_glue_handle_bindings(
        selected.iter().map(|item|item.key).collect::<Vec<_>>().into());
    Ok(crate::module_emission::lower_syntax_program(&input, isa, &items)?)
}
