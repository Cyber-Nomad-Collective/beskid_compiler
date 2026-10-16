//! Bidirectional expected types: the type one expression is checked against.
//!
//! Every contextual fact in this crate (an unsuffixed integer literal, an empty array literal,
//! a `clif { }` block, a numeric cast intent, an array literal's declared element type) needs
//! the same answer: what does the position of this expression require? This module is the single
//! walk from an expression to its typed parent context. It climbs only transparent wrappers
//! (`Expression`, grouping, a literal's expression node, unary operators) and stops at the first
//! node that owns a type requirement:
//!
//! - an annotated `let`, an assignment target, a declared `return` type;
//! - inside a lambda, the function type the lambda's own position declares: a block body's
//!   `return` and an expression body expect its result (a lambda is never an expectation of its
//!   own, only through that function type), and an untyped parameter takes its parameter type
//!   through `lambda_source_identity`;
//! - the matching parameter of a resolved callee, with the callee's generic parameters bound by
//!   its explicit type arguments or its instantiated nominal receiver, never by inference from
//!   the other arguments (that would be a cycle through argument typing). The one exception is a
//!   bare integer literal argument outside a generic item: inference never types it through this
//!   query, so it takes its parameter from the inferred call specialization instead of
//!   defaulting to `i32` and being truncated; the parameter of a call through a function value is that function type's parameter; a trailing argument of a
//!   `bulk T[]` callee expects the element type;
//! - the declared payload or field type of an enum constructor or struct literal, with the
//!   declaration's generic parameters bound by the written type arguments;
//! - the element type of an array literal or the match expression of an arm, each forwarding
//!   its own expected type; the final expression statement of a block expression that is a value
//!   (an arm body, a call argument) forwards the block's expected type, every other statement is
//!   `unit`, and a lambda's block body forwards nothing (its `return`s carry the expectation);
//! - the proven representation of the peer operand of a binary expression, or, when the peer
//!   is itself an unsuffixed numeric literal, the binary expression's own expected type for the
//!   arithmetic and bitwise operators;
//! - `unit` for an expression statement;
//! - for a runtime intrinsic the unit does not admit, the canonical ABI-v5 manifest parameter.
//!
//! Inside a generic item the enclosing specialization's substitutions interpret the parameter
//! names its declared types mention. Without one, a declared type that still names an enclosing
//! generic stays unavailable, as does every position with no typed context.

use std::collections::HashMap;

use beskid_analysis::syntax::{self, Spanned};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

use super::*;

type Program = Spanned<syntax::Program>;
type Environment<'a> = HashMap<&'a str, &'a GenericSourceTypeIdentity>;

fn unavailable() -> SemanticError {
    SemanticError::unavailable("expected_type")
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn expected_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ExpectedType> {
    with_node(db, syntax, key, |program, index, _node| Some(expected_type_for(db, program, index, key, None)))?
        .transpose()
}

/// The one checking-direction entry for every synthesis query that reads an expected type:
/// [`expected_type`] without an enclosing specialization, [`expected_type_in_environment`] with
/// one. `Ok(None)` when the node has no registered syntax.
pub(in crate::semantic_contract) fn expected_type_in(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<ExpectedType> {
    match enclosing {
        None => expected_type(db, key),
        Some(enclosing) => expected_type_in_environment(db, key, enclosing),
    }
}

/// [`expected_type`] inside the immutable specialization of the enclosing generic item.
///
/// Declared types that name the item's own generic parameters (`T[] values = []`,
/// `return []` from `T[] Make<T>()`, `Append<T>([], item)`) are interpreted through the
/// instance's substitutions. Everything else answers exactly as the target-neutral query.
pub fn expected_type_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<ExpectedType> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    if index.generation() != key.generation || index.metadata_for(key.generation, key.node).is_none() {
        return Ok(None);
    }
    expected_type_for(db, syntax.expanded_program(db), index, key, Some(enclosing)).map(Some)
}

fn enclosing_environment(enclosing: Option<&GenericSpecializationInstance>) -> Environment<'_> {
    enclosing
        .map(|enclosing| {
            enclosing
                .substitutions
                .iter()
                .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
                .collect()
        })
        .unwrap_or_default()
}

