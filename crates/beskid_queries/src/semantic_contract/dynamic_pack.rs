//! Concrete Dynamic packing signatures projected from the existing Salsa shape
//! authority. No layout, descriptor, source parser or parallel type cache lives here.
use super::mod_shapes::{ShapeBodyProjection, ShapeIdentity, mod_shape_projection};
use super::*;
use beskid_analysis::syntax::{EnumDefinition, FunctionDefinition, TypeDefinition};
use beskid_analysis::syntax_query::NodeKind;

const MAX_NODES: usize = 1024;
const MAX_EDGES: usize = 65536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicPackingField {
    pub declaration: AstNodeKey,
    pub name: String,
    pub ty: u32,
    pub managed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicPackingVariant {
    pub name: String,
    pub ordinal: u32,
    pub fields: Vec<DynamicPackingField>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalContainerKind {
    List,
    Map,
    Optional,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicPackingNode {
    Scalar {
        name: &'static str,
        managed: bool,
    },
    Array {
        element: u32,
    },
    Record {
        container: Option<CanonicalContainerKind>,
        declaration: AstNodeKey,
        identity: Arc<str>,
        arguments: Vec<u32>,
        fields: Vec<DynamicPackingField>,
    },
    Enum {
        container: Option<CanonicalContainerKind>,
        declaration: AstNodeKey,
        identity: Arc<str>,
        arguments: Vec<u32>,
        variants: Vec<DynamicPackingVariant>,
    },
}
/// Lookup keys remain generation-bound and never become portable signature bytes.
/// Codegen projects each declaration through its current TypedProgram source witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicPackingShape {
    declaration: AstNodeKey,
    nodes: Vec<DynamicPackingNode>,
}
impl DynamicPackingShape {
    pub fn declaration(&self) -> AstNodeKey {
        self.declaration
    }
    pub fn nodes(&self) -> &[DynamicPackingNode] {
        &self.nodes
    }
    /// Derive a finite canonical root ordering from an already issued graph.
    /// No caller-selected declarations or source identities enter this operation.
    pub fn rooted_at(&self, root: u32) -> Result<Self, SemanticError> {
        fn visit(
            graph: &DynamicPackingShape,
            old: u32,
            depth: usize,
            mapped: &mut HashMap<u32, u32>,
            nodes: &mut Vec<Option<DynamicPackingNode>>,
        ) -> Result<u32, SemanticError> {
            if let Some(index) = mapped.get(&old) {
                return Ok(*index);
            }
            if depth > 64 || nodes.len() >= MAX_NODES {
                return Err(error("rooted shape depth/node bound"));
            }
            let source = graph.nodes.get(old as usize).ok_or_else(|| error("rooted shape reference outside graph"))?;
            let index = nodes.len() as u32;
            mapped.insert(old, index);
            nodes.push(None);
            let mut node = source.clone();
            let mut edge = |old: u32| visit(graph, old, depth + 1, mapped, nodes);
            match &mut node {
                DynamicPackingNode::Scalar { .. } => {}
                DynamicPackingNode::Array { element } => {
                    *element = edge(*element)?;
                }
                DynamicPackingNode::Record { arguments, fields, .. } => {
                    for argument in arguments {
                        *argument = edge(*argument)?;
                    }
                    for field in fields {
                        field.ty = edge(field.ty)?;
                    }
                }
                DynamicPackingNode::Enum { arguments, variants, .. } => {
                    for argument in arguments {
                        *argument = edge(*argument)?;
                    }
                    for variant in variants {
                        for field in &mut variant.fields {
                            field.ty = edge(field.ty)?;
                        }
                    }
                }
            }
            nodes[index as usize] = Some(node);
            Ok(index)
        }
        let mut nodes = Vec::new();
        let mut mapped = HashMap::new();
        visit(self, root, 0, &mut mapped, &mut nodes)?;
        Ok(Self {
            declaration: self.declaration,
            nodes: nodes.into_iter().map(|node| node.expect("visited shape published")).collect(),
        })
    }

    /// The serialization wire view of one field value whose `word` scalars are
    /// written at an explicit width. The generated writer passes a field's word
    /// width through arrays and Foundation containers, never into a nominal
    /// with its own adapter, so only that region is rewritten; every `word`
    /// there becomes the fixed-width unsigned scalar the wire carries. The
    /// result is rooted at the rewritten field value.
    pub fn with_word_width(&self, root: u32, width: u8) -> Result<Self, SemanticError> {
        let scalar: &'static str = match width {
            8 => "u8",
            16 => "u16",
            32 => "u32",
            64 => "u64",
            _ => return Err(error("word wire width must be 8, 16, 32 or 64")),
        };
        struct Rewrite<'a> {
            graph: &'a DynamicPackingShape,
            scalar: &'static str,
            mapped: HashMap<(u32, bool), u32>,
            scalars: HashMap<&'static str, u32>,
            nodes: Vec<Option<DynamicPackingNode>>,
        }
        impl Rewrite<'_> {
            fn visit(&mut self, old: u32, rewrite: bool, depth: usize) -> Result<u32, SemanticError> {
                let graph = self.graph;
                let source = graph.nodes.get(old as usize).ok_or_else(|| error("word rewrite reference outside graph"))?;
                if let DynamicPackingNode::Scalar { name, managed } = source {
                    let name = if rewrite && *name == "word" { self.scalar } else { *name };
                    if let Some(index) = self.scalars.get(name) {
                        return Ok(*index);
                    }
                    let index = self.nodes.len() as u32;
                    self.scalars.insert(name, index);
                    self.nodes.push(Some(DynamicPackingNode::Scalar { name, managed: *managed }));
                    return Ok(index);
                }
                if let Some(index) = self.mapped.get(&(old, rewrite)) {
                    return Ok(*index);
                }
                if depth > 64 || self.nodes.len() >= MAX_NODES {
                    return Err(error("word rewrite depth/node bound"));
                }
                let index = self.nodes.len() as u32;
                self.mapped.insert((old, rewrite), index);
                self.nodes.push(None);
                let mut node = source.clone();
                match &mut node {
                    DynamicPackingNode::Scalar { .. } => {}
                    DynamicPackingNode::Array { element } => *element = self.visit(*element, rewrite, depth + 1)?,
                    DynamicPackingNode::Record { container, arguments, fields, .. } => {
                        let inner = rewrite && container.is_some();
                        for argument in arguments {
                            *argument = self.visit(*argument, inner, depth + 1)?;
                        }
                        for field in fields {
                            field.ty = self.visit(field.ty, inner, depth + 1)?;
                        }
                    }
                    DynamicPackingNode::Enum { container, arguments, variants, .. } => {
                        let inner = rewrite && container.is_some();
                        for argument in arguments {
                            *argument = self.visit(*argument, inner, depth + 1)?;
                        }
                        for variant in variants {
                            for field in &mut variant.fields {
                                field.ty = self.visit(field.ty, inner, depth + 1)?;
                            }
                        }
                    }
                }
                self.nodes[index as usize] = Some(node);
                Ok(index)
            }
        }
        let mut rewrite =
            Rewrite { graph: self, scalar, mapped: HashMap::new(), scalars: HashMap::new(), nodes: Vec::new() };
        let rewritten_root = rewrite.visit(root, true, 0)?;
        let rewritten = Self {
            declaration: self.declaration,
            nodes: rewrite.nodes.into_iter().map(|node| node.expect("rewritten shape published")).collect(),
        };
        rewritten.rooted_at(rewritten_root)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicPackingBridge {
    instance: GenericSpecializationInstance,
    result_factory: AstNodeKey,
    invalid_factory: AstNodeKey,
    box_literal: AstNodeKey,
}
impl DynamicPackingBridge {
    pub fn instance(&self) -> &GenericSpecializationInstance {
        &self.instance
    }
    pub fn result_factory(&self) -> AstNodeKey {
        self.result_factory
    }
    pub fn invalid_factory(&self) -> AstNodeKey {
        self.invalid_factory
    }
    pub fn box_literal(&self) -> AstNodeKey {
        self.box_literal
    }
}

/// Exact call-derived generic Shape<T> binding. Its descriptor remains the
/// canonical Pack<T> box allocation, reissued in the same declaration unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicShapeBinding {
    instance: GenericSpecializationInstance,
    packing: GenericSpecializationInstance,
}
impl DynamicShapeBinding {
    pub fn instance(&self) -> &GenericSpecializationInstance {
        &self.instance
    }
    pub fn packing(&self) -> &GenericSpecializationInstance {
        &self.packing
    }
}
pub fn dynamic_shape_binding(
    db: &dyn Db,
    shape: &GenericSpecializationInstance,
) -> SemanticQueryResult<DynamicShapeBinding> {
    let Some(syntax) = db.syntax_unit(shape.declaration.unit).filter(|s| s.accepts_key(db, shape.declaration)) else {
        return Ok(None);
    };
    if syntax.revision(db).runtime_source_authority.as_deref()
        != Some(beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH)
        || item_name(db, shape.declaration)?.as_deref() != Some("Shape")
    {
        return Ok(None);
    }
    let mut packing = None;
    for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
        let key = AstNodeKey { unit: shape.declaration.unit, generation: shape.declaration.generation, node };
        if item_name(db, key)?.as_deref() == Some("Pack") {
            if packing.replace(key).is_some() {
                return Err(error("ambiguous canonical Pack declaration"));
            }
        }
    }
    let packing = generic_specialization_instance(
        db,
        packing.ok_or_else(|| error("canonical Pack unavailable"))?,
        shape.substitutions.clone(),
    )?
    .ok_or_else(|| error("Shape lacks concrete canonical Pack substitution"))?;
    dynamic_packing_shape(db, &packing)?.ok_or_else(|| error("Shape type graph is unavailable"))?;
    let reissued = generic_specialization_instance(db, shape.declaration, shape.substitutions.clone())?
        .ok_or_else(|| error("Shape specialization unavailable"))?;
    if reissued != *shape {
        return Err(error("Shape instance differs from registered source authority"));
    }
    let mut pending = vec![shape.declaration];
    let mut seen = HashSet::new();
    let mut bridge = None;
    while let Some(key) = pending.pop() {
        if !seen.insert(key) {
            continue;
        }
        if seen.len() > 10000 {
            return Err(error("Shape body exceeds node bound"));
        }
        if node_kind(db, key)? == Some(IndexedNodeKind::CallExpression) {
            if let Some(instance) = generic_call_specialization_in_environment(db, key, shape)? {
                if instance.declaration.unit == shape.declaration.unit
                    && item_name(db, instance.declaration)?.as_deref() == Some("beskid_dynamic_v1_shape_tag")
                {
                    if bridge.replace(instance).is_some() {
                        return Err(error("duplicate canonical Shape bridge"));
                    }
                }
            }
        }
        pending.extend(child_nodes(db, key)?.unwrap_or_default().iter().copied());
    }
    Ok(Some(DynamicShapeBinding {
        instance: bridge.ok_or_else(|| error("canonical Shape bridge unavailable"))?,
        packing,
    }))
}

