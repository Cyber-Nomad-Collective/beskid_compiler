//! Focused ABI semantic implementation.

use super::super::*;
use super::statement_abi_type_for_node;

/// Prove an ABI representation for one bare integer literal or integer constant from its
/// [`expected_type`].
///
/// This is contextual typing for literals, not a conversion: the source literal has no ABI
/// suffix and is emitted directly at the expected exact ABI width, in whichever context supplies
/// it (an annotated local, an assignment, a callee parameter, an enum payload, a struct field, a
/// return, an array element, a match arm, a block result, or a binary operand's peer). Nodes
/// that are not bare integer values have no fact. Explicit literal suffixes, contexts that prove
/// no primitive integer type, and out-of-range values fail closed rather than receiving an
/// implicit numeric widening.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn contextual_integer_literal_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |_program, _index, _node| {
        let bare_integer = (|| Ok::<bool, SemanticError>(
            unsuffixed_integer_literal(db, key)? || contextual_constant_integer(db, key)?.is_some(),
        ))();
        match bare_integer {
            Ok(true) => {}
            // An explicitly suffixed literal already fixed its width; a contextual width is a gap.
            Ok(false) if matches!(suffixed_integer_literal(db, key), Ok(true)) => {
                return Some(Err(SemanticError::unavailable("contextual_integer_literal_abi_type")));
            }
            Ok(false) => return None,
            Err(error) => return Some(Err(error)),
        }
        Some((|| {
            let unavailable = || SemanticError::unavailable("contextual_integer_literal_abi_type");
            let expected = expected_type(db, key)?.ok_or_else(unavailable)?.abi_type();
            (primitive_integer(expected) && contextual_integer_operand_fits_abi(db, key, expected)?)
                .then_some(expected)
                .ok_or_else(unavailable)
        })())
    })?
    .transpose()
}

/// The ABI of `!operand` or `-operand` from its operand's ABI fact: `!` requires and keeps
/// `bool`; `-` keeps a signed integer or float operand.
fn unary_operand_abi_type(db: &dyn Db, key: AstNodeKey) -> Result<SemanticTypeId, SemanticError> {
    let unavailable = || SemanticError::unavailable("abi_type");
    let children = child_nodes(db, key)?.ok_or_else(unavailable)?;
    let [_, operand] = children.as_ref() else { return Err(unavailable()) };
    let operand = abi_type(db, *operand)?.ok_or_else(unavailable)?;
    match operator_fact(db, key)? {
        Some(OperatorFact::Not) if operand == SemanticTypeId::BOOL => Ok(operand),
        Some(OperatorFact::Neg)
            if matches!(
                operand,
                SemanticTypeId::I8
                    | SemanticTypeId::I16
                    | SemanticTypeId::I32
                    | SemanticTypeId::I64
                    | SemanticTypeId::F32
                    | SemanticTypeId::F64
            ) =>
        {
            Ok(operand)
        }
        _ => Err(unavailable()),
    }
}

