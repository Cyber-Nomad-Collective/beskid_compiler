//! Reachability-scoped semantic legality gate.
//!
//! `check_items` is the single production authority `beskid_codegen::lower_syntax_program` calls
//! before it collects generic specializations or selects ISLE rules, and again after
//! specialization has discovered new bodies (contract witnesses, the `pending` worklist in
//! `module_emission/specialization.rs`). It judges exactly the items its caller passes -- never
//! every root of the assembly -- so a legality violation in an item nothing reachable from the
//! requested lowering ever calls cannot poison an unrelated request (see
//! `docs/superpowers/specs/2026-09-23-production-semantic-diagnostics-design.md` section 2.1).
//!
//! A legality fact is a positive, Salsa-tracked description of one user error
//! (`unresolved_type_reference` for E1201, `unresolved_call_target` for E1101/E1108/E1203,
//! `call_arity_mismatch` for E1204, `generic_parameter_conflict` for E1229,
//! `member_reference_legality` for E1211/E1301/E1302/E1307, `match_exhaustiveness` for E1304;
//! lambda stack-reference captures for E1233; generic function values without an instance for
//! E1203/E1204/E1229/E1610; `unresolved_imports` for E1105, judged once per unit that owns
//! a judged item; `scoped_cleanup` for E1230 and `dead_collection_growth` for E1231, which
//! `build_typed_program` no longer judges eagerly for every unit): it is never string
//! classification of an opaque `SemanticError::unavailable`.
//! The `beskid check` gate (`semantic_diagnostics_for_roots`) additionally evaluates
//! `check_typing_obligations` (`typing_obligations` and `value_obligations`, E1206-E1208),
//! `check_gate_obligations`, `check_unit_obligations`, and `extern_profile_findings`: the legacy
//! `TypeChecker` and resolver classes that have no lowering fact of their own.
//! `check_items` collects every finding of the pass rather than stopping at the first, so
//! `beskid test` and `beskid build` report every violation the requested items carry in one run.

use super::local_type_resolution::UnresolvedTypeKind;
use super::*;
use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::syntax::{FunctionDefinition, MethodDefinition};
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};

pub(super) mod calls;
mod attributes;
mod captures;
mod cleanup;
mod externs;
mod function_values;
mod gate;
mod generics;
mod imports;
mod members;
mod typing;
mod units;
mod value_paths;
mod values;

pub use attributes::{
    AttributeResolution, UnitAttributeDeclaration, UnitAttributeSurface, UnitAttributeUse, attribute_findings,
    check_attribute_obligations, resolve_attribute_use, unit_attribute_surface,
};
pub use externs::extern_profile_findings;
pub use gate::{GateObligation, GateObligationKind, check_gate_obligations, gate_obligations};
pub use typing::{TypingObligation, TypingObligationKind, typing_obligations};
pub use units::{UnitObligation, UnitObligationKind, check_unit_obligations, unit_obligations};
pub use values::{ValueObligation, value_obligations};

pub use calls::{
    UnresolvedCallKind, UnresolvedCallTarget, UnresolvedValueArgument, unresolved_call_target,
    unresolved_value_argument,
};
pub use generics::{GenericParameterConflict, generic_parameter_conflict};
pub use imports::{UnresolvedImport, unresolved_imports};
pub use members::{
    MemberReferenceFinding, MemberReferenceKind, NonExhaustiveMatch, match_exhaustiveness, member_reference_legality,
};

/// A direct call whose supplied argument count does not match its resolved declaration's
/// parameter count (plus one for an implicit or explicit method receiver).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CallArityMismatch {
    /// The call expression (the diagnostic site).
    pub call: AstNodeKey,
    pub expected: usize,
    pub actual: usize,
}

/// Report the first direct call in `key` (an item) whose argument count does not match its
/// resolved declaration.
///
/// Only `CallLowering::Direct` calls to a `FunctionDefinition` or `MethodDefinition` are judged:
/// this is exactly the declaration shape and the exact `parameters.len() + is_method` formula
/// `module_emission::specialization::resolve_module_items`'s own arity check already uses, so
/// this fact can never disagree with what lowering does. Dynamic calls, manifest builtins,
/// runtime intrinsics, Corelib services, and calls to a callee with a `bulk` parameter are not
/// judged here; they have their own authority.
pub fn call_arity_mismatch(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CallArityMismatch> {
    with_registered_syntax(db, key, call_arity_mismatch_tracked)
}

#[salsa::tracked(persist)]
fn call_arity_mismatch_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CallArityMismatch> {
    let calls = with_node(db, syntax, key, |_program, index, _node| {
        let mut calls = Vec::new();
        collect_call_sites(index, key.node, &mut calls);
        Some(calls)
    })?
    .unwrap_or_default();
    for call_node in calls {
        let call_key = AstNodeKey { node: call_node, ..key };
        let Some(mismatch) = call_arity_mismatch_for_call(db, call_key)? else { continue };
        return Ok(Some(mismatch));
    }
    Ok(None)
}

fn collect_call_sites(
    index: &SyntaxIndex,
    id: beskid_analysis::syntax::AstNodeId,
    calls: &mut Vec<beskid_analysis::syntax::AstNodeId>,
) {
    if index.kind(id) == Some(NodeKind::CallExpression) {
        calls.push(id);
    }
    if let Some(children) = index.children(id) {
        for child in children {
            collect_call_sites(index, *child, calls);
        }
    }
}

fn call_arity_mismatch_for_call(db: &dyn Db, call: AstNodeKey) -> SemanticQueryResult<CallArityMismatch> {
    let Some(CallLowering::Direct(declaration)) = call_lowering(db, call)? else { return Ok(None) };
    let Some(declaration_syntax) = db.syntax_unit(declaration.unit) else { return Ok(None) };
    if !declaration_syntax.accepts_key(db, AstNodeKey { node: declaration.node, ..declaration }) {
        return Ok(None);
    }
    let index = declaration_syntax.syntax_index(db);
    let program = declaration_syntax.expanded_program(db);
    let Some(declaration_node) = index.node_at(program, declaration.node) else { return Ok(None) };
    let (parameters, receiver) = if let Some(function) = declaration_node.of::<FunctionDefinition>() {
        (&function.parameters, 0)
    } else if let Some(method) = declaration_node.of::<MethodDefinition>() {
        (&method.parameters, 1)
    } else {
        return Ok(None);
    };
    // A `bulk T[]` callee accepts any number of scalar arguments: lowering packs every argument
    // into one rooted array (`CodegenInput::bulk_array_static_plan`, ISLE `emit_bulk_call`), so
    // the scalar-arity formula does not apply. Those calls have their own authority.
    if parameters.iter().any(|parameter| parameter.node.bulk) {
        return Ok(None);
    }
    let expected = parameters.len() + receiver;
    let Some(arguments) = call_arguments(db, call)? else { return Ok(None) };
    let actual = arguments.len();
    if actual == expected {
        return Ok(None);
    }
    Ok(Some(CallArityMismatch { call, expected, actual }))
}

/// A direct call argument whose proven source type can never satisfy the callee's declared
/// parameter type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CallArgumentTypeMismatch {
    /// The call expression.
    pub call: AstNodeKey,
    /// The argument expression (the diagnostic site).
    pub argument: AstNodeKey,
    /// The parameter's declared type, as the callee spells it.
    pub expected: Arc<str>,
    /// The argument's proven source type.
    pub actual: Arc<str>,
}

