//! Type-check output and shared lowering metadata types.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::hir::{HirExpressionNode, HirProgram};
use crate::resolve::{HirNodeId, ItemId, LocalId, Resolution};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::checker::TypeChecker;
use crate::types::lowering_prep::{CastIntent, LoweringPrep};
use crate::types::surface::UnitTypeSurface;
use crate::types::{TypeId, TypeTable};

/// Type mismatch, missing annotation, invalid operation, or extern-surface violation at a span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    UnknownType {
        span: SpanInfo,
        name: String,
    },
    UnknownValueType {
        span: SpanInfo,
    },
    UnknownStructType {
        span: SpanInfo,
    },
    InvalidMemberTarget {
        span: SpanInfo,
    },
    UnknownEnumType {
        span: SpanInfo,
    },
    UnknownStructField {
        span: SpanInfo,
        name: String,
    },
    UnknownEnumVariant {
        span: SpanInfo,
        name: String,
    },
    MissingStructField {
        span: SpanInfo,
        name: String,
    },
    MissingTypeAnnotation {
        span: SpanInfo,
        name: String,
    },
    TypeMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: TypeId,
    },
    MatchArmTypeMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: TypeId,
    },
    CallArityMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    CallArgumentMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: TypeId,
    },
    EnumConstructorMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    UnknownCallTarget {
        span: SpanInfo,
    },
    InvalidBinaryOp {
        span: SpanInfo,
    },
    InvalidUnaryOp {
        span: SpanInfo,
    },
    NonBoolCondition {
        span: SpanInfo,
    },
    UnsupportedExpression {
        span: SpanInfo,
    },
    InvalidTryTarget {
        span: SpanInfo,
    },
    InvalidEventInvocationScope {
        span: SpanInfo,
    },
    InvalidEventCapacity {
        span: SpanInfo,
    },
    InvalidEventSubscriptionTarget {
        span: SpanInfo,
    },
    SpawnTargetNotFiberCompatible {
        span: SpanInfo,
    },
    JoinWouldDeadlock {
        span: SpanInfo,
    },
    StackReferenceEscapesSpawn {
        span: SpanInfo,
    },
    ReturnTypeMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: Option<TypeId>,
    },
    MissingTypeArguments {
        span: SpanInfo,
    },
    GenericArgumentMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    NonIterableForTarget {
        span: SpanInfo,
    },
    IterableNextArityMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    IterableNextReturnNotOption {
        span: SpanInfo,
    },
    IterableOptionSomeArityMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    // Extern interface validation errors
    ExternInvalidAbi {
        span: SpanInfo,
        abi: Option<String>,
    },
    ExternMissingLibrary {
        span: SpanInfo,
    },
    ExternDisallowedParamType {
        span: SpanInfo,
        method: String,
    },
    ExternDisallowedReturnType {
        span: SpanInfo,
        method: String,
    },
}

fn type_error_span_loc(span: SpanInfo) -> String {
    format!(
        "{}:{}-{}:{}",
        span.line_col_start.0, span.line_col_start.1, span.line_col_end.0, span.line_col_end.1
    )
}