/// Whether `key` is an integer literal with an explicit ABI suffix, through singleton wrappers
/// and unary negation.
fn suffixed_integer_literal(db: &dyn Db, key: AstNodeKey) -> Result<bool, SemanticError> {
    if let Some(literal) = literal_fact(db, key)? {
        return Ok(matches!(literal, LiteralFact::Integer(text) if integer_has_explicit_abi_suffix(&text)));
    }
    let Some(children) = child_nodes(db, key)? else {
        return Ok(false);
    };
    match children.as_ref() {
        [child] => suffixed_integer_literal(db, *child),
        [_, operand]
            if node_kind(db, key)? == Some(IndexedNodeKind::UnaryExpression)
                && operator_fact(db, key)? == Some(OperatorFact::Neg) =>
        {
            suffixed_integer_literal(db, *operand)
        }
        _ => Ok(false),
    }
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        if node.of::<beskid_analysis::syntax::TryExpression>().is_some() {
            return Some(try_expression_fact(db, key).and_then(|fact| {
                fact.map(|fact| fact.payload_type).ok_or_else(|| SemanticError::unavailable("abi_type"))
            }));
        }
        if let Some(projection) = super::super::layouts::nominal_field_projection(db, key) {
            return Some(projection.map(|(_, identity)| identity.abi_type()));
        }
        if node.of::<beskid_analysis::syntax::MemberExpression>().is_some() {
            // `call().field`: the field access fact names the projected field's shape.
            return Some(aggregate_field_access(db, key).and_then(|access| {
                access.and_then(|access| aggregate_field_access_abi_type(&access)).ok_or_else(|| SemanticError::unavailable("abi_type"))
            }));
        }
        if node.of::<beskid_analysis::syntax::ArrayLiteralExpression>().is_some() {
            // Array values have one managed-pointer representation, independently of the
            // element representation validated by array construction/lowering.
            return Some(Ok(SemanticTypeId::POINTER));
        }
        if node.of::<beskid_analysis::syntax::StructLiteralExpression>().is_some() {
            // A struct literal is a managed nominal reference once its written path proves a
            // nominal source identity; an unresolved or non-nominal path stays unavailable.
            return Some(generic_source_expression_identity(db, key).and_then(|identity| match identity {
                GenericSourceTypeIdentity::Nominal { .. } => Ok(SemanticTypeId::POINTER),
                _ => Err(SemanticError::unavailable("abi_type")),
            }));
        }
        if node.of::<beskid_analysis::syntax::LambdaExpression>().is_some() {
            // A lambda value is its managed closure record, whatever its callable signature.
            return Some(Ok(SemanticTypeId::POINTER));
        }
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && let Ok(Some(_)) = function_item_value(db, key)
        {
            // A named function read as a value is its managed closure record.
            return Some(Ok(SemanticTypeId::POINTER));
        }
        if node.of::<beskid_analysis::syntax::SpawnExpression>().is_some() {
            return Some(spawn_handle_type(db, key).and_then(|fact| {
                fact.map(|_| SemanticTypeId::POINTER).ok_or_else(|| SemanticError::unavailable("abi_type"))
            }));
        }
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return Some(abi_type_for_binary_expression(db, program, index, key, binary));
        }
        if node.of::<beskid_analysis::syntax::UnaryExpression>().is_some() {
            // Unary semantic typing already validates the operator and preserves the
            // operand type; expose that same authority at ABI conversion boundaries. An operand
            // node typing does not cover (`!this.state.input.failed`, a field read) has the ABI
            // its own fact proves, which `!` and `-` preserve.
            return Some(match node_type(db, key) {
                Ok(Some(ty)) => Ok(ty),
                Ok(None) => unary_operand_abi_type(db, key),
                Err(error) if error.is_unavailable() => unary_operand_abi_type(db, key),
                Err(error) => Err(error),
            });
        }
        if let Some(expression) = node.of::<beskid_analysis::syntax::Expression>() {
            return Some(abi_type_for_expression(db, program, index, key, expression));
        }
        if node.of::<beskid_analysis::syntax::GroupedExpression>().is_some() {
            let inner = normalized_expression_node(index, key.node);
            return Some(
                (inner != key.node)
                    .then_some(AstNodeKey { node: inner, ..key })
                    .ok_or_else(|| SemanticError::unavailable("abi_type"))
                    .and_then(|inner| abi_type(db, inner)?.ok_or_else(|| SemanticError::unavailable("abi_type"))),
            );
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
            return Some(Ok(semantic_type_for_literal(literal)));
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::LiteralExpression>() {
            return Some(Ok(semantic_type_for_literal(&literal.literal.node)));
        }
        if let Some(statement) = node.of::<beskid_analysis::syntax::LetStatement>() {
            return Some(
                statement
                    .type_annotation
                    .as_ref()
                    .ok_or_else(|| SemanticError::unavailable("abi_type"))
                    .and_then(|ty| abi_type_from_syntax(db, key, &ty.node)),
            );
        }
        if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
            return Some(abi_type_for_local_path(db, program, index, key, &path.path.node));
        }
        if let Some(indexed) = node.of::<beskid_analysis::syntax::IndexExpression>()
            && string_index_target(db, program, index, key, indexed)
        {
            // A string index reads one UTF-8 byte, wherever the string comes from.
            return Some(Ok(SemanticTypeId::U8));
        }
        if node.of::<beskid_analysis::syntax::AssignExpression>().is_some()
            || node.of::<beskid_analysis::syntax::IndexExpression>().is_some()
        {
            // An index assignment is expression-valued only after the same declared-array fact
            // that authorizes its element store proves the destination representation. Other
            // assignment shapes intentionally retain no syntax ABI fact here.
            return match array_index_element_abi_type(db, key) {
                Ok(Some(element)) => Some(Ok(element)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            };
        }
        if node.of::<beskid_analysis::syntax::Identifier>().is_some() {
            return abi_local_declaration_type(db, program, index, key, key.node);
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some() {
            match primitive_numeric_conversion(db, key) {
                Ok(Some(conversion)) => return Some(Ok(conversion.to)),
                Ok(None) => (),
                Err(error) => return Some(Err(error)),
            }
            let lowering = match call_lowering(db, key) {
                Ok(Some(lowering)) => lowering,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            if !matches!(
                lowering,
                CallLowering::Direct(_)
                    | CallLowering::FunctionValue(_)
                    | CallLowering::Runtime(_)
                    | CallLowering::ManifestBuiltin(_)
                    | CallLowering::NativeModCallback(_)
                    | CallLowering::CorelibService(_)
            ) {
                return Some(Err(SemanticError::unavailable("abi_type")));
            }
            let signature = match call_abi_signature(db, key) {
                Ok(Some(signature)) => signature,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            return Some(Ok(signature.result));
        }
        if let Some(syntax_type) = node.of::<beskid_analysis::syntax::Type>() {
            return Some(abi_type_from_syntax(db, key, syntax_type));
        }
        if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
            return Some(Ok(semantic_type_for_literal(literal)));
        }
        None
    })?
    .transpose()
}

