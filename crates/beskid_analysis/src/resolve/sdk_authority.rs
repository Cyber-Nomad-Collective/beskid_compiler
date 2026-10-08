//! Source-issued native SDK callbacks for `beskid check`.
//!
//! The production semantic layer (`beskid_queries::semantic_contract::native_mod_callbacks`)
//! admits an unqualified `__mod_semantic_*` / `__mod_query_*` call only inside the exact
//! canonical compiler-SDK source that this compiler embeds, and only as the body call of the
//! named wrapper function. The callback's signature is the wrapper's own signature. This module
//! mirrors that admission for the resolver; it grants nothing to any other source.

use crate::projects::assembly::SourceUnit;

/// The canonical SDK path (`src/Beskid/...`) a unit's exact bytes correspond to, mirroring the
/// `sdk_source_authority` assignment in `beskid_queries::typed_program`: identical bytes, a
/// matching path suffix, and an unrecovered parse equal to the unit's own syntax tree.
pub(crate) fn canonical_sdk_source_authority(unit: &SourceUnit) -> Option<&'static str> {
    let expected = beskid_abi::sdk_source::canonical_sdk_sources().iter().find(|expected| {
        expected.path().ends_with(".bd")
            && expected.bytes() == unit.source.as_bytes()
            && unit.path.ends_with(std::path::Path::new(expected.path().strip_prefix("src/").unwrap_or(expected.path())))
    })?;
    let parsed = crate::services::parse_program_with_source_name_and_diagnostics(&unit.logical_name, &unit.source).ok()?;
    (!parsed.recovered && parsed.diagnostics.is_empty() && parsed.program == unit.program).then_some(expected.path())
}

/// The wrapper function name and canonical SDK source path that admit callback `name`.
/// Mirrors the operation table of `native_mod_callback_for`.
pub(crate) fn native_mod_callback_wrapper(name: &str) -> Option<(&str, &'static str)> {
    const SEMANTIC: &str = "src/Beskid/Compiler/Semantic.bd";
    const QUERY: &str = "src/Beskid/Compiler/Query.bd";
    Some(match name {
        "__mod_semantic_plan_canonical_paths" => ("PlanCanonicalPaths", SEMANTIC),
        "__mod_semantic_resolve_function" => ("ResolveFunction", SEMANTIC),
        "__mod_semantic_check_serializable" => ("CheckSerializable", SEMANTIC),
        "__mod_semantic_resolve_syntax_template" => ("ResolveSyntaxTemplate", SEMANTIC),
        "__mod_semantic_resolve_syntax_type" => ("ResolveSyntaxType", SEMANTIC),
        "__mod_semantic_resolve_type" => ("ResolveType", SEMANTIC),
        "__mod_semantic_type_shape" => ("TypeShape", SEMANTIC),
        "__mod_semantic_capture_catchall" => ("CaptureCatchall", SEMANTIC),
        "__mod_semantic_validate_catchall" => ("ValidateCatchall", SEMANTIC),
        _ => (name.strip_prefix("__mod_query_")?, QUERY),
    })
}
