//! Private generic contribution correspondence; a template is never a concrete shape.
use super::*;
use beskid_analysis::syntax::{EnumDefinition, Node, Spanned, TypeDefinition};
use std::collections::HashSet;

#[derive(Clone)]
pub struct CompiledSerializationTemplate {
    project: ProjectSession,
    assembly: ProgramAssembly,
    owner: AstNodeKey,
    parameters: Vec<String>,
    contribution: Spanned<ProgramItem>,
    contribution_index: u32,
    /// The unbound items of the same generated group, attributed with the contribution.
    members: Vec<ModContributionMember>,
}
impl CompiledSerializationTemplate {
    pub fn contribution_index(&self) -> u32 {
        self.contribution_index
    }
    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }
    pub fn contribution(&self) -> &Spanned<ProgramItem> {
        &self.contribution
    }
    pub fn members(&self) -> &[ModContributionMember] {
        &self.members
    }
    /// See `CompiledSerializationTarget::appended_attribution`.
    pub(crate) fn appended_attribution(
        &self,
        db: &dyn Db,
        project: ProjectSession,
        current: &ProgramAssembly,
    ) -> Result<Option<(SourceUnitId, Vec<crate::AstNodeId>)>, crate::SemanticError> {
        if project != self.project {
            return Err(crate::SemanticError::new("serialization template belongs to another project issuer"));
        }
        serialization_target::appended_group_attribution(
            db,
            &self.assembly,
            self.owner,
            &self.contribution,
            &self.members,
            current,
        )
    }
    /// Current owner declaration of this template and the exact appended
    /// contribution. Original source/package provenance must be unchanged.
    pub(super) fn current_owner(
        &self,
        db: &dyn Db,
        program: &crate::TypedProgram,
    ) -> Result<AstNodeKey, ModSemanticError> {
        let reject = ModSemanticQueryAuthority::error;
        if program.project != self.project {
            return Err(reject("foreign template project"));
        }
        let old_unit = self
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(db, unit.path.clone()) == self.owner.unit)
            .ok_or_else(|| reject("original template unit absent"))?;
        let old_index = self
            .assembly
            .units
            .iter()
            .zip(self.assembly.syntax_indexes.iter())
            .find(|(unit, _)| unit.path == old_unit.path)
            .map(|(_, index)| index)
            .ok_or_else(|| reject("original template index absent"))?;
        let old_node =
            old_index.node_at(&old_unit.program, self.owner.node).ok_or_else(|| reject("original template node absent"))?;
        let current_unit = program
            .assembly
            .units
            .iter()
            .find(|unit| unit.path == old_unit.path)
            .ok_or_else(|| reject("current template unit absent"))?;
        if current_unit.source != old_unit.source
            || program.assembly.package_identities().for_source(&current_unit.path)
                != self.assembly.package_identities().for_source(&old_unit.path)
        {
            return Err(reject("serialization template prepared source/package provenance changed"));
        }
        let original_entry = self.assembly.entry_unit();
        let current_entry = program
            .assembly
            .units
            .iter()
            .find(|unit| unit.path == original_entry.path)
            .ok_or_else(|| reject("serialization template entry correspondence absent"))?;
        serialization_target::exact_append(
            &original_entry.program.node.items,
            &current_entry.program.node.items,
            &self.contribution,
            &self.members,
        )?;
        let syntax = db
            .syntax_unit(SourceUnitId::new(db, current_unit.path.clone()))
            .ok_or_else(|| reject("current template registration absent"))?;
        let candidates = syntax
            .syntax_index(db)
            .ids_of_kind(NodeKind::TypeDefinition)
            .chain(syntax.syntax_index(db).ids_of_kind(NodeKind::EnumDefinition))
            .filter_map(|node| {
                let current = syntax.syntax_index(db).node_at(&current_unit.program, node)?;
                let same = match (old_node.of::<TypeDefinition>(), current.of::<TypeDefinition>()) {
                    (Some(a), Some(b)) => a == b,
                    _ => {
                        old_node.of::<EnumDefinition>().is_some()
                            && old_node.of::<EnumDefinition>() == current.of::<EnumDefinition>()
                    }
                };
                same.then_some(AstNodeKey { unit: syntax.unit(db), generation: program.generation, node })
            })
            .collect::<Vec<_>>();
        let [owner] = candidates.as_slice() else {
            return Err(reject("template correspondence ambiguous/absent"));
        };
        Ok(*owner)
    }

    /// The single specialization gate. Every concrete application of a template
    /// passes here before any shape, descriptor or contribution binds to it.
    fn specialized(
        &self,
        authority: &ModSemanticQueryAuthority<'_>,
        db: &dyn Db,
        program: &crate::TypedProgram,
        arguments: &Arc<[ShapeIdentity]>,
    ) -> Result<(AstNodeKey, crate::DynamicPackingShape), ModSemanticError> {
        if arguments.len() != self.parameters.len() {
            return Err(ModSemanticQueryAuthority::error("serialization template argument arity differs"));
        }
        let owner = self.current_owner(db, program)?;
        authority.validate_key(owner)?;
        let applied = authority.issue(owner, arguments.clone())?;
        Ok((owner, authority.serialization_template_shape(applied)?))
    }

    /// Concrete arguments are current canonical identities, not source DTO names.
    /// Exact append/body/package and canonical contract admission are reissued by
    /// the same concrete-target authority; this method never issues an unapplied handle.
    pub(crate) fn specialize(
        &self,
        db: &dyn Db,
        program: &crate::TypedProgram,
        arguments: Arc<[ShapeIdentity]>,
        environment: &[crate::GenericSubstitution],
    ) -> Result<ReboundSerializationTarget, ModSemanticError> {
        let authority = ModSemanticQueryAuthority::for_registered_assembly(db, program.project, &program.assembly)?;
        let (owner, _) = self.specialized(&authority, db, program, &arguments)?;
        let syntax = db.syntax_unit(owner.unit).ok_or_else(|| ModSemanticQueryAuthority::error("template source absent"))?;
        let body = mod_shape_projection(db, syntax, owner, arguments.clone())
            .map_err(|error| ModSemanticQueryAuthority::error(&error.to_string()))?
            .body;
        let lexical = mod_shape_lexical_path(db, owner)
            .ok_or_else(|| ModSemanticQueryAuthority::error("template lexical correspondence absent"))?;
        serialization_target::CompiledSerializationTarget::from_template(
            self.project,
            self.assembly.clone(),
            self.owner,
            arguments,
            body,
            lexical,
            self.contribution.clone(),
            self.contribution_index,
            self.members.clone(),
        )
        .rebind_in_environment(db, program, environment)
    }
}