/// Source-issued typed extraction, retaining concrete constructor specializations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicUnpackingBridge {
    instance: GenericSpecializationInstance,
    packing: GenericSpecializationInstance,
    result_factory: GenericSpecializationInstance,
    invalid_factory: GenericSpecializationInstance,
}
impl DynamicUnpackingBridge {
    pub fn instance(&self) -> &GenericSpecializationInstance {
        &self.instance
    }
    pub fn packing(&self) -> &GenericSpecializationInstance {
        &self.packing
    }
    pub fn result_factory(&self) -> &GenericSpecializationInstance {
        &self.result_factory
    }
    pub fn invalid_factory(&self) -> &GenericSpecializationInstance {
        &self.invalid_factory
    }
}
pub fn dynamic_unpacking_bridge(
    db: &dyn Db,
    unpack: &GenericSpecializationInstance,
) -> SemanticQueryResult<DynamicUnpackingBridge> {
    let Some(syntax) = db.syntax_unit(unpack.declaration.unit).filter(|s| s.accepts_key(db, unpack.declaration)) else {
        return Ok(None);
    };
    if syntax.revision(db).runtime_source_authority.as_deref()
        != Some(beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH)
        || item_name(db, unpack.declaration)?.as_deref() != Some("Unpack")
    {
        return Ok(None);
    }
    let mut declarations = HashMap::new();
    for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
        let key = AstNodeKey { unit: unpack.declaration.unit, generation: unpack.declaration.generation, node };
        if let Some(name) = item_name(db, key)? {
            if matches!(name.as_ref(), "Pack" | "UnpackedResultV1" | "UnpackedInvalidV1") {
                if declarations.insert(name.to_string(), key).is_some() {
                    return Err(error("ambiguous canonical extraction factory"));
                }
            }
        }
    }
    let specialized = |name: &str| -> Result<GenericSpecializationInstance, SemanticError> {
        let key = *declarations.get(name).ok_or_else(|| error("canonical extraction declaration unavailable"))?;
        generic_specialization_instance(db, key, unpack.substitutions.clone())?
            .ok_or_else(|| error("canonical extraction specialization unavailable"))
    };
    let packing = specialized("Pack")?;
    dynamic_packing_shape(db, &packing)?.ok_or_else(|| error("Unpack type graph unavailable"))?;
    let reissued = generic_specialization_instance(db, unpack.declaration, unpack.substitutions.clone())?
        .ok_or_else(|| error("Unpack source specialization unavailable"))?;
    if reissued != *unpack {
        return Err(error("Unpack differs from canonical source authority"));
    }
    let mut pending = vec![unpack.declaration];
    let mut seen = HashSet::new();
    let mut bridge = None;
    while let Some(key) = pending.pop() {
        if !seen.insert(key) {
            continue;
        }
        if seen.len() > 10000 {
            return Err(error("Unpack body exceeds node bound"));
        }
        if node_kind(db, key)? == Some(IndexedNodeKind::CallExpression) {
            if let Some(instance) = generic_call_specialization_in_environment(db, key, unpack)? {
                if instance.declaration.unit == unpack.declaration.unit
                    && item_name(db, instance.declaration)?.as_deref() == Some("beskid_dynamic_v1_unpack_box")
                {
                    if bridge.replace(instance).is_some() {
                        return Err(error("duplicate canonical Unpack bridge"));
                    }
                }
            }
        }
        pending.extend(child_nodes(db, key)?.unwrap_or_default().iter().copied());
    }
    Ok(Some(DynamicUnpackingBridge {
        instance: bridge.ok_or_else(|| error("canonical Unpack bridge unavailable"))?,
        packing,
        result_factory: specialized("UnpackedResultV1")?,
        invalid_factory: specialized("UnpackedInvalidV1")?,
    }))
}

