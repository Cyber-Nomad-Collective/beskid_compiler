//! Invocation-lifetime projection of registered Salsa type identities.
mod canonical_paths;
mod serialization_template;
pub use serialization_template::{CompiledSerializationTemplate, serialization_template_getter_shape};
mod serialization_target;
pub use serialization_target::{
    CompiledSerializationTarget, ReboundSerializationTarget, SerializationContributionKind,
};
mod catchall;
mod functions;
use crate::semantic_contract::mod_shapes::{
    ShapeBodyProjection, ShapeIdentity, mod_shape_is_managed, mod_shape_lexical_path, mod_shape_projection,
    mod_shape_stable_identity,
};
use crate::{AstNodeKey, Db, ProjectSession, SourceUnitId};
use beskid_analysis::{
    mod_host::*,
    projects::ProgramAssembly,
    syntax::{PrimitiveType, SyntaxGenerationId},
    syntax_query::NodeKind,
};
pub use catchall::{CompiledCatchallBinding, ModSemanticCatchall, ReboundCatchallBinding};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
const HANDLE_LIMIT: usize = 65536;
const DEPTH_LIMIT: usize = 128;

pub struct ModSemanticQueryAuthority<'db> {
    pub(crate) db: &'db dyn Db,
    project: ProjectSession,
    pub(crate) assembly: &'db ProgramAssembly,
    pub(crate) syntax_state: RefCell<crate::mod_syntax_authority::SyntaxState>,
    handles: RefCell<HashMap<ModSemanticHandle, (AstNodeKey, Arc<[ShapeIdentity]>)>>,
    pub(crate) catchalls: RefCell<HashMap<(u64, u64), ModSemanticCatchall>>,
}
impl<'db> ModSemanticQueryAuthority<'db> {
    pub fn for_registered_assembly(
        db: &'db dyn Db,
        project: ProjectSession,
        assembly: &'db ProgramAssembly,
    ) -> Result<Self, ModSemanticError> {
        let authority = Self {
            db,
            project,
            assembly,
            handles: RefCell::new(HashMap::new()),
            catchalls: RefCell::new(HashMap::new()),
            syntax_state: RefCell::default(),
        };
        authority.validate_registration()?;
        Ok(authority)
    }
    pub(crate) fn error(message: &str) -> ModSemanticError {
        ModSemanticError(message.into())
    }
    /// Resolve an issued source-unit path to its registered assembly unit.
    ///
    /// Issued references carry the interned `SourceUnitId` path, which is normalized
    /// (for example `/var` resolves to `/private/var` on macOS). Registration identity
    /// is therefore the interned id, never the raw prepared path. Paths that do not
    /// intern to a registered unit stay rejected.
    pub(crate) fn registered_unit(
        &self,
        issued: &std::path::Path,
    ) -> Option<&'db beskid_analysis::projects::SourceUnit> {
        let issued = SourceUnitId::new(self.db, issued.to_path_buf());
        self.assembly.units.iter().find(|unit| SourceUnitId::new(self.db, unit.path.clone()) == issued)
    }
    pub(crate) fn validate_registration(&self) -> Result<(), ModSemanticError> {
        if self.assembly.units.is_empty() {
            return Err(Self::error("empty semantic assembly"));
        }
        {
            let registry = self.db.syntax_dependency_registry().lock().expect("syntax dependency registry");
            if registry.package_identities.get(&(self.project, self.assembly.generation))
                != Some(self.assembly.package_identities())
            {
                return Err(Self::error("package proof is not bound to registered project generation"));
            }
        }
        self.assembly.package_identities().validate().map_err(|error| Self::error(&error.to_string()))?;
        for unit in self.assembly.units.iter() {
            self.assembly
                .package_identities()
                .validate_source(&unit.path, &unit.source)
                .map_err(|error| Self::error(&error.to_string()))?;
            let id = SourceUnitId::new(self.db, unit.path.clone());
            let syntax = self.db.syntax_unit(id).ok_or_else(|| Self::error("unregistered semantic source unit"))?;
            if syntax.project(self.db) != self.project
                || syntax.generation(self.db) != self.assembly.generation
                || syntax.expanded_program(self.db).as_ref() != &unit.program
            {
                return Err(Self::error("semantic assembly registration differs from current owner/generation/tree"));
            }
        }
        Ok(())
    }
    fn validate_key(&self, key: AstNodeKey) -> Result<(), ModSemanticError> {
        self.validate_registration()?;
        if key.generation != self.assembly.generation
            || !self.assembly.units.iter().any(|unit| SourceUnitId::new(self.db, unit.path.clone()) == key.unit)
        {
            return Err(Self::error("foreign or stale semantic key"));
        }
        let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("unknown semantic unit"))?;
        if !syntax.accepts_key(self.db, key)
            || !matches!(
                syntax.syntax_index(self.db).kind(key.node),
                Some(NodeKind::TypeDefinition | NodeKind::EnumDefinition)
            )
        {
            return Err(Self::error("semantic key is not a current type declaration"));
        }
        Ok(())
    }
    /// Bound shape graph from a current invocation-owned instantiated handle, never caller
    /// type arguments. Pointer/resource eligibility shares the catchall shape-closure gate.
    pub fn serialization_shape(
        &self,
        handle: ModSemanticHandle,
    ) -> Result<crate::DynamicPackingShape, ModSemanticError> {
        self.validate_registration()?;
        let (key, arguments) = self
            .handles
            .borrow()
            .get(&handle)
            .cloned()
            .ok_or_else(|| Self::error("serialization target has foreign/stale issuer"))?;
        self.validate_key(key)?;
        let qualified_name = mod_shape_stable_identity(self.db, key)
            .ok_or_else(|| Self::error("serialization nominal identity absent"))?;
        let identity = ShapeIdentity::Nominal { qualified_name, arguments };
        let mut remaining = HANDLE_LIMIT;
        // The target's own fields skipped in both directions are not serialized
        // data, as in the Mod's closure; nested shapes are checked in full.
        self.eligible_serialization_value(&identity, 0, &mut remaining, &mut std::collections::HashSet::new(), true)?;
        crate::semantic_contract::project_serialization_shape(self.db, key, &identity)
            .map_err(|error| Self::error(&error.to_string()))
    }
    pub fn resolve_type(&self, key: AstNodeKey) -> Result<ModSemanticHandle, ModSemanticError> {
        self.issue(key, Arc::from([]))
    }
    fn issue(&self, key: AstNodeKey, arguments: Arc<[ShapeIdentity]>) -> Result<ModSemanticHandle, ModSemanticError> {
        self.validate_key(key)?;
        let mut handles = self.handles.borrow_mut();
        if let Some((handle, _)) = handles.iter().find(|(_, identity)| identity.0 == key && identity.1 == arguments) {
            return Ok(*handle);
        }
        if handles.len() >= HANDLE_LIMIT {
            return Err(Self::error("semantic handle limit exceeded"));
        }
        let token = NEXT_HANDLE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value.checked_add(1))
            .map_err(|_| Self::error("semantic issuer tokens exhausted"))?;
        let handle = ModSemanticHandle::from_token(token);
        handles.insert(handle, (key, arguments));
        Ok(handle)
    }
    fn nominal_key(&self, name: &str) -> Result<AstNodeKey, ModSemanticError> {
        let mut result = None;
        for unit in self.assembly.units.iter() {
            let id = SourceUnitId::new(self.db, unit.path.clone());
            let syntax = self.db.syntax_unit(id).ok_or_else(|| Self::error("unknown semantic unit"))?;
            for node in [NodeKind::TypeDefinition, NodeKind::EnumDefinition]
                .into_iter()
                .flat_map(|kind| syntax.syntax_index(self.db).ids_of_kind(kind))
            {
                let key = AstNodeKey { unit: id, generation: self.assembly.generation, node };
                if mod_shape_stable_identity(self.db, key).as_deref() == Some(name) {
                    if result.replace(key).is_some() {
                        return Err(Self::error("ambiguous canonical nominal identity"));
                    }
                }
            }
        }
        result.ok_or_else(|| Self::error("unavailable canonical nominal identity"))
    }
    fn field_type(
        &self,
        identity: &ShapeIdentity,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<ModSemanticFieldType, ModSemanticError> {
        if depth > DEPTH_LIMIT || *remaining == 0 {
            return Err(Self::error("semantic identity traversal limit"));
        }
        *remaining -= 1;
        match identity {
            ShapeIdentity::Abi(ty) => {
                use crate::SemanticTypeId as S;
                let primitive = match *ty {
                    S::UNIT => PrimitiveType::Unit,
                    S::BOOL => PrimitiveType::Bool,
                    S::I8 => PrimitiveType::I8,
                    S::I16 => PrimitiveType::I16,
                    S::U16 => PrimitiveType::U16,
                    S::U64 => PrimitiveType::U64,
                    S::F32 => PrimitiveType::F32,
                    S::I32 => PrimitiveType::I32,
                    S::I64 => PrimitiveType::I64,
                    S::U8 => PrimitiveType::U8,
                    S::U32 => PrimitiveType::U32,
                    S::F64 => PrimitiveType::F64,
                    S::CHAR => PrimitiveType::Char,
                    S::STRING => PrimitiveType::String,
                    S::WORD => PrimitiveType::Word,
                    S::POINTER => PrimitiveType::Pointer,
                    S::NEVER => PrimitiveType::Never,
                    _ => return Err(Self::error("unavailable primitive semantic identity")),
                };
                Ok(ModSemanticFieldType::Scalar(primitive))
            }
            ShapeIdentity::Nominal { qualified_name, arguments } => {
                // Validate the complete bounded application before issuing its opaque identity.
                for argument in arguments.iter() {
                    self.field_type(argument, depth + 1, remaining)?;
                }
                Ok(ModSemanticFieldType::Nominal(self.issue(self.nominal_key(qualified_name)?, arguments.clone())?))
            }
            ShapeIdentity::Array(element) => {
                Ok(ModSemanticFieldType::Array(Box::new(self.field_type(element, depth + 1, remaining)?)))
            }
            ShapeIdentity::Function { .. } => {
                Err(Self::error("function shape unavailable for structural serialization"))
            }
        }
    }
    fn declaration(&self, key: AstNodeKey) -> Result<ModSemanticDeclaration, ModSemanticError> {
        let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("unknown semantic declaration"))?;
        let span = syntax
            .syntax_index(self.db)
            .node_at(syntax.expanded_program(self.db), key.node)
            .ok_or_else(|| Self::error("unknown semantic declaration node"))?
            .span()
            .ok_or_else(|| Self::error("semantic declaration span unavailable"))?;
        Ok(ModSemanticDeclaration {
            source_unit: key.unit.path(self.db).clone(),
            generation: key.generation,
            node: key.node,
            span,
        })
    }
}
impl ModSemanticAuthority for ModSemanticQueryAuthority<'_> {
    fn plan_canonical_paths(
        &self,
        invocation: u64,
        reference: &ModSyntaxNodeRef,
        requested: &[String],
    ) -> Result<Vec<ModCanonicalPath>, ModSemanticError> {
        self.canonical_paths(invocation, reference, requested)
    }
    fn resolve_function(
        &self,
        invocation: u64,
        reference: &ModSyntaxNodeRef,
        path: &[String],
    ) -> Result<ModSemanticFunctionSignature, ModSemanticError> {
        self.resolve_function_signature(invocation, reference, path)
    }
    /// The Mod-facing view of the host gate shared with target compilation; never a second rule.
    fn check_serializable(&self, handle: ModSemanticHandle) -> Result<(), ModSemanticError> {
        self.serialization_shape(handle).map(|_| ())
    }
    fn compile_serialization_target(
        &self,
        invocation: u64,
        contribution_index: u32,
        target: ModSemanticHandle,
        contribution: &beskid_analysis::syntax::Spanned<ProgramItem>,
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        self.compile_target(invocation, contribution_index, target, contribution)
    }
    fn compile_serialization_template(
        &self,
        invocation: u64,
        contribution_index: u32,
        reference: &ModSyntaxNodeRef,
        declaration: &ModSemanticDeclaration,
        contribution: &beskid_analysis::syntax::Spanned<ProgramItem>,
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        self.compile_template(invocation, contribution_index, reference, declaration, contribution)
    }
    fn compile_catchall(
        &self,
        invocation: u64,
        claim: &ModSemanticCatchallClaim,
    ) -> Result<ModCompiledMetadata, ModSemanticError> {
        self.compile_retained_catchall(invocation, claim)
    }
    fn issue_catchall(
        &self,
        invocation: u64,
        owner: ModSemanticHandle,
        field: &ModSemanticDeclaration,
    ) -> Result<ModSemanticCatchallClaim, ModSemanticError> {
        self.retain_catchall(invocation, owner, field)
    }
    fn lookup_catchall(&self, invocation: u64, token: u64) -> Result<ModSemanticCatchallClaim, ModSemanticError> {
        self.lookup_retained_catchall(invocation, token)
    }
    fn validate_catchall_claim(
        &self,
        invocation: u64,
        claim: &ModSemanticCatchallClaim,
    ) -> Result<(), ModSemanticError> {
        self.validate_retained_catchall(invocation, claim)
    }

    fn generation(&self) -> SyntaxGenerationId {
        self.assembly.generation
    }
    fn resolve_declaration(&self, declaration: &ModSemanticDeclaration) -> Result<ModSemanticHandle, ModSemanticError> {
        self.validate_registration()?;
        if declaration.generation != self.assembly.generation {
            return Err(Self::error("stale semantic declaration generation"));
        }
        let unit = self
            .registered_unit(&declaration.source_unit)
            .ok_or_else(|| Self::error("foreign semantic declaration source unit"))?;
        let key = AstNodeKey {
            unit: SourceUnitId::new(self.db, unit.path.clone()),
            generation: declaration.generation,
            node: declaration.node,
        };
        self.validate_key(key)?;
        if self.declaration(key)? != *declaration {
            return Err(Self::error("semantic declaration span differs from registered syntax"));
        }
        self.resolve_type(key)
    }
    fn type_shape(&self, handle: ModSemanticHandle) -> Result<ModSemanticShape, ModSemanticError> {
        self.validate_registration()?;
        let (key, arguments) = self
            .handles
            .borrow()
            .get(&handle)
            .cloned()
            .ok_or_else(|| Self::error("foreign, expired or unknown semantic handle"))?;
        self.validate_key(key)?;
        let syntax = self.db.syntax_unit(key.unit).ok_or_else(|| Self::error("unknown semantic source"))?;
        let projection = mod_shape_projection(self.db, syntax, key, arguments.clone())
            .map_err(|error| Self::error(&error.to_string()))?;
        let mut remaining = HANDLE_LIMIT;
        let type_arguments = arguments
            .iter()
            .map(|identity| self.field_type(identity, 0, &mut remaining))
            .collect::<Result<Vec<_>, _>>()?;
        let mut fields = |projected: &[(AstNodeKey, String, ShapeIdentity)]| {
            projected
                .iter()
                .map(|(field, name, identity)| {
                    let ty = self.field_type(identity, 0, &mut remaining)?;
                    let ownership = if mod_shape_is_managed(identity) {
                        ModSemanticOwnership::GcManaged
                    } else {
                        ModSemanticOwnership::NativeOrScalar
                    };
                    Ok(ModSemanticField { name: name.clone(), declaration: self.declaration(*field)?, ty, ownership })
                })
                .collect::<Result<Vec<_>, ModSemanticError>>()
        };
        let body = match &projection.body {
            ShapeBodyProjection::Record(projected) => ModSemanticShapeBody::Record { fields: fields(projected)? },
            ShapeBodyProjection::Enum(projected) => ModSemanticShapeBody::Enum {
                variants: projected
                    .iter()
                    .map(|variant| {
                        Ok(ModSemanticVariant {
                            name: variant.name.clone(),
                            ordinal: variant.ordinal,
                            declaration: self.declaration(variant.key)?,
                            fields: fields(&variant.fields)?,
                        })
                    })
                    .collect::<Result<Vec<_>, ModSemanticError>>()?,
            },
        };
        let unit = self
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(self.db, unit.path.clone()) == key.unit)
            .ok_or_else(|| Self::error("shape source is not registered"))?;
        let package = self
            .assembly
            .package_identities()
            .validate_source(&unit.path, &unit.source)
            .map_err(|error| Self::error(&error.to_string()))?
            .cloned();
        let package_declaration = if package.is_some() {
            let root = self
                .assembly
                .package_identities()
                .for_source(&unit.path)
                .ok_or_else(|| Self::error("verified package source root unavailable"))?;
            Some(ModPackageDeclaration {
                source_path: root
                    .relative_source_path(&unit.path)
                    .ok_or_else(|| Self::error("package-relative source identity unavailable"))?,
                lexical_path: mod_shape_lexical_path(self.db, key)
                    .ok_or_else(|| Self::error("lexical declaration identity unavailable"))?,
            })
        } else {
            None
        };
        Ok(ModSemanticShape {
            package_declaration,
            name: projection.name,
            declaration_identity: mod_shape_stable_identity(self.db, key)
                .ok_or_else(|| Self::error("canonical declaration identity unavailable"))?
                .to_string(),
            declaration: self.declaration(key)?,
            type_arguments,
            body,
            package,
        })
    }
}
