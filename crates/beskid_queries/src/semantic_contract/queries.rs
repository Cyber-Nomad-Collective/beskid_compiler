//! Stable public query wrappers over generation-bound Salsa facts.

use super::*;

pub(super) fn with_registered_syntax<T>(
    db: &dyn Db,
    key: AstNodeKey,
    query: impl FnOnce(&dyn Db, SyntaxUnitInput, AstNodeKey) -> SemanticQueryResult<T>,
) -> SemanticQueryResult<T> {
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return Ok(None);
    };
    query(db, syntax, key)
}

/// Resolve a single-segment value path to an exact function declaration in lexical module scope.
///
/// Local declarations shadow item names. Ambiguous, qualified, generic, and unresolved paths
/// contain no item fact. Stale, unregistered, and non-path nodes also contain no fact.
pub fn resolved_item(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ResolvedItem> {
    with_registered_syntax(db, key, resolved_item_tracked)
}

/// Resolve a single-segment value path to its generation-safe lexical declaration key.
///
/// Function and method parameters, lets, lambda parameters, for iterators, and match bindings are
/// supported. Out-of-scope, self-initializing, qualified, generic, and unresolved paths contain no
/// local fact.
pub fn resolved_local(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ResolvedLocal> {
    with_registered_syntax(db, key, resolved_local_tracked)
}

/// Integer immediate for an unshadowed module constant in the current source unit.
pub fn constant_integer(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<i64> {
    with_registered_syntax(db, key, constant_integer_tracked)
}

/// Return the deterministic owner-qualified slot for an exact local declaration identifier.
///
/// Function and method parameters precede body declarations in expanded-AST order. Lambda frames
/// have distinct owner keys. Stale, unregistered, ownerless, and non-declaration identifiers
/// contain no fact.
pub fn local_slot(db: &dyn Db, declaration: AstNodeKey) -> SemanticQueryResult<LocalSlot> {
    with_registered_syntax(db, declaration, local_slot_tracked)
}

/// Return the exact mutable local destination for a simple assignment expression.
///
/// The fact rejects immutable declarations, non-path targets, qualified paths, compound targets,
/// and stale or unregistered syntax. Codegen uses it as the only authority for local writes.
pub fn mutable_local_assignment(db: &dyn Db, assignment: AstNodeKey) -> SemanticQueryResult<MutableLocalAssignment> {
    with_registered_syntax(db, assignment, mutable_local_assignment_tracked)
}

/// Return primitive types proven by literals, explicit syntax, or exact lexical declarations.
///
/// Complex declarations and expression shapes requiring inference remain explicitly unavailable.
/// Stale, unregistered, and non-typable nodes contain no fact.
pub fn node_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, node_type_tracked)
}

/// Return the exact root expression keys of positional call arguments in source order.
///
/// Empty calls contain an empty fact. Stale, unregistered, and non-call nodes contain no fact.
/// A current argument that cannot be mapped through the authoritative syntax index is explicitly
/// unavailable.
pub fn call_arguments(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, call_arguments_tracked)
}

/// Return exact current-generation bounds for the syntax-only `range(start, end)` loop form.
pub fn range_for_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<RangeForFact> {
    with_registered_syntax(db, key, range_for_fact_tracked)
}

/// Return the iterator declaration identity and element type for one current `ForStatement`.
///
/// Only the syntax-only `range(start, end)` iterable proves an element type. Stale generations,
/// unregistered nodes, and non-for statements contain no fact.
pub fn for_iterator_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ForIteratorFact> {
    with_registered_syntax(db, key, for_iterator_fact_tracked)
}

/// Return the payload, error, and enclosing-return ABI facts for postfix `Result` propagation.
///
/// The query accepts only a direct local parameter operand and an enclosing function returning
/// the same syntactic `Result<TPayload, TError>` instantiation. All other forms fail closed as unavailable.
pub fn try_expression_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<TryExpressionFact> {
    with_registered_syntax(db, key, try_expression_fact_tracked)
}