/// The per-instantiation contract rebind of a retained template: the serde precedent of a
/// generic `impl<T: Serialize> Serialize for Owner<T>` proven at every concrete use.
///
/// `instance` is one emitted specialization of a method. When that method is declared by the
/// impl block of a retained template contribution, the template is specialized with the
/// owner's arguments taken from the instance's substitutions and its canonical `Serializable`
/// or `Decoder` application is rebound in the instance's complete environment (the encoder,
/// reader and field decoder parameters included). `Ok(None)` means the method is not a
/// retained template contribution. Codegen fails closed on any error.
pub fn serialization_template_instance_rebind(
    db: &dyn Db,
    program: &crate::TypedProgram,
    instance: &crate::GenericSpecializationInstance,
) -> Result<Option<ReboundSerializationTarget>, ModSemanticError> {
    let reject = ModSemanticQueryAuthority::error;
    let entry = program.assembly.entry_unit();
    let entry_id = SourceUnitId::new(db, entry.path.clone());
    let method = instance.declaration;
    if method.unit != entry_id || method.generation != program.generation {
        return Ok(None);
    }
    let index = program.assembly.entry_syntax_index();
    if index.kind(method.node) != Some(NodeKind::MethodDefinition) {
        return Ok(None);
    }
    let Some(block) = index.metadata().get(method.node.0 as usize).and_then(|metadata| metadata.parent) else {
        return Ok(None);
    };
    if index.kind(block) != Some(NodeKind::ImplBlock) {
        return Ok(None);
    }
    let Some(root) = index.ids_of_kind(NodeKind::Program).next() else { return Ok(None) };
    // A program item is its own syntax node (the `Node` wrapper) whose child is the impl block,
    // so the block's parent is the top-level item the template contribution was merged as.
    let Some(item_node) = index.metadata().get(block.0 as usize).and_then(|metadata| metadata.parent) else {
        return Ok(None);
    };
    let Some(item) = entry.program.node.items.iter().find(|item| {
        index.direct_child_id(&entry.program, root, beskid_analysis::syntax_query::DynNodeRef::from(*item))
            == Some(item_node)
    }) else {
        return Ok(None);
    };
    let templates = program
        .assembly
        .compiled_mod_metadata
        .iter()
        .filter_map(|metadata| metadata.issuer_payload::<CompiledSerializationTemplate>())
        .filter(|template| template.contribution == *item)
        .collect::<Vec<_>>();
    let [template] = templates.as_slice() else {
        if templates.is_empty() {
            return Ok(None);
        }
        return Err(reject("serialization template contribution is retained more than once"));
    };
    let owner = template.current_owner(db, program)?;
    let syntax = db.syntax_unit(owner.unit).ok_or_else(|| reject("template source absent"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), owner.node)
        .ok_or_else(|| reject("template declaration absent"))?;
    let parameters = if let Some(record) = node.of::<TypeDefinition>() {
        record.generics.iter().map(|p| p.node.name.clone()).collect::<Vec<_>>()
    } else if let Some(enumeration) = node.of::<EnumDefinition>() {
        enumeration.generics.iter().map(|p| p.node.name.clone()).collect::<Vec<_>>()
    } else {
        return Err(reject("template target is not a nominal declaration"));
    };
    if parameters != template.parameters {
        return Err(reject("template owner generic parameters changed"));
    }
    let mut arguments = Vec::with_capacity(parameters.len());
    for parameter in &parameters {
        let binding = instance
            .substitutions
            .iter()
            .find(|binding| binding.parameter.as_ref() == parameter.as_str())
            .ok_or_else(|| {
                reject(&format!(
                    "serialization template instance does not bind owner parameter `{parameter}`; \
                     a generated adapter method is emitted only in a complete substitution environment"
                ))
            })?;
        arguments.push(binding.source_identity().clone());
    }
    template.specialize(db, program, arguments.into(), &instance.substitutions).map(Some)
}

