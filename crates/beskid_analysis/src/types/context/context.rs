//! Type checking against [`Resolution`](crate::resolve::Resolution): expression types, signatures, casts, and errors.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::builtins::{BuiltinType, builtin_specs};
use crate::hir::{HirContractNode, HirItem, HirPrimitiveType, HirProgram};
use crate::resolve::{ItemId, ItemKind, LocalId, Resolution, ResolvedType};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::{TypeId, TypeInfo, TypeTable};

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

/// Output of type checking: intern table, per-span expression types, signatures, and cast intents.
#[derive(Debug)]
pub struct TypeResult {
    pub types: TypeTable,
    pub named_type_names: HashMap<ItemId, String>,
    pub expr_types: HashMap<SpanInfo, TypeId>,
    pub scoped_expr_types: HashMap<std::path::PathBuf, HashMap<SpanInfo, TypeId>>,
    pub local_types: HashMap<LocalId, TypeId>,
    pub function_signatures: HashMap<ItemId, FunctionSignature>,
    /// Method dispatch signatures for free functions with a leading `self` parameter (params exclude `self`).
    pub method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: HashMap<ItemId, Vec<String>>,
    pub call_kinds: HashMap<SpanInfo, CallLoweringKind>,
    pub scoped_call_kinds: HashMap<std::path::PathBuf, HashMap<SpanInfo, CallLoweringKind>>,
    pub contract_method_order: HashMap<ItemId, Vec<String>>,
    pub contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    // Canonical output contract for safe implicit numeric conversions.
    // Invariants (normalized in `TypeContext::type_program`):
    // - sorted by (span.start, span.end, from, to)
    // - exact duplicates removed
    // - conflicting reverse intents for the same span are rejected upstream
    pub cast_intents: Vec<CastIntent>,
}

impl TypeResult {
    pub fn cast_intent_for_span(&self, span: SpanInfo) -> Option<&CastIntent> {
        self.cast_intents.iter().find(|intent| intent.span == span)
    }

    pub fn cast_intents_for_span(&self, span: SpanInfo) -> impl Iterator<Item = &CastIntent> {
        self.cast_intents
            .iter()
            .filter(move |intent| intent.span == span)
    }

    pub fn cast_intents_for_entry(
        &self,
        entry_source_path: Option<&std::path::PathBuf>,
    ) -> impl Iterator<Item = &CastIntent> {
        self.cast_intents
            .iter()
            .filter(move |intent| cast_intent_belongs_to_entry(intent, entry_source_path))
    }

    pub fn expr_type_at(
        &self,
        span: SpanInfo,
        source_path: Option<&std::path::PathBuf>,
    ) -> Option<TypeId> {
        if let Some(path) = source_path {
            for (scoped_path, types) in &self.scoped_expr_types {
                if crate::paths::same_file(scoped_path, path) {
                    if let Some(type_id) = types.get(&span) {
                        return Some(*type_id);
                    }
                    if let Some((_, type_id)) =
                        types.iter().find(|(stored, _)| stored.start == span.start)
                    {
                        return Some(*type_id);
                    }
                }
            }
        }

        let mut candidate = None;
        for types in self.scoped_expr_types.values() {
            let Some(type_id) = types.get(&span).copied().or_else(|| {
                types
                    .iter()
                    .find(|(stored, _)| stored.start == span.start)
                    .map(|(_, type_id)| *type_id)
            }) else {
                continue;
            };
            if candidate.is_some() {
                candidate = None;
                break;
            }
            candidate = Some(type_id);
        }
        candidate.or_else(|| {
            self.expr_types.get(&span).copied().or_else(|| {
                self.expr_types
                    .iter()
                    .find(|(stored, _)| stored.start == span.start)
                    .map(|(_, type_id)| *type_id)
            })
        })
    }