/// The expected type of the node that forwards its requirement to `key` (a match expression, an
/// array literal, or a binary expression whose peer is a bare literal).
fn forwarded(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<ExpectedType, SemanticError> {
    match enclosing {
        None => expected_type(db, key)?.ok_or_else(unavailable),
        Some(enclosing) => expected_type_for(db, program, index, key, Some(enclosing)),
    }
}

fn expected_type_for(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<ExpectedType, SemanticError> {
    let mut child = key.node;
    let mut parent = parent_node(index, child).ok_or_else(unavailable)?;
    while matches!(
        index.kind(parent),
        Some(NodeKind::Expression | NodeKind::GroupedExpression | NodeKind::LiteralExpression | NodeKind::UnaryExpression)
    ) {
        child = parent;
        parent = parent_node(index, parent).ok_or_else(unavailable)?;
    }
    let parent_key = AstNodeKey { node: parent, ..key };
    let node = index.node_at(program, parent).ok_or_else(unavailable)?;
    let is_child = |target: DynNodeRef<'_>| index.direct_child_id(program, parent, target) == Some(child);
    let environment = enclosing_environment(enclosing);

    if let Some(statement) = node.of::<syntax::LetStatement>() {
        if !is_child(DynNodeRef::from(&statement.value)) {
            return Err(unavailable());
        }
        let annotation = statement.type_annotation.as_ref().ok_or_else(unavailable)?;
        let identity = generic_source_type_identity_with_substitutions(db, parent_key, &annotation.node, &environment)?;
        return Ok(ExpectedType::new(ExpectedTypeContext::DeclaredLocal { statement: parent_key }, identity));
    }

    if let Some(assignment) = node.of::<syntax::AssignExpression>() {
        if !is_child(DynNodeRef::from(assignment.value.as_ref())) {
            return Err(unavailable());
        }
        let target = index
            .direct_child_id(program, parent, DynNodeRef::from(assignment.target.as_ref()))
            .map(|target| AstNodeKey { node: normalized_expression_node(index, target), ..key })
            .ok_or_else(unavailable)?;
        let identity = specialized_source_expression_identity(db, target, enclosing)?;
        return Ok(ExpectedType::new(ExpectedTypeContext::Assignment { assignment: parent_key }, identity));
    }

    if let Some(call) = node.of::<syntax::CallExpression>() {
        let position = call.args.iter().position(|argument| is_child(DynNodeRef::from(argument))).ok_or_else(unavailable)?;
        let identity = match call_argument_identity(db, program, index, parent_key, call, position, &environment, enclosing)
        {
            Err(error) if error.is_unavailable() && enclosing.is_none() => {
                inferred_integer_argument_identity(db, parent_key, call, AstNodeKey { node: child, ..key }, position)
                    .map_err(|_| error)?
            }
            result => result?,
        };
        let position = u32::try_from(position).map_err(|_| unavailable())?;
        return Ok(ExpectedType::new(ExpectedTypeContext::CallArgument { call: parent_key, position }, identity));
    }

    if let Some(constructor) = node.of::<syntax::EnumConstructorExpression>() {
        let position =
            constructor.args.iter().position(|argument| is_child(DynNodeRef::from(argument))).ok_or_else(unavailable)?;
        let identity = enum_payload_identity(db, parent_key, constructor, position, enclosing)?;
        let position = u32::try_from(position).map_err(|_| unavailable())?;
        return Ok(ExpectedType::new(ExpectedTypeContext::EnumPayload { constructor: parent_key, position }, identity));
    }

    if let Some(field) = node.of::<syntax::StructLiteralField>() {
        if !is_child(DynNodeRef::from(&field.value)) {
            return Err(unavailable());
        }
        let literal_node = parent_node(index, parent).ok_or_else(unavailable)?;
        let literal = index
            .node_at(program, literal_node)
            .and_then(|node| node.of::<syntax::StructLiteralExpression>())
            .ok_or_else(unavailable)?;
        let literal_key = AstNodeKey { node: literal_node, ..key };
        let field_name = field.name.node.name.as_str();
        let identity = struct_field_identity(db, program, index, literal_key, literal, field_name, &environment)?;
        return Ok(ExpectedType::new(
            ExpectedTypeContext::StructField { literal: literal_key, field: Arc::from(field_name) },
            identity,
        ));
    }

    if let Some(statement) = node.of::<syntax::ReturnStatement>() {
        let value = statement.value.as_ref().ok_or_else(unavailable)?;
        if !is_child(DynNodeRef::from(value)) {
            return Err(unavailable());
        }
        let item = nearest_ancestor(index, parent, |kind| {
            matches!(kind, NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::LambdaExpression)
        })
        .ok_or_else(unavailable)?;
        let item_key = AstNodeKey { node: item, ..key };
        let item_node = index.node_at(program, item).ok_or_else(unavailable)?;
        if item_node.of::<syntax::LambdaExpression>().is_some() {
            // A `return` in a block-bodied lambda leaves the lambda; its value is checked against
            // the result of the function type the lambda's own position declares. A lambda never
            // supplies an expectation of its own.
            if !lambda_has_statement_body(program, index, item) {
                return Err(unavailable());
            }
            let outer = forwarded(db, program, index, item_key, enclosing)?;
            let (_, result) = outer.function_identity().ok_or_else(unavailable)?;
            return Ok(ExpectedType::new(ExpectedTypeContext::Return { item: item_key }, result.clone()));
        }
        let return_type = item_node
            .of::<syntax::FunctionDefinition>()
            .and_then(|function| function.return_type.as_ref())
            .or_else(|| item_node.of::<syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref()))
            .ok_or_else(unavailable)?;
        let identity = generic_source_type_identity_with_substitutions(db, item_key, &return_type.node, &environment)?;
        return Ok(ExpectedType::new(ExpectedTypeContext::Return { item: item_key }, identity));
    }

    if let Some(lambda) = node.of::<syntax::LambdaExpression>() {
        // The expression body of a lambda is checked against the result of the function type the
        // lambda's position declares. A block body forwards nothing: its `return`s carry the
        // expectation.
        if !is_child(DynNodeRef::from(lambda.body.as_ref())) || index.kind(child) == Some(NodeKind::BlockExpression) {
            return Err(unavailable());
        }
        let outer = forwarded(db, program, index, parent_key, enclosing)?;
        let (_, result) = outer.function_identity().ok_or_else(unavailable)?;
        return Ok(ExpectedType::new(ExpectedTypeContext::LambdaBody { lambda: parent_key }, result.clone()));
    }

    if let Some(array) = node.of::<syntax::ArrayLiteralExpression>() {
        if !array.elements.iter().any(|element| is_child(DynNodeRef::from(element))) {
            return Err(unavailable());
        }
        let outer = forwarded(db, program, index, parent_key, enclosing)?;
        let element = outer.array_element_identity().cloned().ok_or_else(unavailable)?;
        return Ok(ExpectedType::new(ExpectedTypeContext::ArrayElement { array: parent_key }, element));
    }

    if let Some(arm) = node.of::<syntax::MatchArm>() {
        if !is_child(DynNodeRef::from(&arm.value)) {
            return Err(unavailable());
        }
        let expression = parent_node(index, parent).ok_or_else(unavailable)?;
        if index.kind(expression) != Some(NodeKind::MatchExpression) {
            return Err(unavailable());
        }
        let expression_key = AstNodeKey { node: expression, ..key };
        let outer = forwarded(db, program, index, expression_key, enclosing)?;
        return Ok(ExpectedType::new(ExpectedTypeContext::MatchArm { expression: expression_key }, outer.identity));
    }

    if let Some(binary) = node.of::<syntax::BinaryExpression>() {
        if index.kind(child) == Some(NodeKind::BinaryOp) {
            return Err(unavailable());
        }
        let peer = index
            .children(parent)
            .ok_or_else(unavailable)?
            .iter()
            .copied()
            .filter(|candidate| *candidate != child)
            .find(|candidate| index.kind(*candidate) != Some(NodeKind::BinaryOp))
            .ok_or_else(unavailable)?;
        let peer_key = AstNodeKey { node: normalized_expression_node(index, peer), ..key };
        // A binary operand's expectation is its peer's scalar representation; a pointer-shaped
        // peer (a constructor, a struct or array literal, a lambda) proves no scalar, and typing
        // it would read this operand back through its own expectation.
        if matches!(
            index.kind(peer_key.node),
            Some(
                NodeKind::EnumConstructorExpression
                    | NodeKind::StructLiteralExpression
                    | NodeKind::ArrayLiteralExpression
                    | NodeKind::LambdaExpression
            )
        ) {
            return Err(unavailable());
        }
        let identity = if contextual_numeric_operand(db, peer_key)? {
            // Two bare literals share the representation their binary expression is checked
            // against; a comparison of two bare literals has no such requirement.
            use syntax::BinaryOp;
            if !matches!(
                binary.op.node,
                BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::Shl
                    | BinaryOp::Shr
            ) {
                return Err(unavailable());
            }
            forwarded(db, program, index, parent_key, enclosing)?.identity
        } else {
            match value_abi_type(db, peer_key) {
                Ok(Some(abi)) => GenericSourceTypeIdentity::Abi(abi),
                Ok(None) => return Err(unavailable()),
                Err(error) if error.is_unavailable() => return Err(unavailable()),
                Err(error) => return Err(error),
            }
        };
        return Ok(ExpectedType::new(
            ExpectedTypeContext::BinaryOperand { expression: parent_key, peer: peer_key },
            identity,
        ));
    }

    if let Some(statement) = node.of::<syntax::ExpressionStatement>() {
        if !is_child(DynNodeRef::from(&statement.expression)) {
            return Err(unavailable());
        }
        // The final statement of a block expression that is itself a value (`=> { Ok(v); }`)
        // yields the block's value, so it forwards the block's expected type. A lambda's block
        // body is not a value (its `return`s are), and every other statement discards its value.
        if let Some(block) = value_block_of_final_statement(db, index, parent_key)? {
            let outer = forwarded(db, program, index, block, enclosing)?;
            return Ok(ExpectedType::new(ExpectedTypeContext::BlockResult { block }, outer.identity));
        }
        return Ok(ExpectedType::new(
            ExpectedTypeContext::ExpressionStatement { statement: parent_key },
            GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT),
        ));
    }

    Err(unavailable())
}

