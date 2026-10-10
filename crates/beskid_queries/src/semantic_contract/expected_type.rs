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
//! - the matching parameter of a resolved callee, with the callee's generic parameters bound by
//!   its explicit type arguments or its instantiated nominal receiver, never by inference from
//!   the other arguments (that would be a cycle through argument typing);
//! - the declared payload or field type of an enum constructor or struct literal, with the
//!   declaration's generic parameters bound by the written type arguments;
//! - the element type of an array literal, the match expression of an arm, or the block
//!   expression of a value-producing last statement, each forwarding its own expected type;
//! - the proven representation of the peer operand of a binary expression, or, when the peer
//!   is itself an unsuffixed numeric literal, the binary expression's own expected type for the
//!   arithmetic and bitwise operators;
//! - `unit` for an expression statement.
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
/// array literal, a block expression, or a binary expression whose peer is a bare literal).
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
        let identity = call_argument_identity(db, parent_key, call, position, &environment)?;
        let position = u32::try_from(position).map_err(|_| unavailable())?;
        return Ok(ExpectedType::new(ExpectedTypeContext::CallArgument { call: parent_key, position }, identity));
    }

    if let Some(constructor) = node.of::<syntax::EnumConstructorExpression>() {
        let position =
            constructor.args.iter().position(|argument| is_child(DynNodeRef::from(argument))).ok_or_else(unavailable)?;
        let identity = enum_payload_identity(db, program, index, parent_key, constructor, position, &environment)?;
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
        // A lambda's result type is inferred from its body, so it is never an expectation.
        let return_type = item_node
            .of::<syntax::FunctionDefinition>()
            .and_then(|function| function.return_type.as_ref())
            .or_else(|| item_node.of::<syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref()))
            .ok_or_else(unavailable)?;
        let identity = generic_source_type_identity_with_substitutions(db, item_key, &return_type.node, &environment)?;
        return Ok(ExpectedType::new(ExpectedTypeContext::Return { item: item_key }, identity));
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
        if let Some(block) = block_result_owner(db, index, parent_key) {
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

/// The block expression whose value `statement` produces: the statement is the last executable
/// statement of a `Block` owned by a `BlockExpression`.
fn block_result_owner(db: &dyn Db, index: &SyntaxIndex, statement: AstNodeKey) -> Option<AstNodeKey> {
    let wrapper = parent_node(index, statement.node)?;
    (index.kind(wrapper) == Some(NodeKind::Statement)).then_some(())?;
    let block = parent_node(index, wrapper)?;
    (index.kind(block) == Some(NodeKind::Block)).then_some(())?;
    let block_expression = parent_node(index, block)?;
    (index.kind(block_expression) == Some(NodeKind::BlockExpression)).then_some(())?;
    let statements = block_statement_nodes(db, AstNodeKey { node: block, ..statement }).ok()??;
    (statements.last().map(|last| last.node) == Some(statement.node)).then_some(())?;
    Some(AstNodeKey { node: block_expression, ..statement })
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
/// generic stays unavailable rather than being inferred from the sibling arguments. Runtime,
/// manifest, corelib-service and native Mod callables contribute their ABI parameter.
fn call_argument_identity(
    db: &dyn Db,
    call_key: AstNodeKey,
    call: &syntax::CallExpression,
    position: usize,
    environment: &Environment<'_>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    // `u8(97)` converts its operand; it does not check it against `u8`.
    if primitive_numeric_conversion_target(call).is_some() {
        return Err(unavailable());
    }
    match call_lowering(db, call_key)?.ok_or_else(unavailable)? {
        CallLowering::Direct(declaration) => {
            direct_parameter_identity(db, call_key, call, declaration, position, environment)
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

fn direct_parameter_identity(
    db: &dyn Db,
    call_key: AstNodeKey,
    call: &syntax::CallExpression,
    declaration: AstNodeKey,
    position: usize,
    environment: &Environment<'_>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let callee_syntax =
        db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)).ok_or_else(unavailable)?;
    let callee = callee_syntax
        .syntax_index(db)
        .node_at(callee_syntax.expanded_program(db), declaration.node)
        .ok_or_else(unavailable)?;
    let parameters = callee
        .of::<syntax::FunctionDefinition>()
        .map(|function| &function.parameters)
        .or_else(|| callee.of::<syntax::MethodDefinition>().map(|method| &method.parameters))
        .or_else(|| callee.of::<syntax::ContractMethodSignature>().map(|method| &method.parameters))
        .ok_or_else(unavailable)?;
    if parameters.len() != call.args.len() {
        return Err(unavailable());
    }
    let parameter = &parameters.get(position).ok_or_else(unavailable)?.node;
    // A `bulk T[]` parameter packs the written arguments; the pack, not one argument, has the
    // declared type.
    if parameter.bulk {
        return Err(unavailable());
    }
    let generic_names = generic_callable_parameters(db, declaration).map(|(names, _)| names).unwrap_or_default();
    let mentioned = generic_names
        .iter()
        .copied()
        .filter(|name| type_syntax_mentions_generic_parameter(&parameter.ty.node, name))
        .collect::<Vec<_>>();
    let mut bindings = Vec::<(String, GenericSourceTypeIdentity)>::new();
    if !mentioned.is_empty() {
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
                receiver
                    .substitutions
                    .iter()
                    .map(|binding| (binding.parameter.to_string(), binding.source_identity().clone())),
            );
        }
        if mentioned.iter().any(|name| !bindings.iter().any(|(bound, _)| bound == name)) {
            return Err(unavailable());
        }
    }
    let callee_environment = bindings.iter().map(|(name, identity)| (name.as_str(), identity)).collect::<Environment<'_>>();
    generic_source_type_identity_with_substitutions(db, declaration, &parameter.ty.node, &callee_environment)
}

fn enum_payload_identity(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    constructor_key: AstNodeKey,
    constructor: &syntax::EnumConstructorExpression,
    position: usize,
    environment: &Environment<'_>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let path = contextual_enum_constructor_type_path(db, program, index, constructor_key, constructor)
        .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
    let declaration = resolve_type_declaration(db, constructor_key, &path).ok_or_else(unavailable)?;
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
    let generics = definition.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
    let type_arguments = path.segments.iter().flat_map(|segment| segment.node.type_args.iter()).collect::<Vec<_>>();
    applied_member_identity(db, constructor_key, declaration, payload_type, &generics, &type_arguments, environment)
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