fn type_id_label(id: TypeId) -> String {
    format!("#{}", id.0)
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = |span: SpanInfo| type_error_span_loc(span);
        match self {
            TypeError::UnknownType { span, name } => {
                write!(f, "unknown type `{name}` at {}", at(*span))
            }
            TypeError::UnknownValueType { span } => {
                write!(f, "unknown value type at {}", at(*span))
            }
            TypeError::UnknownStructType { span } => {
                write!(f, "unknown struct type at {}", at(*span))
            }
            TypeError::InvalidMemberTarget { span } => {
                write!(f, "invalid member access target at {}", at(*span))
            }
            TypeError::UnknownEnumType { span } => write!(f, "unknown enum type at {}", at(*span)),
            TypeError::UnknownStructField { span, name } => {
                write!(f, "unknown struct field `{name}` at {}", at(*span))
            }
            TypeError::UnknownEnumVariant { span, name } => {
                write!(f, "unknown enum variant `{name}` at {}", at(*span))
            }
            TypeError::MissingStructField { span, name } => {
                write!(f, "missing struct field `{name}` at {}", at(*span))
            }
            TypeError::MissingTypeAnnotation { span, name } => {
                write!(f, "missing type annotation for `{name}` at {}", at(*span))
            }
            TypeError::TypeMismatch {
                span,
                expected,
                actual,
            } => write!(
                f,
                "type mismatch at {}: expected type {}, found type {}",
                at(*span),
                type_id_label(*expected),
                type_id_label(*actual)
            ),
            TypeError::MatchArmTypeMismatch {
                span,
                expected,
                actual,
            } => write!(
                f,
                "match arm type mismatch at {}: expected type {}, found type {}",
                at(*span),
                type_id_label(*expected),
                type_id_label(*actual)
            ),
            TypeError::CallArityMismatch {
                span,
                expected,
                actual,
            } => write!(
                f,
                "call arity mismatch at {}: expected {expected} arguments, got {actual}",
                at(*span)
            ),
            TypeError::CallArgumentMismatch {
                span,
                expected,
                actual,
            } => write!(
                f,
                "call argument type mismatch at {}: expected type {}, found type {}",
                at(*span),
                type_id_label(*expected),
                type_id_label(*actual)
            ),
            TypeError::EnumConstructorMismatch {
                span,
                expected,
                actual,
            } => write!(
                f,
                "enum constructor arity mismatch at {}: expected {expected} arguments, got {actual}",
                at(*span)
            ),
            TypeError::UnknownCallTarget { span } => {
                write!(f, "unknown call target at {}", at(*span))
            }
            TypeError::InvalidBinaryOp { span } => {
                write!(f, "invalid binary operation at {}", at(*span))
            }
            TypeError::InvalidUnaryOp { span } => {
                write!(f, "invalid unary operation at {}", at(*span))
            }
            TypeError::NonBoolCondition { span } => {
                write!(f, "non-boolean condition at {}", at(*span))
            }
            TypeError::UnsupportedExpression { span } => {
                write!(f, "unsupported expression at {}", at(*span))
            }
            TypeError::InvalidTryTarget { span } => {
                write!(f, "invalid try target at {}", at(*span))
            }
            TypeError::InvalidEventInvocationScope { span } => {
                write!(f, "invalid event invocation scope at {}", at(*span))
            }
            TypeError::InvalidEventCapacity { span } => {
                write!(f, "invalid event capacity at {}", at(*span))
            }
            TypeError::InvalidEventSubscriptionTarget { span } => {
                write!(f, "invalid event subscription target at {}", at(*span))
            }
            TypeError::SpawnTargetNotFiberCompatible { span } => {
                write!(
                    f,
                    "spawn target is not a valid fiber entry at {}",
                    at(*span)
                )
            }
            TypeError::JoinWouldDeadlock { span } => {
                write!(
                    f,
                    "join would deadlock: child fiber cannot join an ancestor handle at {}",
                    at(*span)
                )
            }
            TypeError::StackReferenceEscapesSpawn { span } => {
                write!(
                    f,
                    "stack reference would escape across spawn boundary at {}",
                    at(*span)
                )
            }
            TypeError::ReturnTypeMismatch {
                span,
                expected,
                actual,
            } => match actual {
                Some(a) => write!(
                    f,
                    "return type mismatch at {}: expected type {}, found type {}",
                    at(*span),
                    type_id_label(*expected),
                    type_id_label(*a)
                ),
                None => write!(
                    f,
                    "return type mismatch at {}: expected type {}, found no value",
                    at(*span),
                    type_id_label(*expected)
                ),
            },
            TypeError::MissingTypeArguments { span } => {
                write!(f, "missing type arguments at {}", at(*span))
            }
            TypeError::GenericArgumentMismatch {
                span,
                expected,
                actual,
            } => write!(
                f,
                "generic argument count mismatch at {}: expected {expected}, got {actual}",
                at(*span)
            ),
            TypeError::NonIterableForTarget { span } => {
                write!(f, "non-iterable for-loop target at {}", at(*span))
            }
            TypeError::IterableNextArityMismatch {
                span,
                expected,
                actual,
            } => write!(
                f,
                "iterable `next` arity mismatch at {}: expected {expected} bindings, got {actual}",
                at(*span)
            ),
            TypeError::IterableNextReturnNotOption { span } => write!(
                f,
                "iterable `next` must return an option-like type at {}",
                at(*span)
            ),
            TypeError::IterableOptionSomeArityMismatch {
                span,
                expected,
                actual,
            } => write!(
                f,
                "option `Some` arity mismatch at {}: expected {expected} arguments, got {actual}",
                at(*span)
            ),
            TypeError::ExternInvalidAbi { span, abi } => match abi {
                Some(a) => write!(f, "invalid extern ABI `{a}` at {}", at(*span)),
                None => write!(f, "invalid extern ABI at {}", at(*span)),
            },
            TypeError::ExternMissingLibrary { span } => {
                write!(f, "extern declaration missing library at {}", at(*span))
            }
            TypeError::ExternDisallowedParamType { span, method } => write!(
                f,
                "extern method `{method}` has a disallowed parameter type at {}",
                at(*span)
            ),
            TypeError::ExternDisallowedReturnType { span, method } => write!(
                f,
                "extern method `{method}` has a disallowed return type at {}",
                at(*span)
            ),
        }
    }
}
/// Where a method or contract dispatch receiver came from (expression vs local).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodReceiverSource {
    Expression(SpanInfo),
    Local(LocalId),
}