/// The block expression whose value the expression statement `statement` yields: the statement
/// is the last of the block's statements, the block is a `BlockExpression`, and that expression
/// is not the body of a lambda. `None` for every other statement.
fn value_block_of_final_statement(
    db: &dyn Db,
    index: &SyntaxIndex,
    statement: AstNodeKey,
) -> Result<Option<AstNodeKey>, SemanticError> {
    let Some(wrapper) = parent_node(index, statement.node).filter(|node| index.kind(*node) == Some(NodeKind::Statement))
    else {
        return Ok(None);
    };
    let Some(block) = parent_node(index, wrapper).filter(|node| index.kind(*node) == Some(NodeKind::Block)) else {
        return Ok(None);
    };
    let Some(expression) =
        parent_node(index, block).filter(|node| index.kind(*node) == Some(NodeKind::BlockExpression))
    else {
        return Ok(None);
    };
    if parent_node(index, expression).and_then(|parent| index.kind(parent)) == Some(NodeKind::LambdaExpression) {
        return Ok(None);
    }
    let statements = block_statement_nodes(db, AstNodeKey { node: block, ..statement })?.ok_or_else(unavailable)?;
    Ok(statements.last().filter(|last| last.node == statement.node).map(|_| AstNodeKey { node: expression, ..statement }))
}