    pub fn call_kind_at(
        &self,
        span: SpanInfo,
        source_path: Option<&std::path::PathBuf>,
    ) -> Option<CallLoweringKind> {
        if let Some(path) = source_path {
            for (scoped_path, kinds) in &self.scoped_call_kinds {
                if crate::paths::same_file(scoped_path, path) {
                    if let Some(kind) = kinds.get(&span) {
                        return Some(*kind);
                    }
                    if let Some((_, kind)) =
                        kinds.iter().find(|(stored, _)| stored.start == span.start)
                    {
                        return Some(*kind);
                    }
                }
            }
        }

        let mut candidate = None;
        for kinds in self.scoped_call_kinds.values() {
            let Some(kind) = kinds.get(&span).copied().or_else(|| {
                kinds
                    .iter()
                    .find(|(stored, _)| stored.start == span.start)
                    .map(|(_, kind)| *kind)
            }) else {
                continue;
            };
            if candidate.is_some() {
                candidate = None;
                break;
            }
            candidate = Some(kind);
        }
        candidate.or_else(|| {
            self.call_kinds.get(&span).copied().or_else(|| {
                self.call_kinds
                    .iter()
                    .find(|(stored, _)| stored.start == span.start)
                    .map(|(_, kind)| *kind)
            })
        })
    }
}

/// Parameter and return [`TypeId`]s for a callable item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub params: Vec<TypeId>,
    pub return_type: TypeId,
}

/// Recorded numeric or widening conversion the codegen layer may insert at `span`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastIntent {
    pub span: SpanInfo,
    pub from: TypeId,
    pub to: TypeId,
    pub source_path: Option<std::path::PathBuf>,
}

fn cast_intent_belongs_to_entry(
    intent: &CastIntent,
    entry_source_path: Option<&std::path::PathBuf>,
) -> bool {
    match (&intent.source_path, entry_source_path) {
        (None, None) => true,
        (Some(intent_path), Some(entry_path)) => crate::paths::same_file(intent_path, entry_path),
        _ => false,
    }
}

/// Stateful visitor over one [`HirProgram`](crate::hir::HirProgram) and its [`Resolution`].
pub struct TypeContext<'a> {
    pub(super) resolution: &'a Resolution,
    pub(super) type_table: TypeTable,
    pub(super) primitive_types: HashMap<HirPrimitiveType, TypeId>,
    pub(super) named_types: HashMap<ItemId, TypeId>,
    pub(super) struct_fields: HashMap<ItemId, HashMap<String, TypeId>>,
    pub(super) struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub(super) struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub(super) enum_variants: HashMap<ItemId, HashMap<String, Vec<TypeId>>>,
    pub(super) enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub(super) expr_types: HashMap<SpanInfo, TypeId>,
    pub(super) scoped_expr_types: HashMap<std::path::PathBuf, HashMap<SpanInfo, TypeId>>,
    pub(super) current_source_path: Option<std::path::PathBuf>,
    pub(super) contextual_expected_type: Option<TypeId>,
    pub(super) local_types: HashMap<LocalId, TypeId>,
    pub(super) function_signatures: HashMap<ItemId, FunctionSignature>,
    pub(super) method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub(super) cast_intents: Vec<CastIntent>,
    pub(super) errors: Vec<TypeError>,
    pub(super) current_return_type: Option<TypeId>,
    pub(super) generic_params: HashMap<String, TypeId>,
    pub(super) generic_items: HashMap<ItemId, Vec<String>>,
    pub(super) call_kinds: HashMap<SpanInfo, CallLoweringKind>,
    pub(super) scoped_call_kinds: HashMap<std::path::PathBuf, HashMap<SpanInfo, CallLoweringKind>>,
    pub(super) methods_by_receiver: HashMap<(ItemId, String), ItemId>,
    pub(super) contract_method_order: HashMap<ItemId, Vec<String>>,
    pub(super) contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub(super) current_receiver_item_id: Option<ItemId>,
    pub(super) fiber_scope_stack: Vec<usize>,
    pub(super) fiber_scope_parent: HashMap<usize, usize>,
    pub(super) next_fiber_scope: usize,
    pub(super) fiber_handle_scopes: HashMap<SpanInfo, usize>,
    pub(super) fiber_handle_locals: HashMap<crate::resolve::LocalId, usize>,
}

