//! Compiler-issued field decoder correspondence behind the canonical `Reader.ReadField<T,R>`.
//!
//! `ReadField<T,R>` has no source body for its `FieldDecoderBridge<T,R>` call. For each concrete
//! instance the compiler issues the exact function that reads one T over R:
//!
//! - a `[Serialize]` target application: the field decoder factory of its retained generated
//!   decoder group, admitted through the group's target and contribution correspondence;
//! - a scalar: the canonical scalar read of `Core/Serialization/PrimitiveDecoders.bd`;
//! - an array or a canonical Foundation List, string-key Map or Option: the canonical container
//!   read of `Core/Serialization/ContainerDecoders.bd`, which reads its elements through
//!   `ReadField` again.
//!
//! Every issued function must resolve, in its exact environment, to the same parameter and result
//! applications as the requested `ReadField<T,R>` instance. A name, factory string, template or
//! universal value never substitutes for this correspondence; any other T rejects.
use super::*;
use crate::mod_semantic_authority::generated_field_decoder_factory;
use beskid_analysis::syntax::FunctionDefinition;
use beskid_analysis::syntax_query::NodeKind;

const SERIALIZATION_PACKAGE: &str = "corelib_serialization";
const PRIMITIVE_READS_PATH: &str = "Core/Serialization/PrimitiveDecoders.bd";
const CONTAINER_READS_PATH: &str = "Core/Serialization/ContainerDecoders.bd";

/// One issued field read: the bridge call of a `ReadField<T,R>` instance and the exact function
/// instance its compiler-issued body forwards to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationFieldDecoderBinding {
    instance: GenericSpecializationInstance,
    target: GenericSpecializationInstance,
}
impl SerializationFieldDecoderBinding {
    /// The `FieldDecoderBridge<T,R>` method instance whose body the compiler emits.
    pub fn instance(&self) -> &GenericSpecializationInstance {
        &self.instance
    }
    /// The issued read function instance, with the bridge's exact ABI signature.
    pub fn target(&self) -> &GenericSpecializationInstance {
        &self.target
    }
}

fn error(message: &str) -> SemanticError {
    SemanticError::new(format!("serialization field decoder correspondence: {message}"))
}

/// The `ReadField<T,R>` function definition behind `read_field`, when it is the canonical one.
fn canonical_read_field(
    db: &dyn Db,
    read_field: &GenericSpecializationInstance,
) -> SemanticQueryResult<FunctionDefinition> {
    let Some(syntax) = db.syntax_unit(read_field.declaration.unit).filter(|s| s.accepts_key(db, read_field.declaration))
    else {
        return Ok(None);
    };
    if crate::canonical_corelib_source_path(db, read_field.declaration).as_deref()
        != Some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_READER_SOURCE_PATH)
        || item_name(db, read_field.declaration)?.as_deref() != Some("ReadField")
    {
        return Ok(None);
    }
    let function = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), read_field.declaration.node)
        .and_then(|node| node.of::<FunctionDefinition>())
        .cloned()
        .ok_or_else(|| SemanticError::unavailable("serialization_field_decoder.function"))?;
    if function.generics.len() != 2
        || read_field.substitutions.len() != 2
        || function.generics.iter().zip(read_field.substitutions.iter()).any(|(generic, binding)| {
            generic.node.name.as_str() != binding.parameter.as_ref()
        })
    {
        return Err(error("ReadField requires its exact concrete T and R substitutions"));
    }
    Ok(Some(function))
}

/// The bridge call instance of a canonical `ReadField<T,R>` instance. This is the cheap
/// recognition step; [`serialization_field_decoder_binding`] issues the forwarded function.
pub fn serialization_field_decoder_bridge(
    db: &dyn Db,
    read_field: &GenericSpecializationInstance,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    if canonical_read_field(db, read_field)?.is_none() {
        return Ok(None);
    }
    let reissued = generic_specialization_instance(db, read_field.declaration, read_field.substitutions.clone())?
        .ok_or_else(|| SemanticError::unavailable("serialization_field_decoder.specialization"))?;
    // A call-derived instance also carries the proven `R: Reader` witness, which the
    // declaration-level reissue does not; the declaration, environment and ABI must match.
    if reissued.declaration != read_field.declaration
        || reissued.substitutions != read_field.substitutions
        || reissued.signature != read_field.signature
    {
        return Err(error("ReadField specialization differs from source authority"));
    }
    let mut pending = vec![read_field.declaration];
    let mut visited = HashSet::new();
    let mut bridge = None;
    while let Some(key) = pending.pop() {
        if !visited.insert(key) {
            continue;
        }
        if visited.len() > 10000 {
            return Err(error("ReadField body limit"));
        }
        if node_kind(db, key)? == Some(IndexedNodeKind::CallExpression)
            && let Some(instance) = generic_call_specialization_in_environment(db, key, read_field)?
            && instance.declaration.unit == read_field.declaration.unit
            && item_name(db, instance.declaration)?.as_deref() == Some("beskid_serialization_read_field")
            && bridge.replace(instance).is_some()
        {
            return Err(error("ReadField bridge call is ambiguous"));
        }
        pending.extend(child_nodes(db, key)?.unwrap_or_default().iter().copied());
    }
    Ok(Some(bridge.ok_or_else(|| SemanticError::unavailable("serialization_field_decoder.bridge"))?))
}

