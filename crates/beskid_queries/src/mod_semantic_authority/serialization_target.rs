//! Exact owned typed-contribution correspondence. Capture is not conformance admission.
use super::*;
use beskid_analysis::syntax::{Node, Spanned};

#[derive(Clone)]
pub struct CompiledSerializationTarget {
    project: ProjectSession,
    assembly: ProgramAssembly,
    owner: AstNodeKey,
    arguments: Arc<[ShapeIdentity]>,
    body: ShapeBodyProjection,
    lexical: Vec<String>,
    contribution: Spanned<ProgramItem>,
    contribution_index: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationContributionKind {
    Encoder,
    Decoder,
}
#[derive(Clone)]
pub struct ReboundSerializationTarget {
    kind: SerializationContributionKind,
    receiver: AstNodeKey,
    owner: AstNodeKey,
    implementation: AstNodeKey,
    arguments: Arc<[ShapeIdentity]>,
    applications: Vec<(crate::AppliedContractIdentity, Arc<[(AstNodeKey, AstNodeKey)]>)>,
}
impl ReboundSerializationTarget {
    pub fn kind(&self) -> SerializationContributionKind {
        self.kind
    }
    pub fn receiver(&self) -> AstNodeKey {
        self.receiver
    }
    pub fn owner(&self) -> AstNodeKey {
        self.owner
    }
    pub fn implementation(&self) -> AstNodeKey {
        self.implementation
    }
    pub fn arguments(&self) -> &[ShapeIdentity] {
        &self.arguments
    }
    pub fn applications(&self) -> &[(crate::AppliedContractIdentity, Arc<[(AstNodeKey, AstNodeKey)]>)] {
        &self.applications
    }
    /// The retained target cannot substitute caller-selected graph facts. Reissue its
    /// exact current application through the registered typed assembly.
    pub fn shape(
        &self,
        db: &dyn Db,
        program: &crate::TypedProgram,
    ) -> Result<crate::DynamicPackingShape, ModSemanticError> {
        let authority = ModSemanticQueryAuthority::for_registered_assembly(db, program.project, &program.assembly)?;
        let handle = authority.issue(self.owner, self.arguments.clone())?;
        authority.serialization_shape(handle)
    }
}
impl CompiledSerializationTarget {
    pub(super) fn from_template(
        project: ProjectSession,
        assembly: ProgramAssembly,
        owner: AstNodeKey,
        arguments: Arc<[ShapeIdentity]>,
        body: ShapeBodyProjection,
        lexical: Vec<String>,
        contribution: Spanned<ProgramItem>,
        contribution_index: u32,
    ) -> Self {
        Self { project, assembly, owner, arguments, body, lexical, contribution, contribution_index }
    }