/// Return the declaration identifier for the receiver of an exact `local.Method()` path.
///
/// The fact exists only when the local has an explicit nominal parameter or let annotation and
/// that nominal type declares exactly one matching method. Static paths, inferred locals,
/// extension methods, overloads, and chained receivers remain unavailable.
pub fn nominal_member_receiver(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AstNodeKey> {
    with_registered_syntax(db, key, nominal_member_receiver_tracked)
}

/// Classify call shapes whose lowering is certain from expanded syntax alone.
///
/// Immediate lambda calls are dynamic. Exactly resolved single-segment function calls are direct.
/// Ambiguous, shadowed, unresolved, member, runtime, and other call shapes remain explicitly
/// unavailable. Stale, unregistered, and non-call nodes contain no fact.
pub fn call_lowering(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CallLowering> {
    with_registered_syntax(db, key, call_lowering_tracked)
}

/// Select a collection operation only when this call resolves to the canonical Array source unit.
/// A user declaration with the same function name or an unresolved/stale call receives no authority.
pub fn collection_operation(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CollectionOperation> {
    let Some(CallLowering::Direct(declaration)) = call_lowering(db, key)? else {
        return Ok(None);
    };
    let path = declaration.unit.path(db);
    let components = path.iter().rev().take(3).map(|part| part.to_string_lossy()).collect::<Vec<_>>();
    if components.as_slice() != ["Array.bd", "Collections", "Core"] {
        return Ok(None);
    }
    let Some(syntax) = db.syntax_unit(declaration.unit) else {
        return Ok(None);
    };
    if syntax.generation(db) != declaration.generation {
        return Ok(None);
    }
    let Some(function) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
    else {
        return Ok(None);
    };
    Ok(Some(match function.name.node.name.as_str() {
        "Append" => {
            let arguments =
                call_arguments(db, key)?.ok_or_else(|| SemanticError::unavailable("collection_operation"))?;
            let array = *arguments.first().ok_or_else(|| SemanticError::unavailable("collection_operation"))?;
            let owner = if let Some(access) = aggregate_field_access(db, array)? {
                let receiver = resolved_local(db, access.receiver)?
                    .and_then(|resolved| local_slot(db, resolved.declaration).transpose())
                    .transpose()?
                    .ok_or_else(|| SemanticError::unavailable("collection_operation"))?;
                CollectionMutationOwner::AggregateField {
                    receiver,
                    declaration: access.declaration,
                    index: access.index,
                }
            } else {
                let resolved =
                    resolved_local(db, array)?.ok_or_else(|| SemanticError::unavailable("collection_operation"))?;
                let syntax = db
                    .syntax_unit(resolved.declaration.unit)
                    .filter(|syntax| syntax.generation(db) == resolved.declaration.generation)
                    .ok_or_else(|| SemanticError::unavailable("collection_operation"))?;
                let mutable = with_node(db, syntax, resolved.declaration, |program, index, _| {
                    Some(local_declaration_is_mutable(program, index, resolved.declaration.node))
                })?
                .unwrap_or(false);
                if !mutable {
                    return Err(SemanticError::unavailable("collection_operation"));
                }
                let slot = local_slot(db, resolved.declaration)?
                    .ok_or_else(|| SemanticError::unavailable("collection_operation"))?;
                CollectionMutationOwner::Local(slot)
            };
            CollectionOperation::Append { owner }
        }
        "Capacity" => CollectionOperation::Capacity,
        "Clear" => CollectionOperation::Clear,
        "RemoveLast" => CollectionOperation::RemoveLast,
        _ => return Ok(None),
    }))
}

pub fn primitive_numeric_conversion(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<PrimitiveNumericConversion> {
    with_registered_syntax(db, key, primitive_numeric_conversion_tracked)
}

/// Return the exact declared generic target for one current call with explicit terminal type
/// arguments. Arity mismatches, stale generations, and inferred calls remain unavailable.
pub fn generic_call_instantiation(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<GenericCallInstantiation> {
    with_registered_syntax(db, key, generic_call_instantiation_tracked)
}

/// Return the exact source-derived ABI specialization for one generic direct call.
///
/// Inferred generic arguments are accepted only when every ABI type is proven by the current
/// call arguments.  The returned declaration plus signature is suitable for a mangled module
/// identity and never consults legacy HIR lowering.
pub fn generic_call_specialization(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<GenericCallSpecialization> {
    with_registered_syntax(db, key, generic_call_specialization_tracked)
}

/// Return a nested generic call that forwards enclosing type parameters explicitly in source.
/// Concrete calls use [`generic_call_specialization`] instead; templates cannot be executed
/// until module emission supplies the enclosing instance environment.
pub fn generic_call_template(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<GenericCallTemplate> {
    with_registered_syntax(db, key, generic_call_template_tracked)
}

/// Resolve a direct generic nominal method call's explicitly applied owner environment.
pub fn generic_nominal_method_receiver(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericNominalMethodReceiver> {
    with_registered_syntax(db, key, generic_nominal_method_receiver_tracked)
}

/// Return the generation-safe ABI representation of a source value or declared storage boundary.
pub fn value_abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, value_abi_type_tracked)
}

/// Return numeric cast intents proven by an exact typed-let constraint.
///
/// Inferred, complex, non-numeric, and other unported coercion contexts remain explicitly
/// unavailable. Stale, unregistered, and non-expression nodes contain no fact.
pub fn cast_intents(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[CastIntent]>> {
    with_registered_syntax(db, key, cast_intents_tracked)
}

/// Return AST-derived fall-through facts for executable nodes in the current generation.
///
/// Loops are conservative because their body may execute zero times. Stale, unregistered, and
/// non-executable nodes contain no fact.
pub fn control_flow(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ControlFlow> {
    with_registered_syntax(db, key, control_flow_tracked)
}

/// Return exact callable signatures whose types have generation-independent primitive identities.
///
/// Complex, array, and function types remain unavailable until their Salsa type identities are
/// ported. Stale, unregistered, and non-callable nodes contain no fact.
pub fn item_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, item_signature_tracked)
}

/// Return the scalar ABI representation signature proven by current source syntax.
pub fn item_abi_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, item_abi_signature_tracked)
}

/// Return the exact ABI signature selected by one direct call expression.
///
/// Generic parameters are substituted only from matching current argument facts; no HIR type
/// result or inferred fallback participates in this boundary.
pub fn call_abi_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, call_abi_signature_tracked)
}

/// Return target-neutral source field shapes for a nominal `type` definition.
pub fn aggregate_layout(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AggregateLayoutFact> {
    with_registered_syntax(db, key, aggregate_layout_tracked)
}

/// Return the current nominal `type` declaration constructed by a struct literal.
pub fn aggregate_literal_declaration(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AstNodeKey> {
    with_registered_syntax(db, key, aggregate_literal_declaration_tracked)
}

/// Return the source-proven element ABI for an empty array literal used directly as a declared
/// nominal aggregate field.  No inferred or otherwise context-free empty array receives a fact.
pub fn empty_array_literal_element_abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, empty_array_literal_element_abi_type_tracked)
}

/// Return the source-proven element ABI for indexing an explicitly declared local array.
pub fn array_index_element_abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, array_index_element_abi_type_tracked)
}

