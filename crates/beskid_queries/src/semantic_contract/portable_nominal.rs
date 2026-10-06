//! Readonly package-relative identity from the current prepared semantic assembly.
use super::*;
use beskid_analysis::{mod_host::ModPackageDeclaration, projects::VerifiedPackageIdentity};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableNominalIdentity {
    package: VerifiedPackageIdentity,
    declaration: ModPackageDeclaration,
}
impl PortableNominalIdentity {
    pub fn package(&self) -> &VerifiedPackageIdentity {
        &self.package
    }
    pub fn declaration(&self) -> &ModPackageDeclaration {
        &self.declaration
    }
}
pub fn portable_nominal_identity(
    db: &dyn Db,
    program: &TypedProgram,
    key: AstNodeKey,
) -> Result<PortableNominalIdentity, SemanticError> {
    let rejected = |reason: &str| SemanticError::new(format!("portable shape identity: {reason}"));
    if key.generation != program.generation || program.assembly.generation != program.generation {
        return Err(rejected("stale declaration generation"));
    }
    let registry = db.syntax_dependency_registry().lock().map_err(|_| rejected("package registry poisoned"))?;
    if registry.package_identities.get(&(program.project, program.generation))
        != Some(program.assembly.package_identities())
    {
        return Err(rejected("package closure is not registered for current project generation"));
    }
    drop(registry);
    program.assembly.package_identities().validate().map_err(|e| rejected(&e.to_string()))?;
    let unit = program
        .assembly
        .units
        .iter()
        .find(|unit| SourceUnitId::new(db, unit.path.clone()) == key.unit)
        .ok_or_else(|| rejected("declaration outside prepared assembly"))?;
    let owner = program
        .assembly
        .package_identities()
        .for_source(&unit.path)
        .ok_or_else(|| rejected("declaration lacks prepare-owned package provenance"))?;
    program
        .assembly
        .package_identities()
        .validate_source(&unit.path, &unit.source)
        .map_err(|e| rejected(&e.to_string()))?;
    let syntax = db.syntax_unit(key.unit).ok_or_else(|| rejected("source unit is not registered"))?;
    if !syntax.accepts_key(db, key)
        || syntax.project(db) != program.project
        || syntax.generation(db) != program.generation
        || syntax.expanded_program(db).as_ref() != &unit.program
    {
        return Err(rejected("source registration/tree differs from prepared assembly"));
    }
    if !matches!(node_kind(db, key)?, Some(IndexedNodeKind::TypeDefinition | IndexedNodeKind::EnumDefinition)) {
        return Err(rejected("source key is not a nominal declaration"));
    }
    let source_path =
        owner.relative_source_path(&unit.path).ok_or_else(|| rejected("ambiguous source ownership"))?;
    let lexical_path = super::mod_shapes::mod_shape_lexical_path(db, key)
        .ok_or_else(|| rejected("lexical nominal ancestry absent"))?;
    Ok(PortableNominalIdentity {
        package: owner.identity().clone(),
        declaration: ModPackageDeclaration { source_path, lexical_path },
    })
}

/// Resolve an implementation receiver using the current source-scoped nominal authority.
pub(crate) fn serialization_impl_receiver(db: &dyn Db, key: AstNodeKey) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(key.unit)?;
    if !syntax.accepts_key(db, key) {
        return None;
    }
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let block = index.node_at(program, key.node)?.of::<beskid_analysis::syntax::ImplBlock>()?;
    let beskid_analysis::syntax::Type::Complex(receiver) = &block.receiver_type.node else {
        return None;
    };
    layouts::resolve_type_declaration(db, key, &receiver.node)
}