    pub fn original_owner(&self) -> AstNodeKey {
        self.owner
    }
    pub fn contribution_index(&self) -> u32 {
        self.contribution_index
    }
    pub fn contribution(&self) -> &Spanned<ProgramItem> {
        &self.contribution
    }
    /// Reissue only after exact append correspondence. Current contract admission remains
    /// independent: this returns a target key, never an executable conformance witness.
    pub fn rebind(
        &self,
        db: &dyn Db,
        program: &crate::TypedProgram,
    ) -> Result<ReboundSerializationTarget, ModSemanticError> {
        self.rebind_in_environment(db, program, &[])
    }
    pub(crate) fn rebind_in_environment(
        &self,
        db: &dyn Db,
        program: &crate::TypedProgram,
        environment: &[crate::GenericSubstitution],
    ) -> Result<ReboundSerializationTarget, ModSemanticError> {
        let reject = ModSemanticQueryAuthority::error;
        if program.project != self.project {
            return Err(reject("serialization contribution belongs to another project issuer"));
        }
        let original_unit = self
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(db, unit.path.clone()) == self.owner.unit)
            .ok_or_else(|| reject("original serialization target source absent"))?;
        let current_unit = program
            .assembly
            .units
            .iter()
            .find(|unit| unit.path == original_unit.path)
            .ok_or_else(|| reject("serialization target source was removed"))?;
        if current_unit.source != original_unit.source
            || program.assembly.package_identities().for_source(&current_unit.path)
                != self.assembly.package_identities().for_source(&original_unit.path)
        {
            return Err(reject("serialization target prepared source/package provenance changed"));
        }
        let current_authority =
            ModSemanticQueryAuthority::for_registered_assembly(db, program.project, &program.assembly)?;
        // The actual merger appends typed items to the entry. Require preserved original
        // items plus the exact owned item, rather than accepting an equal leaf name.
        let original_entry = self.assembly.entry_unit();
        let current_entry = program
            .assembly
            .units
            .iter()
            .find(|unit| unit.path == original_entry.path)
            .ok_or_else(|| reject("serialization contribution entry correspondence absent"))?;
        exact_append(&original_entry.program.node.items, &current_entry.program.node.items, &self.contribution)?;
        let old_syntax = self
            .assembly
            .syntax_indexes
            .iter()
            .zip(self.assembly.units.iter())
            .find(|(_, unit)| unit.path == original_unit.path)
            .map(|(index, _)| index)
            .ok_or_else(|| reject("serialization target original syntax index absent"))?;
        let old_node = old_syntax
            .node_at(&original_unit.program, self.owner.node)
            .ok_or_else(|| reject("serialization original target absent"))?;
        let syntax = db
            .syntax_unit(SourceUnitId::new(db, current_unit.path.clone()))
            .ok_or_else(|| reject("serialization current target not registered"))?;
        let index = syntax.syntax_index(db);
        let mut candidates = Vec::new();
        for node in index.ids_of_kind(NodeKind::TypeDefinition).chain(index.ids_of_kind(NodeKind::EnumDefinition)) {
            let key = AstNodeKey { unit: syntax.unit(db), generation: program.generation, node };
            if mod_shape_lexical_path(db, key).as_ref() == Some(&self.lexical)
                && index.node_at(&current_unit.program, node).is_some_and(|new_node| {
                    match (
                        old_node.of::<beskid_analysis::syntax::TypeDefinition>(),
                        new_node.of::<beskid_analysis::syntax::TypeDefinition>(),
                    ) {
                        (Some(a), Some(b)) => a == b,
                        _ => {
                            old_node.of::<beskid_analysis::syntax::EnumDefinition>()
                                == new_node.of::<beskid_analysis::syntax::EnumDefinition>()
                        }
                    }
                })
            {
                candidates.push(key);
            }
        }
        let [owner] = candidates.as_slice() else {
            return Err(reject("serialization target correspondence missing/ambiguous"));
        };
        require_contribution_field_access(db, *owner, &current_entry.path)?;
        // A copied target claim paired with an unrelated implementation must not
        // authorize a preexisting conformance on the target.
        let entry_syntax = db
            .syntax_unit(SourceUnitId::new(db, current_entry.path.clone()))
            .ok_or_else(|| reject("serialization contribution entry is not registered"))?;
        let Node::ImplBlock(expected_block) = &self.contribution.node else {
            return Err(reject("serialization contribution kind changed"));
        };
        let mut implementations = Vec::new();
        for node in entry_syntax.syntax_index(db).ids_of_kind(NodeKind::ImplBlock) {
            if entry_syntax
                .syntax_index(db)
                .node_at(&current_entry.program, node)
                .and_then(|item| item.of::<beskid_analysis::syntax::ImplBlock>())
                .is_some_and(|block| block == &expected_block.node)
            {
                implementations.push(AstNodeKey { unit: entry_syntax.unit(db), generation: program.generation, node });
            }
        }
        let [implementation] = implementations.as_slice() else {
            return Err(reject("serialization implementation correspondence missing/ambiguous"));
        };
        let projection =
            mod_shape_projection(db, syntax, *owner, self.arguments.clone()).map_err(|e| reject(&e.to_string()))?;
        if !same_semantic_body(&self.body, &projection.body) {
            return Err(reject("serialization target resolved field/variant applications changed"));
        }
        let (receiver, receiver_arguments) =
            crate::semantic_contract::serialization_contribution_receiver_arguments_in_environment(
                db,
                *owner,
                &self.arguments,
                *implementation,
                environment,
            )
            .map_err(|error| reject(&error.to_string()))?;
        let methods = crate::semantic_contract::serialization_target_contract_methods_in_environment(
            db,
            receiver,
            &receiver_arguments,
            *implementation,
            environment,
        )
        .map_err(|error| reject(&error.to_string()))?;
        let canonical = methods
            .into_iter()
            .filter(|(application, _)| {
                db.syntax_unit(application.declaration().unit).is_some_and(|syntax| {
                    syntax.accepts_key(db, application.declaration())
                        && crate::canonical_corelib_source_path(db, application.declaration()).as_deref()
                            == Some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH)
                        && matches!(
                            crate::corelib_source_authority::registered_declaration_name(db, application.declaration())
                                .as_deref(),
                            Some("Serializable" | "Decoder")
                        )
                })
            })
            .collect::<Vec<_>>();
        if canonical.len() != 1 || canonical[0].0.arguments().len() != 1 {
            return Err(reject(
                "serialization contribution lacks one exact canonical Serializable or Decoder application",
            ));
        }
        let kind = match crate::corelib_source_authority::registered_declaration_name(db, canonical[0].0.declaration())
            .as_deref()
        {
            Some("Serializable") => {
                if receiver != *owner || receiver_arguments.as_ref() != self.arguments.as_ref() {
                    return Err(reject(
                        "Serializable contribution receiver differs from exact issued target application",
                    ));
                }
                let ShapeIdentity::Nominal { qualified_name, arguments: encoder_arguments } =
                    &canonical[0].0.arguments()[0]
                else {
                    return Err(reject("Serializable encoder argument is not an exact nominal application"));
                };
                let encoder = current_authority.nominal_key(qualified_name.as_ref())?;
                let encoder_methods =
                    crate::semantic_contract::serialization_encoder_methods(db, encoder, encoder_arguments.as_ref())
                        .map_err(|error| reject(&error.to_string()))?;
                if encoder_methods.is_empty() {
                    return Err(reject("Serializable encoder lacks canonical Encoder implementation"));
                }
                SerializationContributionKind::Encoder
            }
            Some("Decoder") => {
                let identity =
                    mod_shape_stable_identity(db, *owner).ok_or_else(|| reject("decoder target identity absent"))?;
                let expected = ShapeIdentity::Nominal { qualified_name: identity, arguments: self.arguments.clone() };
                if canonical[0].0.arguments()[0] != expected {
                    return Err(reject("Decoder contribution output differs from exact issued target application"));
                }
                SerializationContributionKind::Decoder
            }
            _ => return Err(reject("serialization contribution canonical contract differs")),
        };
        let handle = current_authority.issue(*owner, self.arguments.clone())?;
        current_authority.serialization_shape(handle)?;
        Ok(ReboundSerializationTarget {
            kind,
            receiver,
            owner: *owner,
            implementation: *implementation,
            arguments: self.arguments.clone(),
            applications: canonical,
        })
    }
}
impl ModSemanticQueryAuthority<'_> {
    pub(super) fn compile_target(
        &self,
        invocation: u64,
        contribution_index: u32,
        target: ModSemanticHandle,
        contribution: &Spanned<ProgramItem>,
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        self.validate_registration()?;
        self.admit_semantic_invocation(invocation)?;
        let (owner, arguments) = self
            .handles
            .borrow()
            .get(&target)
            .cloned()
            .ok_or_else(|| Self::error("serialization target has foreign/stale invocation issuer"))?;
        self.validate_key(owner)?;
        self.serialization_shape(target)?;
        require_contribution_field_access(self.db, owner, &self.assembly.entry_unit().path)?;
        let Node::ImplBlock(block) = &contribution.node else {
            return Err(Self::error("serialization target requires an owned typed impl contribution"));
        };
        if block.node.methods.is_empty() || block.node.conformances.is_empty() {
            return Err(Self::error("serialization contribution lacks concrete methods/conformance"));
        }
        let syntax =
            self.db.syntax_unit(owner.unit).ok_or_else(|| Self::error("serialization target source absent"))?;
        let body = mod_shape_projection(self.db, syntax, owner, arguments.clone())
            .map_err(|e| Self::error(&e.to_string()))?
            .body;
        let lexical = mod_shape_lexical_path(self.db, owner)
            .ok_or_else(|| Self::error("serialization target lexical identity absent"))?;
        let proof = CompiledSerializationTarget {
            project: self.project,
            assembly: self.assembly.clone(),
            owner,
            arguments,
            body,
            lexical,
            contribution: contribution.clone(),
            contribution_index,
        };
        Ok(ModCompiledMetadata::with_issuer_payload(
            ModSerializationTargetClaim { contribution_index, target, generation: self.assembly.generation },
            proof,
        ))
    }
}

