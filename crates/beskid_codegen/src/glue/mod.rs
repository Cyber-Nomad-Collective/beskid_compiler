pub mod artifact;
mod checked;
mod native;
pub(crate) mod handles;
mod rust;
pub use artifact::{GeneratedFile, GlueArtifact, GlueArtifactError, GlueBindingManifest, GlueManifest};
pub use checked::emit_checked_export;
pub use rust::{emit, rust_identifier};
pub use handles::{EmittedHandleTransport,GlueTransportKind,emit_source_handle_transports};

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
    // Exports are executable items: their bodies are emitted under the packet's body symbol.
    for (key, binding) in selected_bindings(input, selected, &current)? {
        if binding.direction == "export" {
            let item = items
                .iter_mut()
                .find(|item| item.key == key)
                .ok_or(crate::backend::BackendError::MissingRustGlueBinding(key))?;
            item.symbol = binding.body_symbol.clone();
        }
    }
    let input = glue_body_input(input, selected, &current)?;
    Ok(crate::module_emission::lower_syntax_program(&input, isa, &items)?)
}

/// The packet binding of every selected declaration that has a Glue binding fact (handle types
/// only bind transports and have none).
fn selected_bindings<'packet>(
    input: &crate::CodegenInput<'_>,
    selected: &[crate::module_emission::SyntaxModuleItem],
    packet: &'packet GlueArtifact,
) -> Result<Vec<(beskid_queries::AstNodeKey, &'packet GlueBindingManifest)>, crate::backend::BackendError> {
    let mut bindings = Vec::new();
    for selected_item in selected {
        let Some(fact) = beskid_queries::glue_binding(input.database(), selected_item.key)
            .map_err(|error| crate::backend::BackendError::RustGlueFact { key: selected_item.key, message: error.to_string() })?
        else {
            continue;
        };
        let direction = match fact.direction {
            beskid_queries::GlueDirection::Export => "export",
            beskid_queries::GlueDirection::Import => "import",
        };
        let binding = packet
            .manifest
            .bindings
            .iter()
            .find(|binding| binding.direction == direction && binding.symbol == fact.symbol)
            .ok_or(crate::backend::BackendError::MissingRustGlueBinding(selected_item.key))?;
        bindings.push((selected_item.key, binding));
    }
    Ok(bindings)
}

/// The codegen input that lowers the native bodies of `packet`. Imports are contract method
/// signatures, never items; a call to one lowers to the packet's import body symbol (the generated
/// adapter that reaches the owner), so the map is keyed by the declaration itself.
pub fn glue_body_input<'db>(
    input: &crate::CodegenInput<'db>,
    selected: &[crate::module_emission::SyntaxModuleItem],
    packet: &GlueArtifact,
) -> Result<crate::CodegenInput<'db>, crate::backend::BackendError> {
    let imports = selected_bindings(input, selected, packet)?
        .into_iter()
        .filter(|(_, binding)| binding.direction == "import")
        .map(|(key, binding)| (key, binding.body_symbol.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    Ok(input
        .with_glue_imports(imports)
        .with_glue_handle_bindings(selected.iter().map(|item| item.key).collect::<Vec<_>>().into()))
}

/// Fallible exports of `packet`: the source entry and the checked body counterpart symbol that
/// the export's checked invocation calls ([`artifact::checked_body_symbol`]).
pub fn fallible_exports(
    input: &crate::CodegenInput<'_>,
    selected: &[crate::module_emission::SyntaxModuleItem],
    packet: &GlueArtifact,
) -> Result<Vec<(beskid_queries::AstNodeKey, String)>, crate::backend::BackendError> {
    Ok(selected_bindings(input, selected, packet)?
        .into_iter()
        .filter(|(_, binding)| binding.direction == "export" && binding.fallible)
        .map(|(key, binding)| (key, artifact::checked_body_symbol(binding)))
        .collect())
}
