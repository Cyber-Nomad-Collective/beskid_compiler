pub mod artifact;
mod native;
pub(crate) mod handles;
mod rust;
pub use artifact::{GeneratedFile, GlueArtifact, GlueArtifactError, GlueBindingManifest, GlueManifest};
pub use rust::emit;
pub use handles::{EmittedHandleTransport,emit_source_handle_transports};

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
    let mut items = all_items.to_vec();
    let mut imports = std::collections::HashMap::new();
    for item in &mut items {
        if !selected.iter().any(|selected| selected.key == item.key) {
            continue;
        }
        let fact = beskid_queries::glue_binding(input.database(), item.key)
            .map_err(|error| crate::backend::BackendError::RustGlueFact { key: item.key, message: error.to_string() })?
            .ok_or(crate::backend::BackendError::MissingRustGlueBinding(item.key))?;
        if fact.direction == beskid_queries::GlueDirection::Export {
            let binding = current
                .manifest
                .bindings
                .iter()
                .find(|binding| binding.direction == "export" && binding.symbol == fact.symbol)
                .ok_or(crate::backend::BackendError::MissingRustGlueBinding(item.key))?;
            item.symbol = binding.body_symbol.clone();
        } else {
            let binding = current
                .manifest
                .bindings
                .iter()
                .find(|binding| binding.direction == "import" && binding.symbol == fact.symbol)
                .ok_or(crate::backend::BackendError::MissingRustGlueBinding(item.key))?;
            imports.insert(item.key, binding.body_symbol.clone());
        }
    }
    let input = input.with_glue_imports(imports).with_glue_handle_bindings(
        selected.iter().map(|item|item.key).collect::<Vec<_>>().into());
    Ok(crate::module_emission::lower_syntax_program(&input, isa, &items)?)
}