/// Report the first direct call in `key` (an item) that passes an argument of a provably
/// different nominal type than its parameter declares (E1205).
///
/// Only the shapes whose identities the semantic contract proves exactly are judged: a parameter
/// declared as a concrete record or enum (no type arguments, not a callee type parameter, not a
/// contract) against an argument whose source identity is a nominal or a proven scalar, and a
/// primitive parameter against a nominal argument. Scalar-versus-scalar representation
/// selection, generic and contract-bounded parameters, arrays, functions, and arguments without
/// a proven identity are not judged here; they keep their own authorities. The legacy checker
/// never sees this error for a non-entry unit: it discards dependency-body findings, which is
/// how `Query.Span(Query.At(root))` reached a native worker as a `NodeRef`-typed `SyntaxQuery`.
pub fn call_argument_type_mismatch(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CallArgumentTypeMismatch> {
    with_registered_syntax(db, key, call_argument_type_mismatch_tracked)
}

#[salsa::tracked(persist)]
fn call_argument_type_mismatch_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CallArgumentTypeMismatch> {
    let calls = with_node(db, syntax, key, |_program, index, _node| {
        let mut calls = Vec::new();
        collect_call_sites(index, key.node, &mut calls);
        Some(calls)
    })?
    .unwrap_or_default();
    for call_node in calls {
        let call_key = AstNodeKey { node: call_node, ..key };
        let Some(mismatch) = call_argument_type_mismatch_for_call(db, call_key)? else { continue };
        return Ok(Some(mismatch));
    }
    Ok(None)
}

enum ExpectedArgumentType {
    Nominal(Arc<str>),
    Scalar(SemanticTypeId),
}

fn call_argument_type_mismatch_for_call(
    db: &dyn Db,
    call: AstNodeKey,
) -> SemanticQueryResult<CallArgumentTypeMismatch> {
    use beskid_analysis::syntax::{EnumDefinition, Type, TypeDefinition};

    let Some(CallLowering::Direct(declaration)) = call_lowering(db, call)? else { return Ok(None) };
    let Some(declaration_syntax) = db.syntax_unit(declaration.unit) else { return Ok(None) };
    if !declaration_syntax.accepts_key(db, declaration) {
        return Ok(None);
    }
    let index = declaration_syntax.syntax_index(db);
    let program = declaration_syntax.expanded_program(db);
    let Some(declaration_node) = index.node_at(program, declaration.node) else { return Ok(None) };
    let (parameters, receiver) = if let Some(function) = declaration_node.of::<FunctionDefinition>() {
        (&function.parameters, 0)
    } else if let Some(method) = declaration_node.of::<MethodDefinition>() {
        (&method.parameters, 1)
    } else {
        return Ok(None);
    };
    if parameters.iter().any(|parameter| parameter.node.bulk) {
        return Ok(None);
    }
    let generic_names = generic_callable_parameters(db, declaration)
        .map(|(names, _)| names.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>())
        .unwrap_or_default();
    let Some(arguments) = call_arguments(db, call)? else { return Ok(None) };
    if arguments.len() != parameters.len() + receiver {
        return Ok(None);
    }
    for (parameter, argument) in parameters.iter().zip(arguments.iter().skip(receiver)) {
        let expected = match &parameter.node.ty.node {
            Type::Complex(path) => {
                if path.node.segments.iter().any(|segment| !segment.node.type_args.is_empty()) {
                    continue;
                }
                if let [segment] = path.node.segments.as_slice()
                    && generic_names.contains(&segment.node.name.node.name)
                {
                    continue;
                }
                let Some(target) = resolve_type_declaration(db, declaration, &path.node) else { continue };
                let Some(target_syntax) = db.syntax_unit(target.unit).filter(|syntax| syntax.accepts_key(db, target))
                else {
                    continue;
                };
                let concrete_nominal = target_syntax
                    .syntax_index(db)
                    .node_at(target_syntax.expanded_program(db), target.node)
                    .is_some_and(|node| {
                        node.of::<TypeDefinition>().is_some_and(|definition| definition.generics.is_empty())
                            || node.of::<EnumDefinition>().is_some_and(|definition| definition.generics.is_empty())
                    });
                if !concrete_nominal {
                    continue;
                }
                let Some(identity) = stable_declaration_identity(db, target) else { continue };
                ExpectedArgumentType::Nominal(identity)
            }
            Type::Primitive(_) => match semantic_type_from_syntax(&parameter.node.ty.node) {
                Ok(scalar) => ExpectedArgumentType::Scalar(scalar),
                Err(_) => continue,
            },
            _ => continue,
        };
        let Ok(actual) = generic_source_expression_identity(db, *argument) else { continue };
        let mismatch = match (&expected, &actual) {
            (ExpectedArgumentType::Nominal(expected), GenericSourceTypeIdentity::Nominal { qualified_name, .. }) => {
                expected != qualified_name
            }
            (ExpectedArgumentType::Nominal(_), GenericSourceTypeIdentity::Abi(_))
            | (ExpectedArgumentType::Scalar(_), GenericSourceTypeIdentity::Nominal { .. }) => true,
            _ => false,
        };
        if !mismatch {
            continue;
        }
        let spelled = |ty: &Type| match ty {
            Type::Complex(path) => path
                .node
                .segments
                .iter()
                .map(|segment| segment.node.name.node.name.as_str())
                .collect::<Vec<_>>()
                .join("."),
            other => format!("{other:?}"),
        };
        let expected_name: Arc<str> = match &expected {
            ExpectedArgumentType::Nominal(_) => Arc::from(spelled(&parameter.node.ty.node)),
            ExpectedArgumentType::Scalar(scalar) => Arc::from(scalar.to_string()),
        };
        let actual_name: Arc<str> = match &actual {
            GenericSourceTypeIdentity::Nominal { qualified_name, .. } => qualified_name.clone(),
            GenericSourceTypeIdentity::Abi(scalar) => Arc::from(scalar.to_string()),
            other => Arc::from(format!("{other:?}")),
        };
        return Ok(Some(CallArgumentTypeMismatch {
            call,
            argument: *argument,
            expected: expected_name,
            actual: actual_name,
        }));
    }
    Ok(None)
}

/// An assignment whose target is a single-segment path that resolves to an immutable local: a
/// `let` without `mut`, a parameter without `mut`, or a `for` iterator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ImmutableLocalAssignment {
    /// The assignment expression (the diagnostic site).
    pub assignment: AstNodeKey,
    /// The immutable local's name.
    pub name: Arc<str>,
}

/// Report the first assignment in `key` (an item) that writes an immutable local (E1214).
///
/// The target resolves through `resolve_lexical_declaration`, the same lexical authority
/// `mutable_local_assignment` uses when ISLE selects `emit_local_assign`. That query fails closed
/// on an immutable target, so without this fact such a write reaches lowering and surfaces as
/// `MissingRuleOrFact AssignExpression` instead of a user diagnostic.
pub fn immutable_local_assignment(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ImmutableLocalAssignment> {
    with_registered_syntax(db, key, immutable_local_assignment_tracked)
}

#[salsa::tracked(persist)]
fn immutable_local_assignment_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ImmutableLocalAssignment> {
    let finding = with_node(db, syntax, key, |program, index, _node| {
        let mut assignments = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::AssignExpression, &mut assignments);
        assignments.into_iter().find_map(|assignment| immutable_assignment_target(program, index, key, assignment))
    })?;
    Ok(finding)
}

