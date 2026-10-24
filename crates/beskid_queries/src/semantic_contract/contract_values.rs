//! Contract values (runtime dispatch, v0.6.7).
//!
//! A value whose static type is a contract is a *contract box*: a 32-byte managed record holding
//! the implementing value's receiver word (an object reference, or a primitive's bits widened to
//! one word) at offset 16 and the address of a static *witness table* at offset 24. The witness
//! table of one (implementing type, applied contract) pair lists the implementation's methods in
//! the contract's method order (`contract_method_slot`). A call through a contract value loads the
//! receiver word and the table entry of the method's slot and calls it indirectly.
//!
//! A witness table is `WITNESS_HEADER_WORDS` header words (the implementing type's tag and the
//! receiver kind, read by `==` on contract values), then the method entries in slot order, then
//! the addresses of the implementing type's tables for every contract the contract embeds
//! ([`contract_embedded_contracts`] order), read when a contract value converts to an embedded
//! contract.
//!
//! This module answers these questions for code generation:
//! - [`contract_box_coercion`]: is the expression at `key` a concrete value written into a
//!   contract-typed destination, and with which witness table is it boxed?
//! - [`contract_dispatch`]: is the call at `key` a contract method call without a static target,
//!   and which slot and signature does it dispatch with?
//! - [`contract_value_receiver`]: is the receiver of the direct method call at `key` a contract
//!   box whose receiver word the call reads out (conf2's static selection)?
//! - [`contract_upcast`]: is the expression at `key` a contract value converted to a contract its
//!   own contract embeds, and which table word holds the embedded table?
//! - [`contract_value_equality`]: is the binary expression at `key` `==`/`!=` on two contract
//!   values?

use std::collections::HashMap;

use beskid_analysis::syntax::{CallExpression, ContractDefinition, ContractMethodSignature, Spanned};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

use super::abi::generic_abi_type;
use super::contracts::{
    contract_declaration_for_identity, contract_embedded_contracts, contract_method_slot, contract_method_slots, contract_parameter_declarations,
    is_conformable_primitive, prove_box_conformance, specialized_source_expression_identity,
};
use super::expected_type::expected_type_in;
use super::*;

type Program = Spanned<beskid_analysis::syntax::Program>;

/// Byte offset of the receiver word inside a contract box (after the 16-byte object header).
pub const CONTRACT_BOX_RECEIVER_OFFSET: u32 = 16;
/// Byte offset of the witness table address inside a contract box.
pub const CONTRACT_BOX_WITNESS_OFFSET: u32 = 24;
/// Size of a contract box in bytes, header included.
pub const CONTRACT_BOX_SIZE: u64 = 32;
/// Words of a witness table before its method entries: the type tag, then the receiver kind.
pub const WITNESS_HEADER_WORDS: u32 = 2;
/// Receiver kind word of a table whose receiver word is a managed reference.
pub const WITNESS_RECEIVER_REFERENCE: u64 = 0;
/// Receiver kind word of a table whose receiver word holds integer, `bool` or `char` bits.
pub const WITNESS_RECEIVER_BITS: u64 = 1;
/// Receiver kind word of a table whose receiver word holds an `f32` bit pattern.
pub const WITNESS_RECEIVER_F32: u64 = 2;
/// Receiver kind word of a table whose receiver word holds an `f64` bit pattern.
pub const WITNESS_RECEIVER_F64: u64 = 3;

/// What the receiver word of a contract box holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ContractBoxReceiver {
    /// A managed reference: a nominal value or a `string`. The box's pointer map covers the word.
    Reference,
    /// The bits of a primitive of this type, widened to one word; outside the pointer map.
    Scalar(SemanticTypeId),
}

impl ContractBoxReceiver {
    /// The receiver kind word of a witness table (`WITNESS_RECEIVER_*`).
    pub fn witness_kind(self) -> u64 {
        match self {
            Self::Reference | Self::Scalar(SemanticTypeId::STRING) => WITNESS_RECEIVER_REFERENCE,
            Self::Scalar(SemanticTypeId::F32) => WITNESS_RECEIVER_F32,
            Self::Scalar(SemanticTypeId::F64) => WITNESS_RECEIVER_F64,
            Self::Scalar(_) => WITNESS_RECEIVER_BITS,
        }
    }
}