/// One generation-bound ABI representation fact for source values and declared storage
/// boundaries. This composes only existing syntax facts; it never reconstructs HIR types.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn value_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        let normalized = normalized_expression_node(index, key.node);
        if normalized != key.node {
            return Some(
                value_abi_type(db, AstNodeKey { node: normalized, ..key })
                    .and_then(|value| value.ok_or_else(|| SemanticError::unavailable("value_abi_type"))),
            );
        }

        if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
            && event_operation_tracked(db, syntax, key)
                .ok()
                .flatten()
                .is_some_and(|fact| fact.operation == EventOperationKind::Raise)
        {
            return Some(Ok(SemanticTypeId::UNIT));
        }
        if node.of::<beskid_analysis::syntax::Identifier>().is_some()
            && event_handler_lambda_for_local_tracked(db, syntax, key).ok().flatten().is_some()
        {
            return Some(Ok(SemanticTypeId::POINTER));
        }
        if let Some(statement) = statement_abi_type_for_node(db, syntax, program, index, key, node) {
            return Some(statement);
        }
        Some((|| {
            // Floating literals use the exact contextual width; decoding into F64 first would
            // introduce double rounding before an F32 boundary.
            if matches!(literal_fact(db, key)?, Some(LiteralFact::Float(_)))
                && let Some(intents) = optional_abi_fact(cast_intents(db, key))?
                && let Some(intent) = intents.first()
                && matches!(intent.to, SemanticTypeId::F32 | SemanticTypeId::F64)
            {
                return Ok(intent.to);
            }
            let contextual = optional_abi_fact(contextual_integer_literal_abi_type(db, key))?;
            let call_result = optional_abi_fact(call_abi_signature(db, key))?.map(|signature| signature.result);
            let enum_value = optional_abi_fact(enum_constructor(db, key))?.map(|_| SemanticTypeId::POINTER);
            let abi = optional_abi_fact(abi_type(db, key))?;
            let semantic = optional_abi_fact(node_type(db, key))?;
            contextual
                .or(call_result)
                .or(enum_value)
                .or(abi)
                .or(semantic)
                .ok_or_else(|| SemanticError::unavailable("value_abi_type"))
        })())
    })?
    .transpose()
}

