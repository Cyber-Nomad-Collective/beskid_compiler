//! Registered Dynamic mapping plans. A plan is approved only from the two
//! canonical Pack<S> and Pack<D> type graphs that the shared Mod semantic shape
//! authority issues; no caller-authored signature, reflection or name table
//! participates. An approved plan always builds a fresh owned destination: mutable
//! aggregates (records, arrays) are copied, and only immutable values (strings and
//! enums whose closure holds no mutable aggregate) are shared.
use super::dynamic_pack::{CanonicalContainerKind, DynamicPackingNode, DynamicPackingShape};
use super::*;
use beskid_analysis::syntax::TypeDefinition;
use beskid_analysis::syntax_query::NodeKind;

const MAX_STEPS: usize = 4096;
const MAX_DEPTH: usize = 64;

/// Exact call-derived `Mapping<S, D>` binding. Both packing instances are the
/// canonical Pack specializations whose box descriptors are registered shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicMappingBinding {
    instance: GenericSpecializationInstance,
    source: GenericSpecializationInstance,
    destination: GenericSpecializationInstance,
}
impl DynamicMappingBinding {
    /// The private token bridge specialization emitted by the compiler.
    pub fn instance(&self) -> &GenericSpecializationInstance {
        &self.instance
    }
    /// Canonical `Pack<S>` for the source shape.
    pub fn source(&self) -> &GenericSpecializationInstance {
        &self.source
    }
    /// Canonical `Pack<D>` for the destination shape.
    pub fn destination(&self) -> &GenericSpecializationInstance {
        &self.destination
    }
}

/// One mapping step. Node indices refer to the source or destination packing graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicMappingStep {
    /// Identical scalar representation. `utf8` is an immutable shared string reference and
    /// `unit` carries no storage.
    Exact { scalar: &'static str },
    /// Approved lossless integer widening.
    Widen { source: &'static str, destination: &'static str },
    /// An enum value with the identical shape version whose closure holds only scalars,
    /// strings and enums. It is immutable, so the destination shares it.
    Shared { source: u32 },
    /// A declared optional destination field that the source does not carry: a fresh `None`.
    OptionalNone { destination: u32, ordinal: u32 },
    /// A fresh array whose elements are mapped by `element`.
    Array { source: u32, destination: u32, element: u32 },
    /// A fresh record. Every destination field is listed in destination declaration order.
    Record { source: u32, destination: u32, fields: Vec<DynamicMappingFieldStep> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicMappingFieldStep {
    /// Source field position, or `None` for a declared optional destination field.
    pub source: Option<usize>,
    /// Destination field position.
    pub destination: usize,
    pub step: u32,
}

/// An approved plan. Step 0 maps the root value of the two Pack boxes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicMappingPlan {
    source: DynamicPackingShape,
    destination: DynamicPackingShape,
    steps: Vec<DynamicMappingStep>,
}
impl DynamicMappingPlan {
    pub fn source(&self) -> &DynamicPackingShape {
        &self.source
    }
    pub fn destination(&self) -> &DynamicPackingShape {
        &self.destination
    }
    pub fn steps(&self) -> &[DynamicMappingStep] {
        &self.steps
    }
}

/// The plan outcome. A rejection is a checked result, not an internal failure: the
/// mapping token for a rejected plan is never registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicMappingOutcome {
    Approved(DynamicMappingPlan),
    Rejected(String),
}

fn error(message: &str) -> SemanticError {
    SemanticError::new(format!("Dynamic mapping: {message}"))
}