/// Whether a binary operand is a bare numeric literal (or integer constant) that takes its
/// representation from context instead of proving one.
fn contextual_numeric_operand(db: &dyn Db, key: AstNodeKey) -> Result<bool, SemanticError> {
    if integer_literal_text(db, key)?.is_some() || contextual_constant_integer(db, key)?.is_some() {
        return Ok(true);
    }
    unsuffixed_float_literal(db, key)
}

fn unsuffixed_float_literal(db: &dyn Db, key: AstNodeKey) -> Result<bool, SemanticError> {
    if let Some(literal) = literal_fact(db, key)? {
        return Ok(matches!(literal, LiteralFact::Float(text) if !text.ends_with("_f32") && !text.ends_with("_f64")));
    }
    let Some(children) = child_nodes(db, key)? else {
        return Ok(false);
    };
    match children.as_ref() {
        [child] => unsuffixed_float_literal(db, *child),
        [_, operand]
            if node_kind(db, key)? == Some(IndexedNodeKind::UnaryExpression)
                && operator_fact(db, key)? == Some(OperatorFact::Neg) =>
        {
            unsuffixed_float_literal(db, *operand)
        }
        _ => Ok(false),
    }
}

/// The declared type of the callee parameter at `position`.
///
/// A direct callee contributes its written parameter syntax, resolved in the callee's own unit.
/// Its generic parameters are bound only by this call's explicit type arguments or by the
/// instantiated nominal receiver of a method call; a parameter that still names an unbound
/// generic stays unavailable rather than being inferred from the sibling arguments. A call
/// through a function value contributes the parameter of the callee's source function type.
/// Runtime, manifest, corelib-service and native Mod callables contribute their ABI parameter.
#[expect(clippy::too_many_arguments, reason = "one walk carries the call, its position and both environments")]
fn call_argument_identity(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    call_key: AstNodeKey,
    call: &syntax::CallExpression,
    position: usize,
    environment: &Environment<'_>,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    // `u8(97)` converts its operand; it does not check it against `u8`.
    if primitive_numeric_conversion_target(call).is_some() {
        return Err(unavailable());
    }
    let lowering = match call_lowering(db, call_key) {
        Ok(Some(lowering)) => lowering,
        // A runtime intrinsic the unit does not admit has no lowering, but its canonical ABI-v5
        // signature is target-independent syntax authority for the width of its arguments.
        Ok(None) => return canonical_intrinsic_argument_identity(program, index, call_key, call, position),
        Err(error) if error.is_unavailable() => {
            return canonical_intrinsic_argument_identity(program, index, call_key, call, position);
        }
        Err(error) => return Err(error),
    };
    match lowering {
        CallLowering::Direct(declaration) => {
            direct_parameter_identity(db, call_key, call, declaration, position, environment, enclosing)
        }
        CallLowering::FunctionValue(callee) => {
            let identity = specialized_source_expression_identity(db, callee, enclosing)?;
            let GenericSourceTypeIdentity::Function { parameters, .. } = identity else {
                return Err(unavailable());
            };
            if parameters.len() != call.args.len() {
                return Err(unavailable());
            }
            parameters.get(position).cloned().ok_or_else(unavailable)
        }
        CallLowering::Runtime(_)
        | CallLowering::ManifestBuiltin(_)
        | CallLowering::CorelibService(_)
        | CallLowering::NativeModCallback(_) => {
            let signature = call_abi_signature(db, call_key)?.ok_or_else(unavailable)?;
            if signature.parameters.len() != call.args.len() {
                return Err(unavailable());
            }
            signature.parameters.get(position).copied().map(GenericSourceTypeIdentity::Abi).ok_or_else(unavailable)
        }
        CallLowering::Dynamic => Err(unavailable()),
    }
}