/// Issue the private erasure bridge from the actual Pack body call in its exact
/// immutable specialization environment. Caller-supplied method signatures do
/// not authorize representation conversion.
pub fn dynamic_packing_bridge(
    db: &dyn Db,
    pack: &GenericSpecializationInstance,
) -> SemanticQueryResult<DynamicPackingBridge> {
    if dynamic_packing_shape(db, pack)?.is_none() {
        return Ok(None);
    }
    let syntax = db.syntax_unit(pack.declaration.unit).ok_or_else(|| error("missing Pack source"))?;
    let mut pending = vec![pack.declaration];
    let mut bridge = None;
    let mut literal = None;
    let mut visited = HashSet::new();
    while let Some(key) = pending.pop() {
        if !visited.insert(key) {
            continue;
        }
        if visited.len() > 10000 {
            return Err(error("Pack body exceeds node bound"));
        }
        if node_kind(db, key)? == Some(IndexedNodeKind::CallExpression) {
            if let Some(instance) = generic_call_specialization_in_environment(db, key, pack)? {
                if instance.declaration.unit == pack.declaration.unit
                    && item_name(db, instance.declaration)?.as_deref() == Some("beskid_dynamic_v1_pack_box")
                {
                    if bridge.replace(instance).is_some() {
                        return Err(error("duplicate canonical Pack bridge"));
                    }
                }
            }
        }
        if node_kind(db, key)? == Some(IndexedNodeKind::StructLiteralExpression) {
            if literal.replace(key).is_some() {
                return Err(error("ambiguous Pack box construction"));
            }
        }
        pending.extend(child_nodes(db, key)?.unwrap_or_default().iter().copied());
    }
    let mut result_factory = None;
    let mut invalid_factory = None;
    for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
        let key = AstNodeKey { unit: pack.declaration.unit, generation: pack.declaration.generation, node };
        if item_name(db, key)?.as_deref() == Some("PackedResultV1") {
            if result_factory.replace(key).is_some() {
                return Err(error("ambiguous typed Pack Result constructor"));
            }
        }
        if item_name(db, key)?.as_deref() == Some("PackedInvalidV1") {
            if invalid_factory.replace(key).is_some() {
                return Err(error("ambiguous typed Pack error constructor"));
            }
        }
    }
    let result_factory = result_factory.ok_or_else(|| error("typed Pack Result constructor unavailable"))?;
    item_abi_signature(db, result_factory)?.ok_or_else(|| error("typed Pack Result ABI unavailable"))?;
    let invalid_factory = invalid_factory.ok_or_else(|| error("typed Pack invalid constructor unavailable"))?;
    item_abi_signature(db, invalid_factory)?.ok_or_else(|| error("typed Pack invalid ABI unavailable"))?;
    Ok(Some(DynamicPackingBridge {
        instance: bridge.ok_or_else(|| error("canonical Pack bridge call unavailable"))?,
        result_factory,
        invalid_factory,
        box_literal: literal.ok_or_else(|| error("canonical Pack box literal unavailable"))?,
    }))
}