fn same_semantic_body(a: &ShapeBodyProjection, b: &ShapeBodyProjection) -> bool {
    fn fields(a: &[(AstNodeKey, String, ShapeIdentity)], b: &[(AstNodeKey, String, ShapeIdentity)]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|((_, an, at), (_, bn, bt))| an == bn && at == bt)
    }
    match (a, b) {
        (ShapeBodyProjection::Record(a), ShapeBodyProjection::Record(b)) => fields(a, b),
        (ShapeBodyProjection::Enum(a), ShapeBodyProjection::Enum(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| a.name == b.name && a.ordinal == b.ordinal && fields(&a.fields, &b.fields))
        }
        _ => false,
    }
}

/// The compiler merger appends owned typed items to the entry. The current entry
/// must preserve every original item and contain the exact contribution once.
pub(super) fn exact_append(
    old_items: &[Spanned<ProgramItem>],
    new_items: &[Spanned<ProgramItem>],
    contribution: &Spanned<ProgramItem>,
) -> Result<(), ModSemanticError> {
    if new_items.len() < old_items.len()
        || new_items[..old_items.len()] != old_items[..]
        || new_items[old_items.len()..].iter().filter(|item| **item == *contribution).count() != 1
    {
        return Err(ModSemanticQueryAuthority::error(
            "serialization contribution differs from exact compiler append correspondence",
        ));
    }
    Ok(())
}