/// Return the exact field selected by a direct nominal local receiver member expression.
pub fn aggregate_field_access(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AggregateFieldAccess> {
    with_registered_syntax(db, key, aggregate_field_access_tracked)
}

/// Return target-neutral source variants and field shapes for a non-generic `enum` definition or
/// an enum constructor whose applied type arguments fully instantiate its generic declaration.
pub fn enum_layout(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<EnumLayoutFact> {
    with_registered_syntax(db, key, enum_layout_tracked)
}

/// Return the exact source enum constructor selection for the current syntax generation.
///
/// Constructors with multiple payload fields remain unavailable until the generated ISLE enum
/// emitter has an equally explicit multi-field payload representation.
pub fn enum_constructor(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<EnumConstructorFact> {
    with_registered_syntax(db, key, enum_constructor_tracked)
}

/// Return the exact source enum declaration and arms selected by one `match` expression.
///
/// Guarded and payload-destructuring arms remain unavailable until generated ISLE owns their
/// binding and control-flow representation.
pub fn enum_match(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<EnumMatchFact> {
    with_registered_syntax(db, key, enum_match_tracked)
}

/// Return the scalar ABI representation for one current syntax node.
pub fn abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, abi_type_tracked)
}

/// Return the exact call-parameter ABI selected for one bare integer argument.
///
/// Only a singleton unsuffixed integer argument of a direct call receives this contextual fact;
/// all other expressions remain unavailable rather than being implicitly coerced.
pub fn call_argument_abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, call_argument_abi_type_tracked)
}

pub fn binary_operand_abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, binary_operand_abi_type_tracked)
}

/// Return the exact ABI selected for a bare integer literal at a declared contextual boundary.
///
/// This fact never performs a numeric conversion: it is unavailable for inferred locals,
/// explicit literal suffixes, compound values, immutable destinations, and out-of-range values.
pub fn contextual_integer_literal_abi_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, contextual_integer_literal_abi_type_tracked)
}