fn immutable_assignment_target(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    assignment: beskid_analysis::syntax::AstNodeId,
) -> Option<ImmutableLocalAssignment> {
    let node = index.node_at(program, assignment)?;
    let assign = node.of::<beskid_analysis::syntax::AssignExpression>()?;
    let beskid_analysis::syntax::Expression::Path(path) = &assign.target.node else { return None };
    let [segment] = path.node.path.node.segments.as_slice() else { return None };
    let name = segment.node.name.node.name.as_str();
    let declaration = resolve_lexical_declaration(program, index, assignment, name)?;
    let declaring = parent_node(index, declaration)?;
    let immutable = match index.kind(declaring)? {
        NodeKind::LetStatement | NodeKind::Parameter => !local_declaration_is_mutable(program, index, declaration),
        NodeKind::ForStatement => true,
        _ => false,
    };
    immutable
        .then(|| ImmutableLocalAssignment { assignment: AstNodeKey { node: assignment, ..key }, name: Arc::from(name) })
}

fn collect_nodes_of_kind(
    index: &SyntaxIndex,
    id: beskid_analysis::syntax::AstNodeId,
    kind: NodeKind,
    found: &mut Vec<beskid_analysis::syntax::AstNodeId>,
) {
    if index.kind(id) == Some(kind) {
        found.push(id);
    }
    if let Some(children) = index.children(id) {
        for child in children {
            collect_nodes_of_kind(index, *child, kind, found);
        }
    }
}

/// Evaluate every legality fact for every item in `items`, in order, collecting every finding of
/// the pass. `Ok(())` means every item is clear to specialize and lower; `Err(findings)` means at
/// least one is not, and the caller must stop before backend code generation
/// (`compiler--build-pipeline--stage-ordering`: "Semantic error diagnostics must stop lowering
/// before backend code generation").
///
/// Items whose facts are unavailable (a genuine compiler gap, not a user error) are not findings
/// here: they surface at the module-emission boundary as an internal error instead, per section
/// 2.3 of the design.
pub fn check_items(db: &dyn Db, items: &[AstNodeKey]) -> Result<(), Vec<SemanticFinding>> {
    let mut findings = import_findings(db, items);
    for &item in items {
        if let Ok(Some(finding)) = unresolved_value_argument(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::ResolveUnknownValue { name: finding.name.to_string() },
                site: finding.site,
                related: Vec::new(),
            });
        }
        if let Ok(Some(finding)) = unresolved_type_reference(db, item) {
            let kind = match finding.kind {
                UnresolvedTypeKind::UnknownType => SemanticIssueKind::TypeUnknownType { name: finding.name.to_string() },
                UnresolvedTypeKind::UnknownModulePath { path } => {
                    SemanticIssueKind::ResolveUnknownModulePath { path: path.to_string() }
                }
                UnresolvedTypeKind::AmbiguousModulePath { path, candidates } => {
                    SemanticIssueKind::ResolveAmbiguousModulePath { path: path.to_string(), candidates }
                }
                UnresolvedTypeKind::AmbiguousItemInModule { module_path, name, candidates } => {
                    SemanticIssueKind::ResolveAmbiguousItemInModule {
                        module_path: module_path.to_string(),
                        name: name.to_string(),
                        candidates,
                    }
                }
                UnresolvedTypeKind::PrivateItemInModule { module_path, name } => {
                    SemanticIssueKind::ResolvePrivateItemInModule {
                        module_path: module_path.to_string(),
                        name: name.to_string(),
                    }
                }
                UnresolvedTypeKind::UnknownTypeInModule { module_path, name } => {
                    SemanticIssueKind::ResolveUnknownTypeInModule {
                        module_path: module_path.to_string(),
                        name: name.to_string(),
                    }
                }
            };
            findings.push(SemanticFinding { kind, site: finding.site, related: Vec::new() });
        }
        if let Ok(Some(finding)) = immutable_local_assignment(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::ImmutableAssignment { name: finding.name.to_string() },
                site: finding.assignment,
                related: Vec::new(),
            });
        }
        if let Ok(Some(finding)) = unresolved_call_target(db, item) {
            let kind = match finding.kind {
                UnresolvedCallKind::UnknownValue { name } => {
                    SemanticIssueKind::ResolveUnknownValue { name: name.to_string() }
                }
                UnresolvedCallKind::UnknownModulePath { path } => {
                    SemanticIssueKind::ResolveUnknownModulePath { path: path.to_string() }
                }
                UnresolvedCallKind::MissingTypeArguments => SemanticIssueKind::TypeMissingTypeArguments { parameter: None },
                UnresolvedCallKind::PrivateBuiltin { name } => {
                    SemanticIssueKind::ResolvePrivateRuntimeBuiltin { name: name.to_string() }
                }
                UnresolvedCallKind::UnknownValueInModule { module_path, name } => {
                    SemanticIssueKind::ResolveUnknownValueInModule {
                        module_path: module_path.to_string(),
                        name: name.to_string(),
                    }
                }
                UnresolvedCallKind::PrivateItemInModule { module_path, name } => {
                    SemanticIssueKind::ResolvePrivateItemInModule {
                        module_path: module_path.to_string(),
                        name: name.to_string(),
                    }
                }
                UnresolvedCallKind::AmbiguousModulePath { path, candidates } => {
                    SemanticIssueKind::ResolveAmbiguousModulePath { path: path.to_string(), candidates }
                }
                UnresolvedCallKind::AmbiguousItemInModule { module_path, name, candidates } => {
                    SemanticIssueKind::ResolveAmbiguousItemInModule {
                        module_path: module_path.to_string(),
                        name: name.to_string(),
                        candidates,
                    }
                }
            };
            findings.push(SemanticFinding { kind, site: finding.call, related: Vec::new() });
        }
        if let Ok(Some(finding)) = member_reference_legality(db, item) {
            let kind = match finding.kind {
                MemberReferenceKind::UnknownStructField { name } => {
                    SemanticIssueKind::TypeUnknownStructField { name: name.to_string() }
                }
                MemberReferenceKind::InaccessibleStructField { name } => {
                    SemanticIssueKind::TypeInaccessibleStructField { name: name.to_string() }
                }
                MemberReferenceKind::MissingStructField { name } => {
                    SemanticIssueKind::TypeMissingStructField { name: name.to_string() }
                }
                MemberReferenceKind::UnknownEnumVariant { enum_name, variant } => SemanticIssueKind::UnknownEnumPath {
                    enum_name: enum_name.to_string(),
                    variant_name: variant.to_string(),
                },
                MemberReferenceKind::EnumConstructorArity { expected, actual } => {
                    SemanticIssueKind::EnumConstructorArityMismatch { expected, actual }
                }
                MemberReferenceKind::PatternArity { expected, actual } => {
                    SemanticIssueKind::PatternArityMismatch { expected, actual }
                }
            };
            findings.push(SemanticFinding { kind, site: finding.site, related: Vec::new() });
        }
        if let Ok(Some(finding)) = match_exhaustiveness(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::MatchNonExhaustive {
                    enum_name: finding.enum_name.to_string(),
                    missing_pattern: finding.missing_pattern.to_string(),
                },
                site: finding.site,
                related: Vec::new(),
            });
        }
        if let Ok(Some(finding)) = generic_parameter_conflict(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::TypeGenericParameterConflict {
                    parameter: finding.conflict.parameter.to_string(),
                    first_name: finding.conflict.first.to_string(),
                    second_name: finding.conflict.second.to_string(),
                },
                site: finding.call,
                related: Vec::new(),
            });
        }
        findings.extend(cleanup::scoped_cleanup_findings(db, item));
        findings.extend(cleanup::dead_growth_findings(db, item));
        findings.extend(captures::lambda_capture_findings(db, item));
        findings.extend(function_values::generic_function_value_semantic_findings(db, item));
        if let Ok(Some(finding)) = call_argument_type_mismatch(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::TypeCallArgumentMismatch {
                    expected_name: finding.expected.to_string(),
                    actual_name: finding.actual.to_string(),
                },
                site: finding.argument,
                related: vec![(finding.call, "in this call")],
            });
        }
        if let Ok(Some(finding)) = call_arity_mismatch(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::TypeCallArityMismatch { expected: finding.expected, actual: finding.actual },
                site: finding.call,
                related: Vec::new(),
            });
        }
    }
    if findings.is_empty() { Ok(()) } else { Err(findings) }
}