fn optional_abi_fact<T>(result: SemanticQueryResult<T>) -> Result<Option<T>, SemanticError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) if error.is_unavailable() => Ok(None),
        Err(error) => Err(error),
    }
}

pub(in crate::semantic_contract) fn abi_type_for_expression(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::Expression;

    match expression {
        Expression::Spawn(_)
        | Expression::ArrayLiteral(_)
        | Expression::StructLiteral(_)
        | Expression::Unary(_)
        | Expression::Lambda(_)
        | Expression::Try(_) => {
            let normalized = normalized_expression_node(index, key.node);
            abi_type(db, AstNodeKey { node: normalized, ..key })?.ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        Expression::Literal(literal) => Ok(semantic_type_for_literal(&literal.node.literal.node)),
        // Field facts are keyed by the path expression itself, not by its expression wrapper
        // (the operand key a binary expression hands out): a bare receiver field inside a
        // method resolves only through the path node.
        Expression::Path(_) => {
            let path_key = AstNodeKey { node: normalized_expression_node(index, key.node), ..key };
            if index.kind(path_key.node) != Some(beskid_analysis::syntax_query::NodeKind::PathExpression) {
                return Err(SemanticError::unavailable("abi_type"));
            }
            abi_type(db, path_key)?.ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        Expression::Member(_) => {
            let member_key = AstNodeKey { node: normalized_expression_node(index, key.node), ..key };
            if index.kind(member_key.node) != Some(beskid_analysis::syntax_query::NodeKind::MemberExpression) {
                return Err(SemanticError::unavailable("abi_type"));
            }
            abi_type(db, member_key)?.ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        Expression::Grouped(_) => {
            let inner = normalized_expression_node(index, key.node);
            if inner == key.node {
                return Err(SemanticError::unavailable("abi_type"));
            }
            abi_type(db, AstNodeKey { node: inner, ..key })?.ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        Expression::Index(_) => {
            let indexed = normalized_expression_node(index, key.node);
            array_index_element_abi_type(db, AstNodeKey { node: indexed, ..key })?
                .ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        Expression::Call(call) => {
            let call = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(call))
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            // Preserve the normalized call's conversion authority as well as ordinary
            // callable signatures; primitive conversions are not callable declarations.
            abi_type(db, call)?.ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        Expression::Binary(binary) => {
            let binary_key = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(binary))
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            abi_type(db, binary_key)?.ok_or_else(|| SemanticError::unavailable("abi_type"))
        }
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

/// ABI representation of the field a resolved access reads: scalars and managed references keep
/// their own representation, a nominal field is a managed pointer.
pub(in crate::semantic_contract) fn aggregate_field_access_abi_type(access: &AggregateFieldAccess) -> Option<SemanticTypeId> {
    access.layout.fields.get(usize::try_from(access.index).ok()?).map(|(_, shape)| match shape {
        AggregateFieldShape::Scalar(semantic) | AggregateFieldShape::ManagedReference(semantic) => *semantic,
        AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
    })
}

pub(in crate::semantic_contract) fn abi_type_for_binary_expression(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    binary: &beskid_analysis::syntax::BinaryExpression,
) -> Result<SemanticTypeId, SemanticError> {
    let left = index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(binary.left.as_ref()))
        .map(|node| AstNodeKey { node, ..key })
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let right = index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(binary.right.as_ref()))
        .map(|node| AstNodeKey { node, ..key })
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let left_type = value_abi_type(db, left)?.ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let right_type = value_abi_type(db, right)?.ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    if left_type != right_type {
        if integer_literal_text(db, left)?.is_some() && primitive_integer(right_type) {
            integer_literal_fits_abi(db, left, right_type)?
                .then_some(())
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            return binary_result_type(binary.op.node, right_type);
        }
        if integer_literal_text(db, right)?.is_some() && primitive_integer(left_type) {
            integer_literal_fits_abi(db, right, left_type)?
                .then_some(())
                .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            return binary_result_type(binary.op.node, left_type);
        }
    }
    use beskid_analysis::syntax::BinaryOp;
    match binary.op.node {
        BinaryOp::Add if left_type == SemanticTypeId::STRING || right_type == SemanticTypeId::STRING => {
            Ok(SemanticTypeId::STRING)
        }
        BinaryOp::Eq | BinaryOp::NotEq
            if left_type == SemanticTypeId::STRING && right_type == SemanticTypeId::STRING =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Or | BinaryOp::And if left_type == SemanticTypeId::BOOL && right_type == SemanticTypeId::BOOL => {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::IdentityEq
        | BinaryOp::IdentityNotEq
        | BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Lte
        | BinaryOp::Gt
        | BinaryOp::Gte
            if left_type == right_type =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod
            if left_type == right_type && primitive_numeric(left_type) =>
        {
            Ok(left_type)
        }
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::Shl | BinaryOp::Shr
            if left_type == right_type && primitive_integer(left_type) =>
        {
            Ok(left_type)
        }
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

fn binary_result_type(
    operator: beskid_analysis::syntax::BinaryOp,
    operand: SemanticTypeId,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::BinaryOp;

    match operator {
        BinaryOp::IdentityEq
        | BinaryOp::IdentityNotEq
        | BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Lte
        | BinaryOp::Gt
        | BinaryOp::Gte => Ok(SemanticTypeId::BOOL),
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod if primitive_numeric(operand) => {
            Ok(operand)
        }
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::Shl | BinaryOp::Shr if primitive_integer(operand) => Ok(operand),
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

pub(in crate::semantic_contract) fn abi_type_from_syntax(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::Type;

    match syntax_type {
        Type::Primitive(_) => semantic_type_from_syntax(syntax_type),
        Type::Complex(path) => nominal_aggregate_abi_type(db, key, &path.node),
        Type::Associated { .. } => Err(SemanticError::unavailable("abi_type")),
        Type::This => method_this_type(db, key)
            .ok_or_else(|| SemanticError::unavailable("abi_type"))
            .and_then(|receiver| abi_type_from_syntax(db, key, &receiver)),
        Type::Array(_) => Ok(SemanticTypeId::POINTER),
        // A function-typed value is one managed closure handle, whatever its signature: that is
        // what a lambda expression, an event handler field, and a captured callable already lower
        // to, so a `unit(ConsoleSize) handler` parameter has the same representation.
        Type::Function { .. } => Ok(SemanticTypeId::POINTER),
    }
}

/// Whether the target of the index expression `key` is a `string`: a local, a parameter, a field
/// at any depth, or any other expression whose source identity is `string`.
pub(in crate::semantic_contract) fn string_index_target(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    indexed: &beskid_analysis::syntax::IndexExpression,
) -> bool {
    index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(indexed.target.as_ref()))
        .map(|target| AstNodeKey { node: normalized_expression_node(index, target), ..key })
        .is_some_and(|target| {
            matches!(
                generic_source_expression_identity(db, target),
                Ok(GenericSourceTypeIdentity::Abi(SemanticTypeId::STRING))
            )
        })
}