fn error(message: &str) -> SemanticError {
    SemanticError::new(format!("Dynamic packing: {message}"))
}
fn scalar(ty: SemanticTypeId) -> Result<(&'static str, bool), SemanticError> {
    Ok(match ty {
        SemanticTypeId::I8 => ("i8", false),
        SemanticTypeId::I16 => ("i16", false),
        SemanticTypeId::I32 => ("i32", false),
        SemanticTypeId::I64 => ("i64", false),
        SemanticTypeId::U8 => ("u8", false),
        SemanticTypeId::U16 => ("u16", false),
        SemanticTypeId::U32 => ("u32", false),
        SemanticTypeId::U64 => ("u64", false),
        SemanticTypeId::F32 => ("f32", false),
        SemanticTypeId::F64 => ("f64", false),
        SemanticTypeId::BOOL => ("bool", false),
        SemanticTypeId::CHAR => ("char", false),
        SemanticTypeId::WORD => ("word", false),
        SemanticTypeId::STRING => ("utf8", true),
        SemanticTypeId::UNIT => ("unit", false),
        _ => return Err(error("raw pointers, never and unknown primitive identities are not packable")),
    })
}

struct Projection<'db> {
    db: &'db dyn Db,
    owner: AstNodeKey,
    identities: HashMap<ShapeIdentity, u32>,
    nodes: Vec<Option<DynamicPackingNode>>,
    edges: usize,
    /// Serialization wire view: fields skipped in both directions are built by
    /// their default factory and never reach the wire, so they are not part of
    /// the serialized shape at any nominal node. Dynamic packing keeps them.
    wire: bool,
}
impl Projection<'_> {
    fn nominal(&self, identity: &ShapeIdentity) -> Result<AstNodeKey, SemanticError> {
        let ShapeIdentity::Nominal { qualified_name, arguments } = identity else { return Err(error("not nominal")) };
        let owner = self.db.syntax_unit(self.owner.unit).ok_or_else(|| error("owner is unregistered"))?;
        let mut units = HashSet::from([self.owner.unit]);
        {
            let registry = self.db.syntax_dependency_registry().lock().expect("syntax dependency registry");
            units.extend(
                registry
                    .modules
                    .iter()
                    .filter(|((generation, _), _)| *generation == self.owner.generation)
                    .flat_map(|(_, units)| units.iter().copied()),
            );
        }
        let mut found = None;
        for unit in units {
            let Some(syntax) = self.db.syntax_unit(unit) else { continue };
            if syntax.project(self.db) != owner.project(self.db) || syntax.generation(self.db) != self.owner.generation
            {
                continue;
            }
            for node in [NodeKind::TypeDefinition, NodeKind::EnumDefinition]
                .into_iter()
                .flat_map(|kind| syntax.syntax_index(self.db).ids_of_kind(kind))
            {
                let key = AstNodeKey { unit, node, generation: self.owner.generation };
                if stable_declaration_identity(self.db, key).as_ref() != Some(qualified_name) {
                    continue;
                }
                let source = syntax
                    .syntax_index(self.db)
                    .node_at(syntax.expanded_program(self.db), node)
                    .ok_or_else(|| error("nominal declaration disappeared"))?;
                let arity = source
                    .of::<TypeDefinition>()
                    .map(|definition| definition.generics.len())
                    .or_else(|| source.of::<EnumDefinition>().map(|definition| definition.generics.len()))
                    .ok_or_else(|| error("nominal declaration kind changed"))?;
                if arity != arguments.len() || found.replace(key).is_some() {
                    return Err(error("ambiguous or unapplied nominal identity"));
                }
            }
        }
        found.ok_or_else(|| error("nominal source identity is unavailable"))
    }
    fn reserve_edges(&mut self, count: usize) -> Result<(), SemanticError> {
        self.edges = self
            .edges
            .checked_add(count)
            .filter(|total| *total <= MAX_EDGES)
            .ok_or_else(|| error("signature closure exceeds aggregate edge bound"))?;
        Ok(())
    }
    fn fields(
        &mut self,
        fields: Vec<(AstNodeKey, String, ShapeIdentity)>,
        depth: usize,
        payload: bool,
    ) -> Result<Vec<DynamicPackingField>, SemanticError> {
        self.reserve_edges(fields.len())?;
        let mut kept = Vec::with_capacity(fields.len());
        for field in fields {
            if self.wire {
                let policy = if payload {
                    crate::serialization_payload_field_policy(self.db, field.0)?
                } else {
                    crate::serialization_field_policy(self.db, field.0)?
                };
                if policy.absent_from_wire() {
                    continue;
                }
            }
            kept.push(field);
        }
        kept.into_iter()
            .map(|(declaration, name, ty)| {
                let managed = ty.managed_reference_kind() == ManagedReferenceKind::GcManaged;
                Ok(DynamicPackingField { declaration, name, ty: self.node(&ty, depth + 1)?, managed })
            })
            .collect()
    }
    fn node(&mut self, identity: &ShapeIdentity, depth: usize) -> Result<u32, SemanticError> {
        if let Some(index) = self.identities.get(identity) {
            return Ok(*index);
        }
        if depth > 64 || self.nodes.len() >= MAX_NODES {
            return Err(error("signature closure exceeds depth64/nodes1024"));
        }
        let index = self.nodes.len() as u32;
        self.identities.insert(identity.clone(), index);
        self.nodes.push(None);
        let node = match identity {
            ShapeIdentity::Abi(ty) => {
                let (name, managed) = scalar(*ty)?;
                DynamicPackingNode::Scalar { name, managed }
            }
            ShapeIdentity::Array(element) => {
                self.reserve_edges(1)?;
                DynamicPackingNode::Array { element: self.node(element, depth + 1)? }
            }
            ShapeIdentity::Function { .. } => {
                return Err(error("function/callback payload needs an explicit admitted transport profile"));
            }
            ShapeIdentity::Nominal { qualified_name, arguments } => {
                self.reserve_edges(arguments.len())?;
                let declaration = self.nominal(identity)?;
                let syntax =
                    self.db.syntax_unit(declaration.unit).ok_or_else(|| error("unregistered nominal declaration"))?;
                // Bound fan-out before the shared projection allocates its field vectors.
                let source = syntax
                    .syntax_index(self.db)
                    .node_at(syntax.expanded_program(self.db), declaration.node)
                    .ok_or_else(|| error("nominal source disappeared"))?;
                let fanout = if let Some(record) = source.of::<TypeDefinition>() {
                    record.fields.len()
                } else if let Some(enumeration) = source.of::<EnumDefinition>() {
                    if enumeration.variants.len() > MAX_EDGES - self.edges {
                        return Err(error("variant closure exceeds bound"));
                    }
                    enumeration
                        .variants
                        .iter()
                        .try_fold(enumeration.variants.len(), |sum, variant| sum.checked_add(variant.node.fields.len()))
                        .ok_or_else(|| error("field count overflow"))?
                } else {
                    return Err(error("nominal kind changed"));
                };
                if fanout > MAX_EDGES - self.edges {
                    return Err(error("nominal closure exceeds aggregate field bound"));
                }
                let application =
                    arguments.iter().map(|argument| self.node(argument, depth + 1)).collect::<Result<Vec<_>, _>>()?;
                let projection = mod_shape_projection(self.db, syntax, declaration, arguments.clone())?;
                let container = canonical_container_kind(self.db, declaration, arguments.len());
                match projection.body {
                    ShapeBodyProjection::Record(fields) => DynamicPackingNode::Record {
                        container,
                        declaration,
                        identity: qualified_name.clone(),
                        arguments: application,
                        fields: self.fields(fields, depth, false)?,
                    },
                    ShapeBodyProjection::Enum(variants) => {
                        self.reserve_edges(variants.len())?;
                        let variants = variants
                            .into_iter()
                            .map(|variant| {
                                Ok(DynamicPackingVariant {
                                    name: variant.name,
                                    ordinal: variant.ordinal,
                                    fields: self.fields(variant.fields, depth, true)?,
                                })
                            })
                            .collect::<Result<Vec<_>, SemanticError>>()?;
                        DynamicPackingNode::Enum {
                            container,
                            declaration,
                            identity: qualified_name.clone(),
                            arguments: application,
                            variants,
                        }
                    }
                }
            }
        };
        self.nodes[index as usize] = Some(node);
        Ok(index)
    }
}

