//! Exact current field proof, retained by the issuing invocation through codec emission.
use super::*;
use std::collections::HashSet;

const MAP_PATH: &str = "Core/Collections/Map.bd";
const MAP_SOURCE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Collections/Map.bd"));

/// No public constructor, Clone serialization, or Deserialize transport grants this proof.
/// Outbound semantic DTOs may describe it; emission revalidates this retained issuer value.
#[derive(Debug)]
pub struct ModSemanticCatchall {
    owner: ModSemanticHandle,
    field: ModSemanticDeclaration,
    map: ModSemanticHandle,
    value: ModSemanticFieldType,
}
impl ModSemanticCatchall {
    pub fn owner(&self) -> ModSemanticHandle {
        self.owner
    }
    pub fn field(&self) -> &ModSemanticDeclaration {
        &self.field
    }
    pub fn map(&self) -> ModSemanticHandle {
        self.map
    }
    pub fn value(&self) -> &ModSemanticFieldType {
        &self.value
    }
}
impl ModSemanticQueryAuthority<'_> {
    /// Mod-owned explicit field selection is checked against the actual instantiated record.
    /// A matching field name, wire digest, or native callback packet cannot supply this proof.
    pub fn catchall_field(
        &self,
        owner: ModSemanticHandle,
        field: &ModSemanticDeclaration,
    ) -> Result<ModSemanticCatchall, ModSemanticError> {
        self.validate_registration()?;
        let (key, arguments) = self
            .handles
            .borrow()
            .get(&owner)
            .cloned()
            .ok_or_else(|| Self::error("catchall owner has foreign/stale issuer"))?;
        self.validate_key(key)?;
        let owner_unit = self
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(self.db, unit.path.clone()) == key.unit)
            .ok_or_else(|| Self::error("catchall record source absent"))?;
        if self.assembly.package_identities().for_source(&owner_unit.path).is_none() {
            return Err(Self::error("catchall record lacks prepare-owned package identity"));
        }
        let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("catchall owner unavailable"))?;
        let projection =
            mod_shape_projection(self.db, syntax, key, arguments).map_err(|error| Self::error(&error.to_string()))?;
        let ShapeBodyProjection::Record(fields) = projection.body else {
            return Err(Self::error("catchall requires a record owner"));
        };
        let mut selected = None;
        for (candidate, _, identity) in fields {
            if self.declaration(candidate)? == *field {
                if selected.replace(identity).is_some() {
                    return Err(Self::error("ambiguous catchall declaration"));
                }
            }
        }
        let Some(ShapeIdentity::Nominal { qualified_name, arguments }) = selected else {
            return Err(Self::error("catchall field is not an exact owned applied nominal"));
        };
        let map = self.nominal_key(&qualified_name)?;
        let unit = self
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(self.db, unit.path.clone()) == map.unit)
            .ok_or_else(|| Self::error("catchall map source absent"))?;
        let root = self
            .assembly
            .package_identities()
            .for_source(&unit.path)
            .ok_or_else(|| Self::error("catchall map lacks prepare-owned package proof"))?;
        if root.identity().package_name() != "corelib_foundation"
            || !matches!(root.identity().source(), beskid_analysis::projects::VerifiedPackageSource::Corelib)
            || root.relative_source_path(&unit.path).as_deref() != Some(MAP_PATH)
            || unit.source.as_bytes() != MAP_SOURCE.as_bytes()
            || crate::item_name(self.db, map).ok().flatten().as_deref() != Some("Map")
        {
            return Err(Self::error("catchall requires exact canonical Foundation Map declaration"));
        }
        let parsed =
            beskid_analysis::services::parse_program_with_source_name_and_diagnostics(&unit.logical_name, &unit.source)
                .map_err(|error| Self::error(&error.to_string()))?;
        if parsed.recovered || !parsed.diagnostics.is_empty() || parsed.program != unit.program {
            return Err(Self::error("catchall map source does not authorize modified/recovered syntax"));
        }
        if arguments.len() != 2 || arguments[0] != ShapeIdentity::Abi(crate::SemanticTypeId::STRING) {
            return Err(Self::error("catchall requires canonical Map<string,V> arguments"));
        }
        let mut remaining = HANDLE_LIMIT;
        self.eligible_catchall_value(&arguments[1], 0, &mut remaining, &mut HashSet::new())?;
        let value = self.field_type(&arguments[1], 0, &mut remaining)?;
        Ok(ModSemanticCatchall { owner, field: field.clone(), map: self.issue(map, arguments)?, value })
    }
    pub fn validate_catchall(&self, witness: &ModSemanticCatchall) -> Result<(), ModSemanticError> {
        let current = self.catchall_field(witness.owner, &witness.field)?;
        if current.map != witness.map || current.value != witness.value {
            return Err(Self::error("catchall application changed before emission"));
        }
        Ok(())
    }
    pub(super) fn eligible_catchall_value(
        &self,
        identity: &ShapeIdentity,
        depth: usize,
        remaining: &mut usize,
        seen: &mut HashSet<ShapeIdentity>,
    ) -> Result<(), ModSemanticError> {
        self.eligible_serialization_value(identity, depth, remaining, seen, false)
    }
    /// Shared serialization eligibility. `owner_policy` exempts the fields of
    /// this nominal that its field policy skips in both directions; a skipped
    /// field is built only by its default factory and is not serialized data.
    pub(super) fn eligible_serialization_value(
        &self,
        identity: &ShapeIdentity,
        depth: usize,
        remaining: &mut usize,
        seen: &mut HashSet<ShapeIdentity>,
        owner_policy: bool,
    ) -> Result<(), ModSemanticError> {
        if depth > DEPTH_LIMIT || *remaining == 0 {
            return Err(Self::error("catchall value eligibility bound exhausted"));
        }
        *remaining -= 1;
        match identity {
            ShapeIdentity::Abi(ty) if matches!(*ty, crate::SemanticTypeId::POINTER | crate::SemanticTypeId::NEVER) => {
                Err(Self::error("catchall value is pointer/never, not serializable data"))
            }
            ShapeIdentity::Abi(_) => Ok(()),
            ShapeIdentity::Function { .. } => Err(Self::error("catchall value contains executable function identity")),
            ShapeIdentity::Array(element) => self.eligible_catchall_value(element, depth + 1, remaining, seen),
            ShapeIdentity::Nominal { qualified_name, arguments } => {
                if !seen.insert(identity.clone()) {
                    return Ok(());
                }
                let key = self.nominal_key(qualified_name)?;
                if crate::canonical_container_kind(self.db, key, arguments.len())
                    == Some(crate::CanonicalContainerKind::Map)
                    && arguments.first() != Some(&ShapeIdentity::Abi(crate::SemanticTypeId::STRING))
                {
                    return Err(Self::error("serialization supports canonical string-key maps only"));
                }

                let sdk_resource = self.db.syntax_unit(key.unit).is_some_and(|unit| {
                    unit.accepts_key(self.db, key)
                        && unit.revision(self.db).sdk_source_authority.as_deref()
                            == Some("src/Beskid/Compiler/Semantic.bd")
                        && matches!(
                            crate::item_name(self.db, key).ok().flatten().as_deref(),
                            Some("SemanticHandle" | "CatchallWitness")
                        )
                });
                let process_resource =
                    crate::process_source_authority::canonical_process_resource_kind(self.db, key).is_some();
                if sdk_resource || process_resource || crate::runtime_managed_opaque_kind(self.db, key).is_some() {
                    return Err(Self::error("canonical opaque runtime resource lacks serialization eligibility proof"));
                }
                let unit = self
                    .assembly
                    .units
                    .iter()
                    .find(|unit| SourceUnitId::new(self.db, unit.path.clone()) == key.unit)
                    .ok_or_else(|| Self::error("catchall value source absent"))?;
                if self.assembly.package_identities().for_source(&unit.path).is_none() {
                    return Err(Self::error("catchall nominal value lacks prepare-owned package identity"));
                }
                for argument in arguments.iter() {
                    self.eligible_catchall_value(argument, depth + 1, remaining, seen)?;
                }
                let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("catchall value owner absent"))?;
                let shape = mod_shape_projection(self.db, syntax, key, arguments.clone())
                    .map_err(|error| Self::error(&error.to_string()))?;
                let policy_error = |error: crate::SemanticError| Self::error(&error.to_string());
                match shape.body {
                    ShapeBodyProjection::Record(fields) => {
                        for (field, _, ty) in fields {
                            if owner_policy
                                && crate::serialization_field_policy(self.db, field)
                                    .map_err(policy_error)?
                                    .absent_from_wire()
                            {
                                continue;
                            }
                            self.eligible_catchall_value(&ty, depth + 1, remaining, seen)?;
                        }
                    }
                    ShapeBodyProjection::Enum(variants) => {
                        for variant in variants {
                            for (field, _, ty) in variant.fields {
                                if owner_policy
                                    && crate::serialization_payload_field_policy(self.db, field)
                                        .map_err(policy_error)?
                                        .absent_from_wire()
                                {
                                    continue;
                                }
                                self.eligible_catchall_value(&ty, depth + 1, remaining, seen)?;
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

impl ModSemanticQueryAuthority<'_> {
    pub(super) fn retain_catchall(
        &self,
        invocation: u64,
        owner: ModSemanticHandle,
        field: &ModSemanticDeclaration,
    ) -> Result<ModSemanticCatchallClaim, ModSemanticError> {
        self.require_live_catchall_invocation(invocation)?;
        let witness = self.catchall_field(owner, field)?;
        let mut retained = self.catchalls.borrow_mut();
        if let Some(((_scope, token), prior)) = retained.iter().find(|((scope, _), prior)| {
            *scope == invocation
                && prior.owner == witness.owner
                && prior.field == witness.field
                && prior.map == witness.map
                && prior.value == witness.value
        }) {
            return Ok(ModSemanticCatchallClaim {
                token: *token,
                owner: prior.owner,
                field: prior.field.clone(),
                map: prior.map,
                value: prior.value.clone(),
            });
        }
        if retained.len() >= HANDLE_LIMIT {
            return Err(Self::error("catchall invocation witness bound exhausted"));
        }
        let token = NEXT_HANDLE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |token| token.checked_add(1))
            .map_err(|_| Self::error("catchall issuer tokens exhausted"))?;
        let claim = ModSemanticCatchallClaim {
            token,
            owner: witness.owner,
            field: witness.field.clone(),
            map: witness.map,
            value: witness.value.clone(),
        };
        retained.insert((invocation, token), witness);
        Ok(claim)
    }
    pub(super) fn validate_retained_catchall(
        &self,
        invocation: u64,
        claim: &ModSemanticCatchallClaim,
    ) -> Result<(), ModSemanticError> {
        self.require_live_catchall_invocation(invocation)?;
        let retained = self.catchalls.borrow();
        let witness = retained
            .get(&(invocation, claim.token))
            .ok_or_else(|| Self::error("catchall token has foreign/expired invocation issuer"))?;
        if witness.owner != claim.owner
            || witness.field != claim.field
            || witness.map != claim.map
            || witness.value != claim.value
        {
            return Err(Self::error("catchall claim differs from private issued declaration/application"));
        }
        self.validate_catchall(witness)
    }
}

impl ModSemanticQueryAuthority<'_> {
    fn require_live_catchall_invocation(&self, invocation: u64) -> Result<(), ModSemanticError> {
        self.validate_registration()?;
        self.admit_semantic_invocation(invocation)
    }
    pub(super) fn lookup_retained_catchall(
        &self,
        invocation: u64,
        token: u64,
    ) -> Result<ModSemanticCatchallClaim, ModSemanticError> {
        self.require_live_catchall_invocation(invocation)?;
        let retained = self.catchalls.borrow();
        let witness = retained
            .get(&(invocation, token))
            .ok_or_else(|| Self::error("catchall token has foreign/expired invocation issuer"))?;
        self.validate_catchall(witness)?;
        Ok(ModSemanticCatchallClaim {
            token,
            owner: witness.owner,
            field: witness.field.clone(),
            map: witness.map,
            value: witness.value.clone(),
        })
    }
}

/// Sealed current-generation binding. It contains no live invocation token and cannot be
/// reconstructed from callback JSON, names, hashes, or a public metadata description.
#[derive(Clone)]
pub struct CompiledCatchallBinding {
    project: ProjectSession,
    assembly: ProgramAssembly,
    owner: AstNodeKey,
    owner_arguments: Arc<[ShapeIdentity]>,
    field: ModSemanticDeclaration,
    map: AstNodeKey,
    map_arguments: Arc<[ShapeIdentity]>,
}
#[derive(Clone)]
pub struct ReboundCatchallBinding {
    owner: AstNodeKey,
    field: ModSemanticDeclaration,
    map: AstNodeKey,
    map_arguments: Arc<[ShapeIdentity]>,
}
impl ReboundCatchallBinding {
    pub fn owner(&self) -> AstNodeKey {
        self.owner
    }
    pub fn field(&self) -> &ModSemanticDeclaration {
        &self.field
    }
    pub fn map(&self) -> AstNodeKey {
        self.map
    }
    pub fn map_arguments(&self) -> &[ShapeIdentity] {
        &self.map_arguments
    }
}
impl CompiledCatchallBinding {
    pub fn owner(&self) -> AstNodeKey {
        self.owner
    }
    pub fn field(&self) -> &ModSemanticDeclaration {
        &self.field
    }
    pub fn map(&self) -> AstNodeKey {
        self.map
    }
    /// Rebase only alongside the independently rebound, exact generated target.
    pub fn rebind(
        &self,
        db: &dyn Db,
        program: &crate::TypedProgram,
        target: &super::ReboundSerializationTarget,
    ) -> Result<ReboundCatchallBinding, ModSemanticError> {
        let reject = ModSemanticQueryAuthority::error;
        if program.project != self.project || target.arguments() != self.owner_arguments.as_ref() {
            return Err(reject("catchall target has another project/application"));
        }
        let original = self
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(db, unit.path.clone()) == self.owner.unit)
            .ok_or_else(|| reject("catchall original owner absent"))?;
        let current = program
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(db, unit.path.clone()) == target.owner().unit)
            .ok_or_else(|| reject("catchall current owner absent"))?;
        if original.path != current.path
            || original.source != current.source
            || self.assembly.package_identities().for_source(&original.path)
                != program.assembly.package_identities().for_source(&current.path)
        {
            return Err(reject("catchall owner prepared provenance changed"));
        }
        let original_index = self
            .assembly
            .syntax_indexes
            .iter()
            .zip(self.assembly.units.iter())
            .find(|(_, unit)| unit.path == original.path)
            .map(|(index, _)| index)
            .ok_or_else(|| reject("catchall original syntax absent"))?;
        let old = original_index
            .node_at(&original.program, self.owner.node)
            .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
            .ok_or_else(|| reject("catchall original record absent"))?;
        let syntax = db
            .syntax_unit(target.owner().unit)
            .filter(|s| s.accepts_key(db, target.owner()))
            .ok_or_else(|| reject("catchall target is stale"))?;
        let new = syntax
            .syntax_index(db)
            .node_at(&current.program, target.owner().node)
            .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
            .ok_or_else(|| reject("catchall current record absent"))?;
        if old != new {
            return Err(reject("catchall rebound target differs from original record"));
        }
        let authority = ModSemanticQueryAuthority::for_registered_assembly(db, program.project, &program.assembly)?;
        let owner = authority.issue(target.owner(), self.owner_arguments.clone())?;
        let shape = authority.type_shape(owner)?;
        let ModSemanticShapeBody::Record { fields } = shape.body else {
            return Err(reject("catchall rebound owner is not record"));
        };
        let fields = fields
            .iter()
            .filter(|field| {
                field.declaration.source_unit == self.field.source_unit && field.declaration.span == self.field.span
            })
            .collect::<Vec<_>>();
        let [field] = fields.as_slice() else {
            return Err(reject("catchall rebound field correspondence missing/ambiguous"));
        };
        let proof = authority.catchall_field(owner, &field.declaration)?;
        let handles = authority.handles.borrow();
        let (map, args) = handles.get(&proof.map).ok_or_else(|| reject("catchall rebound map issuer absent"))?;
        if args.as_ref() != self.map_arguments.as_ref() {
            return Err(reject("catchall applied map/value changed"));
        }
        Ok(ReboundCatchallBinding {
            owner: target.owner(),
            field: field.declaration.clone(),
            map: *map,
            map_arguments: args.clone(),
        })
    }
    /// Revalidation includes the prepare-issued package closure and exact current registered
    /// trees, then reissues the canonical applied Map and value eligibility proof.
    pub fn revalidate(&self, db: &dyn Db) -> Result<(), ModSemanticError> {
        let authority = ModSemanticQueryAuthority::for_registered_assembly(db, self.project, &self.assembly)?;
        let owner = authority.issue(self.owner, self.owner_arguments.clone())?;
        let witness = authority.catchall_field(owner, &self.field)?;
        let handles = authority.handles.borrow();
        if handles.get(&witness.map) != Some(&(self.map, self.map_arguments.clone())) {
            return Err(ModSemanticQueryAuthority::error("compiled catchall application changed"));
        }
        Ok(())
    }
}
impl ModSemanticQueryAuthority<'_> {
    pub(super) fn compile_retained_catchall(
        &self,
        invocation: u64,
        claim: &ModSemanticCatchallClaim,
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        self.validate_retained_catchall(invocation, claim)?;
        let handles = self.handles.borrow();
        let (owner, owner_arguments) =
            handles.get(&claim.owner).cloned().ok_or_else(|| Self::error("compiled catchall owner issuer absent"))?;
        let (map, map_arguments) =
            handles.get(&claim.map).cloned().ok_or_else(|| Self::error("compiled catchall map issuer absent"))?;
        let binding = CompiledCatchallBinding {
            project: self.project,
            assembly: self.assembly.clone(),
            owner,
            owner_arguments,
            field: claim.field.clone(),
            map,
            map_arguments,
        };
        binding.revalidate(self.db)?;
        Ok(ModCompiledMetadata::with_issuer_payload(claim.clone(), binding))
    }
}
