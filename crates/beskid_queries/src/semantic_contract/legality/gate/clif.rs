//! `clif { ... }` block obligations (E1232).
//!
//! Mirrors the legacy `type_clif_block`, `type_clif_block_statement`, `check_clif_block_body`,
//! and `check_clif_parameters` (`beskid_analysis` `types/checker/expressions/clif.rs`) with
//! their exact detail texts, at the block expression (the legacy block span):
//!
//! * a block that is directly the expression of an expression statement is checked in
//!   `ClifBlockMode::Statement`; every other block is a value and takes its context's type;
//! * a value block initializing an unannotated `let` has no typed context (the legacy checker
//!   clears the contextual type there);
//! * a value block whose proven context (`node_type`, i.e. `clif_block_context_type`) is a
//!   declared primitive outside the CLIF scalars is rejected; nominal and array contexts share the
//!   `pointer` ABI representation with the `pointer` scalar and are never judged;
//! * the body is parsed with `parse_clif_surface_in` (`clif_block_body`), and every `%N` is held
//!   to the enclosing callable's CLIF parameters (`clif_block_parameters`; unavailable inside a
//!   lambda, whose `%N` the legacy checker rejects outright).
//!
//! Fail-closed in the query direction: a value block is judged only when its context is a
//! surface-declared position (typed `let`, `return`, local assignment, argument of a direct
//! call) and proven; a parameter is judged only when its declared type proves the legacy
//! classification (primitives and primitive arrays; nominal and generic parameters are not).

use super::super::typing::{enclosing_callable, nodes_of_kind};
use super::*;
use beskid_analysis::clif_surface::{
    CLIF_PAYLOAD_ELEMENT_TYPES, ClifBlockMode, ClifBlockSurface, parse_clif_surface_in,
};
use beskid_analysis::syntax::{
    AstNodeId, FunctionDefinition, LetStatement, MethodDefinition, Parameter, PrimitiveType, Spanned, Type,
};

/// Legacy `type_clif_block` detail when `contextual_expected_type` is absent.
const NEEDS_TYPED_CONTEXT: &str =
    "a clif block needs a typed context: use it as a return value, a typed `let` initializer, or a call argument";
/// Legacy `type_clif_block` detail when the context is not a CLIF scalar.
const NOT_SCALAR_CONTEXT: &str = "a clif block must produce a scalar value (bool, char, integer, f64, word, or pointer)";
/// Legacy `check_clif_parameters` detail inside a lambda.
const LAMBDA_PARAMETERS: &str = "clif blocks inside lambdas cannot name parameters with `%N`";

pub(super) fn collect_clif_obligations(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::ClifBlockExpression) {
        let site = AstNodeKey { node, ..key };
        let mut report = |detail: String| {
            findings.push(GateObligation { kind: GateObligationKind::InvalidClifBlock { detail: Arc::from(detail) }, site });
        };
        let mode = match block_position(db, program, index, site) {
            BlockPosition::Statement => ClifBlockMode::Statement,
            BlockPosition::Untyped => {
                report(NEEDS_TYPED_CONTEXT.to_string());
                continue;
            }
            BlockPosition::Value => match node_type(db, site) {
                Ok(Some(context)) if context == SemanticTypeId::POINTER => continue,
                Ok(Some(context)) if clif_scalar(context) => ClifBlockMode::Value,
                Ok(Some(_)) => {
                    report(NOT_SCALAR_CONTEXT.to_string());
                    continue;
                }
                Ok(None) | Err(_) => continue,
            },
            BlockPosition::Unproven => continue,
        };
        let Ok(Some(body)) = clif_block_body(db, site) else { continue };
        let surface = match parse_clif_surface_in(&body, mode) {
            Ok(surface) => surface,
            Err(error) => {
                report(error.to_string());
                continue;
            }
        };
        for detail in parameter_violations(db, program, index, site, &surface) {
            report(detail);
        }
    }
}

/// Where a block sits, as the legacy checker types it.
enum BlockPosition {
    /// Directly the expression of an expression statement (`type_clif_block_statement`).
    Statement,
    /// The initializer of an unannotated `let` (`infer_local_type_from_expression` clears the
    /// contextual type).
    Untyped,
    /// A value whose context is a surface-declared position the legacy checker types it under.
    Value,
    /// Any other position (operands, nested arguments of non-direct calls, lambda bodies, ...).
    Unproven,
}

fn block_position(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
) -> BlockPosition {
    // `Statement::Expression` matches `Expression::ClifBlock` directly: only the `Expression`
    // wrapper may sit between the statement and the block.
    let mut child = site.node;
    let Some(mut parent) = parent_node(index, child) else { return BlockPosition::Unproven };
    while index.kind(parent) == Some(NodeKind::Expression) {
        child = parent;
        let Some(next) = parent_node(index, child) else { return BlockPosition::Unproven };
        parent = next;
    }
    if index.kind(parent) == Some(NodeKind::ExpressionStatement) {
        return BlockPosition::Statement;
    }
    while index.kind(parent) == Some(NodeKind::GroupedExpression) || index.kind(parent) == Some(NodeKind::Expression) {
        child = parent;
        let Some(next) = parent_node(index, child) else { return BlockPosition::Unproven };
        parent = next;
    }
    match index.kind(parent) {
        Some(NodeKind::LetStatement) => {
            let Some(binding) = index.node_at(program, parent).and_then(|node| node.of::<LetStatement>()) else {
                return BlockPosition::Unproven;
            };
            if binding.type_annotation.is_some() { BlockPosition::Value } else { BlockPosition::Untyped }
        }
        Some(NodeKind::ReturnStatement | NodeKind::AssignExpression) => BlockPosition::Value,
        Some(NodeKind::CallExpression) => {
            if matches!(call_lowering(db, AstNodeKey { node: parent, ..site }), Ok(Some(CallLowering::Direct(_)))) {
                BlockPosition::Value
            } else {
                BlockPosition::Unproven
            }
        }
        _ => BlockPosition::Unproven,
    }
}