/// The parameter of an inferred generic call at `position` when its argument is a bare integer
/// literal.
///
/// Generic inference ([`generic_call_specialization`]) binds `T` from the exactly typed sibling
/// arguments and admits the bare literal only when it fits that binding, so `Equal(x, 2147483649)`
/// with an `i64` `x` passes the literal as `i64`. The literal must take that width here too;
/// otherwise it would fall back to the default `i32` and be silently truncated at the call. Only
/// a bare integer argument consults the specialization: inference never asks for the expected
/// type of such an argument, so the query cannot cycle.
fn inferred_integer_argument_identity(
    db: &dyn Db,
    call_key: AstNodeKey,
    call: &syntax::CallExpression,
    argument: AstNodeKey,
    position: usize,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    if !unsuffixed_integer_literal(db, argument)? {
        return Err(unavailable());
    }
    let arguments = call_arguments(db, call_key)?.ok_or_else(unavailable)?;
    let receiver = arguments.len().checked_sub(call.args.len()).ok_or_else(unavailable)?;
    if arguments.get(receiver + position) != Some(&argument) {
        return Err(unavailable());
    }
    let specialization = generic_call_specialization(db, call_key)?.ok_or_else(unavailable)?;
    if specialization.substitutions.is_empty() || specialization.signature.parameters.len() != arguments.len() {
        return Err(unavailable());
    }
    let parameter = specialization.signature.parameters[receiver + position];
    if !integer_literal_fits_abi(db, argument, parameter)? {
        return Err(unavailable());
    }
    Ok(GenericSourceTypeIdentity::Abi(parameter))
}

/// The canonical manifest parameter of an unshadowed single-segment runtime intrinsic callee.
fn canonical_intrinsic_argument_identity(
    program: &Program,
    index: &SyntaxIndex,
    call_key: AstNodeKey,
    call: &syntax::CallExpression,
    position: usize,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let syntax::Expression::Path(path) = &call.callee.node else {
        return Err(unavailable());
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return Err(unavailable());
    };
    let name = segment.node.name.node.name.as_str();
    if !segment.node.type_args.is_empty() || resolve_lexical_declaration(program, index, call_key.node, name).is_some() {
        return Err(unavailable());
    }
    canonical_intrinsic_parameter_type(name, position).map(GenericSourceTypeIdentity::Abi).ok_or_else(unavailable)
}

fn direct_parameter_identity(
    db: &dyn Db,
    call_key: AstNodeKey,
    call: &syntax::CallExpression,
    declaration: AstNodeKey,
    position: usize,
    environment: &Environment<'_>,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let callee_syntax =
        db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)).ok_or_else(unavailable)?;
    let callee = callee_syntax
        .syntax_index(db)
        .node_at(callee_syntax.expanded_program(db), declaration.node)
        .ok_or_else(unavailable)?;
    let parameter_type = declared_parameter_type(callee, call, position)?;
    let generic_names = generic_callable_parameters(db, declaration).map(|(names, _)| names).unwrap_or_default();
    let mentioned = generic_names
        .iter()
        .copied()
        .filter(|name| type_syntax_mentions_generic_parameter(parameter_type, name))
        .collect::<Vec<_>>();
    let bindings = if mentioned.is_empty() {
        Vec::new()
    } else {
        let bindings = callee_generic_bindings(db, call_key, call, declaration, &generic_names, environment, enclosing)?;
        if mentioned.iter().any(|name| !bindings.iter().any(|(bound, _)| bound == name)) {
            return Err(unavailable());
        }
        bindings
    };
    let callee_environment = bindings.iter().map(|(name, identity)| (name.as_str(), identity)).collect::<Environment<'_>>();
    generic_source_type_identity_with_substitutions(db, declaration, parameter_type, &callee_environment)
}

/// The written type of the callee parameter that receives argument `position`.
fn declared_parameter_type<'a>(
    callee: DynNodeRef<'a>,
    call: &syntax::CallExpression,
    position: usize,
) -> Result<&'a syntax::Type, SemanticError> {
    let parameters = callee
        .of::<syntax::FunctionDefinition>()
        .map(|function| &function.parameters)
        .or_else(|| callee.of::<syntax::MethodDefinition>().map(|method| &method.parameters))
        .or_else(|| callee.of::<syntax::ContractMethodSignature>().map(|method| &method.parameters))
        .ok_or_else(unavailable)?;
    // A trailing `bulk T[]` parameter takes every remaining written argument as one `T`; any
    // other parameter takes exactly its position.
    let bulk_element = match parameters.split_last() {
        Some((last, fixed)) if last.node.bulk => {
            if call.args.len() < fixed.len() {
                return Err(unavailable());
            }
            let syntax::Type::Array(element) = &last.node.ty.node else {
                return Err(unavailable());
            };
            (position >= fixed.len()).then_some(element.as_ref())
        }
        _ => {
            if parameters.len() != call.args.len() {
                return Err(unavailable());
            }
            None
        }
    };
    match bulk_element {
        Some(element) => Ok(&element.node),
        None => {
            let parameter = &parameters.get(position).ok_or_else(unavailable)?.node;
            if parameter.bulk {
                return Err(unavailable());
            }
            Ok(&parameter.ty.node)
        }
    }
}