/// Issue the mapping binding from the exact canonical `Mapping<S, D>` body call in its
/// immutable specialization environment.
pub fn dynamic_mapping_binding(
    db: &dyn Db,
    mapping: &GenericSpecializationInstance,
) -> SemanticQueryResult<DynamicMappingBinding> {
    let Some(syntax) = db.syntax_unit(mapping.declaration.unit).filter(|s| s.accepts_key(db, mapping.declaration))
    else {
        return Ok(None);
    };
    if syntax.revision(db).runtime_source_authority.as_deref()
        != Some(beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH)
        || item_name(db, mapping.declaration)?.as_deref() != Some("Mapping")
    {
        return Ok(None);
    }
    if mapping.substitutions.len() != 2 {
        return Err(error("Mapping requires exactly its source and destination bindings"));
    }
    let binding = |parameter: &str| {
        mapping
            .substitutions
            .iter()
            .find(|binding| binding.parameter.as_ref() == parameter)
            .ok_or_else(|| error("Mapping binding parameter is unavailable"))
    };
    let (source_binding, destination_binding) = (binding("S")?, binding("D")?);
    let mut packing = None;
    for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
        let key = AstNodeKey { unit: mapping.declaration.unit, generation: mapping.declaration.generation, node };
        if item_name(db, key)?.as_deref() == Some("Pack") && packing.replace(key).is_some() {
            return Err(error("ambiguous canonical Pack declaration"));
        }
    }
    let packing = packing.ok_or_else(|| error("canonical Pack unavailable"))?;
    let specialized = |binding: &GenericSubstitution| -> Result<GenericSpecializationInstance, SemanticError> {
        let instance = generic_specialization_instance(db, packing, Arc::from([binding.rebind("T")]))?
            .ok_or_else(|| error("Mapping lacks a concrete canonical Pack substitution"))?;
        super::dynamic_pack::dynamic_packing_shape(db, &instance)?
            .ok_or_else(|| error("Mapping type graph is unavailable"))?;
        Ok(instance)
    };
    let source = specialized(source_binding)?;
    let destination = specialized(destination_binding)?;
    let reissued = generic_specialization_instance(db, mapping.declaration, mapping.substitutions.clone())?
        .ok_or_else(|| error("Mapping specialization unavailable"))?;
    if reissued != *mapping {
        return Err(error("Mapping instance differs from registered source authority"));
    }
    let mut pending = vec![mapping.declaration];
    let mut seen = HashSet::new();
    let mut bridge = None;
    while let Some(key) = pending.pop() {
        if !seen.insert(key) {
            continue;
        }
        if seen.len() > 10000 {
            return Err(error("Mapping body exceeds node bound"));
        }
        if node_kind(db, key)? == Some(IndexedNodeKind::CallExpression) {
            if let Some(instance) = generic_call_specialization_in_environment(db, key, mapping)? {
                if instance.declaration.unit == mapping.declaration.unit
                    && item_name(db, instance.declaration)?.as_deref() == Some("beskid_dynamic_v1_mapping_token")
                    && bridge.replace(instance).is_some()
                {
                    return Err(error("duplicate canonical Mapping bridge"));
                }
            }
        }
        pending.extend(child_nodes(db, key)?.unwrap_or_default().iter().copied());
    }
    Ok(Some(DynamicMappingBinding {
        instance: bridge.ok_or_else(|| error("canonical Mapping bridge unavailable"))?,
        source,
        destination,
    }))
}

/// Approve or reject the field plan between the two canonical packing graphs.
pub fn dynamic_mapping_plan(
    db: &dyn Db,
    binding: &DynamicMappingBinding,
) -> Result<DynamicMappingOutcome, SemanticError> {
    let source = super::dynamic_pack::dynamic_packing_shape(db, binding.source())?
        .ok_or_else(|| error("source type graph is unavailable"))?;
    let destination = super::dynamic_pack::dynamic_packing_shape(db, binding.destination())?
        .ok_or_else(|| error("destination type graph is unavailable"))?;
    let mut planner =
        Planner { db, source: &source, destination: &destination, steps: Vec::new(), memo: HashMap::new() };
    match planner.plan(0, 0, 0) {
        Ok(root) => {
            if root != 0 {
                return Err(error("root mapping step must be the first step"));
            }
            let steps = planner
                .steps
                .into_iter()
                .map(|step| step.ok_or_else(|| error("unfinished mapping step")))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DynamicMappingOutcome::Approved(DynamicMappingPlan { source, destination, steps }))
        }
        Err(Rejection::Plan(reason)) => Ok(DynamicMappingOutcome::Rejected(reason)),
        Err(Rejection::Semantic(error)) => Err(error),
    }
}

