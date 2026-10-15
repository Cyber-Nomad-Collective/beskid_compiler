//! Compiler-issued field codec correspondence behind the canonical `Reader.ReadField<T,R>` and
//! `Contracts.WriteField<T,E>`.
//!
//! Neither function has a source body for its bridge call. For each concrete instance the
//! compiler issues the exact function that reads (or writes) one T over the format R (or E):
//!
//! - a `[Serialize]` target application: the field decoder (or encoder) factory of its retained
//!   generated decoder (or encoder) group, admitted through the group's target and contribution
//!   correspondence;
//! - a scalar: the canonical scalar read of `Core/Serialization/PrimitiveDecoders.bd` (or write
//!   of `PrimitiveEncoders.bd`);
//! - an array or a canonical Foundation List, string-key Map or Option: the canonical container
//!   read of `ContainerDecoders.bd` (or write of `ContainerEncoders.bd`), which handles its
//!   elements through `ReadField` (or `WriteField`) again.
//!
//! Every issued function must resolve, in its exact environment, to the same parameter and result
//! applications as the requested instance. A name, factory string, template or universal value
//! never substitutes for this correspondence; any other T rejects.
use super::*;
use crate::mod_semantic_authority::{SerializationContributionKind, generated_field_codec_factory};
use beskid_analysis::syntax::FunctionDefinition;
use beskid_analysis::syntax_query::NodeKind;

const SERIALIZATION_PACKAGE: &str = "corelib_serialization";

/// One direction of the field codec correspondence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Read,
    Write,
}
impl Direction {
    fn canonical_path(self) -> &'static str {
        match self {
            Self::Read => beskid_abi::runtime_source::CANONICAL_SERIALIZATION_READER_SOURCE_PATH,
            Self::Write => beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH,
        }
    }
    fn entry(self) -> &'static str {
        match self {
            Self::Read => "ReadField",
            Self::Write => "WriteField",
        }
    }
    fn bridge(self) -> &'static str {
        match self {
            Self::Read => "beskid_serialization_read_field",
            Self::Write => "beskid_serialization_write_field",
        }
    }
    fn primitives(self) -> &'static str {
        match self {
            Self::Read => "Core/Serialization/PrimitiveDecoders.bd",
            Self::Write => "Core/Serialization/PrimitiveEncoders.bd",
        }
    }
    fn containers(self) -> &'static str {
        match self {
            Self::Read => "Core/Serialization/ContainerDecoders.bd",
            Self::Write => "Core/Serialization/ContainerEncoders.bd",
        }
    }
    fn verb(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Write => "Write",
        }
    }
    fn group(self) -> SerializationContributionKind {
        match self {
            Self::Read => SerializationContributionKind::Decoder,
            Self::Write => SerializationContributionKind::Encoder,
        }
    }
}

/// One issued field read or write: the bridge call of a `ReadField<T,R>` or `WriteField<T,E>`
/// instance and the exact function instance its compiler-issued body forwards to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationFieldCodecBinding {
    instance: GenericSpecializationInstance,
    target: GenericSpecializationInstance,
}
impl SerializationFieldCodecBinding {
    /// The `FieldDecoderBridge<T,R>` or `FieldEncoderBridge<T,E>` method instance whose body the
    /// compiler emits.
    pub fn instance(&self) -> &GenericSpecializationInstance {
        &self.instance
    }
    /// The issued read or write function instance, with the bridge's exact ABI signature.
    pub fn target(&self) -> &GenericSpecializationInstance {
        &self.target
    }
}

fn error(message: &str) -> SemanticError {
    SemanticError::new(format!("serialization field codec correspondence: {message}"))
}

/// The `ReadField<T,R>` or `WriteField<T,E>` function definition behind `entry`, with its
/// direction, when it is one of the canonical ones.
fn canonical_entry(
    db: &dyn Db,
    entry: &GenericSpecializationInstance,
) -> SemanticQueryResult<(Direction, FunctionDefinition)> {
    let Some(syntax) = db.syntax_unit(entry.declaration.unit).filter(|s| s.accepts_key(db, entry.declaration)) else {
        return Ok(None);
    };
    let Some(path) = crate::canonical_corelib_source_path(db, entry.declaration) else { return Ok(None) };
    let Some(direction) =
        [Direction::Read, Direction::Write].into_iter().find(|direction| direction.canonical_path() == path)
    else {
        return Ok(None);
    };
    if item_name(db, entry.declaration)?.as_deref() != Some(direction.entry()) {
        return Ok(None);
    }
    let function = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), entry.declaration.node)
        .and_then(|node| node.of::<FunctionDefinition>())
        .cloned()
        .ok_or_else(|| SemanticError::unavailable("serialization_field_codec.function"))?;
    if function.generics.len() != 2
        || entry.substitutions.len() != 2
        || function
            .generics
            .iter()
            .zip(entry.substitutions.iter())
            .any(|(generic, binding)| generic.node.name.as_str() != binding.parameter.as_ref())
    {
        return Err(error("the field codec entry requires its exact concrete T and format substitutions"));
    }
    Ok(Some((direction, function)))
}

