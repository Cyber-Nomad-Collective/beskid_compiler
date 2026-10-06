//! Source-fact selection for Rust Glue owners, shared by `beskid build --backend glue-rust` and the
//! native harness. Owner type and callable tables come only from `RustOwner` source facts joined to
//! the compiler-issued rows of the owner packet; no caller supplies mapping data.
use super::{AotResult, invalid};
use super::rust_owner::{RustOwnerCallable, RustOwnerType};
use beskid_codegen::{
    CodegenInput,
    glue::{GlueArtifact, GlueBindingManifest},
    module_emission::SyntaxModuleItem,
};
use beskid_queries::{
    AstNodeKey, GlueDirection, GlueOwnerBindingDigest, RustOwnerCallableRow, RustOwnerTables, RustOwnerTypeRow,
    SourceUnitId,
};

/// One declaration of the current generation that has a canonical Glue fact.
#[derive(Debug, Clone)]
pub struct GlueDeclaration {
    pub item: SyntaxModuleItem,
    pub direction: GlueDirection,
    /// Import library; `None` for exports.
    pub library: Option<String>,
    /// Canonical Glue fact symbol.
    pub symbol: String,
}

impl GlueDeclaration {
    pub fn is_import_of(&self, library: &str) -> bool {
        self.direction == GlueDirection::Import && self.library.as_deref() == Some(library)
    }
}

/// Every declaration with a canonical Glue fact, imports and exports, in source order.
///
/// An `Extern` contract method is a Glue import only when its `Library` is one of
/// `glue_libraries` (the manifest `glue "<library>"` blocks), exactly as the semantic extern
/// profile classifies it. Every other `Extern` method (user C imports and the Corelib
/// `beskid_runtime` bridges, whose nominal types carry no `GlueHandle` brand) is not a Glue
/// declaration and is not judged by `glue_binding`. A Glue import or export whose fact is
/// rejected still fails the build.
pub fn glue_declarations(
    input: &CodegenInput<'_>,
    items: &[SyntaxModuleItem],
    glue_libraries: &[String],
) -> AotResult<Vec<GlueDeclaration>> {
    fn visit(
        input: &CodegenInput<'_>,
        items: &[SyntaxModuleItem],
        glue_libraries: &[String],
        key: AstNodeKey,
        out: &mut Vec<GlueDeclaration>,
    ) -> AotResult<()> {
        let db = input.database();
        let glue_candidate = match beskid_queries::extern_contract_import_for_declaration(db, key) {
            Some((_, _, library)) => {
                library.is_some_and(|library| glue_libraries.iter().any(|candidate| *candidate == library))
            }
            None => true,
        };
        if glue_candidate
            && let Some(fact) = beskid_queries::glue_binding(db, key).map_err(|error| invalid(error.to_string()))?
            && fact.declaration == key
        {
            let symbol = match items.iter().find(|item| item.key == key) {
                Some(item) => item.symbol.clone(),
                None => beskid_queries::item_name(db, key)
                    .map_err(|error| invalid(error.to_string()))?
                    .ok_or_else(|| invalid("unnamed Glue declaration"))?
                    .to_string(),
            };
            let library = match fact.direction {
                GlueDirection::Import => fact.library.clone(),
                GlueDirection::Export => None,
            };
            out.push(GlueDeclaration {
                item: SyntaxModuleItem { key, symbol },
                direction: fact.direction,
                library,
                symbol: fact.symbol,
            });
        }
        let children = beskid_queries::child_nodes(db, key).map_err(|error| invalid(error.to_string()))?;
        for child in children.unwrap_or_default().iter().copied() {
            visit(input, items, glue_libraries, child, out)?;
        }
        Ok(())
    }
    let mut out = Vec::new();
    for root in input.roots() {
        visit(input, items, glue_libraries, *root, &mut out)?;
    }
    Ok(out)
}

/// `(logical unit, library)` of every `RustOwner` placement in every assembly unit. Each unit is
/// validated by `rust_owner_declarations`; the caller rejects a library without a manifest glue block.
pub fn rust_owner_declaration_libraries(input: &CodegenInput<'_>) -> AotResult<Vec<(String, String)>> {
    let db = input.database();
    let mut libraries = Vec::new();
    for unit in input.typed_program().assembly.units.iter() {
        let id = SourceUnitId::new(db, unit.path.clone());
        let declarations =
            beskid_queries::rust_owner_declarations(db, id).map_err(|error| invalid(error.to_string()))?;
        for declaration in declarations.iter() {
            libraries.push((unit.logical_name.clone(), declaration.library.clone()));
        }
    }
    Ok(libraries)
}

fn opaque_digests(binding: &GlueBindingManifest) -> Vec<(Option<usize>, String)> {
    binding
        .parameters
        .iter()
        .enumerate()
        .filter_map(|(index, parameter)| {
            parameter.opaque.as_ref().map(|opaque| (Some(index), opaque.brand_sha256.clone()))
        })
        .chain(binding.result.opaque.as_ref().map(|opaque| (None, opaque.brand_sha256.clone())))
        .collect()
}

/// Joins every import row of `packet` (the owner packet emitted over exactly `imports` for
/// `library`) to its unique source declaration and forms the owner tables from `RustOwner` facts.
pub fn rust_owner_source_tables(
    input: &CodegenInput<'_>,
    library: &str,
    imports: &[GlueDeclaration],
    packet: &GlueArtifact,
) -> AotResult<RustOwnerTables> {
    if imports.iter().any(|declaration| !declaration.is_import_of(library)) {
        return Err(invalid(format!("Rust owner `{library}` selection contains a declaration that is not its import")));
    }
    let rows = packet.manifest.bindings.iter().filter(|binding| binding.direction == "import").collect::<Vec<_>>();
    if rows.len() != imports.len() || rows.iter().any(|binding| binding.library != library) {
        return Err(invalid(format!("Rust owner `{library}` packet rows differ from the selected imports")));
    }
    let mut digests = Vec::with_capacity(rows.len());
    for binding in rows {
        let mut matches = imports.iter().filter(|declaration| declaration.symbol == binding.symbol);
        let (Some(declaration), None) = (matches.next(), matches.next()) else {
            return Err(invalid(format!("Rust owner packet row `{}` has no unique source import", binding.symbol)));
        };
        digests.push(GlueOwnerBindingDigest {
            binding: declaration.item.key,
            symbol: binding.symbol.clone(),
            identity_sha256: binding.identity_sha256.clone(),
            opaque: opaque_digests(binding),
        });
    }
    beskid_queries::rust_owner_tables(input.database(), library, &digests).map_err(|error| invalid(error.to_string()))
}

impl From<RustOwnerTypeRow> for RustOwnerType {
    fn from(row: RustOwnerTypeRow) -> Self {
        Self { brand_sha256: row.brand_sha256, rust_type_path: row.rust_type_path }
    }
}

impl From<RustOwnerCallableRow> for RustOwnerCallable {
    fn from(row: RustOwnerCallableRow) -> Self {
        Self {
            binding_identity_sha256: row.binding_identity_sha256,
            rust_callable_path: row.rust_callable_path,
            fallible: row.fallible,
        }
    }
}