/// The callee's type parameters this call binds without inference: a method's owner parameters
/// from its receiver (inside a specialization, the receiver's identity in that environment: the
/// enclosing method's own receiver, a local or parameter typed by the enclosing generics; outside
/// one, its instantiated nominal receiver) and its own from the type arguments written on the
/// member name; a function's from the type arguments on its path.
#[expect(clippy::too_many_arguments, reason = "one call walk with its callee and both environments")]
fn callee_generic_bindings(
    db: &dyn Db,
    call_key: AstNodeKey,
    call: &syntax::CallExpression,
    declaration: AstNodeKey,
    generic_names: &[&str],
    environment: &Environment<'_>,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<Vec<(String, GenericSourceTypeIdentity)>, SemanticError> {
    let mut bindings = Vec::<(String, GenericSourceTypeIdentity)>::new();
    let is_method = matches!(generic_callable_parameters(db, declaration), Some((_, true)));
    if is_method {
        let owner_bindings = specialized_owner_bindings(db, call_key, declaration, enclosing)?;
        if let Some(owner_bindings) = owner_bindings {
            bindings.extend(
                owner_bindings.iter().map(|binding| (binding.parameter.to_string(), binding.source_identity().clone())),
            );
        } else if let Some(receiver) = generic_nominal_method_receiver(db, call_key)? {
            bindings.extend(
                receiver
                    .substitutions
                    .iter()
                    .map(|binding| (binding.parameter.to_string(), binding.source_identity().clone())),
            );
        }
        if let Some(arguments) = call_explicit_member_type_arguments(db, call_key) {
            let own = &generic_names[generic_callable_owner_count(db, declaration).min(generic_names.len())..];
            if own.len() != arguments.len() {
                return Err(unavailable());
            }
            for (name, argument) in own.iter().zip(arguments.iter()) {
                let identity = generic_source_type_identity_with_substitutions(db, call_key, &argument.node, environment)?;
                bindings.push(((*name).to_owned(), identity));
            }
        }
        return Ok(bindings);
    }
    if let syntax::Expression::Path(path) = &call.callee.node
        && let Some(arguments) = explicit_generic_type_argument_syntax(&path.node.path.node)
    {
        if arguments.len() != generic_names.len() {
            return Err(unavailable());
        }
        for (name, argument) in generic_names.iter().zip(arguments) {
            let identity = generic_source_type_identity_with_substitutions(db, call_key, &argument.node, environment)?;
            bindings.push(((*name).to_owned(), identity));
        }
    } else if let Some(receiver) = generic_nominal_method_receiver(db, call_key)? {
        bindings.extend(
            receiver.substitutions.iter().map(|binding| (binding.parameter.to_string(), binding.source_identity().clone())),
        );
    }
    Ok(bindings)
}