/// Consume only an actual call-derived concrete instance of canonical Pack<T>.
/// This is a projection of existing semantic facts, not a second signature cache.
pub fn dynamic_packing_shape(
    db: &dyn Db,
    instance: &GenericSpecializationInstance,
) -> SemanticQueryResult<DynamicPackingShape> {
    let key = instance.declaration;
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else { return Ok(None) };
    if syntax.revision(db).runtime_source_authority.as_deref()
        != Some(beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH)
    {
        return Ok(None);
    }
    let Some(function) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)
        .and_then(|node| node.of::<FunctionDefinition>())
    else {
        return Ok(None);
    };
    if function.name.node.name != "Pack" {
        return Ok(None);
    }
    if function.generics.len() != 1
        || instance.substitutions.len() != 1
        || instance.substitutions[0].parameter.as_ref() != function.generics[0].node.name.as_str()
    {
        return Err(error("Pack requires its exact concrete declaration binding"));
    }
    let validated = generic_specialization_instance(db, key, instance.substitutions.clone())?
        .ok_or_else(|| error("packing specialization is unavailable"))?;
    if validated != *instance {
        return Err(error("packing instance differs from canonical signature/witness authority"));
    }
    Ok(Some(project_source_shape(db, key, instance.substitutions[0].source_identity())?))
}