/// Serialization contributions are appended to the entry source unit, so a generated adapter
/// reads and constructs its target from the entry unit. Rust places derive output in the
/// module of the input item, where private fields are visible; this merger does the same only
/// when the target is declared in the entry unit. A target record declared in another source
/// unit with any non-`pub` value field cannot be bound: its adapter would read or build a field
/// that is private to the declaring unit (E1211). Rejecting at the binding keeps the field
/// visibility policy single-sourced; no contribution is attributed to another unit.
pub(super) fn require_contribution_field_access(
    db: &dyn Db,
    owner: AstNodeKey,
    entry: &std::path::Path,
) -> Result<(), ModSemanticError> {
    let contribution = AstNodeKey {
        unit: SourceUnitId::new(db, entry.to_path_buf()),
        generation: owner.generation,
        node: crate::AstNodeId(0),
    };
    let inaccessible = crate::semantic_contract::serialization_contribution_inaccessible_fields(db, contribution, owner)
        .map_err(|error| ModSemanticQueryAuthority::error(&error.to_string()))?;
    let Some(field) = inaccessible.first() else {
        return Ok(());
    };
    let name = mod_shape_lexical_path(db, owner).map(|path| path.join(".")).unwrap_or_else(|| "<unnamed>".into());
    Err(ModSemanticQueryAuthority::error(&format!(
        "serialization target `{name}` is declared in source unit `{}` and its field `{field}` is not `pub`; \
         generated serialization adapters are merged into the entry source unit `{}` and cannot read or \
         construct a field that is private to another source unit (E1211); mark the field `pub` or declare \
         the type in the entry source unit",
        owner.unit.path(db).display(),
        entry.display(),
    )))
}