impl<'a> TypeContext<'a> {
    pub fn new(resolution: &'a Resolution) -> Self {
        let mut context = Self {
            resolution,
            type_table: TypeTable::new(),
            primitive_types: HashMap::new(),
            named_types: HashMap::new(),
            struct_fields: HashMap::new(),
            struct_fields_ordered: HashMap::new(),
            struct_event_fields: HashMap::new(),
            enum_variants: HashMap::new(),
            enum_variants_ordered: HashMap::new(),
            expr_types: HashMap::new(),
            scoped_expr_types: HashMap::new(),
            current_source_path: None,
            contextual_expected_type: None,
            local_types: HashMap::new(),
            function_signatures: HashMap::new(),
            method_function_signatures: HashMap::new(),
            cast_intents: Vec::new(),
            errors: Vec::new(),
            current_return_type: None,
            generic_params: HashMap::new(),
            generic_items: HashMap::new(),
            call_kinds: HashMap::new(),
            scoped_call_kinds: HashMap::new(),
            methods_by_receiver: HashMap::new(),
            contract_method_order: HashMap::new(),
            contract_signatures: HashMap::new(),
            current_receiver_item_id: None,
            fiber_scope_stack: vec![0],
            fiber_scope_parent: HashMap::from([(0, 0)]),
            next_fiber_scope: 1,
            fiber_handle_scopes: HashMap::new(),
            fiber_handle_locals: HashMap::new(),
        };
        context.seed_types();
        context.seed_builtin_signatures();
        context
    }

    fn seed_builtin_signatures(&mut self) {
        for (item_id, index) in &self.resolution.builtin_items {
            let Some(spec) = builtin_specs().get(*index) else {
                continue;
            };
            let mut params = Vec::with_capacity(spec.params.len());
            for param in spec.params {
                if let Some(type_id) = self.builtin_surface_type_id(spec, *param, false) {
                    params.push(type_id);
                }
            }
            let return_type = self.builtin_surface_type_id(spec, spec.returns, true);
            let Some(return_type) = return_type else {
                continue;
            };
            self.function_signatures.insert(
                *item_id,
                FunctionSignature {
                    params,
                    return_type,
                },
            );
        }
    }

    pub(super) fn u8_array_type_id(&mut self) -> Option<TypeId> {
        let u8_id = self.primitive_type_id(HirPrimitiveType::U8)?;
        Some(
            self.type_table
                .find_array_of(u8_id)
                .unwrap_or_else(|| self.type_table.intern(TypeInfo::Array(u8_id))),
        )
    }

    fn builtin_surface_type_id(
        &mut self,
        spec: &crate::builtins::BuiltinSpec,
        builtin: BuiltinType,
        is_return: bool,
    ) -> Option<TypeId> {
        if builtin == BuiltinType::Ptr {
            let path = spec.beskid_path;
            if is_return {
                if matches!(
                    path,
                    &["__bytes_from_str"]
                        | &["__syscall_read_bytes"]
                        | &["__bytes_set"]
                        | &["__str_new"]
                        | &["__str_slice"]
                ) {
                    if matches!(path, &["__str_new"] | &["__str_slice"]) {
                        return self.primitive_type_id(HirPrimitiveType::String);
                    }
                    return self.u8_array_type_id();
                }
            }
            if matches!(
                path,
                &["__bytes_copy"]
                    | &["__bytes_get"]
                    | &["__bytes_set"]
                    | &["__bytes_compare"]
                    | &["__syscall_write_bytes"]
            ) {
                return self.u8_array_type_id();
            }
            return self.primitive_type_id(HirPrimitiveType::I64);
        }
        self.builtin_type_id(builtin)
    }

    fn builtin_type_id(&self, builtin: BuiltinType) -> Option<TypeId> {
        match builtin {
            BuiltinType::String => self.primitive_type_id(HirPrimitiveType::String),
            BuiltinType::Unit => self.primitive_type_id(HirPrimitiveType::Unit),
            BuiltinType::Never => self.primitive_type_id(HirPrimitiveType::Never),
            BuiltinType::Usize | BuiltinType::U64 => self.primitive_type_id(HirPrimitiveType::I64),
            BuiltinType::Ptr => self.primitive_type_id(HirPrimitiveType::I64),
        }
    }

    pub fn type_program(self, program: &Spanned<HirProgram>) -> Result<TypeResult, Vec<TypeError>> {
        let (result, errors) = self.type_program_with_errors(program);
        if errors.is_empty() {
            Ok(result)
        } else {
            Err(errors)
        }
    }