/// Shared bounded graph builder, called only by source-bound issuers. The source identity
/// parameter is crate-private and never reconstructed from a callback DTO or signature.
pub(crate) fn project_source_shape(
    db: &dyn Db,
    owner: AstNodeKey,
    identity: &ShapeIdentity,
) -> Result<DynamicPackingShape, SemanticError> {
    project_shape(db, owner, identity, false)
}

/// The serialization wire view of the same bounded graph. It omits every field
/// skipped in both directions, as the generated adapters and descriptors do.
pub(crate) fn project_serialization_shape(
    db: &dyn Db,
    owner: AstNodeKey,
    identity: &ShapeIdentity,
) -> Result<DynamicPackingShape, SemanticError> {
    project_shape(db, owner, identity, true)
}

fn project_shape(
    db: &dyn Db,
    owner: AstNodeKey,
    identity: &ShapeIdentity,
    wire: bool,
) -> Result<DynamicPackingShape, SemanticError> {
    let mut projection = Projection { db, owner, identities: HashMap::new(), nodes: Vec::new(), edges: 0, wire };
    projection.node(identity, 0)?;
    let nodes = projection
        .nodes
        .into_iter()
        .map(|node| node.ok_or_else(|| error("unfinished signature closure")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DynamicPackingShape { declaration: owner, nodes })
}

/// Logical container policy requires exact current canonical source identity.
pub fn canonical_container_kind(db: &dyn Db, key: AstNodeKey, arity: usize) -> Option<CanonicalContainerKind> {
    match (
        crate::canonical_corelib_source_path(db, key).as_deref(),
        crate::corelib_source_authority::registered_declaration_name(db, key).as_deref(),
        arity,
    ) {
        (Some(beskid_abi::runtime_source::CANONICAL_FOUNDATION_LIST_SOURCE_PATH), Some("List"), 1) => {
            Some(CanonicalContainerKind::List)
        }
        (Some(beskid_abi::runtime_source::CANONICAL_FOUNDATION_MAP_SOURCE_PATH), Some("Map"), 2) => {
            Some(CanonicalContainerKind::Map)
        }
        (Some(beskid_abi::runtime_source::CANONICAL_FOUNDATION_OPTION_SOURCE_PATH), Some("Option"), 1) => {
            Some(CanonicalContainerKind::Optional)
        }
        _ => None,
    }
}