/// Evaluate the typing obligations (E1206, E1207, E1208) of every item in `items`, collecting
/// every finding of the pass. This is the `beskid check` complement of [`check_items`]: the
/// legality gate judges what lowering needs before it can run, the typing obligations judge the
/// value positions the legacy `TypeChecker` judged: proven primitives (`typing_obligations`) and
/// source type identities of nominal, array, and contextual positions (`value_obligations`).
/// Items whose typing facts are unavailable are not findings here (the same fail-closed rule as
/// the legality gate).
pub fn check_typing_obligations(db: &dyn Db, items: &[AstNodeKey]) -> Vec<SemanticFinding> {
    let mut findings = Vec::new();
    for &item in items {
        if let Ok(Some(obligations)) = value_obligations(db, item) {
            findings.extend(obligations.iter().map(|obligation| SemanticFinding {
                kind: SemanticIssueKind::TypeMismatch {
                    expected_name: obligation.expected.to_string(),
                    actual_name: obligation.actual.to_string(),
                },
                site: obligation.site,
                related: Vec::new(),
            }));
        }
        let Ok(Some(obligations)) = typing_obligations(db, item) else { continue };
        for obligation in obligations.iter() {
            let kind = match obligation.kind {
                TypingObligationKind::Mismatch { expected, actual } => SemanticIssueKind::TypeMismatch {
                    expected_name: expected.display_name(),
                    actual_name: actual.display_name(),
                },
                TypingObligationKind::MissingReturnValue { expected } => SemanticIssueKind::TypeReturnMismatch {
                    expected_name: expected.display_name(),
                    actual_name: SemanticTypeId::UNIT.display_name(),
                },
                TypingObligationKind::NonBoolCondition { .. } => SemanticIssueKind::TypeNonBoolCondition,
            };
            findings.push(SemanticFinding { kind, site: obligation.site, related: Vec::new() });
        }
    }
    findings
}