/// One witness table: the methods an implementing type instance supplies for an applied
/// contract, in the contract's slot order, with the environment their bodies are specialized in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractWitnessTable {
    /// The implementing declaration: a `type`, an `enum`, or a primitive `impl` block.
    pub concrete: AstNodeKey,
    pub contract: AstNodeKey,
    /// The module-unique symbol of the table's static data.
    pub symbol: Arc<str>,
    /// Receiver kind every entry takes first.
    pub receiver: ContractBoxReceiver,
    /// `(contract method, implementation method)` in slot order.
    pub methods: Arc<[(AstNodeKey, AstNodeKey)]>,
    /// Generic environment of a generic implementation (`impl<T> Box<T> : C` for `Box<i64>`).
    pub environment: Arc<[GenericSubstitution]>,
    /// The implementing type's identity tag: equal for every table of one implementing type
    /// (whatever the contract and the module), different for different types.
    pub type_tag: u64,
    /// The implementing type's tables for the contracts the contract embeds, in
    /// [`contract_embedded_contracts`] order (non-generic embedded contracts only).
    pub embedded: Arc<[ContractWitnessTable]>,
}

/// A contract value converted to a contract its own contract embeds (`Reader r = channel;`): the
/// box is rebuilt with the same receiver word and the table at word `table_word` of the source
/// box's witness table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractUpcast {
    pub table_word: u32,
}

/// A concrete value boxed into a contract-typed destination.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractBoxCoercion {
    pub witness: ContractWitnessTable,
}

/// A contract method call dispatched through a contract box.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractDispatch {
    pub contract: AstNodeKey,
    pub signature_node: AstNodeKey,
    /// Index of the method in every witness table of the contract.
    pub slot: u32,
    /// The callee ABI: the receiver word first (as `POINTER`), then the parameters.
    pub signature: ItemSignature,
}

/// The exact coercion the expression at `key` performs when it is written into a contract-typed
/// destination in `enclosing`: `Ok(None)` when the destination is not a contract, when the value
/// is already a contract value (it passes through), or when the call it is an argument of selects
/// a static specialization for it (a nominal argument of a contract-typed parameter).
pub fn contract_box_coercion(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<ContractBoxCoercion> {
    let Some((qualified_name, arguments, source)) = conversion_site(db, key, enclosing)? else {
        return Ok(None);
    };
    if matches!(source, GenericSourceTypeIdentity::Contract { .. }) {
        return Ok(None);
    }
    let Some(contract) = contract_declaration_for_identity(db, key, &qualified_name) else {
        return Err(SemanticError::unavailable_at("contract_box", key));
    };
    let witness = witness_table(db, key, &source, contract, &arguments)?;
    Ok(Some(ContractBoxCoercion { witness }))
}

/// The conversion a contract value at `key` performs when it is written into a destination of a
/// contract its own contract embeds (`Reader r = channel;` with `contract Channel { Reader; ... }`):
/// `Ok(None)` for a value of the destination's own contract, which passes through, and for every
/// position that is no conversion site.
pub fn contract_upcast(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<ContractUpcast> {
    let Some((target_name, _, source)) = conversion_site(db, key, enclosing)? else {
        return Ok(None);
    };
    let GenericSourceTypeIdentity::Contract { qualified_name: source_name, .. } = &source else {
        return Ok(None);
    };
    if source_name.as_ref() == target_name.as_ref() {
        return Ok(None);
    }
    let (Some(target), Some(source)) = (
        contract_declaration_for_identity(db, key, &target_name),
        contract_declaration_for_identity(db, key, source_name),
    ) else {
        return Err(SemanticError::unavailable_at("contract_upcast", key));
    };
    let Some(position) = upcast_targets(db, source).and_then(|targets| targets.iter().position(|item| *item == target))
    else {
        // Not an embedded contract: the value legality reports it (E1206).
        return Ok(None);
    };
    let methods = contract_method_slots(db, source).ok_or_else(|| SemanticError::unavailable("contract_methods"))?;
    let word = usize::try_from(WITNESS_HEADER_WORDS).unwrap_or(usize::MAX)
        .checked_add(methods.len())
        .and_then(|word| word.checked_add(position))
        .and_then(|word| u32::try_from(word).ok())
        .ok_or_else(|| SemanticError::new("witness table exceeds the ABI word"))?;
    Ok(Some(ContractUpcast { table_word: word }))
}

/// The contracts a value of `contract` converts to: the non-generic contracts it embeds, in the
/// order their tables follow the method entries of every witness table of `contract`.
pub(in crate::semantic_contract) fn upcast_targets(db: &dyn Db, contract: AstNodeKey) -> Option<Vec<AstNodeKey>> {
    let embedded = contract_embedded_contracts(db, contract)?;
    Some(embedded.into_iter().filter(|embedded| contract_generic_count(db, *embedded) == Some(0)).collect())
}

/// The number of generic parameters a contract declares.
fn contract_generic_count(db: &dyn Db, contract: AstNodeKey) -> Option<usize> {
    let syntax = db.syntax_unit(contract.unit).filter(|syntax| syntax.accepts_key(db, contract))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), contract.node)
        .and_then(|node| node.of::<ContractDefinition>())?;
    Some(definition.generics.len())
}