/// The owner bindings of a method call's receiver inside the specialization `enclosing`: the
/// enclosing method's implicit receiver carries the instance's own substitutions; any other
/// receiver (`output.Send(..)` with `Port<Item<T>> output`, `b.Take()` with `Box<R> b` in a
/// lambda) is read through its identity in the environment. `None` outside a specialization or
/// when the receiver has no owner identity there.
fn specialized_owner_bindings(
    db: &dyn Db,
    call_key: AstNodeKey,
    declaration: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<Option<Arc<[GenericSubstitution]>>, SemanticError> {
    let Some(enclosing) = enclosing else { return Ok(None) };
    if generic_callable_owner_count(db, declaration) == 0 {
        return Ok(None);
    }
    let Some(owner) = method_owner_declaration(db, declaration) else { return Ok(None) };
    let Some(arguments) = call_arguments(db, call_key)? else { return Ok(None) };
    let Some(receiver) = arguments.first().copied() else { return Ok(None) };
    if implicit_method_receiver(db, receiver)? == Some(enclosing.declaration)
        && method_owner_declaration(db, enclosing.declaration) == Some(owner)
    {
        return Ok(Some(enclosing.substitutions.clone()));
    }
    Ok(specialized_method_receiver_bindings(db, call_key, receiver, owner, enclosing))
}

/// The parameter types a lambda literal argument takes from its callee when the function-typed
/// callee parameter still names an unbound type parameter in its result only
/// (`U Apply<U>((T) => U f)` with `T` bound by the receiver). This is the 2026-10-13 refinement of
/// the `expected_type` rule: the parameter types are supplied, the result has no expected type and
/// is synthesized from the lambda body. `Ok(None)` when the lambda is not such an argument.
pub(in crate::semantic_contract) fn lambda_argument_parameter_identities(
    db: &dyn Db,
    lambda: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<Option<Vec<GenericSourceTypeIdentity>>, SemanticError> {
    let Some(syntax) = db.syntax_unit(lambda.unit).filter(|syntax| syntax.accepts_key(db, lambda)) else {
        return Ok(None);
    };
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let mut child = lambda.node;
    let Some(mut parent) = parent_node(index, child) else { return Ok(None) };
    while matches!(index.kind(parent), Some(NodeKind::Expression | NodeKind::GroupedExpression)) {
        child = parent;
        let Some(next) = parent_node(index, parent) else { return Ok(None) };
        parent = next;
    }
    let Some(call) = index.node_at(program, parent).and_then(|node| node.of::<syntax::CallExpression>()) else {
        return Ok(None);
    };
    let Some(position) =
        call.args.iter().position(|argument| index.direct_child_id(program, parent, DynNodeRef::from(argument)) == Some(child))
    else {
        return Ok(None);
    };
    let call_key = AstNodeKey { node: parent, ..lambda };
    let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, call_key) else { return Ok(None) };
    let Some(callee_syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))
    else {
        return Ok(None);
    };
    let Some(callee) = callee_syntax.syntax_index(db).node_at(callee_syntax.expanded_program(db), declaration.node)
    else {
        return Ok(None);
    };
    let Ok(syntax::Type::Function { parameters, .. }) = declared_parameter_type(callee, call, position) else {
        return Ok(None);
    };
    let generic_names = generic_callable_parameters(db, declaration).map(|(names, _)| names).unwrap_or_default();
    let bindings = callee_generic_bindings(
        db,
        call_key,
        call,
        declaration,
        &generic_names,
        &enclosing_environment(enclosing),
        enclosing,
    )?;
    let bindings = sibling_argument_bindings(
        db,
        program,
        index,
        call_key,
        call,
        callee,
        declaration,
        position,
        &generic_names,
        bindings,
    );
    let callee_environment = bindings.iter().map(|(name, identity)| (name.as_str(), identity)).collect::<Environment<'_>>();
    let mut identities = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        if generic_names.iter().any(|name| {
            type_syntax_mentions_generic_parameter(&parameter.node, name) && !callee_environment.contains_key(name)
        }) {
            return Ok(None);
        }
        identities.push(generic_source_type_identity_with_substitutions(
            db,
            declaration,
            &parameter.node,
            &callee_environment,
        )?);
    }
    Ok(Some(identities))
}

/// Inference steps 3 and 4 for one lambda argument at `position`: extend the receiver and
/// explicit bindings by unifying, left to right, every non-lambda argument's proven type with its
/// declared parameter type, then every earlier lambda argument's synthesized function type. Bare
/// integer literals are skipped (their type follows from the call, not the other way round), and
/// an argument whose type is unavailable or conflicts contributes nothing here; the call's own
/// specialization reports the conflict (E1229).
#[expect(clippy::too_many_arguments, reason = "one call walk with its callee and bindings")]
fn sibling_argument_bindings(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    call_key: AstNodeKey,
    call: &syntax::CallExpression,
    callee: DynNodeRef<'_>,
    declaration: AstNodeKey,
    position: usize,
    generic_names: &[&str],
    bindings: Vec<(String, GenericSourceTypeIdentity)>,
) -> Vec<(String, GenericSourceTypeIdentity)> {
    let mut abi = bindings.iter().map(|(name, identity)| (name.clone(), identity.abi_type())).collect::<HashMap<_, _>>();
    let mut sources = bindings
        .iter()
        .map(|(name, identity)| {
            (name.clone(), GenericSubstitution::from_source(name.as_str(), identity.abi_type(), identity.clone()))
        })
        .collect::<HashMap<_, _>>();
    let lambdas = call.args.iter().map(|argument| expression_is_lambda(&argument.node)).collect::<Vec<_>>();
    let order = (0..call.args.len())
        .filter(|candidate| *candidate != position && !lambdas[*candidate])
        .chain((0..position).filter(|candidate| lambdas[*candidate]));
    for candidate in order {
        let Ok(parameter_type) = declared_parameter_type(callee, call, candidate) else { continue };
        if !generic_names
            .iter()
            .any(|name| type_syntax_mentions_generic_parameter(parameter_type, name) && !abi.contains_key(*name))
        {
            continue;
        }
        let Some(argument) = index.direct_child_id(program, call_key.node, DynNodeRef::from(&call.args[candidate]))
        else {
            continue;
        };
        let argument = AstNodeKey { node: normalized_expression_node(index, argument), ..call_key };
        if !lambdas[candidate] && matches!(unsuffixed_integer_literal(db, argument), Ok(true) | Err(_)) {
            continue;
        }
        let Ok(identity) = generic_source_expression_identity(db, argument) else { continue };
        let (mut next_abi, mut next_sources) = (abi.clone(), sources.clone());
        if infer_source_substitutions(
            db,
            declaration,
            parameter_type,
            &identity,
            generic_names,
            &mut next_abi,
            &mut next_sources,
        )
        .is_ok()
        {
            abi = next_abi;
            sources = next_sources;
        }
    }
    let mut extended = sources
        .into_iter()
        .map(|(name, binding)| (name, binding.source_identity().clone()))
        .collect::<Vec<_>>();
    extended.sort_by(|left, right| left.0.cmp(&right.0));
    extended
}