/// E1105: every top-level `use` that names no assembled module, in each unit that owns one of
/// `items`, judged once per unit in first-owner order. A `use` inside a unit nothing requested is
/// not judged, so an assembly may carry units whose imports reach outside it as long as nothing
/// is lowered from them.
fn import_findings(db: &dyn Db, items: &[AstNodeKey]) -> Vec<SemanticFinding> {
    let mut judged_units = Vec::new();
    let mut findings = Vec::new();
    for &item in items {
        let unit_root = AstNodeKey { node: beskid_analysis::syntax::AstNodeId(0), ..item };
        if judged_units.contains(&unit_root) {
            continue;
        }
        judged_units.push(unit_root);
        findings.extend(attributes::attribute_findings(db, unit_root));
        if let Ok(Some(imports)) = unresolved_imports(db, unit_root) {
            findings.extend(imports.iter().map(|import| SemanticFinding {
                kind: SemanticIssueKind::UnknownImportPath { path: import.path.to_string() },
                site: import.site,
                related: Vec::new(),
            }));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BeskidDatabase, ProjectSession, build_typed_program};
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use beskid_analysis::services::parse_program_with_source_name;
    use std::sync::Arc;

    pub(super) fn function_definitions(db: &dyn Db, key: AstNodeKey) -> Vec<AstNodeKey> {
        let mut found = Vec::new();
        if node_kind(db, key).expect("node kind") == Some(IndexedNodeKind::FunctionDefinition) {
            found.push(key);
        }
        if let Some(children) = child_nodes(db, key).expect("child nodes") {
            for child in children.iter().copied() {
                found.extend(function_definitions(db, child));
            }
        }
        found
    }

    pub(super) fn one_unit_assembly(source: &str, generation: SyntaxGenerationId) -> (BeskidDatabase, AstNodeKey) {
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let source_path = directory.join("Main.bd");
        std::fs::write(&source_path, source).expect("source");
        let program =
            parse_program_with_source_name(source_path.to_str().expect("path"), source).expect("parse source");
        let entry = SourceUnitId::new(&db, source_path.clone());
        let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit {
                logical_name: "Main".into(),
                origin_path: source_path.clone(),
                path: source_path,
                source: source.into(),
                program,
            }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            generation,
        ));
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
        let root = AstNodeKey { unit: entry, generation, node: beskid_analysis::syntax::AstNodeId(0) };
        (db, root)
    }

    const QUERY_SHAPE: &str = "type NodeRef { u64 issuer, u64 node } type NodeSpan { u64 start, u64 end } \
        mod Query { pub type SyntaxQuery { NodeRef start, u64 bound } \
        pub SyntaxQuery At(NodeRef root) { return SyntaxQuery { start: root, bound: 1_u64 }; } \
        pub NodeSpan Span(NodeRef node) { return NodeSpan { start: node.node, end: node.node }; } } ";

    /// `Query.Span(Query.At(root))`: a `SyntaxQuery` record where `Span` declares `NodeRef`.
    #[test]
    fn module_qualified_call_with_a_record_of_another_type_is_reported_at_the_argument() {
        let source = format!(
            "{QUERY_SHAPE}u64 Main(NodeRef root) {{ NodeSpan span = Query.Span(Query.At(root)); return span.start; }}"
        );
        let (db, root) = one_unit_assembly(&source, SyntaxGenerationId(61));
        let main_item = *function_definitions(&db, root).last().expect("Main");
        let mismatch = super::call_argument_type_mismatch(&db, main_item)
            .expect("call_argument_type_mismatch query")
            .expect("SyntaxQuery passed where NodeRef is declared");
        assert_eq!(mismatch.expected.as_ref(), "NodeRef");
        assert!(mismatch.actual.ends_with("SyntaxQuery"), "actual type names the record: {}", mismatch.actual);
        assert_ne!(mismatch.argument, mismatch.call, "the argument, not the call, is the site");
    }

    #[test]
    fn matching_nominal_generic_and_contract_parameters_are_not_reported() {
        let source = format!(
            "{QUERY_SHAPE}contract Sized {{ u64 Size(); }} type Box : Sized {{ u64 width, pub u64 Size() {{ return width; }} }} \
             unit Take<T>(T value) {{ return; }} u64 Measure(Sized value) {{ return value.Size(); }} \
             u64 Main(NodeRef root) {{ NodeSpan span = Query.Span(root); Take<NodeRef>(root); Take(span); \
             u64 total = Measure(Box {{ width: 3_u64 }}) + Query.At(root).bound; return span.start + total; }}"
        );
        let (db, root) = one_unit_assembly(&source, SyntaxGenerationId(62));
        let main_item = *function_definitions(&db, root).last().expect("Main");
        assert_eq!(super::call_argument_type_mismatch(&db, main_item).expect("query"), None);
    }

    #[test]
    fn scalar_parameter_rejects_a_record_and_record_parameter_rejects_a_scalar() {
        let source = "type Point { i64 x } unit Len(string text) { return; } unit Show(Point point) { return; } \
                      unit Main(Point point, string text) { Len(point); Show(text); return; }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(63));
        let main_item = *function_definitions(&db, root).last().expect("Main");
        let mismatch = super::call_argument_type_mismatch(&db, main_item).expect("query").expect("Len(point)");
        assert_eq!(mismatch.expected.as_ref(), "string");
        assert!(mismatch.actual.ends_with("Point"), "{}", mismatch.actual);
    }

    #[test]
    fn direct_call_with_too_few_arguments_is_reported_at_the_call() {
        let source = "unit Helper(i32 a, i32 b) { return; } unit Main() { Helper(1); return; }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(41));
        let main_item = function_definitions(&db, root)[1];

        let mismatch = super::call_arity_mismatch(&db, main_item)
            .expect("call_arity_mismatch query")
            .expect("Helper call is missing one argument");
        assert_eq!(mismatch.expected, 2);
        assert_eq!(mismatch.actual, 1);
    }

    fn attribute_codes(source: &str, generation: u64) -> Vec<(String, String)> {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        super::attribute_findings(&db, root)
            .into_iter()
            .map(|finding| (finding.kind.code().to_string(), finding.kind.message()))
            .collect()
    }

    #[test]
    fn attribute_declaration_targets_are_the_closed_catalogue() {
        let findings = attribute_codes("pub attribute Tag(Widget) { } pub attribute Twice(Field, Field) { }", 6101);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert_eq!(findings[0].0, "E1509");
        assert!(findings[0].1.contains("Widget"));
        assert_eq!(findings[1].0, "E1508");
        let (db, root) = one_unit_assembly("pub attribute Tag(method) { }", SyntaxGenerationId(6102));
        let finding = super::attribute_findings(&db, root).pop().expect("E1509");
        let SemanticIssueKind::UnknownAttributeDeclarationTarget { allowed, .. } = finding.kind else {
            panic!("E1509 expected")
        };
        assert!(allowed.iter().any(|name| name == "MethodDefinition"));
    }

    #[test]
    fn attribute_uses_are_checked_against_their_declaration() {
        // Declaration without targets accepts nothing.
        let findings = attribute_codes("pub attribute Sealed { } [Sealed] pub type Door { i64 width }", 6103);
        assert_eq!(findings.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["E1510"]);
        // Field attribute on a type, then on a field.
        let findings =
            attribute_codes("pub attribute FluentInner(Field) { } [FluentInner] pub type Wrapper { i64 inner }", 6104);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].1.contains("TypeDefinition"), "{findings:?}");
        assert!(attribute_codes("pub attribute FluentInner(Field) { } pub type Wrapper { [FluentInner] i64 inner }", 6105).is_empty());
        // Undeclared.
        let findings = attribute_codes("[Frozen] pub type Ice { i64 temperature }", 6106);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].1.contains("no declaration"), "{findings:?}");
    }

    #[test]
    fn attributes_at_new_sites_are_enforced() {
        let source = "pub attribute Trace(Parameter) { } pub attribute Pure(LambdaExpression) { } \
                      pub attribute Hot(LambdaParameter) { } pub attribute Any(TypeParameter) { } \
                      pub attribute Pinned(LetStatement) { } pub attribute Legacy(EnumVariant) { } \
                      pub attribute Receiver(ReceiverParameter) { } \
                      pub enum Mode { Fast, [Legacy] Slow } \
                      pub type Counter { i64 count, pub i64 Read([Receiver] this) { return this.count; } } \
                      unit Keep<[Any] T>(T value) { } \
                      i64 Add([Trace] i64 left, i64 right) { [Pinned] mut i64 total = 0; \
                      let square = [Pure] ([Hot] i64 x) => x * x; return left + right; }";
        assert!(attribute_codes(source, 6107).is_empty(), "{:?}", attribute_codes(source, 6107));
        let wrong = "pub attribute Trace(Parameter) { } i64 Add([Trace] i64 left) { [Trace] let y = left; return y; }";
        let findings = attribute_codes(wrong, 6108);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].1.contains("LetStatement"));
    }

    #[test]
    fn compiler_recognized_attributes_keep_their_targets_without_corelib() {
        let findings = attribute_codes("[Extern(Abi:\"C\", Library:\"libc\")] pub i64 Getpid() { return 0; }", 6109);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].0, "E1510");
        let (db, root) = one_unit_assembly("pub type Point { [Immutable] i64 x, i64 y }", SyntaxGenerationId(6110));
        let finding = super::attribute_findings(&db, root).pop().expect("E1510");
        let SemanticIssueKind::AttributeTargetNotAllowed { allowed, declared, .. } = finding.kind else { panic!() };
        assert!(declared);
        assert_eq!(allowed, ["TypeDefinition"]);
        assert!(attribute_codes("[Export(Abi:\"C\")] pub i64 Answer() { return 42; }", 6111).is_empty());
    }

    #[test]
    fn a_user_declaration_shadows_a_recognized_attribute() {
        let source = "pub attribute Immutable(Field) { } pub type Point { [Immutable] i64 x }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(6112));
        assert!(super::attribute_findings(&db, root).is_empty());
        assert!(!super::resolve_attribute_use(&db, root, "Immutable").is_recognized());
        let (db, root) = one_unit_assembly("pub type Point { i64 x }", SyntaxGenerationId(6113));
        assert!(super::resolve_attribute_use(&db, root, "Immutable").is_recognized());
    }

    fn nodes_of_kind(db: &dyn Db, key: AstNodeKey, kind: IndexedNodeKind, found: &mut Vec<AstNodeKey>) {
        if node_kind(db, key).expect("node kind") == Some(kind) {
            found.push(key);
        }
        if let Some(children) = child_nodes(db, key).expect("child nodes") {
            for child in children.iter().copied() {
                nodes_of_kind(db, child, kind, found);
            }
        }
    }

    /// Every finding of `check_items` over every function and method plus the substitutions of
    /// each call in the last function.
    fn method_generic_report(source: &str, generation: u64) -> (Vec<(String, String)>, Vec<String>) {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        let mut items = Vec::new();
        nodes_of_kind(&db, root, IndexedNodeKind::FunctionDefinition, &mut items);
        nodes_of_kind(&db, root, IndexedNodeKind::MethodDefinition, &mut items);
        let mut findings = super::check_items(&db, &items)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|finding| (finding.kind.code().to_string(), finding.kind.message()))
            .collect::<Vec<_>>();
        findings.extend(
            super::check_gate_obligations(&db, &items)
                .into_iter()
                .map(|finding| (finding.kind.code().to_string(), finding.kind.message())),
        );
        let last = *function_definitions(&db, root).last().expect("function");
        let mut calls = Vec::new();
        nodes_of_kind(&db, last, IndexedNodeKind::CallExpression, &mut calls);
        let substitutions = calls
            .into_iter()
            .map(|call| format!("{:?} => {}", call_lowering(&db, call).map(|lowering| lowering.map(|l| format!("{l:?}").chars().take(40).collect::<String>())), match generic_call_specialization(&db, call) {
                Ok(Some(specialization)) => specialization
                    .substitutions
                    .iter()
                    .map(|binding| format!("{}={}", binding.parameter, binding.argument.display_name()))
                    .collect::<Vec<_>>()
                    .join(","),
                Ok(None) => "none".to_string(),
                Err(error) => format!("error: {error}"),
            }))
            .collect();
        (findings, substitutions)
    }

    const BOX: &str = "pub type Box<T> { T item, \
        pub U Apply<U>((T) => U f) { return f(this.item); } \
        pub Box<U> Map<U>((T) => U f) { return Box<U> { item: f(this.item) }; } } ";

    const SCOPE: &str = "pub type Task<T> { T value } pub enum Outcome<T> { Done(T value), Failed(string reason) } \
        pub type Scope { i64 id, \
        pub Task<T> Spawn<T>(() => T body) { return Task<T> { value: body() }; } \
        pub i64 SpawnTry<T>(() => Outcome<T> body) { return 0_i64; } } ";
    const SEQ: &str = "pub contract Folder<T> { T Add(T a, T b); } \
        pub type AddAll : Folder<i64> { i64 unused, pub i64 Add(i64 a, i64 b) { return a + b; } } \
        pub type Plain { i64 x } \
        pub contract Shown { i64 Show(); } \
        pub type Label : Shown { i64 x, pub i64 Show() { return this.x; } } \
        pub type Cell<T> { T value, pub T Read() where T: Shown { return this.value; } } \
        pub type Seq<T> { T[] items, pub T Fold<C>(T seed, C combiner) where C: Folder<T> { return seed; } } \
        pub T Pick<T>(T a, T b) { return a; } ";
    const CLASH: &str = "pub type Clash<T> { T value, pub T Same<T>(T other) { return other; } } ";

    fn assert_method_call(source: &str, generation: u64, findings: &[&str], substitutions: &[&str]) {
        let (actual_findings, actual_substitutions) = method_generic_report(source, generation);
        let codes = actual_findings.iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>();
        assert_eq!(codes, findings, "{source}: {actual_findings:?}");
        for (expected, actual) in substitutions.iter().zip(actual_substitutions.iter()) {
            assert!(actual.ends_with(&format!("=> {expected}")), "{source}: expected {expected}, got {actual}");
        }
    }

    #[test]
    fn generic_methods_bind_explicit_type_arguments_on_paths_and_member_accesses() {
        let main = "i64 Main() { Box<bool> b = Box<bool> { item: true }; return b.Apply<i64>((bool s) => 3_i64); }";
        assert_method_call(&format!("{BOX}{main}"), 6200, &[], &["T=bool,U=i64"]);
        let chain =
            "Box<bool> Twice(Box<i64> b) { return b.Map<i64>((i64 x) => x * 2_i64).Map<bool>((i64 y) => y > 2_i64); }";
        assert_method_call(&format!("{BOX}{chain}"), 6201, &[], &["T=i64,U=bool", "T=i64,U=i64"]);
        let wrong = "i64 Main() { Box<bool> b = Box<bool> { item: true }; return b.Apply<i64, bool>((bool s) => 3_i64); }";
        assert_method_call(&format!("{BOX}{wrong}"), 6202, &["E1204"], &[]);
    }

    #[test]
    fn method_type_arguments_are_inferred_from_receiver_arguments_and_lambda_results() {
        for (offset, main) in [
            "i64 Main() { Box<bool> b = Box<bool> { item: true }; return b.Apply((bool s) => 3_i64); }",
            "i64 Main() { Box<bool> b = Box<bool> { item: true }; return b.Apply((s) => 3_i64); }",
            "i64 Main() { Box<bool> b = Box<bool> { item: true }; return b.Apply((s) => { return 7_i64; }); }",
        ]
        .iter()
        .enumerate()
        {
            assert_method_call(&format!("{BOX}{main}"), 6203 + offset as u64, &[], &["T=bool,U=i64"]);
        }
        let spawn = format!("{SCOPE}i64 Main(Scope s) {{ Task<i64> t = s.Spawn(() => 20_i64); return t.value; }}");
        assert_method_call(&spawn, 6300, &[], &["T=i64"]);
        let block = format!(
            "{SCOPE}i64 Main(Scope s) {{ Task<i64> t = s.Spawn(() => {{ i64 a = 3_i64; return 7_i64; }}); return t.value; }}"
        );
        assert_method_call(&block, 6301, &[], &["T=i64"]);
        let bare = format!("{SCOPE}i32 Main(Scope s) {{ Task<i32> t = s.Spawn(() => 20); return t.value; }}");
        assert_method_call(&bare, 6310, &[], &["T=i32"]);
        let fold = format!("{SEQ}i64 Main(Seq<i64> numbers) {{ return numbers.Fold(0_i64, AddAll {{ unused: 0_i64 }}); }}");
        assert_method_call(&fold, 6304, &[], &["T=i64,C=pointer"]);
    }

    #[test]
    fn method_type_argument_inference_failures_are_reported() {
        let unresolved = format!("{SCOPE}i64 Main(Scope s) {{ return s.SpawnTry(() => Outcome::Failed(\"no input\")); }}");
        let (findings, _) = method_generic_report(&unresolved, 6302);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].0, "E1203");
        assert!(findings[0].1.contains("`T`"), "{findings:?}");
        let explicit = format!("{SCOPE}i64 Main(Scope s) {{ return s.SpawnTry<i64>(() => Outcome::Failed(\"no input\")); }}");
        assert_method_call(&explicit, 6303, &[], &["T=i64"]);
        let conflict = format!("{SEQ}i64 Main() {{ return Pick(1_i64, true); }}");
        let (findings, _) = method_generic_report(&conflict, 6308);
        assert_eq!(findings.iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>(), ["E1229"]);
        assert!(findings[0].1.contains("i64") && findings[0].1.contains("bool"));
        let clash = format!("{CLASH}i64 Main(Clash<i64> c) {{ return c.Same(1_i64); }}");
        assert_method_call(&clash, 6309, &["E1229"], &[]);
    }

    const FOLD: &str = "pub type Numbers<T> { T[] items, \
        pub U Fold<U>(U seed, (U, T) => U step) { return seed; } \
        pub B Pipe<A, B>(() => A first, (A) => B second) { return second(first()); } } ";

    #[test]
    fn untyped_lambda_parameters_take_types_inferred_from_sibling_arguments() {
        // `acc` is `U`, bound only by the sibling `seed` argument; `x` is the receiver's `T`.
        let fold = format!("{FOLD}i64 Main(Numbers<bool> numbers) {{ return numbers.Fold(0_i64, (acc, x) => acc + 1_i64); }}");
        assert_method_call(&fold, 6330, &[], &["T=bool,U=i64"]);
        // The sibling may follow the lambda: non-lambda arguments are unified first.
        let pipe = format!("{FOLD}bool Main(Numbers<bool> numbers) {{ return numbers.Pipe(() => 3_i64, (a) => a > 2_i64); }}");
        assert_method_call(&pipe, 6331, &[], &["T=bool,A=i64,B=bool"]);
        // A receiver-supplied parameter used in the body types the result.
        let apply = format!("{BOX}bool Main() {{ Box<bool> b = Box<bool> {{ item: true }}; return b.Apply((s) => s); }}");
        assert_method_call(&apply, 6332, &[], &["T=bool,U=bool"]);
    }

    #[test]
    fn an_explicit_receiver_resolves_like_the_implicit_one() {
        let source = "pub attribute Receiver(ReceiverParameter) { } \
            pub type Counter { i64 count, pub i64 Read([Receiver] this) { return this.count; } \
            pub i64 Bump(this, i64 by) { return this.count + by; } } \
            i64 Main(Counter c) { return c.Read() + c.Bump(2_i64); }";
        let (findings, substitutions) = method_generic_report(source, 6320);
        assert!(findings.is_empty(), "{findings:?}");
        assert!(substitutions.iter().all(|call| call.starts_with("Ok(Some(\"Direct")), "{substitutions:?}");
    }

    #[test]
    fn method_where_bounds_are_proven_at_the_call() {
        let own = format!("{SEQ}i64 Main(Seq<i64> numbers) {{ return numbers.Fold<Plain>(0_i64, Plain {{ x: 1_i64 }}); }}");
        let (findings, _) = method_generic_report(&own, 6305);
        assert_eq!(findings.iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>(), ["E1610"]);
        assert!(findings[0].1.contains("Folder"));
        let owner = format!("{SEQ}i64 Main(Cell<Plain> c) {{ Plain p = c.Read(); return p.x; }}");
        let (findings, _) = method_generic_report(&owner, 6306);
        assert_eq!(findings.iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>(), ["E1610"]);
        assert!(findings[0].1.contains("Shown"));
        let satisfied = format!("{SEQ}i64 Main(Cell<Label> c) {{ Label l = c.Read(); return l.x; }}");
        assert_method_call(&satisfied, 6307, &[], &[]);
    }

    fn assert_immutable_assignment(source: &str, generation: u64, item: usize, name: &str) {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        let key = function_definitions(&db, root)[item];
        let finding = super::immutable_local_assignment(&db, key)
            .expect("immutable_local_assignment query")
            .unwrap_or_else(|| panic!("`{name}` reassignment must be reported: {source}"));
        assert_eq!(finding.name.as_ref(), name);
        let findings = super::check_items(&db, &[key]).expect_err("legality gate rejects the item");
        assert!(
            findings.iter().any(|finding| finding.kind.code() == "E1214"),
            "legality gate must carry E1214: {findings:?}"
        );
    }

    #[test]
    fn immutable_local_reassignment_is_reported_for_each_type_and_control_flow_shape() {
        let pair = "pub type Pair { i64 a } ";
        let cases = [
            ("unit Main(pointer p) { pointer q = p; q = p; return; }", "q"),
            ("unit Main(pointer p) { pointer q = p; while true { q = p; } return; }", "q"),
            ("unit Main(pointer p, bool c) { pointer q = p; if c { q = p; } return; }", "q"),
            ("unit Main(pointer p) { pointer q = p; for i in range(0, 4) { q = p; } return; }", "q"),
            ("unit Main() { i64 n = 0; n = 1; return; }", "n"),
            ("unit Main() { i64 n = 0; while n < 3 { n = n + 1; } return; }", "n"),
            ("unit Main(bool c) { i64 n = 0; if c { n = 1; } return; }", "n"),
            ("unit Main() { i64 n = 0; for i in range(0, 4) { n = 1; } return; }", "n"),
            ("unit Main(i64 n) { while true { n = 1; } return; }", "n"),
        ];
        for (offset, (source, name)) in cases.iter().enumerate() {
            assert_immutable_assignment(source, 60 + offset as u64, 0, name);
        }
        let struct_cases = [
            "unit Main() { Pair v = Pair { a: 1 }; v = Pair { a: 2 }; return; }",
            "unit Main() { Pair v = Pair { a: 1 }; while true { v = Pair { a: 2 }; } return; }",
            "unit Main(bool c) { Pair v = Pair { a: 1 }; if c { v = Pair { a: 2 }; } return; }",
            "unit Main() { Pair v = Pair { a: 1 }; for i in range(0, 4) { v = Pair { a: 2 }; } return; }",
        ];
        for (offset, body) in struct_cases.iter().enumerate() {
            assert_immutable_assignment(&format!("{pair}{body}"), 80 + offset as u64, 0, "v");
        }
    }

    #[test]
    fn mutable_local_reassignment_is_not_reported() {
        let source = "unit Main(mut i64 m, pointer p) { mut pointer q = p; mut i64 n = 0; \
                      while n < 3 { q = p; n = n + 1; m = n; } return; }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(90));
        let main_item = function_definitions(&db, root)[0];
        assert_eq!(super::immutable_local_assignment(&db, main_item).expect("immutable_local_assignment query"), None);
    }

    #[test]
    fn bulk_parameter_call_with_any_argument_count_is_not_reported() {
        // `scoped_cleanup_constructors.bd`'s `BulkSum(1_i64, { ... })` shape: lowering packs every
        // argument into the single `bulk i64[]` parameter, so N arguments against one declared
        // parameter is legal.
        let source = "i64 BulkSum(bulk i64[] values) { return values[0_i64] + values[1_i64]; } \
                      i64 Main() { return BulkSum(1_i64, 2_i64); }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(43));
        let main_item = function_definitions(&db, root)[1];

        assert_eq!(super::call_arity_mismatch(&db, main_item).expect("call_arity_mismatch query"), None);
        assert!(super::check_items(&db, &[main_item]).is_ok(), "bulk call must pass the legality gate");
    }

    fn member_finding(source: &str, generation: u64, item: usize) -> Option<MemberReferenceKind> {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        let key = function_definitions(&db, root)[item];
        super::member_reference_legality(&db, key).expect("member_reference_legality query").map(|finding| finding.kind)
    }

    const SHAPE: &str = "pub type Pair { i64 a } enum Shape { Dot, Circle(i64 radius) } ";

    #[test]
    fn unknown_members_and_arities_are_reported_against_the_resolved_declaration() {
        let cases = [
            (
                "i64 Main() { Pair p = Pair { b: 1 }; return 0; }",
                MemberReferenceKind::UnknownStructField { name: "b".into() },
            ),
            ("i64 Main(Pair p) { return p.b; }", MemberReferenceKind::UnknownStructField { name: "b".into() }),
            (
                "i64 Main() { Shape s = Shape::Square; return 0; }",
                MemberReferenceKind::UnknownEnumVariant { enum_name: "Shape".into(), variant: "Square".into() },
            ),
            (
                "i64 Main() { Shape s = Shape::Circle(); return 0; }",
                MemberReferenceKind::EnumConstructorArity { expected: 1, actual: 0 },
            ),
            (
                "i64 Main(Shape s) { return match s { Shape::Dot => 0, Shape::Circle(r, extra) => r, }; }",
                MemberReferenceKind::PatternArity { expected: 1, actual: 2 },
            ),
            (
                "i64 Main(Shape s) { return match s { Shape::Dot => 0, Shape::Square => 1, _ => 2, }; }",
                MemberReferenceKind::UnknownEnumVariant { enum_name: "Shape".into(), variant: "Square".into() },
            ),
        ];
        for (offset, (body, expected)) in cases.into_iter().enumerate() {
            let source = format!("{SHAPE}{body}");
            assert_eq!(member_finding(&source, 100 + offset as u64, 0), Some(expected), "{source}");
        }
    }

    #[test]
    fn well_formed_members_are_not_reported() {
        let cases = [
            "i64 Main() { Pair p = Pair { a: 1 }; return p.a; }",
            "i64 Main() { Shape s = Shape::Circle(2); Shape d = Shape::Dot; return 0; }",
            "i64 Main(Shape s) { return match s { Shape::Dot => 0, Shape::Circle(r) => r, }; }",
        ];
        for (offset, body) in cases.into_iter().enumerate() {
            let source = format!("{SHAPE}{body}");
            assert_eq!(member_finding(&source, 110 + offset as u64, 0), None, "{source}");
            let (db, root) = one_unit_assembly(&source, SyntaxGenerationId(120 + offset as u64));
            let key = function_definitions(&db, root)[0];
            assert_eq!(super::match_exhaustiveness(&db, key).expect("match_exhaustiveness query"), None, "{source}");
        }
    }

    #[test]
    fn match_without_an_arm_for_a_variant_is_reported_non_exhaustive() {
        let source = format!("{SHAPE}i64 Main(Shape s) {{ return match s {{ Shape::Dot => 0, }}; }}");
        let (db, root) = one_unit_assembly(&source, SyntaxGenerationId(130));
        let key = function_definitions(&db, root)[0];
        let finding = super::match_exhaustiveness(&db, key).expect("query").expect("`Circle` has no arm");
        assert_eq!(finding.enum_name.as_ref(), "Shape");
        assert_eq!(finding.missing_variant.as_ref(), "Circle");
        let findings = super::check_items(&db, &[key]).expect_err("legality gate rejects the match");
        assert!(findings.iter().any(|finding| finding.kind.code() == "E1304"), "{findings:?}");

        let wildcard = format!("{SHAPE}i64 Main(Shape s) {{ return match s {{ Shape::Dot => 0, _ => 1, }}; }}");
        let (db, root) = one_unit_assembly(&wildcard, SyntaxGenerationId(131));
        let key = function_definitions(&db, root)[0];
        assert_eq!(super::match_exhaustiveness(&db, key).expect("query"), None);
    }

    /// `check` agrees with ISLE: a refutable payload pattern (a literal, or a nested enum pattern
    /// that does not cover its enum) does not cover its variant, and the finding names one
    /// uncovered value pattern.
    #[test]
    fn refutable_payload_patterns_do_not_cover_their_variant() {
        let cases = [
            (
                "enum Result { Ok(i64 value), Error(i64 error) } \
                 i64 Main(Result r) { return match r { Result::Ok(7_i64) => 1_i64, Result::Error(_) => 0_i64, }; }",
                "Result::Ok(_)",
            ),
            (
                "enum Bit { Zero, One } enum Pair { Both(Bit left, Bit right), Empty } \
                 i64 Main(Pair pair) { return match pair { Pair::Both(Bit::Zero, _) => 0_i64, Pair::Empty => 2_i64, }; }",
                "Pair::Both(Bit::One, _)",
            ),
            (
                "enum Choice { Value(i64 value), Empty } \
                 i64 Main(Choice choice) { return match choice { Choice::Value(v) when v > 0_i64 => v, Choice::Empty => 0_i64, }; }",
                "Choice::Value(_)",
            ),
        ];
        for (offset, (source, missing)) in cases.into_iter().enumerate() {
            let (db, root) = one_unit_assembly(source, SyntaxGenerationId(140 + offset as u64));
            let key = function_definitions(&db, root)[0];
            let finding = super::match_exhaustiveness(&db, key).expect("query").expect(source);
            assert_eq!(finding.missing_pattern.as_ref(), missing, "{source}");
            let findings = super::check_items(&db, &[key]).expect_err("the legality gate rejects the match");
            let rejection = findings.iter().find(|finding| finding.kind.code() == "E1304").expect(source);
            assert!(rejection.kind.message().contains(missing), "{}", rejection.kind.message());
        }
    }

    /// A variant is covered by a catch-all sub-pattern after refutable ones, or by nested enum
    /// patterns that together cover the nested enum.
    #[test]
    fn irrefutable_or_collectively_exhaustive_payload_patterns_cover_their_variant() {
        let cases = [
            "enum Result { Ok(i64 value), Error(i64 error) } \
             i64 Main(Result r) { return match r { Result::Ok(7_i64) => 1_i64, Result::Ok(_) => 2_i64, Result::Error(_) => 0_i64, }; }",
            "enum Bit { Zero, One } enum Pair { Both(Bit left, Bit right), Empty } \
             i64 Main(Pair pair) { return match pair { Pair::Both(Bit::Zero, _) => 0_i64, Pair::Both(Bit::One, other) => 1_i64, Pair::Empty => 2_i64, }; }",
            "enum Choice { Value(i64 value), Empty } \
             i64 Main(Choice choice) { return match choice { Choice::Value(v) when v > 0_i64 => v, Choice::Value(v) => 0_i64, Choice::Empty => 0_i64, }; }",
        ];
        for (offset, source) in cases.into_iter().enumerate() {
            let (db, root) = one_unit_assembly(source, SyntaxGenerationId(150 + offset as u64));
            let key = function_definitions(&db, root)[0];
            assert_eq!(super::match_exhaustiveness(&db, key).expect("query"), None, "{source}");
        }
    }

    #[test]
    fn direct_call_with_matching_arguments_is_not_reported() {
        let source = "unit Helper(i32 a, i32 b) { return; } unit Main() { Helper(1, 2); return; }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(42));
        let main_item = function_definitions(&db, root)[1];

        assert_eq!(super::call_arity_mismatch(&db, main_item).expect("call_arity_mismatch query"), None);
    }

    const EQUAL: &str = "i64 AsI64() { return 1_i64; } word AsWord() { return 1_word; } \
                         unit Equal<T>(T actual, T expected) { return; } ";

    #[test]
    fn generic_parameter_bound_to_two_types_by_one_call_is_reported_at_the_call() {
        let source = format!("{EQUAL}unit Main() {{ Equal(AsI64(), AsWord()); return; }}");
        let (db, root) = one_unit_assembly(&source, SyntaxGenerationId(140));
        let main = function_definitions(&db, root)[3];
        let finding = super::generic_parameter_conflict(&db, main).expect("query").expect("T is bound twice");
        assert_eq!(finding.conflict.parameter.as_ref(), "T");
        assert_eq!(finding.conflict.first.as_ref(), "i64");
        assert_eq!(finding.conflict.second.as_ref(), "word");
        assert_eq!(node_kind(&db, finding.call).expect("kind"), Some(IndexedNodeKind::CallExpression));
        let findings = super::check_items(&db, &[main]).expect_err("legality gate rejects the call");
        assert_eq!(findings.iter().filter(|finding| finding.kind.code() == "E1229").count(), 1, "{findings:?}");
    }

    #[test]
    fn consistent_generic_bindings_and_generic_bodies_are_not_reported() {
        let cases = [
            "unit Main() { Equal(AsI64(), 2_i64); return; }",
            "unit Main() { Equal(AsWord(), AsWord()); return; }",
            // A bare integer literal inherits the binding of the other argument.
            "unit Main() { Equal(AsI64(), 3); return; }",
            // A generic body is specialized through its callers' environment, not judged alone.
            "unit Forward<U>(U first, U second) { Equal(first, second); return; } unit Main() { Forward(1_i64, 2_i64); return; }",
        ];
        for (offset, body) in cases.into_iter().enumerate() {
            let source = format!("{EQUAL}{body}");
            let (db, root) = one_unit_assembly(&source, SyntaxGenerationId(141 + offset as u64));
            for item in function_definitions(&db, root) {
                assert_eq!(super::generic_parameter_conflict(&db, item).expect("query"), None, "{source}");
            }
        }
    }
}