/// How a call site lowers for backends (free function, method, contract, event, or value call).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallLoweringKind {
    MethodDispatch {
        method_item_id: ItemId,
        receiver_source: MethodReceiverSource,
        receiver_type: TypeId,
    },
    ContractDispatch {
        contract_item_id: ItemId,
        receiver_source: MethodReceiverSource,
        receiver_type: TypeId,
    },
    ItemCall {
        item_id: ItemId,
    },
    EventInvoke {
        receiver_source: MethodReceiverSource,
        receiver_type: TypeId,
    },
    CallableValueCall,
}

/// Parameter and return [`TypeId`]s for a callable item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub params: Vec<TypeId>,
    pub return_type: TypeId,
}

/// Output of type checking: intern table, per-node expression types, signatures, and lowering prep.
#[derive(Debug)]
pub struct TypeResult {
    pub types: TypeTable,
    pub named_type_names: HashMap<ItemId, String>,
    pub node_types: HashMap<HirNodeId, TypeId>,
    pub local_types: HashMap<LocalId, TypeId>,
    pub unit_surfaces: HashMap<PathBuf, Arc<UnitTypeSurface>>,
    pub function_signatures: HashMap<ItemId, FunctionSignature>,
    pub method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: HashMap<ItemId, Vec<String>>,
    pub contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub lowering: LoweringPrep,
}

impl TypeResult {
    pub fn node_type(&self, id: HirNodeId) -> Option<TypeId> {
        self.node_types.get(&id).copied()
    }

    pub fn expr_type(&self, node: &Spanned<HirExpressionNode>) -> Option<TypeId> {
        self.node_type(node.id)
    }

    pub fn cast_intent_for_span(&self, span: SpanInfo) -> Option<&CastIntent> {
        self.lowering
            .cast_intents
            .iter()
            .find(|intent| intent.span == span)
    }

    pub fn cast_intents_for_span(&self, span: SpanInfo) -> impl Iterator<Item = &CastIntent> {
        self.lowering
            .cast_intents
            .iter()
            .filter(move |intent| intent.span == span)
    }

    pub fn cast_intents_for_entry(
        &self,
        entry_source_path: Option<&PathBuf>,
    ) -> impl Iterator<Item = &CastIntent> {
        self.lowering.cast_intents.iter().filter(move |intent| {
            match (&intent.source_path, entry_source_path) {
                (None, None) => true,
                (Some(intent_path), Some(entry_path)) => {
                    crate::paths::same_file(intent_path, entry_path)
                }
                _ => false,
            }
        })
    }

    pub fn call_kind_at(
        &self,
        node_id: HirNodeId,
        _source_path: Option<&PathBuf>,
    ) -> Option<CallLoweringKind> {
        self.lowering.call_kinds.get(&node_id).copied()
    }

    /// Infer generic type arguments for a call from already-known argument types.
    pub fn infer_generic_args_from_call_types(
        &self,
        item_id: ItemId,
        arg_types: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let inference_sigs: std::collections::HashMap<
            ItemId,
            crate::types::inference::FunctionSignature,
        > = self
            .function_signatures
            .iter()
            .map(|(id, sig)| {
                (
                    *id,
                    crate::types::inference::FunctionSignature {
                        params: sig.params.clone(),
                        return_type: sig.return_type,
                    },
                )
            })
            .collect();
        crate::types::inference::infer_generic_args_from_call_types(
            &self.types,
            &self.generic_items,
            &inference_sigs,
            item_id,
            arg_types,
        )
    }
}

/// Type-check `program`; returns `Err` when any [`TypeError`] was recorded.
pub fn type_program(
    program: &mut Spanned<HirProgram>,
    resolution: &Resolution,
) -> Result<TypeResult, Vec<TypeError>> {
    let (result, errors) = type_program_with_errors(program, resolution);
    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

/// Like [`type_program`] but returns partial [`TypeResult`] together with all accumulated errors.
pub fn type_program_with_errors(
    program: &mut Spanned<HirProgram>,
    resolution: &Resolution,
) -> (TypeResult, Vec<TypeError>) {
    TypeChecker::check_entry(
        program,
        resolution,
        &[],
        None,
        None,
        true,
        None,
        None,
    )
}
