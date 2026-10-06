//! Source-issued specialization of the private compiled descriptor getter.
use super::*;
use beskid_analysis::syntax::FunctionDefinition;
use beskid_analysis::syntax_query::NodeKind;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationShapeBinding {
    instance: GenericSpecializationInstance,
    graph: DynamicPackingShape,
    factory: AstNodeKey,
    root_factory: AstNodeKey,
    extras_factory: AstNodeKey,
    returns_graph: bool,
}
impl SerializationShapeBinding {
    pub fn instance(&self) -> &GenericSpecializationInstance {
        &self.instance
    }
    pub fn graph(&self) -> &DynamicPackingShape {
        &self.graph
    }
    pub fn factory(&self) -> AstNodeKey {
        self.factory
    }
    pub fn root_factory(&self) -> AstNodeKey {
        self.root_factory
    }
    pub fn extras_factory(&self) -> AstNodeKey {
        self.extras_factory
    }
    pub fn returns_graph(&self) -> bool {
        self.returns_graph
    }
    /// The exact concrete target application the getter was specialized with.
    pub(crate) fn target_identity(&self) -> &GenericSourceTypeIdentity {
        self.instance.substitutions[0].source_identity()
    }
}
pub fn serialization_shape_binding(
    db: &dyn Db,
    getter: &GenericSpecializationInstance,
) -> SemanticQueryResult<SerializationShapeBinding> {
    let Some(syntax) = db.syntax_unit(getter.declaration.unit).filter(|s| s.accepts_key(db, getter.declaration)) else {
        return Ok(None);
    };
    if crate::canonical_corelib_source_path(db, getter.declaration).as_deref()
        != Some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_DESCRIPTORS_SOURCE_PATH)
        || !matches!(item_name(db, getter.declaration)?.as_deref(), Some("CompiledShape" | "CompiledDescriptors"))
    {
        return Ok(None);
    };
    let returns_graph = item_name(db, getter.declaration)?.as_deref() == Some("CompiledDescriptors");
    let function = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), getter.declaration.node)
        .and_then(|n| n.of::<FunctionDefinition>())
        .ok_or_else(|| SemanticError::unavailable("serialization_shape.function"))?;
    if function.generics.len() != 1
        || getter.substitutions.len() != 1
        || getter.substitutions[0].parameter.as_ref() != function.generics[0].node.name.as_str()
    {
        return Err(SemanticError::new("compiled shape requires exact concrete target substitution"));
    }
    let reissued = generic_specialization_instance(db, getter.declaration, getter.substitutions.clone())?
        .ok_or_else(|| SemanticError::unavailable("serialization_shape.specialization"))?;
    if &reissued != getter {
        return Err(SemanticError::new("compiled shape specialization differs from source authority"));
    }
    let graph = project_serialization_shape(db, getter.declaration, getter.substitutions[0].source_identity())?;
    let mut pending = vec![getter.declaration];
    let mut visited = HashSet::new();
    let mut bridge = None;
    while let Some(key) = pending.pop() {
        if !visited.insert(key) {
            continue;
        }
        if visited.len() > 10000 {
            return Err(SemanticError::new("compiled shape body limit"));
        }
        if node_kind(db, key)? == Some(IndexedNodeKind::CallExpression) {
            if let Some(instance) = generic_call_specialization_in_environment(db, key, getter)? {
                if instance.declaration.unit == getter.declaration.unit
                    && item_name(db, instance.declaration)?.as_deref()
                        == Some(if returns_graph {
                            "beskid_serialization_compiled_descriptors"
                        } else {
                            "beskid_serialization_compiled_shape"
                        })
                {
                    if bridge.replace(instance).is_some() {
                        return Err(SemanticError::new("compiled shape bridge ambiguous"));
                    }
                }
            }
        }
        pending.extend(child_nodes(db, key)?.unwrap_or_default().iter().copied());
    }
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    let units = registry
        .corelib_source_paths
        .iter()
        .filter_map(|((unit, generation), path)| {
            (path.as_str() == beskid_abi::runtime_source::CANONICAL_SERIALIZATION_COMPILED_SOURCE_PATH)
                .then_some((*unit, *generation))
        })
        .collect::<Vec<_>>();
    drop(registry);
    let mut factory = None;
    for (unit, generation) in units {
        let Some(owner) = db.syntax_unit(unit).filter(|owner| owner.generation(db) == generation) else { continue };
        for node in owner.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
            let key = AstNodeKey { unit, generation, node };
            if crate::canonical_corelib_source_path(db, key).as_deref()
                == Some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_COMPILED_SOURCE_PATH)
                && item_name(db, key)?.as_deref() == Some("ConstructCompiledGraph")
                && factory.replace(key).is_some()
            {
                return Err(SemanticError::new("compiled descriptor constructor authority ambiguous"));
            }
        }
    }
    let mut root_factory = None;
    let mut extras_factory = None;
    for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
        let key = AstNodeKey { node, ..getter.declaration };
        match item_name(db, key)?.as_deref() {
            Some("CompiledRoot") => {
                if root_factory.replace(key).is_some() {
                    return Err(SemanticError::new("compiled root factory ambiguous"));
                }
            }
            Some("AttachCompiledExtras") => {
                if extras_factory.replace(key).is_some() {
                    return Err(SemanticError::new("compiled extras factory ambiguous"));
                }
            }
            _ => {}
        }
    }
    Ok(Some(SerializationShapeBinding {
        instance: bridge.ok_or_else(|| SemanticError::unavailable("serialization_shape.bridge"))?,
        graph,
        factory: factory.ok_or_else(|| SemanticError::unavailable("serialization_shape.factory"))?,
        root_factory: root_factory.ok_or_else(|| SemanticError::unavailable("serialization_shape.root_factory"))?,
        extras_factory: extras_factory
            .ok_or_else(|| SemanticError::unavailable("serialization_shape.extras_factory"))?,
        returns_graph,
    }))
}