enum Rejection {
    Plan(String),
    Semantic(SemanticError),
}
impl From<SemanticError> for Rejection {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}
fn reject<T>(reason: impl Into<String>) -> Result<T, Rejection> {
    Err(Rejection::Plan(reason.into()))
}

/// Approved lossless integer widening. `word` is target-sized and never widens; floats,
/// `char` and `bool` are not integers for this purpose.
fn widening(source: &str, destination: &str) -> bool {
    matches!(
        (source, destination),
        ("i8", "i16" | "i32" | "i64")
            | ("i16", "i32" | "i64")
            | ("i32", "i64")
            | ("u8", "u16" | "u32" | "u64" | "i16" | "i32" | "i64")
            | ("u16", "u32" | "u64" | "i32" | "i64")
            | ("u32", "u64" | "i64")
    )
}
fn integer(name: &str) -> bool {
    matches!(name, "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "word")
}

struct Planner<'a> {
    db: &'a dyn Db,
    source: &'a DynamicPackingShape,
    destination: &'a DynamicPackingShape,
    steps: Vec<Option<DynamicMappingStep>>,
    memo: HashMap<(u32, u32), u32>,
}

impl Planner<'_> {
    fn node<'g>(graph: &'g DynamicPackingShape, index: u32) -> Result<&'g DynamicPackingNode, Rejection> {
        graph.nodes().get(index as usize).ok_or_else(|| Rejection::Semantic(error("mapping node outside graph")))
    }
    fn push(&mut self, step: DynamicMappingStep) -> Result<u32, Rejection> {
        if self.steps.len() >= MAX_STEPS {
            return reject("mapping plan exceeds 4096 steps");
        }
        self.steps.push(Some(step));
        Ok((self.steps.len() - 1) as u32)
    }

    fn plan(&mut self, source: u32, destination: u32, depth: usize) -> Result<u32, Rejection> {
        if let Some(step) = self.memo.get(&(source, destination)) {
            return Ok(*step);
        }
        if depth > MAX_DEPTH {
            return reject("mapping plan exceeds nesting depth 64");
        }
        let source_node = Self::node(self.source, source)?.clone();
        let destination_node = Self::node(self.destination, destination)?.clone();
        match (&source_node, &destination_node) {
            (DynamicPackingNode::Scalar { name: from, .. }, DynamicPackingNode::Scalar { name: to, .. }) => {
                let (from, to): (&'static str, &'static str) = (*from, *to);
                let step = if from == to {
                    DynamicMappingStep::Exact { scalar: from }
                } else if widening(from, to) {
                    DynamicMappingStep::Widen { source: from, destination: to }
                } else if integer(from) && integer(to) {
                    return reject(format!("numeric narrowing or sign change from {from} to {to} is not lossless"));
                } else if from == "utf8" || to == "utf8" {
                    return reject(format!("string coercion between {from} and {to} is not a mapping"));
                } else {
                    return reject(format!("scalar {from} is not compatible with {to}"));
                };
                let index = self.push(step)?;
                self.memo.insert((source, destination), index);
                Ok(index)
            }
            (DynamicPackingNode::Array { element: from }, DynamicPackingNode::Array { element: to }) => {
                for (graph, element) in [(self.source, *from), (self.destination, *to)] {
                    if matches!(Self::node(graph, element)?, DynamicPackingNode::Scalar { name: "unit", .. }) {
                        return reject("unit array elements have no mapped storage");
                    }
                }
                let index = self.reserve(source, destination)?;
                let element = self.plan(*from, *to, depth + 1)?;
                self.steps[index as usize] =
                    Some(DynamicMappingStep::Array { source, destination, element });
                Ok(index)
            }
            (
                DynamicPackingNode::Record {
                    container: source_container,
                    declaration: source_declaration,
                    identity: source_identity,
                    fields: source_fields,
                    ..
                },
                DynamicPackingNode::Record {
                    container: destination_container,
                    declaration: destination_declaration,
                    identity: destination_identity,
                    fields: destination_fields,
                    ..
                },
            ) => {
                for (declaration, identity) in
                    [(*source_declaration, source_identity), (*destination_declaration, destination_identity)]
                {
                    if resource_declaration(self.db, declaration) {
                        return reject(format!("{identity} is a resource or sealed runtime value, not mappable data"));
                    }
                }
                if (source_container.is_some() || destination_container.is_some())
                    && (source_container != destination_container
                        || !graphs_equal(self.source, source, self.destination, destination, &mut HashSet::new()))
                {
                    return reject(format!(
                        "canonical container {source_identity} maps only to the identical container shape"
                    ));
                }
                let index = self.reserve(source, destination)?;
                let mut consumed = vec![false; source_fields.len()];
                let mut fields = Vec::with_capacity(destination_fields.len());
                for (position, field) in destination_fields.iter().enumerate() {
                    match source_fields.iter().position(|candidate| candidate.name == field.name) {
                        Some(found) => {
                            consumed[found] = true;
                            let step = self.plan(source_fields[found].ty, field.ty, depth + 1).map_err(|rejection| {
                                match rejection {
                                    Rejection::Plan(reason) => Rejection::Plan(format!(
                                        "field `{}` of {destination_identity}: {reason}",
                                        field.name
                                    )),
                                    other => other,
                                }
                            })?;
                            fields.push(DynamicMappingFieldStep { source: Some(found), destination: position, step });
                        }
                        None => {
                            let step = self.optional_none(field.ty).map_err(|rejection| match rejection {
                                Rejection::Plan(_) => Rejection::Plan(format!(
                                    "destination field `{}` of {destination_identity} has no source field and is not \
                                     a declared optional value",
                                    field.name
                                )),
                                other => other,
                            })?;
                            fields.push(DynamicMappingFieldStep { source: None, destination: position, step });
                        }
                    }
                }
                if let Some(unknown) = consumed.iter().position(|used| !used) {
                    return reject(format!(
                        "source field `{}` of {source_identity} is unknown to {destination_identity}",
                        source_fields[unknown].name
                    ));
                }
                self.steps[index as usize] = Some(DynamicMappingStep::Record { source, destination, fields });
                Ok(index)
            }
            (
                DynamicPackingNode::Enum { identity: source_identity, .. },
                DynamicPackingNode::Enum { identity: destination_identity, .. },
            ) => {
                if !graphs_equal(self.source, source, self.destination, destination, &mut HashSet::new()) {
                    return reject(format!(
                        "enum {source_identity} is not the same shape version as {destination_identity}"
                    ));
                }
                if !immutable_closure(self.source, source, &mut HashSet::new()) {
                    return reject(format!("enum {source_identity} carries a mutable aggregate payload"));
                }
                let index = self.push(DynamicMappingStep::Shared { source })?;
                self.memo.insert((source, destination), index);
                Ok(index)
            }
            _ => reject("source and destination values have incompatible kinds"),
        }
    }

    fn reserve(&mut self, source: u32, destination: u32) -> Result<u32, Rejection> {
        if self.steps.len() >= MAX_STEPS {
            return reject("mapping plan exceeds 4096 steps");
        }
        self.steps.push(None);
        let index = (self.steps.len() - 1) as u32;
        self.memo.insert((source, destination), index);
        Ok(index)
    }

    /// Only the canonical Foundation `Option<T>` declares an absent value.
    fn optional_none(&mut self, destination: u32) -> Result<u32, Rejection> {
        let DynamicPackingNode::Enum { container: Some(CanonicalContainerKind::Optional), variants, .. } =
            Self::node(self.destination, destination)?
        else {
            return reject("not optional");
        };
        let none = variants
            .iter()
            .find(|variant| variant.name == "None" && variant.fields.is_empty())
            .ok_or_else(|| Rejection::Semantic(error("canonical Option lacks its field-free None variant")))?;
        let ordinal = none.ordinal;
        self.push(DynamicMappingStep::OptionalNone { destination, ordinal })
    }
}