fn enum_payload_identity(
    db: &dyn Db,
    constructor_key: AstNodeKey,
    constructor: &syntax::EnumConstructorExpression,
    position: usize,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    // The outer constructor's own applied enum: written, or expected by its own position.
    let applied = applied_enum_constructor_identity(db, constructor_key, constructor, enclosing)?;
    let declaration = crate::semantic_contract::contracts::concrete_type_or_enum_declaration(db, constructor_key, &applied)
        .ok_or_else(unavailable)?;
    let enum_syntax =
        db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)).ok_or_else(unavailable)?;
    let definition = enum_syntax
        .syntax_index(db)
        .node_at(enum_syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<syntax::EnumDefinition>())
        .ok_or_else(unavailable)?;
    let variant = definition
        .variants
        .iter()
        .find(|variant| variant.node.name.node.name == constructor.path.node.variant.node.name)
        .ok_or_else(unavailable)?;
    if variant.node.fields.len() != constructor.args.len() {
        return Err(unavailable());
    }
    let payload_type = &variant.node.fields.get(position).ok_or_else(unavailable)?.node.ty.node;
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = &applied else { return Err(unavailable()) };
    if arguments.len() != definition.generics.len() {
        return Err(unavailable());
    }
    let applied = definition
        .generics
        .iter()
        .map(|generic| generic.node.name.as_str())
        .zip(arguments.iter())
        .collect::<Environment<'_>>();
    generic_source_type_identity_with_substitutions(db, declaration, payload_type, &applied)
}

fn struct_field_identity(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    literal_key: AstNodeKey,
    literal: &syntax::StructLiteralExpression,
    field_name: &str,
    environment: &Environment<'_>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let declaration =
        resolve_nominal_layout_declaration(db, program, index, literal_key, &literal.path.node).ok_or_else(unavailable)?;
    let owner_syntax =
        db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)).ok_or_else(unavailable)?;
    let definition = owner_syntax
        .syntax_index(db)
        .node_at(owner_syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<syntax::TypeDefinition>())
        .ok_or_else(unavailable)?;
    let field = definition
        .fields
        .iter()
        .find(|field| field.node.kind == syntax::FieldKind::Value && field.node.name.node.name == field_name)
        .ok_or_else(unavailable)?;
    let generics = definition.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
    let type_arguments =
        literal.path.node.segments.iter().flat_map(|segment| segment.node.type_args.iter()).collect::<Vec<_>>();
    applied_member_identity(db, literal_key, declaration, &field.node.ty.node, &generics, &type_arguments, environment)
}

/// Resolve one declared member type (a payload or field) of a possibly generic declaration as
/// applied at `use_key`. A member type that names none of the declaration's generics is owned by
/// the declaration alone; one that does requires the complete written type arguments.
fn applied_member_identity(
    db: &dyn Db,
    use_key: AstNodeKey,
    declaration: AstNodeKey,
    member_type: &syntax::Type,
    generics: &[&str],
    type_arguments: &[&Spanned<syntax::Type>],
    environment: &Environment<'_>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let mentions_generic = generics.iter().any(|generic| type_syntax_mentions_generic_parameter(member_type, generic));
    if !mentions_generic {
        return generic_source_type_identity_with_substitutions(db, declaration, member_type, &Environment::new());
    }
    if type_arguments.len() != generics.len() {
        return Err(unavailable());
    }
    let bindings = generics
        .iter()
        .zip(type_arguments)
        .map(|(generic, argument)| {
            let identity = generic_source_type_identity_with_substitutions(db, use_key, &argument.node, environment)?;
            Ok(((*generic).to_owned(), identity))
        })
        .collect::<Result<Vec<_>, SemanticError>>()?;
    let applied = bindings.iter().map(|(name, identity)| (name.as_str(), identity)).collect::<Environment<'_>>();
    generic_source_type_identity_with_substitutions(db, declaration, member_type, &applied)
}