    pub fn type_program_with_errors(
        self,
        program: &Spanned<HirProgram>,
    ) -> (TypeResult, Vec<TypeError>) {
        self.type_program_with_errors_and_dependencies(program, &[], None, None, true, None)
    }

    pub fn type_program_with_errors_and_dependencies(
        mut self,
        program: &Spanned<HirProgram>,
        dependency_programs: &[&Spanned<HirProgram>],
        dependency_source_paths: Option<&[std::path::PathBuf]>,
        entry_source_path: Option<std::path::PathBuf>,
        type_dependency_bodies: bool,
        module_index: Option<&crate::projects::assembly::ModuleIndex>,
    ) -> (TypeResult, Vec<TypeError>) {
        if let Some(index) = module_index {
            for path in index.prefetched_paths() {
                self.seed_definitions_from_source_path(path);
            }
        }
        let dependency_errors_before = self.errors.len();
        let dependency_cast_intents_before = self.cast_intents.len();
        for (index, dependency) in dependency_programs.iter().enumerate() {
            self.current_source_path = dependency_source_paths
                .and_then(|paths| paths.get(index))
                .map(|path| crate::paths::unit_path_key(path));
            self.seed_enum_definitions(dependency);
            self.seed_struct_definitions(dependency);
            for item in &dependency.node.items {
                let (span, generics) = match &item.node {
                    HirItem::FunctionDefinition(def) => (item.span, &def.node.generics),
                    HirItem::TypeDefinition(def) => (item.span, &def.node.generics),
                    HirItem::EnumDefinition(def) => (item.span, &def.node.generics),
                    _ => continue,
                };
                if let Some(item_id) = self.item_id_for_span(span) {
                    let names = generics
                        .iter()
                        .map(|generic| generic.node.name.clone())
                        .collect::<Vec<_>>();
                    self.generic_items.insert(item_id, names);
                }
            }
            let errors_before = self.errors.len();
            let cast_intents_before = self.cast_intents.len();
            self.seed_contract_signatures(dependency);
            self.errors.truncate(errors_before);
            self.cast_intents.truncate(cast_intents_before);
            self.register_foreign_function_signatures(dependency);
            // Dependency units are prefetched for symbols only; incomplete generic surfaces
            // (for example `Core.Results.Result<,>`) must not block entry/test typing.
            self.errors.truncate(errors_before);
            self.cast_intents.truncate(cast_intents_before);
            for item in &dependency.node.items {
                if let HirItem::ExtendTypeDefinition(def) = &item.node {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                if let HirItem::TypeDefinition(def) = &item.node {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
            }
        }
        if type_dependency_bodies {
            for (index, dependency) in dependency_programs.iter().enumerate() {
                self.current_source_path = dependency_source_paths
                    .and_then(|paths| paths.get(index))
                    .map(|path| crate::paths::unit_path_key(path));
                self.type_dependency_function_items(&dependency.node.items);
            }
            if let Some(index) = module_index {
                let assembled: HashSet<std::path::PathBuf> = dependency_source_paths
                    .map(|paths| paths.iter().map(|path| crate::paths::unit_path_key(path)).collect())
                    .unwrap_or_default();
                for path in index.prefetched_paths() {
                    let key = crate::paths::unit_path_key(path);
                    if assembled.contains(&key) {
                        continue;
                    }
                    self.type_prefetched_source_path(path);
                }
            }
        }
        self.errors.truncate(dependency_errors_before);
        self.cast_intents.truncate(dependency_cast_intents_before);
        self.current_source_path = entry_source_path
            .as_ref()
            .map(|path| crate::paths::unit_path_key(path));
        self.seed_struct_definitions(program);
        for item in &program.node.items {
            let (span, generics) = match &item.node {
                HirItem::FunctionDefinition(def) => (item.span, &def.node.generics),
                HirItem::TypeDefinition(def) => (item.span, &def.node.generics),
                HirItem::EnumDefinition(def) => (item.span, &def.node.generics),
                _ => continue,
            };
            if let Some(item_id) = self.item_id_for_span(span) {
                let names = generics
                    .iter()
                    .map(|generic| generic.node.name.clone())
                    .collect::<Vec<_>>();
                self.generic_items.insert(item_id, names);
            }
        }
        self.seed_contract_signatures(program);
        for item in &program.node.items {
            match &item.node {
                HirItem::MethodDefinition(def) => {
                    self.seed_method_receiver(item.span, def);
                }
                HirItem::ExtendTypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                HirItem::TypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                _ => {}
            }
        }
        for item in &program.node.items {
            self.type_item(item);
        }
        self.cast_intents.sort_by_key(|intent| {
            (
                intent
                    .source_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                intent.span.start,
                intent.span.end,
                intent.from.0,
                intent.to.0,
            )
        });
        self.cast_intents.dedup_by(|left, right| {
            left.source_path == right.source_path
                && left.span == right.span
                && left.from == right.from
                && left.to == right.to
        });
        let result = TypeResult {
            types: self.type_table,
            named_type_names: self
                .resolution
                .items
                .iter()
                .map(|item| (item.id, item.name.clone()))
                .collect(),
            expr_types: self.expr_types,
            scoped_expr_types: self.scoped_expr_types,
            local_types: self.local_types,
            function_signatures: self.function_signatures,
            method_function_signatures: self.method_function_signatures,
            struct_fields_ordered: self.struct_fields_ordered,
            struct_event_fields: self.struct_event_fields,
            enum_variants_ordered: self.enum_variants_ordered,
            generic_items: self.generic_items,
            call_kinds: self.call_kinds,
            scoped_call_kinds: self.scoped_call_kinds,
            contract_method_order: self.contract_method_order,
            contract_signatures: self.contract_signatures,
            cast_intents: self.cast_intents,
        };
        let errors = std::mem::take(&mut self.errors);
        (result, errors)
    }

    fn seed_method_receiver(
        &mut self,
        method_span: SpanInfo,
        def: &Spanned<crate::hir::HirMethodDefinition>,
    ) {
        let Some(method_item_id) = self.item_id_for_span(method_span) else {
            return;
        };
        let Some(ResolvedType::Item(receiver_item_id)) =
            self.resolved_type_at(def.node.receiver_type.span)
        else {
            return;
        };
        self.methods_by_receiver.insert(
            (receiver_item_id, def.node.name.node.name.clone()),
            method_item_id,
        );
    }

    fn seed_contract_signatures(&mut self, program: &Spanned<HirProgram>) {
        let definitions: HashMap<String, &Spanned<crate::hir::HirContractDefinition>> = program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                HirItem::ContractDefinition(def) => Some((def.node.name.node.name.clone(), def)),
                _ => None,
            })
            .collect();
        let mut cache: HashMap<String, Vec<(String, FunctionSignature)>> = HashMap::new();
        let contract_names = definitions.keys().cloned().collect::<Vec<_>>();

        for contract_name in contract_names {
            let signatures = self.collect_contract_signatures_recursive(
                contract_name.as_str(),
                &definitions,
                &mut cache,
                &mut HashSet::new(),
            );
            let Some(contract_item_id) = self.item_id_for_name(&contract_name, ItemKind::Contract)
            else {
                continue;
            };
            self.contract_method_order.insert(
                contract_item_id,
                signatures.iter().map(|(name, _)| name.clone()).collect(),
            );
            for (method_name, signature) in signatures {
                self.contract_signatures
                    .insert((contract_item_id, method_name), signature);
            }

            // If this contract has an extern interface, perform static validation.
            if let Some(def) = definitions.get(&contract_name)
                && let Some(ext) = &def.node.extern_interface
            {
                // ABI must be exactly "C"
                let abi_ok = ext
                    .abi
                    .as_ref()
                    .map(|s| s.eq_ignore_ascii_case("C"))
                    .unwrap_or(false);
                if !abi_ok {
                    self.errors.push(TypeError::ExternInvalidAbi {
                        span: def.node.name.span,
                        abi: ext.abi.clone(),
                    });
                }
                // Library must be present and non-empty
                let lib_ok = ext
                    .library
                    .as_ref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                if !lib_ok {
                    self.errors.push(TypeError::ExternMissingLibrary {
                        span: def.node.name.span,
                    });
                }

                // Validate method signatures declared directly in this contract
                for node in &def.node.items {
                    if let HirContractNode::MethodSignature(sig) = &node.node {
                        // Params
                        for param in &sig.node.parameters {
                            if !self.is_allowed_ffi_param(param) {
                                self.errors.push(TypeError::ExternDisallowedParamType {
                                    span: param.span,
                                    method: sig.node.name.node.name.clone(),
                                });
                            }
                        }
                        // Return type
                        if let Some(ret) = &sig.node.return_type
                            && !self.is_allowed_ffi_return(ret)
                        {
                            self.errors.push(TypeError::ExternDisallowedReturnType {
                                span: ret.span,
                                method: sig.node.name.node.name.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    fn collect_contract_signatures_recursive(
        &mut self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::hir::HirContractDefinition>>,
        cache: &mut HashMap<String, Vec<(String, FunctionSignature)>>,
        active: &mut HashSet<String>,
    ) -> Vec<(String, FunctionSignature)> {
        if let Some(cached) = cache.get(contract_name) {
            return cached.clone();
        }
        if !active.insert(contract_name.to_string()) {
            return Vec::new();
        }

        let mut methods = Vec::new();
        let Some(definition) = definitions.get(contract_name) else {
            active.remove(contract_name);
            return methods;
        };

        for node in &definition.node.items {
            match &node.node {
                HirContractNode::MethodSignature(signature) => {
                    if methods
                        .iter()
                        .any(|(name, _)| name == &signature.node.name.node.name)
                    {
                        continue;
                    }
                    let mut params = Vec::new();
                    let mut valid = true;
                    for param in &signature.node.parameters {
                        let Some(type_id) = self.type_id_for_type(&param.node.ty) else {
                            valid = false;
                            break;
                        };
                        params.push(type_id);
                    }
                    if !valid {
                        continue;
                    }
                    let return_type = signature
                        .node
                        .return_type
                        .as_ref()
                        .and_then(|ty| self.type_id_for_type(ty))
                        .or_else(|| self.primitive_type_id(HirPrimitiveType::Unit));
                    let Some(return_type) = return_type else {
                        continue;
                    };
                    methods.push((
                        signature.node.name.node.name.clone(),
                        FunctionSignature {
                            params,
                            return_type,
                        },
                    ));
                }
                HirContractNode::Embedding(embedding) => {
                    let embedded = self.collect_contract_signatures_recursive(
                        embedding.node.name.node.name.as_str(),
                        definitions,
                        cache,
                        active,
                    );
                    for (method_name, signature) in embedded {
                        if methods.iter().any(|(name, _)| name == &method_name) {
                            continue;
                        }
                        methods.push((method_name, signature));
                    }
                }
            }
        }

        active.remove(contract_name);
        cache.insert(contract_name.to_string(), methods.clone());
        methods
    }

    fn is_allowed_ffi_primitive(prim: crate::hir::HirPrimitiveType) -> bool {
        use crate::hir::HirPrimitiveType::*;
        matches!(prim, Bool | U8 | I32 | I64 | F64)
    }

    fn is_allowed_ffi_param(&self, param: &Spanned<crate::hir::HirParameter>) -> bool {
        use crate::hir::HirType;
        match &param.node.ty.node {
            HirType::Primitive(p) => Self::is_allowed_ffi_primitive(p.node),
            _ => false,
        }
    }

    fn is_allowed_ffi_return(&self, ret: &Spanned<crate::hir::HirType>) -> bool {
        // Allow: primitives (Bool, U8, I32, I64, F64), or Unit if unspecified upstream
        use crate::hir::{HirPrimitiveType, HirType};
        match &ret.node {
            HirType::Primitive(p) => {
                Self::is_allowed_ffi_primitive(p.node) || matches!(p.node, HirPrimitiveType::Unit)
            }
            _ => false,
        }
    }
}

/// Type-check `program`; returns `Err` when any [`TypeError`] was recorded.
pub fn type_program(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
) -> Result<TypeResult, Vec<TypeError>> {
    TypeContext::new(resolution).type_program(program)
}

/// Like [`type_program`] but returns partial [`TypeResult`] together with all accumulated errors (for IDE).
pub fn type_program_with_errors(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
) -> (TypeResult, Vec<TypeError>) {
    TypeContext::new(resolution).type_program_with_errors(program)
}