/// A position where a value converts to a contract type, with the destination contract's name and
/// applied arguments and the value's own identity: a typed `let`, an assignment, an enum payload,
/// a struct field, a `return`, an array element, a call argument that selects no static
/// specialization, and the target-typed positions a contract expectation reaches through a
/// conditional branch, a `match` arm, a block result or a lambda body. A conditional, `match` or
/// block expression itself never converts: its branches, arms or result do.
fn conversion_site(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<Option<(Arc<str>, Arc<[GenericSourceTypeIdentity]>, GenericSourceTypeIdentity)>, SemanticError> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    if index.node_at(program, key.node).is_none() || !node_is_value_expression(index, key.node) {
        return Ok(None);
    }
    // Only the normalized expression node of a destination converts; its wrappers do not.
    if normalized_expression_node(index, key.node) != key.node {
        return Ok(None);
    }
    if matches!(
        index.kind(key.node),
        Some(NodeKind::ConditionalExpression | NodeKind::MatchExpression | NodeKind::BlockExpression)
    ) {
        return Ok(None);
    }
    let expected = match expected_type_in(db, key, enclosing) {
        Ok(Some(expected)) => expected,
        Ok(None) => return Ok(None),
        Err(error) if error.is_unavailable() => return Ok(None),
        Err(error) => return Err(error),
    };
    let GenericSourceTypeIdentity::Contract { qualified_name, arguments } = &expected.identity else {
        return Ok(None);
    };
    match &expected.context {
        ExpectedTypeContext::DeclaredLocal { .. }
        | ExpectedTypeContext::Assignment { .. }
        | ExpectedTypeContext::EnumPayload { .. }
        | ExpectedTypeContext::StructField { .. }
        | ExpectedTypeContext::Return { .. }
        | ExpectedTypeContext::ArrayElement { .. }
        | ExpectedTypeContext::ConditionalBranch { .. }
        | ExpectedTypeContext::MatchArm { .. }
        | ExpectedTypeContext::BlockResult { .. }
        | ExpectedTypeContext::LambdaBody { .. } => (),
        ExpectedTypeContext::CallArgument { call, position } => {
            if static_contract_argument(db, *call, *position, key, enclosing)? {
                return Ok(None);
            }
        }
        ExpectedTypeContext::BinaryOperand { .. } | ExpectedTypeContext::ExpressionStatement { .. } => {
            return Ok(None);
        }
    }
    let source = match specialized_source_expression_identity(db, key, enclosing) {
        Ok(source) => source,
        Err(error) if error.is_unavailable() => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(Some((qualified_name.clone(), arguments.clone(), source)))
}

/// Whether the binary expression at `key` is `==` or `!=` on two contract values: equal exactly
/// when both hold the same implementing value (the same object for a nominal type or `string`,
/// equal values of the same primitive type for a boxed primitive).
pub fn contract_value_equality(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<bool> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(binary) = index.node_at(program, key.node).and_then(|node| node.of::<beskid_analysis::syntax::BinaryExpression>())
    else {
        return Ok(None);
    };
    if !matches!(binary.op.node, beskid_analysis::syntax::BinaryOp::Eq | beskid_analysis::syntax::BinaryOp::NotEq) {
        return Ok(Some(false));
    }
    let operand = |expression: &Spanned<beskid_analysis::syntax::Expression>| {
        index
            .direct_child_id(program, key.node, DynNodeRef::from(expression))
            .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
    };
    let is_contract = |operand: Option<AstNodeKey>| {
        operand.is_some_and(|operand| {
            matches!(
                specialized_source_expression_identity(db, operand, enclosing),
                Ok(GenericSourceTypeIdentity::Contract { .. })
            )
        })
    };
    Ok(Some(is_contract(operand(binary.left.as_ref())) && is_contract(operand(binary.right.as_ref()))))
}

/// The witness table that boxes a value of `source` into `contract<applied_arguments>`.
pub(super) fn witness_table(
    db: &dyn Db,
    context: AstNodeKey,
    source: &GenericSourceTypeIdentity,
    contract: AstNodeKey,
    applied_arguments: &[GenericSourceTypeIdentity],
) -> Result<ContractWitnessTable, SemanticError> {
    let receiver = match source {
        GenericSourceTypeIdentity::Abi(SemanticTypeId::STRING) => ContractBoxReceiver::Reference,
        GenericSourceTypeIdentity::Abi(primitive) if is_conformable_primitive(*primitive) => {
            ContractBoxReceiver::Scalar(*primitive)
        }
        GenericSourceTypeIdentity::Nominal { .. } => ContractBoxReceiver::Reference,
        other => {
            return Err(SemanticError::new(format!(
                "a value of type `{}` cannot be a contract value",
                other.display_name()
            )));
        }
    };
    let (concrete, environment, methods) = prove_box_conformance(db, context, source, contract, applied_arguments)?;
    // Every slot of the contract must be filled: the table is indexed by slot.
    let slots = contract_method_slots(db, contract).ok_or_else(|| SemanticError::unavailable("contract_methods"))?;
    let mut ordered = Vec::with_capacity(slots.len());
    for slot in &slots {
        let implementation = methods
            .iter()
            .find_map(|(signature, implementation)| (signature == slot).then_some(*implementation))
            .ok_or_else(|| SemanticError::new("missing contract method implementation for a witness table"))?;
        ordered.push((*slot, implementation));
    }
    let contract_name = stable_declaration_identity(db, contract).ok_or_else(|| SemanticError::unavailable("contract"))?;
    let contract_display = GenericSourceTypeIdentity::Contract {
        qualified_name: contract_name,
        arguments: applied_arguments.iter().cloned().collect(),
    }
    .display_name();
    let symbol = Arc::from(witness_symbol(&source.display_name(), &contract_display).as_str());
    let mut embedded = Vec::new();
    for target in upcast_targets(db, contract).ok_or_else(|| SemanticError::unavailable("contract_embeddings"))? {
        embedded.push(witness_table(db, context, source, target, &[])?);
    }
    Ok(ContractWitnessTable {
        concrete,
        contract,
        symbol,
        receiver,
        methods: ordered.into(),
        environment,
        type_tag: type_tag(&source.display_name()),
        embedded: embedded.into(),
    })
}

/// The identity tag of an implementing type: a 64-bit FNV-1a hash of its exact source identity.
fn type_tag(source: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The symbol of one witness table: readable prefix plus a collision-free hash of the exact
/// implementing identity and applied contract.
fn witness_symbol(source: &str, contract: &str) -> String {
    fn sanitize(text: &str) -> String {
        text.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.bytes().chain([0u8]).chain(contract.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("__beskid_witness_{}_{}_{hash:016x}", sanitize(source), sanitize(contract))
}

/// Whether the argument at `position` of `call` is a nominal value of a contract-typed parameter
/// of a source callable: then the call selects a specialization with a witness and the argument
/// stays a raw reference.
fn static_contract_argument(
    db: &dyn Db,
    call: AstNodeKey,
    position: u32,
    argument: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<bool, SemanticError> {
    let Some(CallLowering::Direct(declaration)) = call_lowering(db, call)? else {
        return Ok(false);
    };
    if !contract_parameter_declarations(db, declaration).iter().any(|(_, candidate, _)| *candidate == position) {
        return Ok(false);
    }
    if forwards_expected_type(db, argument) {
        return Ok(false);
    }
    Ok(matches!(
        specialized_source_expression_identity(db, argument, enclosing),
        Ok(GenericSourceTypeIdentity::Nominal { .. })
    ))
}

/// Whether the expression at `key` forwards its expected type to its branches, arms or result (a
/// conditional, a `match` or a block expression): as a contract-typed argument it converts at
/// those positions, so the call takes the unspecialized body that receives a contract value.
pub(super) fn forwards_expected_type(db: &dyn Db, key: AstNodeKey) -> bool {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return false;
    };
    let index = syntax.syntax_index(db);
    matches!(
        index.kind(normalized_expression_node(index, key.node)),
        Some(NodeKind::ConditionalExpression | NodeKind::MatchExpression | NodeKind::BlockExpression)
    )
}

fn node_is_value_expression(index: &SyntaxIndex, node: beskid_analysis::syntax::AstNodeId) -> bool {
    !matches!(
        index.kind(node),
        None | Some(
            NodeKind::LetStatement
                | NodeKind::ReturnStatement
                | NodeKind::Parameter
                | NodeKind::Field
                | NodeKind::Identifier
                | NodeKind::PathSegment
        )
    )
}

/// The dispatch the call at `key` performs through a contract box in `enclosing`: `Ok(None)` for
/// a call that is no contract method call, or that has a static target (a witness of the
/// enclosing specialization, an immutable local with a concrete initializer, an injected field
/// bound to one implementation, which code generation decides from the composition plan).
pub fn contract_dispatch(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<ContractDispatch> {
    let Some(CallLowering::Direct(signature_node)) = call_lowering(db, key)? else {
        return Ok(None);
    };
    if node_kind(db, signature_node)? != Some(NodeKind::ContractMethodSignature) {
        return Ok(None);
    }
    if let Some(enclosing) = enclosing
        && super::contracts::contract_method_specialization(db, key, enclosing)?.is_some()
    {
        return Ok(None);
    }
    let Some(receiver) = call_arguments(db, key)?.and_then(|arguments| arguments.first().copied()) else {
        return Ok(None);
    };
    // The receiver must be a contract value; a bounded generic receiver with a witness and an
    // injected field with one implementation never reach this point with a box.
    let Some(GenericSourceTypeIdentity::Contract { arguments, .. }) = receiver_identity(db, key, receiver, enclosing)
    else {
        return Ok(None);
    };
    let Some(contract) = contract_of_method_signature(db, signature_node)? else {
        return Ok(None);
    };
    let slot = contract_method_slot(db, signature_node).ok_or_else(|| SemanticError::unavailable("contract_slot"))?;
    let signature = dispatch_signature(db, contract, signature_node, &arguments)?;
    Ok(Some(ContractDispatch { contract, signature_node, slot, signature }))
}

/// The ABI of a contract method applied to the receiver's contract arguments: the receiver word
/// first, then the parameters with the contract's generics substituted.
fn dispatch_signature(
    db: &dyn Db,
    contract: AstNodeKey,
    signature_node: AstNodeKey,
    applied_arguments: &[GenericSourceTypeIdentity],
) -> Result<ItemSignature, SemanticError> {
    let syntax = db
        .syntax_unit(signature_node.unit)
        .filter(|syntax| syntax.accepts_key(db, signature_node))
        .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let signature = index
        .node_at(program, signature_node.node)
        .and_then(|node| node.of::<ContractMethodSignature>())
        .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
    let generics = index
        .node_at(program, contract.node)
        .and_then(|node| node.of::<ContractDefinition>())
        .map(|definition| definition.generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    if generics.len() != applied_arguments.len() {
        return Err(SemanticError::unavailable("contract_arguments"));
    }
    let environment: HashMap<String, SemanticTypeId> =
        generics.into_iter().zip(applied_arguments.iter().map(GenericSourceTypeIdentity::abi_type)).collect();
    let mut parameters = vec![SemanticTypeId::POINTER];
    for parameter in &signature.parameters {
        parameters.push(generic_abi_type(db, signature_node, &parameter.node.ty.node, &environment)?);
    }
    let result = signature
        .return_type
        .as_ref()
        .map_or(Ok(SemanticTypeId::UNIT), |ty| generic_abi_type(db, signature_node, &ty.node, &environment))?;
    Ok(ItemSignature { parameters: parameters.into(), result })
}

/// Whether the direct method call at `key` reads its receiver out of a contract box: the callee
/// is a method definition selected statically and the receiver argument is a contract value.
pub fn contract_value_receiver(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<bool> {
    let Some(receiver) = call_arguments(db, key)?.and_then(|arguments| arguments.first().copied()) else {
        return Ok(None);
    };
    Ok(Some(matches!(receiver_identity(db, key, receiver, enclosing), Some(GenericSourceTypeIdentity::Contract { .. }))))
}

/// The identity of a call receiver as call-argument collection denotes it: an expression, the
/// declaring identifier of a local (`g` in `g.Greet()`), a field segment of a path
/// (`holder.greeter` in `holder.greeter.Rank()`), or the callee path itself when the receiver is
/// an unqualified local the collection could not classify.
fn receiver_identity(
    db: &dyn Db,
    call: AstNodeKey,
    receiver: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<GenericSourceTypeIdentity> {
    let syntax = db.syntax_unit(receiver.unit).filter(|syntax| syntax.accepts_key(db, receiver))?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    match index.kind(receiver.node) {
        Some(NodeKind::Identifier) => local_identity(db, program, index, receiver, enclosing),
        Some(NodeKind::PathSegment) => segment_identity(db, receiver, enclosing),
        Some(NodeKind::PathExpression) if is_callee_of(index, program, call, receiver.node) => {
            let path = &index.node_at(program, receiver.node)?.of::<beskid_analysis::syntax::PathExpression>()?.path.node;
            let (_, prefix) = path.segments.split_last()?;
            if let [single] = prefix
                && single.node.type_args.is_empty()
                && let Some(identifier) = resolve_lexical_declaration(program, index, call.node, &single.node.name.node.name)
            {
                return local_identity(db, program, index, AstNodeKey { node: identifier, ..receiver }, enclosing);
            }
            // A field chain, or a field of the implicit receiver (`greeter.Rank()` in a method).
            // An unqualified sibling call (`Probe()` inside a method) has no receiver segment.
            let segment = super::layouts::path_projection_segment(db, receiver, prefix.len().checked_sub(1)?)?;
            segment_identity(db, segment, enclosing)
        }
        _ => specialized_source_expression_identity(db, receiver, enclosing).ok(),
    }
}

/// The identity of the field a path segment reads: a projected field, or an injected field of the
/// implicit receiver (whose declared type is the contract or contract array it holds).
fn segment_identity(
    db: &dyn Db,
    segment: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<GenericSourceTypeIdentity> {
    match super::layouts::nominal_field_projection_in_environment(db, segment, enclosing) {
        Some(Ok((_, identity))) => Some(identity),
        _ => injected_field_identity(db, segment),
    }
}

/// The declared identity of the injected field read at `key` (a field path or path segment):
/// `Greeter` for `inject Greeter greeter`, `Greeter[]` for `inject Greeter[] all`.
pub(in crate::semantic_contract) fn injected_field_identity(db: &dyn Db, key: AstNodeKey) -> Option<GenericSourceTypeIdentity> {
    let access = super::composition::composition_injected_field_access(db, key).ok().flatten()?;
    let syntax = db.syntax_unit(access.field.unit).filter(|syntax| syntax.accepts_key(db, access.field))?;
    let field = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), access.field.node)?
        .of::<beskid_analysis::syntax::Field>()?;
    super::generic_source_type_identity(db, access.field, &field.ty.node).ok()
}

/// Whether `node` is the (normalized) callee of `call`.
fn is_callee_of(index: &SyntaxIndex, program: &Program, call: AstNodeKey, node: beskid_analysis::syntax::AstNodeId) -> bool {
    index
        .node_at(program, call.node)
        .and_then(|node| node.of::<CallExpression>())
        .and_then(|expression| index.direct_child_id(program, call.node, DynNodeRef::from(expression.callee.as_ref())))
        .is_some_and(|callee| normalized_expression_node(index, callee) == node)
}

/// The identity of the local declared by `identifier`, in `enclosing` when given.
fn local_identity(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    identifier: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<GenericSourceTypeIdentity> {
    if let Some(enclosing) = enclosing
        && let Some(Ok(identity)) = super::contracts::specialized_local_declaration_identity(db, identifier, enclosing)
    {
        return Some(identity);
    }
    super::calls::generic_source_local_identity(db, program, index, identifier, identifier.node).ok()
}

/// A member call on any expression whose identity is a contract (`all[0].Greet()`,
/// `list.Get(0).Greet()`, `Make().Greet()`, `this.greeter.Greet()` on a plain field): the
/// contract method and the receiver expression. The call dispatches through the box unless a
/// static target is known.
pub(super) fn contract_value_member_receiver(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
) -> Option<Result<(AstNodeKey, AstNodeKey), SemanticError>> {
    let call = index.node_at(program, key.node)?.of::<CallExpression>()?;
    let callee = index.direct_child_id(program, key.node, DynNodeRef::from(call.callee.as_ref()))?;
    let callee = normalized_expression_node(index, callee);
    let (receiver, member) = match index.node_at(program, callee)? {
        node if node.of::<beskid_analysis::syntax::MemberExpression>().is_some() => {
            let member = node.of::<beskid_analysis::syntax::MemberExpression>()?;
            let receiver = index.direct_child_id(program, callee, DynNodeRef::from(member.target.as_ref()))?;
            (AstNodeKey { node: normalized_expression_node(index, receiver), ..key }, member.member.node.name.clone())
        }
        node => {
            let path = &node.of::<beskid_analysis::syntax::PathExpression>()?.path.node;
            let (member, prefix) = path.segments.split_last()?;
            if !member.node.type_args.is_empty() || prefix.is_empty() {
                return None;
            }
            // An unqualified local receiver is denoted by the callee path itself (the lowering
            // reads the local from its first segment); a field chain by its field segment.
            let receiver = match prefix {
                [single] if single.node.type_args.is_empty() => {
                    resolve_lexical_declaration(program, index, key.node, &single.node.name.node.name)?;
                    AstNodeKey { node: callee, ..key }
                }
                _ => super::layouts::path_projection_segment(db, AstNodeKey { node: callee, ..key }, prefix.len() - 1)?,
            };
            (receiver, member.node.name.node.name.clone())
        }
    };
    let GenericSourceTypeIdentity::Contract { qualified_name, .. } = receiver_identity(db, key, receiver, None)? else {
        return None;
    };
    let contract = contract_declaration_for_identity(db, key, &qualified_name)?;
    Some(super::contracts::contract_member_named(db, contract, &member).map(|signature| (signature, receiver)))
}

/// The witness table that boxes a value of the non-generic nominal `declaration` into
/// `contract` (composition activation stores a registered implementation into a contract-typed
/// injected field).
pub fn contract_witness_table_for_declaration(
    db: &dyn Db,
    context: AstNodeKey,
    declaration: AstNodeKey,
    contract: AstNodeKey,
) -> SemanticQueryResult<ContractWitnessTable> {
    let Some(qualified_name) = stable_declaration_identity(db, declaration) else {
        return Ok(None);
    };
    let source = GenericSourceTypeIdentity::Nominal { qualified_name, arguments: Arc::from([]) };
    witness_table(db, context, &source, contract, &[]).map(Some)
}

/// The exact instances of a witness table's implementation methods, in slot order: the items
/// module emission must lower so the table can name them.
pub fn contract_witness_method_instances(
    db: &dyn Db,
    table: &ContractWitnessTable,
) -> SemanticQueryResult<Arc<[GenericSpecializationInstance]>> {
    let mut instances = Vec::with_capacity(table.methods.len());
    for (_, implementation) in table.methods.iter() {
        instances.push(super::contracts::implementation_method_instance(db, *implementation, table.environment.clone())?);
    }
    Ok(Some(instances.into()))
}

/// The contract an injected field is typed by (`inject Greeter g`, `inject Greeter[] all`), or
/// `Ok(None)` for a field of a nominal type.
pub fn injected_field_contract(db: &dyn Db, field: AstNodeKey) -> SemanticQueryResult<AstNodeKey> {
    let Some(syntax) = db.syntax_unit(field.unit).filter(|syntax| syntax.accepts_key(db, field)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(node) = index.node_at(program, field.node).and_then(|node| node.of::<beskid_analysis::syntax::Field>())
    else {
        return Ok(None);
    };
    let annotation = match &node.ty.node {
        beskid_analysis::syntax::Type::Array(element) => &element.node,
        other => other,
    };
    let beskid_analysis::syntax::Type::Complex(path) = annotation else {
        return Ok(None);
    };
    Ok(super::contracts::resolve_contract_head(db, field, &path.node))
}

/// Whether the array literal at `key` is written into a destination whose element type is a
/// contract (`Greeter[] all = [Hello {}, Bye {}]`, `Formatting[] mixed = [1_i64, true]`): every
/// element is then boxed, so the array holds managed references whatever the elements' own
/// types are.
pub fn array_literal_has_contract_elements(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<bool> {
    let expected = match expected_type_in(db, key, enclosing) {
        Ok(Some(expected)) => expected,
        Ok(None) => return Ok(None),
        Err(error) if error.is_unavailable() => return Ok(Some(false)),
        Err(error) => return Err(error),
    };
    Ok(Some(matches!(expected.array_element_identity(), Some(GenericSourceTypeIdentity::Contract { .. }))))
}