/// Return the exact lambda parameters and outer lexical captures in source order.
///
/// Captures never include declarations owned by the lambda itself. Stale, unregistered, and
/// non-lambda nodes contain no fact.
pub fn closure_environment(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ClosureEnvironment> {
    with_registered_syntax(db, key, closure_environment_tracked)
}

/// Return a generation-bound lambda signature, body key, and deterministic capture ABI shape.
///
/// The shape requires a runtime pointer-map descriptor, but this fact deliberately reports that
/// no generated lowering or closure allocation exists yet. Generic and inferred callable shapes
/// remain unavailable rather than consulting HIR. Stale, unregistered, and non-lambda nodes
/// contain no fact.
pub fn closure_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ClosureSignature> {
    with_registered_syntax(db, key, closure_signature_tracked)
}

/// Return the direct lambda call target selected by one call expression.
///
/// Calls through local bindings and all dynamic closure dispatch remain unavailable; this query
/// does not infer a runtime closure object. Stale, unregistered, and non-call nodes contain no
/// fact.
pub fn closure_call_target(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ClosureCallTarget> {
    with_registered_syntax(db, key, closure_call_target_tracked)
}

/// Return the exact spawn operand and any captures required when it is a lambda expression.
///
/// Stale, unregistered, and non-spawn nodes contain no fact.
pub fn spawn_target(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SpawnTarget> {
    with_registered_syntax(db, key, spawn_target_tracked)
}

/// Return source-owned storage provenance for one exact local-path use.
///
/// Mutable bindings and native pointers are stack references and must not cross a spawn
/// boundary. This is a source-provenance fact only; it does not claim rooted closure storage.
/// Stale, unregistered, non-local, and non-path nodes contain no fact.
pub fn capture_storage(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CaptureStorage> {
    with_registered_syntax(db, key, capture_storage_tracked)
}

/// Return the callable signature proven by the current syntax generation.
///
/// Functions, methods, tests, typed lambdas, direct item paths, and direct item calls are
/// supported. Inferred/complex callable shapes remain unavailable rather than consulting HIR.
pub fn callable_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, callable_signature_tracked)
}

/// Return authoritative spawn legality, result, and precise source diagnostics.
///
/// The fact never inspects HIR. A non-callable target gets `TargetNotCallable`; an entry with
/// parameters gets `TargetRequiresArguments`; a mutable or native-pointer closure capture gets
/// `StackReferenceEscapesSpawn`. Stale, unregistered, and non-spawn nodes contain no fact.
pub fn spawn_legality(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SpawnLegality> {
    with_registered_syntax(db, key, spawn_legality_tracked)
}

/// Return source-only zero-argument spawn-entry validation for the current syntax generation.
///
/// This validation does not claim a generated trampoline, closure allocation, or runtime fiber
/// object. Stale, unregistered, and non-spawn nodes contain no fact.
pub fn spawn_entry_validation(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SpawnEntryValidation> {
    with_registered_syntax(db, key, spawn_entry_validation_tracked)
}

/// Return the manifest-owned intrinsic index for an exact, unshadowed builtin call.
///
/// Unknown, dynamic, and lexically shadowed calls remain explicitly unavailable. Stale or
/// unregistered keys contain no fact.
pub fn runtime_intrinsic(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<RuntimeIntrinsic> {
    with_registered_syntax(db, key, runtime_intrinsic_tracked)
}

/// Return an unprivileged direct-call spelling for the codegen runtime import gate.
///
/// The name alone does not authorize an ABI import; only a canonical-runtime typed program can
/// turn it into one.
pub fn runtime_intrinsic_name(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<RuntimeIntrinsicName> {
    with_registered_syntax(db, key, runtime_intrinsic_name_tracked)
}

pub fn node_kind(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<IndexedNodeKind> {
    with_registered_syntax(db, key, node_kind_tracked)
}

pub fn child_nodes(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, child_nodes_tracked)
}

pub fn literal_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<LiteralFact> {
    with_registered_syntax(db, key, literal_fact_tracked)
}

pub fn clif_block_body(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<str>> {
    with_registered_syntax(db, key, clif_block_body_tracked)
}

pub fn node_span(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SourceSpan> {
    with_registered_syntax(db, key, node_span_tracked)
}

pub fn operator_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<OperatorFact> {
    with_registered_syntax(db, key, operator_fact_tracked)
}

pub fn item_body(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AstNodeKey> {
    with_registered_syntax(db, key, item_body_tracked)
}

/// Return the exact executable statement nodes for a current syntax test definition.
pub fn test_statement_nodes(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, test_statement_nodes_tracked)
}

/// Return executable statements for a current block in source order.
pub fn block_statement_nodes(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, block_statement_nodes_tracked)
}

/// Return the exact declared name for a current syntax function, method, or test item.
pub fn item_name(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<str>> {
    with_registered_syntax(db, key, item_name_tracked)
}

/// Return the explicitly declared linker symbol for a current syntax function.
pub fn item_export_symbol(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ExportSymbol> {
    with_registered_syntax(db, key, item_export_symbol_tracked)
}

/// Return CLI-facing metadata for one current syntax `test` item.
pub fn test_item(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<TestItem> {
    with_registered_syntax(db, key, test_item_tracked)
}

/// Return unique direct function callees in expanded-syntax order.
///
/// Dynamic calls do not add an edge. Any unresolved call makes the result explicitly unavailable
/// so an incomplete graph cannot masquerade as complete.
pub fn direct_callees(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, direct_callees_tracked)
}

/// Traverse direct function calls from an entry using generation-safe declaration keys.
///
/// The result is deterministic depth-first preorder and includes the entry. Recursive cycles are
/// visited once. Missing or unresolved call facts propagate explicit unavailability.
pub fn reachable_items(db: &dyn Db, program: AstNodeKey, entry: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    let Some(syntax) = db.syntax_unit(program.unit) else {
        return Ok(None);
    };
    reachable_items_tracked(db, syntax, program, entry)
}