/// The bridge call instance of a canonical `ReadField<T,R>` or `WriteField<T,E>` instance. This is
/// the cheap recognition step; [`serialization_field_codec_binding`] issues the forwarded function.
pub fn serialization_field_codec_bridge(
    db: &dyn Db,
    entry: &GenericSpecializationInstance,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some((direction, _)) = canonical_entry(db, entry)? else { return Ok(None) };
    let reissued = generic_specialization_instance(db, entry.declaration, entry.substitutions.clone())?
        .ok_or_else(|| SemanticError::unavailable("serialization_field_codec.specialization"))?;
    // A call-derived instance also carries the proven format bound witness, which the
    // declaration-level reissue does not; the declaration, environment and ABI must match.
    if reissued.declaration != entry.declaration
        || reissued.substitutions != entry.substitutions
        || reissued.signature != entry.signature
    {
        return Err(error("field codec entry specialization differs from source authority"));
    }
    let mut pending = vec![entry.declaration];
    let mut visited = HashSet::new();
    let mut bridge = None;
    while let Some(key) = pending.pop() {
        if !visited.insert(key) {
            continue;
        }
        if visited.len() > 10000 {
            return Err(error("field codec entry body limit"));
        }
        if node_kind(db, key)? == Some(IndexedNodeKind::CallExpression)
            && let Some(instance) = generic_call_specialization_in_environment(db, key, entry)?
            && instance.declaration.unit == entry.declaration.unit
            && item_name(db, instance.declaration)?.as_deref() == Some(direction.bridge())
            && bridge.replace(instance).is_some()
        {
            return Err(error("field codec bridge call is ambiguous"));
        }
        pending.extend(child_nodes(db, key)?.unwrap_or_default().iter().copied());
    }
    Ok(Some(bridge.ok_or_else(|| SemanticError::unavailable("serialization_field_codec.bridge"))?))
}

/// Parameter and result applications every issued function must resolve to.
struct ExpectedSignature {
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
        .ok_or_else(|| SemanticError::unavailable("serialization_field_codec.target"))?;
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)
        .and_then(|node| node.of::<FunctionDefinition>())
        .cloned()
        .ok_or_else(|| SemanticError::unavailable("serialization_field_codec.target"))
}

/// Whether `key` in `substitutions` reads or writes exactly the expected application.
fn accepts(
    db: &dyn Db,
    expected: &ExpectedSignature,
    key: AstNodeKey,
    substitutions: &[GenericSubstitution],
) -> Result<bool, SemanticError> {
    let function = function_definition(db, key)?;
    if function.parameters.len() != expected.parameters.len() || function.generics.len() != substitutions.len() {
        return Ok(false);
    }
    Ok(match resolved_signature(db, key, &function, substitutions) {
        Ok((parameters, result)) => parameters == expected.parameters && result == expected.result,
        // An unresolvable candidate signature cannot correspond to the requested field.
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
        let syntax = db.syntax_unit(unit_id).ok_or_else(|| error("canonical field codec unit is not registered"))?;
        for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
            let key = AstNodeKey { unit: unit_id, generation: program.generation, node };
            if item_name(db, key)?.as_deref() == Some(name) && selected.replace(key).is_some() {
                return Err(error("canonical field codec function is ambiguous"));
            }
        }
    }
    selected.ok_or_else(|| error(&format!("canonical field codec function `{name}` is absent from {logical_path}")))
}

/// The canonical scalar function suffix (`Read<suffix>` / `Write<suffix>`) of one primitive.
fn scalar_suffix(ty: SemanticTypeId) -> Result<&'static str, SemanticError> {
    Ok(match ty {
        SemanticTypeId::UNIT => "Unit",
        SemanticTypeId::BOOL => "Boolean",
        SemanticTypeId::I8 => "Signed8",
        SemanticTypeId::I16 => "Signed16",
        SemanticTypeId::I32 => "Signed32",
        SemanticTypeId::I64 => "Signed64",
        SemanticTypeId::U8 => "Unsigned8",
        SemanticTypeId::U16 => "Unsigned16",
        SemanticTypeId::U32 => "Unsigned32",
        SemanticTypeId::U64 => "Unsigned64",
        SemanticTypeId::F32 => "Float32",
        SemanticTypeId::F64 => "Float64",
        SemanticTypeId::CHAR => "Scalar",
        SemanticTypeId::STRING => "String",
        SemanticTypeId::WORD => {
            return Err(error("an abstract word field has no declared wire width; declare the field with WordWidth"));
        }
        _ => return Err(error("raw pointers, never and unknown primitives have no field codec")),
    })
}