/// Parameter and result applications every issued function must resolve to.
struct ExpectedRead {
    parameters: Vec<GenericSourceTypeIdentity>,
    result: GenericSourceTypeIdentity,
}

fn resolved_signature(
    db: &dyn Db,
    key: AstNodeKey,
    function: &FunctionDefinition,
    substitutions: &[GenericSubstitution],
) -> Result<(Vec<GenericSourceTypeIdentity>, GenericSourceTypeIdentity), SemanticError> {
    let environment = substitutions
        .iter()
        .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
        .collect::<HashMap<_, _>>();
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| generic_source_type_identity_with_substitutions(db, key, &parameter.node.ty.node, &environment))
        .collect::<Result<Vec<_>, _>>()?;
    let result = function
        .return_type
        .as_ref()
        .ok_or_else(|| error("field read has no result type"))
        .and_then(|result| generic_source_type_identity_with_substitutions(db, key, &result.node, &environment))?;
    Ok((parameters, result))
}

fn function_definition(db: &dyn Db, key: AstNodeKey) -> Result<FunctionDefinition, SemanticError> {
    let syntax = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .ok_or_else(|| SemanticError::unavailable("serialization_field_decoder.target"))?;
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)
        .and_then(|node| node.of::<FunctionDefinition>())
        .cloned()
        .ok_or_else(|| SemanticError::unavailable("serialization_field_decoder.target"))
}

/// Whether `key` in `substitutions` reads exactly the expected application.
fn accepts(
    db: &dyn Db,
    expected: &ExpectedRead,
    key: AstNodeKey,
    substitutions: &[GenericSubstitution],
) -> Result<bool, SemanticError> {
    let function = function_definition(db, key)?;
    if function.parameters.len() != expected.parameters.len() || function.generics.len() != substitutions.len() {
        return Ok(false);
    }
    Ok(match resolved_signature(db, key, &function, substitutions) {
        Ok((parameters, result)) => parameters == expected.parameters && result == expected.result,
        // An unresolvable candidate signature cannot correspond to the requested read.
        Err(_) => false,
    })
}

/// Bind `function`'s generic parameters positionally to `arguments`.
fn bind(function: &FunctionDefinition, arguments: &[GenericSourceTypeIdentity]) -> Option<Vec<GenericSubstitution>> {
    (function.generics.len() == arguments.len()).then(|| {
        function
            .generics
            .iter()
            .zip(arguments)
            .map(|(generic, identity)| {
                GenericSubstitution::from_source(generic.node.name.as_str(), identity.abi_type(), identity.clone())
            })
            .collect()
    })
}

/// The unique canonical serialization function `name` of `logical_path`, by prepare-owned
/// Corelib package provenance of the exact current assembly.
fn canonical_serialization_function(
    db: &dyn Db,
    program: &crate::TypedProgram,
    logical_path: &str,
    name: &str,
) -> Result<AstNodeKey, SemanticError> {
    let mut selected = None;
    for unit in program.assembly.units.iter() {
        let Some(root) = program.assembly.package_identities().for_source(&unit.path) else { continue };
        if root.identity().package_name() != SERIALIZATION_PACKAGE
            || !matches!(root.identity().source(), beskid_analysis::projects::VerifiedPackageSource::Corelib)
            || root.relative_source_path(&unit.path).as_deref() != Some(logical_path)
        {
            continue;
        }
        let unit_id = SourceUnitId::new(db, unit.path.clone());
        let syntax = db.syntax_unit(unit_id).ok_or_else(|| error("canonical field read unit is not registered"))?;
        for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
            let key = AstNodeKey { unit: unit_id, generation: program.generation, node };
            if item_name(db, key)?.as_deref() == Some(name) && selected.replace(key).is_some() {
                return Err(error("canonical field read is ambiguous"));
            }
        }
    }
    selected.ok_or_else(|| error(&format!("canonical field read `{name}` is absent from {logical_path}")))
}