/// The retained template specialization behind a compiled descriptor getter.
///
/// A generic contribution calls `CompiledShape<Owner<Args>>()` from its own
/// body, so every concrete use of a template reaches codegen as one getter
/// specialization. This binds that application to the retained template whose
/// current owner it names, runs the specialization gate and returns the gated
/// wire shape. `Ok(None)` means no retained template owns the getter target.
pub fn serialization_template_getter_shape(
    db: &dyn Db,
    program: &crate::TypedProgram,
    binding: &crate::SerializationShapeBinding,
) -> Result<Option<crate::DynamicPackingShape>, ModSemanticError> {
    let ShapeIdentity::Nominal { qualified_name, arguments } = binding.target_identity() else {
        return Ok(None);
    };
    if arguments.is_empty() {
        return Ok(None);
    }
    let authority = ModSemanticQueryAuthority::for_registered_assembly(db, program.project, &program.assembly)?;
    let mut selected = None;
    for template in program
        .assembly
        .compiled_mod_metadata
        .iter()
        .filter_map(|metadata| metadata.issuer_payload::<CompiledSerializationTemplate>())
    {
        let owner = template.current_owner(db, program)?;
        if mod_shape_stable_identity(db, owner).as_ref() != Some(qualified_name) {
            continue;
        }
        if selected.replace(template).is_some() {
            return Err(ModSemanticQueryAuthority::error("serialization template owner has repeated retained templates"));
        }
    }
    let Some(template) = selected else { return Ok(None) };
    let (_, shape) = template.specialized(&authority, db, program, arguments)?;
    if shape.nodes() != binding.graph().nodes() {
        return Err(ModSemanticQueryAuthority::error("compiled getter application differs from gated template shape"));
    }
    Ok(Some(shape))
}