/// The legacy `is_clif_scalar` primitives.
fn clif_scalar(ty: SemanticTypeId) -> bool {
    matches!(
        ty,
        SemanticTypeId::BOOL
            | SemanticTypeId::CHAR
            | SemanticTypeId::I32
            | SemanticTypeId::I64
            | SemanticTypeId::U32
            | SemanticTypeId::U8
            | SemanticTypeId::F64
            | SemanticTypeId::WORD
            | SemanticTypeId::POINTER
    )
}

fn primitive_clif_scalar(primitive: PrimitiveType) -> bool {
    matches!(
        primitive,
        PrimitiveType::Bool
            | PrimitiveType::Char
            | PrimitiveType::I32
            | PrimitiveType::I64
            | PrimitiveType::U32
            | PrimitiveType::U8
            | PrimitiveType::F64
            | PrimitiveType::Word
            | PrimitiveType::Pointer
    )
}

/// One legacy `current_clif_parameters` entry, classified from its declared type.
enum LegacyClifParameter {
    /// The method receiver (`None` in the legacy list).
    Receiver,
    /// A declared primitive.
    Primitive(PrimitiveType),
    /// A `u8[]`, `u32[]`, or `i64[]` array (`clif_payload_element` is `Some`).
    PayloadArray,
    /// An array of another primitive element.
    OtherArray,
    /// A nominal, generic, or otherwise unproven type: never judged.
    Unproven,
}

fn legacy_parameter(parameter: &Spanned<Parameter>) -> Option<LegacyClifParameter> {
    Some(match &parameter.node.ty.node {
        Type::Primitive(primitive) if primitive.node == PrimitiveType::Unit => return None,
        Type::Primitive(primitive) => LegacyClifParameter::Primitive(primitive.node),
        Type::Array(element) => match &element.node {
            Type::Primitive(primitive) => {
                if CLIF_PAYLOAD_ELEMENT_TYPES.iter().any(|(name, _)| *name == primitive_name(primitive.node)) {
                    LegacyClifParameter::PayloadArray
                } else {
                    LegacyClifParameter::OtherArray
                }
            }
            _ => LegacyClifParameter::Unproven,
        },
        _ => LegacyClifParameter::Unproven,
    })
}

fn primitive_name(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::U8 => "u8",
        PrimitiveType::U32 => "u32",
        PrimitiveType::I64 => "i64",
        _ => "",
    }
}

/// The legacy `check_clif_parameters` findings of one parsed block, in legacy order.
fn parameter_violations(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    surface: &ClifBlockSurface,
) -> Vec<String> {
    let mut violations = Vec::new();
    let Some(callable) = enclosing_callable(index, site.node) else { return violations };
    if index.kind(callable) == Some(NodeKind::LambdaExpression) {
        if !surface.referenced_parameters().is_empty() {
            violations.push(LAMBDA_PARAMETERS.to_string());
        }
        return violations;
    }
    let Ok(Some(shapes)) = clif_block_parameters(db, site) else { return violations };
    let Some(parameters) = legacy_parameters(program, index, callable) else { return violations };
    if parameters.len() != shapes.len() {
        return violations;
    }
    for index in surface.referenced_parameters() {
        if index >= parameters.len() {
            violations.push(format!(
                "`%{index}` does not name a parameter; the enclosing function has {} CLIF parameter(s)",
                parameters.len()
            ));
        }
    }
    let receiver = |index: usize| format!("`%{index}` is a method receiver or has an unresolved type");
    for index in surface.value_parameters() {
        match parameters.get(index) {
            Some(LegacyClifParameter::Receiver) => violations.push(receiver(index)),
            Some(LegacyClifParameter::Primitive(primitive)) if primitive_clif_scalar(*primitive) => {}
            Some(
                LegacyClifParameter::Primitive(_) | LegacyClifParameter::PayloadArray | LegacyClifParameter::OtherArray,
            ) => violations.push(format!(
                "`%{index}` is not a scalar parameter; read arrays through `payload %{index}` and `length %{index}`"
            )),
            Some(LegacyClifParameter::Unproven) | None => {}
        }
    }
    for index in surface.array_parameters() {
        match parameters.get(index) {
            Some(LegacyClifParameter::Receiver) => violations.push(receiver(index)),
            Some(LegacyClifParameter::Primitive(_) | LegacyClifParameter::OtherArray) => {
                let allowed = CLIF_PAYLOAD_ELEMENT_TYPES.iter().map(|(name, _)| format!("{name}[]")).collect::<Vec<_>>();
                violations.push(format!("`payload`/`length` require a parameter of type {}", allowed.join(", ")));
            }
            Some(LegacyClifParameter::PayloadArray | LegacyClifParameter::Unproven) | None => {}
        }
    }
    violations
}

/// The legacy `current_clif_parameters` of a function (declared parameters, unit omitted), a
/// method (the receiver first), or a test (none).
fn legacy_parameters(
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    callable: AstNodeId,
) -> Option<Vec<LegacyClifParameter>> {
    let node = index.node_at(program, callable)?;
    if let Some(function) = node.of::<FunctionDefinition>() {
        return Some(function.parameters.iter().filter_map(legacy_parameter).collect());
    }
    if let Some(method) = node.of::<MethodDefinition>() {
        return Some(
            std::iter::once(LegacyClifParameter::Receiver)
                .chain(method.parameters.iter().filter_map(legacy_parameter))
                .collect(),
        );
    }
    (index.kind(callable) == Some(NodeKind::TestDefinition)).then(Vec::new)
}