/// Issue the exact function that the bridge of the `ReadField<T,R>` or `WriteField<T,E>`
/// instance `entry` forwards to. `Ok(None)` when `entry` is neither canonical entry; an error when
/// it is and T has no admitted correspondence.
pub fn serialization_field_codec_binding(
    db: &dyn Db,
    program: &crate::TypedProgram,
    entry: &GenericSpecializationInstance,
) -> SemanticQueryResult<SerializationFieldCodecBinding> {
    let Some((direction, function)) = canonical_entry(db, entry)? else { return Ok(None) };
    let Some(instance) = serialization_field_codec_bridge(db, entry)? else { return Ok(None) };
    let target_type = entry.substitutions[0].source_identity().clone();
    let format = &entry.substitutions[1];
    let (parameters, result) = resolved_signature(db, entry.declaration, &function, &entry.substitutions)?;
    let expected = ExpectedSignature { parameters, result };
    let verb = direction.verb();
    let leaf = |path: &str, name: String, arguments: Vec<GenericSourceTypeIdentity>| {
        let key = canonical_serialization_function(db, program, path, &name)?;
        let mut arguments = arguments;
        arguments.push(format.source_identity().clone());
        let substitutions = bind(&function_definition(db, key)?, &arguments)
            .ok_or_else(|| error("canonical field codec function arity changed"))?;
        Ok::<_, SemanticError>((key, substitutions))
    };
    let (key, substitutions) = match &target_type {
        GenericSourceTypeIdentity::Abi(ty) => {
            leaf(direction.primitives(), format!("{verb}{}", scalar_suffix(*ty)?), Vec::new())?
        }
        GenericSourceTypeIdentity::Array(element) => {
            leaf(direction.containers(), format!("{verb}Array"), vec![element.as_ref().clone()])?
        }
        GenericSourceTypeIdentity::Function { .. } => {
            return Err(error("function values have no field codec"));
        }
        GenericSourceTypeIdentity::Nominal { qualified_name, arguments } => {
            let shape = project_serialization_shape(db, entry.declaration, &target_type)?;
            let container = match shape.nodes().first() {
                Some(DynamicPackingNode::Record { container, .. } | DynamicPackingNode::Enum { container, .. }) => {
                    *container
                }
                _ => return Err(error("nominal field application has no issued shape")),
            };
            match container {
                Some(CanonicalContainerKind::List) if arguments.len() == 1 => {
                    leaf(direction.containers(), format!("{verb}List"), vec![arguments[0].clone()])?
                }
                Some(CanonicalContainerKind::Optional) if arguments.len() == 1 => {
                    leaf(direction.containers(), format!("{verb}Optional"), vec![arguments[0].clone()])?
                }
                Some(CanonicalContainerKind::Map)
                    if arguments.len() == 2
                        && arguments[0] == GenericSourceTypeIdentity::Abi(SemanticTypeId::STRING) =>
                {
                    leaf(direction.containers(), format!("{verb}Map"), vec![arguments[1].clone()])?
                }
                Some(_) => return Err(error("canonical container field application has no field codec")),
                None => {
                    let environment = |candidate: &FunctionDefinition| {
                        let mut bound = arguments.to_vec();
                        bound.push(format.source_identity().clone());
                        bind(candidate, &bound)
                    };
                    let issued = generated_field_codec_factory(
                        db,
                        program,
                        direction.group(),
                        qualified_name,
                        arguments,
                        &environment,
                        &|key, substitutions| accepts(db, &expected, key, substitutions),
                    )
                    .map_err(|failure| error(&format!("`{}`: {failure}", target_type.display_name())))?;
                    (issued.function, issued.substitutions.to_vec())
                }
            }
        }
    };
    if !accepts(db, &expected, key, &substitutions)? {
        return Err(error(&format!(
            "issued field codec for `{}` differs from the requested field application",
            target_type.display_name()
        )));
    }
    let mut target = generic_specialization_instance(db, key, substitutions.clone().into())?
        .ok_or_else(|| SemanticError::unavailable("serialization_field_codec.target_specialization"))?;
    // The issued function reads or writes through its format parameter (`reader.BeginRecord()`
    // under `where R: Reader`): its body dispatches through the where-bound witnesses that a
    // source call would prove, proven here from the complete environment.
    target.contract_witnesses =
        super::contracts::contract_witnesses_for_call(db, key, &[], false, None, &substitutions)?;
    if target.signature != instance.signature {
        return Err(error("issued field codec ABI differs from the bridge signature"));
    }
    Ok(Some(SerializationFieldCodecBinding { instance, target }))
}