impl ModSemanticQueryAuthority<'_> {
    pub(super) fn compile_template(
        &self,
        invocation: u64,
        contribution_index: u32,
        reference: &ModSyntaxNodeRef,
        declaration: &ModSemanticDeclaration,
        contribution: &Spanned<ProgramItem>,
        members: &[ModContributionMember],
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        self.validate_registration()?;
        let owner = self.syntax_key(invocation, reference)?;
        if self.declaration(owner)? != *declaration {
            return Err(Self::error("template declaration differs from issued syntax reference"));
        }
        let syntax = self.db.syntax_unit(owner.unit).ok_or_else(|| Self::error("template unit absent"))?;
        let node = syntax
            .syntax_index(self.db)
            .node_at(syntax.expanded_program(self.db), owner.node)
            .ok_or_else(|| Self::error("template declaration absent"))?;
        let parameters = if let Some(record) = node.of::<TypeDefinition>() {
            record.generics.iter().map(|p| p.node.name.clone()).collect::<Vec<_>>()
        } else if let Some(enumeration) = node.of::<EnumDefinition>() {
            enumeration.generics.iter().map(|p| p.node.name.clone()).collect::<Vec<_>>()
        } else {
            return Err(Self::error("template target is not a nominal declaration"));
        };
        if parameters.is_empty() {
            return Err(Self::error("template target has no generic parameters"));
        }
        super::serialization_target::validate_members(contribution_index, members)?;
        let unit = self
            .registered_unit(&declaration.source_unit)
            .ok_or_else(|| Self::error("template prepared unit absent"))?;
        if self
            .assembly
            .package_identities()
            .validate_source(&unit.path, &unit.source)
            .map_err(|e| Self::error(&e.to_string()))?
            .is_none()
        {
            return Err(Self::error("template stable metadata requires verified package provenance"));
        }
        let Node::ImplBlock(block) = &contribution.node else {
            return Err(Self::error("template requires an owned generic impl contribution"));
        };
        let scope = block.node.generics.iter().map(|p| p.node.name.as_str()).collect::<Vec<_>>();
        if block.node.methods.is_empty()
            || block.node.conformances.is_empty()
            || parameters.iter().any(|p| scope.iter().filter(|name| **name == p).count() != 1)
            || scope.iter().enumerate().any(|(i, name)| scope[..i].contains(name))
        {
            return Err(Self::error("template contribution lacks exact generic scope/methods/conformance"));
        }
        let attribution = super::serialization_target::group_attribution(self.db, owner, contribution, members);
        let metadata = ModCompiledMetadata::with_issuer_payload(
            ModSerializationTemplateClaim {
                contribution_index,
                declaration: declaration.clone(),
                parameters: parameters.clone(),
                attribution,
            },
            CompiledSerializationTemplate {
                project: self.project,
                assembly: self.assembly.clone(),
                owner,
                parameters,
                contribution: contribution.clone(),
                contribution_index,
                members: members.to_vec(),
            },
        );
        self.issued_metadata.borrow_mut().push(metadata.clone());
        Ok(metadata)
    }
}