/// Structural identity of two issued graph regions: same nominal identities and
/// arguments, same field and variant names, ordinals and value types.
fn graphs_equal(
    left: &DynamicPackingShape,
    a: u32,
    right: &DynamicPackingShape,
    b: u32,
    visited: &mut HashSet<(u32, u32)>,
) -> bool {
    if !visited.insert((a, b)) {
        return true;
    }
    let (Some(x), Some(y)) = (left.nodes().get(a as usize), right.nodes().get(b as usize)) else { return false };
    let all = |pairs: Vec<(u32, u32)>, visited: &mut HashSet<(u32, u32)>| {
        pairs.into_iter().all(|(p, q)| graphs_equal(left, p, right, q, visited))
    };
    match (x, y) {
        (DynamicPackingNode::Scalar { name: p, .. }, DynamicPackingNode::Scalar { name: q, .. }) => p == q,
        (DynamicPackingNode::Array { element: p }, DynamicPackingNode::Array { element: q }) => {
            graphs_equal(left, *p, right, *q, visited)
        }
        (
            DynamicPackingNode::Record { container: c1, identity: i1, arguments: a1, fields: f1, .. },
            DynamicPackingNode::Record { container: c2, identity: i2, arguments: a2, fields: f2, .. },
        ) => {
            c1 == c2
                && i1 == i2
                && a1.len() == a2.len()
                && f1.len() == f2.len()
                && f1.iter().zip(f2.iter()).all(|(p, q)| p.name == q.name)
                && all(
                    a1.iter().copied().zip(a2.iter().copied()).chain(f1.iter().map(|f| f.ty).zip(f2.iter().map(|f| f.ty))).collect(),
                    visited,
                )
        }
        (
            DynamicPackingNode::Enum { container: c1, identity: i1, arguments: a1, variants: v1, .. },
            DynamicPackingNode::Enum { container: c2, identity: i2, arguments: a2, variants: v2, .. },
        ) => {
            c1 == c2
                && i1 == i2
                && a1.len() == a2.len()
                && v1.len() == v2.len()
                && v1.iter().zip(v2.iter()).all(|(p, q)| {
                    p.name == q.name
                        && p.ordinal == q.ordinal
                        && p.fields.len() == q.fields.len()
                        && p.fields.iter().zip(q.fields.iter()).all(|(f, g)| f.name == g.name)
                })
                && all(
                    a1.iter()
                        .copied()
                        .zip(a2.iter().copied())
                        .chain(
                            v1.iter()
                                .zip(v2.iter())
                                .flat_map(|(p, q)| p.fields.iter().map(|f| f.ty).zip(q.fields.iter().map(|f| f.ty))),
                        )
                        .collect(),
                    visited,
                )
        }
        _ => false,
    }
}

/// True when every value reachable from `node` is a scalar, string or enum.
fn immutable_closure(graph: &DynamicPackingShape, node: u32, visited: &mut HashSet<u32>) -> bool {
    if !visited.insert(node) {
        return true;
    }
    match graph.nodes().get(node as usize) {
        Some(DynamicPackingNode::Scalar { .. }) => true,
        Some(DynamicPackingNode::Enum { variants, .. }) => variants
            .iter()
            .flat_map(|variant| variant.fields.iter().map(|field| field.ty))
            .collect::<Vec<_>>()
            .into_iter()
            .all(|ty| immutable_closure(graph, ty, visited)),
        _ => false,
    }
}

/// Sealed runtime values, Process resources and Glue handles carry ownership, not data.
fn resource_declaration(db: &dyn Db, declaration: AstNodeKey) -> bool {
    if crate::runtime_managed_opaque_kind(db, declaration).is_some()
        || crate::process_source_authority::canonical_process_resource_kind(db, declaration).is_some()
    {
        return true;
    }
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return true;
    };
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<TypeDefinition>())
        .is_none_or(|definition| definition.attributes.iter().any(|attribute| attribute.node.name.node.name == "GlueHandle"))
}