fn scalar_read(ty: SemanticTypeId) -> Result<&'static str, SemanticError> {
    Ok(match ty {
        SemanticTypeId::UNIT => "ReadUnit",
        SemanticTypeId::BOOL => "ReadBoolean",
        SemanticTypeId::I8 => "ReadSigned8",
        SemanticTypeId::I16 => "ReadSigned16",
        SemanticTypeId::I32 => "ReadSigned32",
        SemanticTypeId::I64 => "ReadSigned64",
        SemanticTypeId::U8 => "ReadUnsigned8",
        SemanticTypeId::U16 => "ReadUnsigned16",
        SemanticTypeId::U32 => "ReadUnsigned32",
        SemanticTypeId::U64 => "ReadUnsigned64",
        SemanticTypeId::F32 => "ReadFloat32",
        SemanticTypeId::F64 => "ReadFloat64",
        SemanticTypeId::CHAR => "ReadScalar",
        SemanticTypeId::STRING => "ReadString",
        SemanticTypeId::WORD => {
            return Err(error("an abstract word field has no declared wire width; declare the field with WordWidth"));
        }
        _ => return Err(error("raw pointers, never and unknown primitives have no field read")),
    })
}

/// Issue the exact function that the bridge of the `ReadField<T,R>` instance `read_field`
/// forwards to. `Ok(None)` when `read_field` is not the canonical ReadField; an error when it is
/// and T has no admitted correspondence.
pub fn serialization_field_decoder_binding(
    db: &dyn Db,
    program: &crate::TypedProgram,
    read_field: &GenericSpecializationInstance,
) -> SemanticQueryResult<SerializationFieldDecoderBinding> {
    let Some(function) = canonical_read_field(db, read_field)? else { return Ok(None) };
    let Some(instance) = serialization_field_decoder_bridge(db, read_field)? else { return Ok(None) };
    let target_type = read_field.substitutions[0].source_identity().clone();
    let reader = &read_field.substitutions[1];
    let (parameters, result) = resolved_signature(db, read_field.declaration, &function, &read_field.substitutions)?;
    let expected = ExpectedRead { parameters, result };
    let leaf = |path: &str, name: &str, arguments: Vec<GenericSourceTypeIdentity>| {
        let key = canonical_serialization_function(db, program, path, name)?;
        let mut arguments = arguments;
        arguments.push(reader.source_identity().clone());
        let substitutions =
            bind(&function_definition(db, key)?, &arguments).ok_or_else(|| error("canonical field read arity changed"))?;
        Ok::<_, SemanticError>((key, substitutions))
    };
    let (key, substitutions) = match &target_type {
        GenericSourceTypeIdentity::Abi(ty) => leaf(PRIMITIVE_READS_PATH, scalar_read(*ty)?, Vec::new())?,
        GenericSourceTypeIdentity::Array(element) => {
            leaf(CONTAINER_READS_PATH, "ReadArray", vec![element.as_ref().clone()])?
        }
        GenericSourceTypeIdentity::Function { .. } => {
            return Err(error("function values have no field read"));
        }
        GenericSourceTypeIdentity::Nominal { qualified_name, arguments } => {
            let shape = project_serialization_shape(db, read_field.declaration, &target_type)?;
            let container = match shape.nodes().first() {
                Some(DynamicPackingNode::Record { container, .. } | DynamicPackingNode::Enum { container, .. }) => {
                    *container
                }
                _ => return Err(error("nominal field application has no issued shape")),
            };
            match container {
                Some(CanonicalContainerKind::List) if arguments.len() == 1 => {
                    leaf(CONTAINER_READS_PATH, "ReadList", vec![arguments[0].clone()])?
                }
                Some(CanonicalContainerKind::Optional) if arguments.len() == 1 => {
                    leaf(CONTAINER_READS_PATH, "ReadOptional", vec![arguments[0].clone()])?
                }
                Some(CanonicalContainerKind::Map)
                    if arguments.len() == 2
                        && arguments[0] == GenericSourceTypeIdentity::Abi(SemanticTypeId::STRING) =>
                {
                    leaf(CONTAINER_READS_PATH, "ReadMap", vec![arguments[1].clone()])?
                }
                Some(_) => return Err(error("canonical container field application is not readable")),
                None => {
                    let environment = |candidate: &FunctionDefinition| {
                        let mut bound = arguments.to_vec();
                        bound.push(reader.source_identity().clone());
                        bind(candidate, &bound)
                    };
                    let issued = generated_field_decoder_factory(
                        db,
                        program,
                        qualified_name,
                        arguments,
                        &environment,
                        &|key, substitutions| accepts(db, &expected, key, substitutions),
                    )
                    .map_err(|failure| {
                        error(&format!("`{}`: {failure}", target_type.display_name()))
                    })?;
                    (issued.function, issued.substitutions.to_vec())
                }
            }
        }
    };
    if !accepts(db, &expected, key, &substitutions)? {
        return Err(error(&format!(
            "issued read for `{}` differs from the requested field application",
            target_type.display_name()
        )));
    }
    let target = generic_specialization_instance(db, key, substitutions.into())?
        .ok_or_else(|| SemanticError::unavailable("serialization_field_decoder.target_specialization"))?;
    if target.signature != instance.signature {
        return Err(error("issued read ABI differs from the bridge signature"));
    }
    Ok(Some(SerializationFieldDecoderBinding { instance, target }))
}