impl ModSemanticQueryAuthority<'_> {
    /// Concrete eligibility of one specialized serialization template application.
    ///
    /// Concrete targets get their closure from the Mod's semantic pass, which
    /// pairs declared field syntax with issued shapes and records each proven
    /// Foundation container as a `ContainerSite` on the declared type node. A
    /// generic template has no such pass: its generated adapters treat every
    /// declared nominal, including a `List`, `Map` or `Option` reached through a
    /// type parameter, as an ordinary nominal with its own contribution. The
    /// container proof therefore keeps one authority, the concrete closure, and
    /// a specialized template field that is a Foundation container fails here.
    ///
    /// The remaining gate is the concrete one: scalar pointer/never, function,
    /// string-key Map, resource and provenance rejection through
    /// `serialization_shape`, and direct record recursion, which needs an
    /// array, container or enum payload indirection. Fields skipped in both
    /// directions are not serialized data and are exempt, as in the Mod.
    /// Every `Default` factory is proven here against the substituted field
    /// type, because the Mod has no issued field types for a template.
    pub fn serialization_template_shape(
        &self,
        handle: ModSemanticHandle,
    ) -> Result<crate::DynamicPackingShape, ModSemanticError> {
        self.validate_registration()?;
        let (key, arguments) = self
            .handles
            .borrow()
            .get(&handle)
            .cloned()
            .ok_or_else(|| Self::error("serialization template application has foreign/stale issuer"))?;
        self.validate_key(key)?;
        if arguments.is_empty() {
            return Err(Self::error("serialization template specialization requires concrete type arguments"));
        }
        let qualified_name =
            mod_shape_stable_identity(self.db, key).ok_or_else(|| Self::error("template nominal identity absent"))?;
        let root = ShapeIdentity::Nominal { qualified_name, arguments: arguments.clone() };
        let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("template source absent"))?;
        let projection =
            mod_shape_projection(self.db, syntax, key, arguments).map_err(|error| Self::error(&error.to_string()))?;
        let mut walk = TemplateWalk { remaining: HANDLE_LIMIT, seen: HashSet::from([root.clone()]) };
        let policy_error = |error: crate::SemanticError| Self::error(&error.to_string());
        match &projection.body {
            ShapeBodyProjection::Record(fields) => {
                for (field, name, ty) in fields {
                    let policy = crate::serialization_field_policy(self.db, *field).map_err(policy_error)?;
                    self.template_default_factory(*field, &policy, ty)?;
                    if policy.absent_from_wire() {
                        continue;
                    }
                    self.template_field(name, ty, std::slice::from_ref(&root), &mut walk, 0)?;
                }
            }
            ShapeBodyProjection::Enum(variants) => {
                for variant in variants {
                    for (field, name, ty) in &variant.fields {
                        let policy = crate::serialization_payload_field_policy(self.db, *field).map_err(policy_error)?;
                        self.template_default_factory(*field, &policy, ty)?;
                        if policy.absent_from_wire() {
                            continue;
                        }
                        self.template_field(name, ty, &[], &mut walk, 0)?;
                    }
                }
            }
        }
        self.serialization_shape(handle)
    }

    /// Default factory proof at specialization, as the Mod proves it for a
    /// concrete target: the route resolves from the compilation root where the
    /// generated decoder calls it, to one visible, non-generic, zero-parameter
    /// function whose result is exactly the substituted field type.
    fn template_default_factory(
        &self,
        field: AstNodeKey,
        policy: &crate::SerializationFieldPolicy,
        ty: &ShapeIdentity,
    ) -> Result<(), ModSemanticError> {
        let Some(route) = policy.default_factory() else { return Ok(()) };
        let located = |code: &str, message: &str| -> ModSemanticError {
            match self.declaration(field) {
                Ok(declaration) => Self::error(&format!(
                    "{code}: {message} ({}:{}:{}..{}:{})",
                    declaration.source_unit.display(),
                    declaration.span.line_col_start.0,
                    declaration.span.line_col_start.1,
                    declaration.span.line_col_end.0,
                    declaration.span.line_col_end.1,
                )),
                Err(error) => error,
            }
        };
        let caller = AstNodeKey {
            unit: SourceUnitId::new(self.db, self.assembly.entry_unit().path.clone()),
            generation: self.assembly.generation,
            node: crate::AstNodeId(0),
        };
        let declaration = crate::semantic_contract::mod_shapes::mod_resolve_function_route(self.db, caller, route)
            .ok_or_else(|| {
                located("SerializationDefaultFactory", "default factory does not resolve to one function from the compilation root")
            })?;
        if declaration.generation != self.assembly.generation
            || self.registered_unit(declaration.unit.path(self.db)).is_none()
        {
            return Err(located("SerializationDefaultFactory", "default factory is outside the registered assembly generation"));
        }
        if !crate::semantic_contract::mod_shapes::mod_function_accessible(self.db, caller, declaration) {
            return Err(located("SerializationDefaultFactory", "default factory is not visible from the compilation root"));
        }
        let signature = crate::semantic_contract::mod_shapes::mod_function_signature_projection(self.db, declaration)
            .map_err(|_| located("SerializationDefaultFactory", "default factory signature is unavailable or generic"))?;
        if signature.generic_count != 0 {
            return Err(located("SerializationDefaultFactory", "default factory must not be generic"));
        }
        if !signature.parameters.is_empty() {
            return Err(located("SerializationDefaultFactory", "default factory must take no arguments"));
        }
        // Equal canonical identities issue one handle, so this is handle identity.
        let mut remaining = HANDLE_LIMIT;
        let expected = self.field_type(ty, 0, &mut remaining)?;
        let returned = self.field_type(&signature.result, 0, &mut remaining)?;
        if signature.result != *ty || returned != expected {
            return Err(located(
                "SerializationDefaultType",
                "default factory must return exactly the specialized field type",
            ));
        }
        Ok(())
    }

    /// One serialized field owned by the template. Arrays stay template-owned
    /// syntax, so a container element of an array is rejected as well.
    fn template_field(
        &self,
        field: &str,
        identity: &ShapeIdentity,
        direct: &[ShapeIdentity],
        walk: &mut TemplateWalk,
        depth: usize,
    ) -> Result<(), ModSemanticError> {
        match identity {
            ShapeIdentity::Array(element) => self.template_field(field, element, &[], walk, depth + 1),
            ShapeIdentity::Nominal { qualified_name, arguments } => {
                let key = self.nominal_key(qualified_name)?;
                if crate::canonical_container_kind(self.db, key, arguments.len()).is_some() {
                    return Err(Self::error(&format!(
                        "SerializationTemplateContainer: field `{field}` of a generic serialization template is a \
                         Foundation List, Map or Option after specialization; container proof is issued only for \
                         concrete targets, so declare the container field in a concrete record"
                    )));
                }
                self.template_nested(identity, direct, walk, depth)
            }
            ShapeIdentity::Abi(_) | ShapeIdentity::Function { .. } => Ok(()),
        }
    }

    /// Semantic-only recursion check of the specialized closure. `direct` holds
    /// the nominal identities reached through direct record fields only.
    fn template_nested(
        &self,
        identity: &ShapeIdentity,
        direct: &[ShapeIdentity],
        walk: &mut TemplateWalk,
        depth: usize,
    ) -> Result<(), ModSemanticError> {
        if depth > DEPTH_LIMIT || walk.remaining == 0 {
            return Err(Self::error("serialization template closure bound exhausted"));
        }
        walk.remaining -= 1;
        match identity {
            ShapeIdentity::Abi(_) | ShapeIdentity::Function { .. } => Ok(()),
            ShapeIdentity::Array(element) => self.template_nested(element, &[], walk, depth + 1),
            ShapeIdentity::Nominal { qualified_name, arguments } => {
                if direct.contains(identity) {
                    return Err(Self::error(
                        "SerializationRecursiveCycle: a specialized record reaches itself through direct record \
                         fields; recursion requires an array, container or enum indirection",
                    ));
                }
                let key = self.nominal_key(qualified_name)?;
                if crate::canonical_container_kind(self.db, key, arguments.len()).is_some() {
                    for argument in arguments.iter() {
                        self.template_nested(argument, &[], walk, depth + 1)?;
                    }
                    return Ok(());
                }
                if !walk.seen.insert(identity.clone()) {
                    return Ok(());
                }
                let syntax =
                    self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("template closure source absent"))?;
                let projection = mod_shape_projection(self.db, syntax, key, arguments.clone())
                    .map_err(|error| Self::error(&error.to_string()))?;
                match projection.body {
                    ShapeBodyProjection::Record(fields) => {
                        let mut next = direct.to_vec();
                        next.push(identity.clone());
                        for (_, _, ty) in &fields {
                            self.template_nested(ty, &next, walk, depth + 1)?;
                        }
                    }
                    ShapeBodyProjection::Enum(variants) => {
                        for variant in &variants {
                            for (_, _, ty) in &variant.fields {
                                self.template_nested(ty, &[], walk, depth + 1)?;
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

struct TemplateWalk {
    remaining: usize,
    seen: HashSet<ShapeIdentity>,
}
