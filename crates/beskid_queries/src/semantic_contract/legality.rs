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

mod attributes;
pub(super) mod calls;
mod captures;
mod cleanup;
mod copy_on_assign;
mod externs;
mod function_values;
mod gate;
mod generics;
mod imports;
mod members;
mod ownership;
mod transfer;
mod typing;
mod units;
mod value_paths;
mod values;

pub use attributes::{
    AttributeResolution, UnitAttributeDeclaration, UnitAttributeSurface, UnitAttributeUse, attribute_findings,
    check_attribute_obligations, resolve_attribute_use, unit_attribute_surface,
};
pub use captures::{BoundedLambdaCaptures, bounded_lambda_captures, declared_captures_bound};
pub use externs::extern_profile_findings;
pub use gate::{GateObligation, GateObligationKind, check_gate_obligations, gate_obligations};
pub use transfer::{immutable_field_findings, transfer_declaration_findings};
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
    MemberReferenceFinding, MemberReferenceKind, NonExhaustiveMatch, UnknownMember, match_exhaustiveness,
    member_reference_legality,
};

/// A direct call whose supplied argument count does not match its resolved declaration's
/// parameter count (plus one for an implicit or explicit method receiver).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CallArityMismatch {
    /// The call expression (the diagnostic site).
    pub call: AstNodeKey,
    /// Arguments the call must pass: the parameters without a default value.
    pub required: usize,
    /// Arguments the call may pass: every parameter.
    pub total: usize,
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
    let total = parameters.len() + receiver;
    let Some(arguments) = call_arguments(db, call)? else { return Ok(None) };
    let actual = arguments.len();
    if actual == total {
        return Ok(None);
    }
    // `build_typed_program` fills omitted optional arguments once callees are known
    // (`optional_arguments`), so a call that reaches this check with fewer than every argument
    // was either short of the required ones or could not be filled; the honest expectation for
    // the latter is every argument. A method inherits the defaults of the contract method it
    // implements.
    let optional =
        super::optional_arguments::declared_omitted_arguments(db, declaration).map_or(0, |(omitted, _)| omitted.len());
    let required = parameters.len() - optional + receiver;
    let required = if (required..total).contains(&actual) { total } else { required };
    Ok(Some(CallArityMismatch { call, required, total, actual }))
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

/// An immutable local -- a `let` without `mut`, a parameter without `mut` or `move`, or a `for`
/// iterator -- and every assignment in the item that writes it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ImmutableLocalAssignment {
    /// The binding's name in its declaration (the diagnostic site, where the `mut` fix goes).
    pub declaration: AstNodeKey,
    /// The immutable local's name.
    pub name: Arc<str>,
    /// The declaration form, span, and `mut` insertion point.
    pub binding: beskid_analysis::analysis::diagnostic_kinds::ImmutableBinding,
    /// Every assignment expression that writes the binding, in source order.
    pub assignments: Vec<AstNodeKey>,
}

/// Report every immutable local the item `key` assigns (E1214), one entry per binding in
/// declaration-site order of first assignment.
///
/// The target resolves through `resolve_lexical_declaration`, the same lexical authority
/// `mutable_local_assignment` uses when ISLE selects `emit_local_assign`. That query fails closed
/// on an immutable target, so without this fact such a write reaches lowering and surfaces as
/// `MissingRuleOrFact AssignExpression` instead of a user diagnostic.
pub fn immutable_local_assignment(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Vec<ImmutableLocalAssignment>> {
    with_registered_syntax(db, key, immutable_local_assignment_tracked)
}

#[salsa::tracked(persist)]
fn immutable_local_assignment_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Vec<ImmutableLocalAssignment>> {
    let findings = with_node(db, syntax, key, |program, index, _node| {
        let mut assignments = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::AssignExpression, &mut assignments);
        assignments.sort_unstable();
        let mut grouped: Vec<ImmutableLocalAssignment> = Vec::new();
        for assignment in assignments {
            let Some((declaration, name, binding)) = immutable_assignment_target(program, index, assignment) else {
                continue;
            };
            let site = AstNodeKey { node: assignment, ..key };
            let declaration = AstNodeKey { node: declaration, ..key };
            match grouped.iter_mut().find(|group| group.declaration == declaration) {
                Some(group) => group.assignments.push(site),
                None => grouped.push(ImmutableLocalAssignment {
                    declaration,
                    name: Arc::from(name),
                    binding,
                    assignments: vec![site],
                }),
            }
        }
        (!grouped.is_empty()).then_some(grouped)
    })?;
    Ok(findings)
}

fn immutable_assignment_target<'p>(
    program: &'p beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    assignment: beskid_analysis::syntax::AstNodeId,
) -> Option<(beskid_analysis::syntax::AstNodeId, &'p str, beskid_analysis::analysis::diagnostic_kinds::ImmutableBinding)>
{
    use beskid_analysis::analysis::diagnostic_kinds::ImmutableBinding;
    let node = index.node_at(program, assignment)?;
    let assign = node.of::<beskid_analysis::syntax::AssignExpression>()?;
    let beskid_analysis::syntax::Expression::Path(path) = &assign.target.node else { return None };
    let [segment] = path.node.path.node.segments.as_slice() else { return None };
    let name = segment.node.name.node.name.as_str();
    let declaration = resolve_lexical_declaration(program, index, assignment, name)?;
    let declaring = parent_node(index, declaration)?;
    let declaring_node = index.node_at(program, declaring)?;
    let binding = match index.kind(declaring)? {
        NodeKind::LetStatement | NodeKind::Parameter if local_declaration_is_mutable(program, index, declaration) => {
            return None;
        }
        NodeKind::LetStatement => ImmutableBinding::of_let(
            declaring_node.of::<beskid_analysis::syntax::LetStatement>()?,
            declaring_node.span()?,
        ),
        NodeKind::Parameter => {
            let parameter = declaring_node.of::<beskid_analysis::syntax::Parameter>()?;
            let span = declaring_node.span()?;
            ImmutableBinding {
                form: beskid_analysis::analysis::diagnostic_kinds::BindingForm::Parameter,
                declaration: span,
                mut_at: parameter.ty.span.start,
            }
        }
        NodeKind::ForStatement => {
            let iterator = index.node_at(program, declaration)?.span()?;
            ImmutableBinding::of_for_iterator(iterator)
        }
        _ => return None,
    };
    Some((declaration, name, binding))
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
        findings.extend(transfer::immutable_field_findings(db, item));
        if let Ok(Some(finding)) = unresolved_value_argument(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::ResolveUnknownValue {
                    name: finding.name.to_string(),
                    suggestion: finding.suggestion.as_deref().map(str::to_string),
                },
                site: finding.site,
                related: Vec::new(),
            });
        }
        if let Ok(Some(finding)) = unresolved_type_reference(db, item) {
            let kind = match finding.kind {
                UnresolvedTypeKind::UnknownType => {
                    SemanticIssueKind::TypeUnknownType { name: finding.name.to_string() }
                }
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
        if let Ok(Some(bindings)) = immutable_local_assignment(db, item) {
            findings.extend(bindings.into_iter().map(|binding| SemanticFinding {
                kind: SemanticIssueKind::ImmutableAssignment {
                    name: binding.name.to_string(),
                    binding: binding.binding,
                },
                site: binding.declaration,
                related:
                    binding.assignments.into_iter().map(|assignment| (assignment, "assigned again".into())).collect(),
            }));
        }
        if let Ok(Some(finding)) = unresolved_call_target(db, item) {
            let kind = match finding.kind {
                UnresolvedCallKind::UnknownValue { name, suggestion } => SemanticIssueKind::ResolveUnknownValue {
                    name: name.to_string(),
                    suggestion: suggestion.as_deref().map(str::to_string),
                },
                UnresolvedCallKind::UnknownModulePath { path } => {
                    SemanticIssueKind::ResolveUnknownModulePath { path: path.to_string() }
                }
                UnresolvedCallKind::MissingTypeArguments => {
                    SemanticIssueKind::TypeMissingTypeArguments { parameter: None }
                }
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
                MemberReferenceKind::UnknownStructField(member) => member.issue(),
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
        findings.extend(ownership::affine_findings(db, item));
        findings.extend(captures::captures_argument_findings(db, item));
        findings.extend(captures::isolate_spawn_findings(db, item));
        findings.extend(captures::repeated_capture_argument_findings(db, item));
        findings.extend(function_values::generic_function_value_semantic_findings(db, item));
        if let Ok(Some(finding)) = call_argument_type_mismatch(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::TypeCallArgumentMismatch {
                    expected_name: finding.expected.to_string(),
                    actual_name: finding.actual.to_string(),
                },
                site: finding.argument,
                related: vec![(finding.call, "in this call".into())],
            });
        }
        if let Ok(Some(finding)) = call_arity_mismatch(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::TypeCallArityMismatch {
                    required: finding.required,
                    total: finding.total,
                    actual: finding.actual,
                },
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

/// For a `Serializable` bound violation at `site`: the start of the conforming type's declaration
/// when the site's own unit declares exactly one `type` or `enum` of that name at top level, where
/// the `[Serialize]` fix inserts the attribute. `None` for any other bound or an ambiguous name.
pub(super) fn serialize_declaration_at(
    db: &dyn Db,
    site: AstNodeKey,
    type_name: &str,
    contract_name: &str,
) -> Option<usize> {
    let outer = |name: &str| -> String {
        let head = name.split('<').next().unwrap_or(name).trim();
        head.rsplit(['.', ':']).next().unwrap_or(head).to_string()
    };
    if outer(contract_name) != "Serializable" {
        return None;
    }
    let wanted = outer(type_name);
    let syntax = db.syntax_unit(site.unit).filter(|syntax| syntax.accepts_key(db, site))?;
    let program = syntax.expanded_program(db);
    let mut declarations = program.node.items.iter().filter_map(|item| match &item.node {
        beskid_analysis::syntax::Node::TypeDefinition(definition) if definition.node.name.node.name == wanted => {
            Some(item.span.start)
        }
        beskid_analysis::syntax::Node::EnumDefinition(definition) if definition.node.name.node.name == wanted => {
            Some(item.span.start)
        }
        _ => None,
    });
    let first = declarations.next()?;
    declarations.next().is_none().then_some(first)
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
        findings.extend(transfer::transfer_declaration_findings(db, unit_root));
        findings.extend(copy_on_assign::copy_on_assign_findings(db, unit_root));
        findings.extend(captures::captures_declaration_findings(db, unit_root));
        if let Ok(Some(imports)) = unresolved_imports(db, unit_root) {
            findings.extend(imports.iter().map(|import| SemanticFinding {
                kind: SemanticIssueKind::UnknownImportPath { path: import.path.to_string(), module: None },
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
        unit_assembly_at(source, generation, "Main.bd")
    }

    /// One unit at `relative` under the source root, so its logical module follows the path
    /// (`Core/Parallel/Seq.bd` is module `Core.Parallel.Seq`).
    fn unit_assembly_at(source: &str, generation: SyntaxGenerationId, relative: &str) -> (BeskidDatabase, AstNodeKey) {
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let source_path = directory.join(relative);
        std::fs::create_dir_all(source_path.parent().expect("parent")).expect("module directory");
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
                logical_name: relative.trim_end_matches(".bd").replace('/', ".").into(),
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

    const QUERY_SHAPE: &str = "type NodeRef { u64 issuer; u64 node; } type NodeSpan { u64 start; u64 end; } \
        mod Query { pub type SyntaxQuery { NodeRef start; u64 bound; } \
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
            "{QUERY_SHAPE}contract Sized {{ u64 Size(); }} type Box : Sized {{ u64 width; pub u64 Size() {{ return width; }} }} \
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
        let source = "type Point { i64 x; } unit Len(string text) { return; } unit Show(Point point) { return; } \
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
        assert_eq!((mismatch.required, mismatch.total, mismatch.actual), (2, 2, 1));
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
        let findings = attribute_codes("pub attribute Sealed { } [Sealed] pub type Door { i64 width; }", 6103);
        assert_eq!(findings.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["E1510"]);
        // Field attribute on a type, then on a field.
        let findings =
            attribute_codes("pub attribute FluentInner(Field) { } [FluentInner] pub type Wrapper { i64 inner; }", 6104);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].1.contains("TypeDefinition"), "{findings:?}");
        assert!(
            attribute_codes("pub attribute FluentInner(Field) { } pub type Wrapper { [FluentInner] i64 inner; }", 6105)
                .is_empty()
        );
        // Undeclared.
        let findings = attribute_codes("[Frozen] pub type Ice { i64 temperature; }", 6106);
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
                      pub type Counter { i64 count; pub i64 Read([Receiver] this) { return this.count; } } \
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
        let (db, root) = one_unit_assembly("pub type Point { [Immutable] i64 x; i64 y; }", SyntaxGenerationId(6110));
        let finding = super::attribute_findings(&db, root).pop().expect("E1510");
        let SemanticIssueKind::AttributeTargetNotAllowed { allowed, declared, .. } = finding.kind else { panic!() };
        assert!(declared);
        assert_eq!(allowed, ["TypeDefinition"]);
        assert!(attribute_codes("[Export(Abi:\"C\")] pub i64 Answer() { return 42; }", 6111).is_empty());
    }

    /// v0.6.7 ruling "Export entries resolve by declaration identity": an `[Export]` use is the
    /// compiler-recognized declaration, not any attribute spelled `Export`.
    #[test]
    fn export_resolves_to_the_recognized_declaration_not_by_name() {
        let export_of = |source: &str, generation: u64| {
            let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
            let mut items = Vec::new();
            nodes_of_kind(&db, root, IndexedNodeKind::FunctionDefinition, &mut items);
            items
                .into_iter()
                .filter_map(|item| crate::item_export_symbol(&db, item).expect("export symbol query"))
                .map(|symbol| symbol.0.to_string())
                .collect::<Vec<_>>()
        };
        let exported =
            "[Export(Abi:\"C\", Symbol:\"answer\")] pub i64 Answer() { return 42; } pub i64 Other() { return 1; }";
        assert_eq!(export_of(exported, 6114), ["answer"]);
        let shadowed = format!("pub attribute Export(FunctionDefinition) {{ }} {exported}");
        assert!(export_of(&shadowed, 6115).is_empty(), "a user `Export` declaration exports nothing");
    }

    #[test]
    fn a_user_declaration_shadows_a_recognized_attribute() {
        let source = "pub attribute Immutable(Field) { } pub type Point { [Immutable] i64 x; }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(6112));
        assert!(super::attribute_findings(&db, root).is_empty());
        assert!(!super::resolve_attribute_use(&db, root, "Immutable").is_recognized());
        let (db, root) = one_unit_assembly("pub type Point { i64 x; }", SyntaxGenerationId(6113));
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
            .map(|call| {
                format!(
                    "{:?} => {}",
                    call_lowering(&db, call)
                        .map(|lowering| lowering.map(|l| format!("{l:?}").chars().take(40).collect::<String>())),
                    match generic_call_specialization(&db, call) {
                        Ok(Some(specialization)) => specialization
                            .substitutions
                            .iter()
                            .map(|binding| format!("{}={}", binding.parameter, binding.argument.display_name()))
                            .collect::<Vec<_>>()
                            .join(","),
                        Ok(None) => "none".to_string(),
                        Err(error) => format!("error: {error}"),
                    }
                )
            })
            .collect();
        (findings, substitutions)
    }

    const BOX: &str = "pub type Box<T> { T item; \
        pub U Apply<U>((T) => U f) { return f(this.item); } \
        pub Box<U> Map<U>((T) => U f) { return Box<U> { item: f(this.item) }; } } ";

    const SCOPE: &str = "pub type Task<T> { T value; } pub enum Outcome<T> { Done(T value), Failed(string reason) } \
        pub type Scope { i64 id; \
        pub Task<T> Spawn<T>(() => T body) { return Task<T> { value: body() }; } \
        pub i64 SpawnTry<T>(() => Outcome<T> body) { return 0_i64; } } ";
    const SEQ: &str = "pub contract Folder<T> { T Add(T a, T b); } \
        pub type AddAll : Folder<i64> { i64 unused; pub i64 Add(i64 a, i64 b) { return a + b; } } \
        pub type Plain { i64 x; } \
        pub contract Shown { i64 Show(); } \
        pub type Label : Shown { i64 x; pub i64 Show() { return this.x; } } \
        pub type Cell<T> { T value; pub T Read() where T: Shown { return this.value; } } \
        pub type Seq<T> { T[] items; pub T Fold<C>(T seed, C combiner) where C: Folder<T> { return seed; } } \
        pub T Pick<T>(T a, T b) { return a; } ";
    const CLASH: &str = "pub type Clash<T> { T value; pub T Same<T>(T other) { return other; } } ";

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
        let wrong =
            "i64 Main() { Box<bool> b = Box<bool> { item: true }; return b.Apply<i64, bool>((bool s) => 3_i64); }";
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
        let fold =
            format!("{SEQ}i64 Main(Seq<i64> numbers) {{ return numbers.Fold(0_i64, AddAll {{ unused: 0_i64 }}); }}");
        assert_method_call(&fold, 6304, &[], &["T=i64,C=pointer"]);
    }

    #[test]
    fn method_type_argument_inference_failures_are_reported() {
        let unresolved =
            format!("{SCOPE}i64 Main(Scope s) {{ return s.SpawnTry(() => Outcome::Failed(\"no input\")); }}");
        let (findings, _) = method_generic_report(&unresolved, 6302);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].0, "E1203");
        assert!(findings[0].1.contains("`T`"), "{findings:?}");
        let explicit =
            format!("{SCOPE}i64 Main(Scope s) {{ return s.SpawnTry<i64>(() => Outcome::Failed(\"no input\")); }}");
        assert_method_call(&explicit, 6303, &[], &["T=i64"]);
        let conflict = format!("{SEQ}i64 Main() {{ return Pick(1_i64, true); }}");
        let (findings, _) = method_generic_report(&conflict, 6308);
        assert_eq!(findings.iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>(), ["E1229"]);
        assert!(findings[0].1.contains("i64") && findings[0].1.contains("bool"));
        let clash = format!("{CLASH}i64 Main(Clash<i64> c) {{ return c.Same(1_i64); }}");
        assert_method_call(&clash, 6309, &["E1229"], &[]);
    }

    const FOLD: &str = "pub type Numbers<T> { T[] items; \
        pub U Fold<U>(U seed, (U, T) => U step) { return seed; } \
        pub B Pipe<A, B>(() => A first, (A) => B second) { return second(first()); } } ";

    #[test]
    fn untyped_lambda_parameters_take_types_inferred_from_sibling_arguments() {
        // `acc` is `U`, bound only by the sibling `seed` argument; `x` is the receiver's `T`.
        let fold =
            format!("{FOLD}i64 Main(Numbers<bool> numbers) {{ return numbers.Fold(0_i64, (acc, x) => acc + 1_i64); }}");
        assert_method_call(&fold, 6330, &[], &["T=bool,U=i64"]);
        // The sibling may follow the lambda: non-lambda arguments are unified first.
        let pipe =
            format!("{FOLD}bool Main(Numbers<bool> numbers) {{ return numbers.Pipe(() => 3_i64, (a) => a > 2_i64); }}");
        assert_method_call(&pipe, 6331, &[], &["T=bool,A=i64,B=bool"]);
        // A receiver-supplied parameter used in the body types the result.
        let apply =
            format!("{BOX}bool Main() {{ Box<bool> b = Box<bool> {{ item: true }}; return b.Apply((s) => s); }}");
        assert_method_call(&apply, 6332, &[], &["T=bool,U=bool"]);
    }

    #[test]
    fn an_explicit_receiver_resolves_like_the_implicit_one() {
        let source = "pub attribute Receiver(ReceiverParameter) { } \
            pub type Counter { i64 count; pub i64 Read([Receiver] this) { return this.count; } \
            pub i64 Bump(this, i64 by) { return this.count + by; } } \
            i64 Main(Counter c) { return c.Read() + c.Bump(2_i64); }";
        let (findings, substitutions) = method_generic_report(source, 6320);
        assert!(findings.is_empty(), "{findings:?}");
        assert!(substitutions.iter().all(|call| call.starts_with("Ok(Some(\"Direct")), "{substitutions:?}");
    }

    #[test]
    fn method_where_bounds_are_proven_at_the_call() {
        let own =
            format!("{SEQ}i64 Main(Seq<i64> numbers) {{ return numbers.Fold<Plain>(0_i64, Plain {{ x: 1_i64 }}); }}");
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
        let bindings = super::immutable_local_assignment(&db, key)
            .expect("immutable_local_assignment query")
            .unwrap_or_else(|| panic!("`{name}` reassignment must be reported: {source}"));
        assert_eq!(bindings.len(), 1, "{bindings:?}");
        let finding = &bindings[0];
        assert_eq!(finding.name.as_ref(), name);
        assert!(!finding.assignments.is_empty());
        let findings = super::check_items(&db, &[key]).expect_err("legality gate rejects the item");
        assert!(
            findings.iter().any(|finding| finding.kind.code() == "E1214"),
            "legality gate must carry E1214: {findings:?}"
        );
    }

    #[test]
    fn immutable_local_reassignment_is_reported_for_each_type_and_control_flow_shape() {
        let pair = "pub type Pair { i64 a; } ";
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
    fn a_copy_on_assign_type_without_a_valid_copy_hook_is_e1512_at_the_attribute() {
        let cases = [
            ("[CopyOnAssign] pub type Bag { i64 n; } unit Main() { return; }", "it declares no `Copy` method"),
            (
                "[CopyOnAssign] pub type Bag { i64 n; pub Bag Copy(i64 extra) { return Bag { n: this.n }; } } unit Main() { return; }",
                "`Copy` must take no parameters",
            ),
            (
                "[CopyOnAssign] pub type Bag { i64 n; pub i64 Copy() { return this.n; } } unit Main() { return; }",
                "`Copy` must return the type itself",
            ),
            (
                "[CopyOnAssign] pub type Bag { i64 n; pub This Copy() { return Bag { n: this.n }; } \
                 pub i64 Borrow() { return this.n; } } unit Main() { return; }",
                "`Borrow` must return the type itself",
            ),
            (
                "[CopyOnAssign] pub type Bag { i64 n; pub This Copy() { return Bag { n: this.n }; } \
                 pub This Detached(mut this) { return Bag { n: this.n }; } } unit Main() { return; }",
                "`Detached` must take a borrowed receiver, not `mut this` or `move this`",
            ),
        ];
        for (offset, (source, problem)) in cases.iter().enumerate() {
            let (db, root) = one_unit_assembly(source, SyntaxGenerationId(150 + offset as u64));
            let main = function_definitions(&db, root)[0];
            let findings = super::check_items(&db, &[main]).expect_err("the copy hook is invalid");
            let finding = findings
                .iter()
                .find(|finding| finding.kind.code() == "E1512")
                .unwrap_or_else(|| panic!("E1512 expected: {findings:?}\n{source}"));
            assert!(
                matches!(&finding.kind, SemanticIssueKind::CopyOnAssignWithoutCopy { type_name, problem: found }
                    if type_name == "Bag" && found == problem),
                "{:?}",
                finding.kind
            );
            let span = crate::node_span(&db, finding.site).expect("span").expect("site span");
            assert!(source[span.start..span.end].contains("CopyOnAssign"), "anchored at the attribute");
        }
        let valid = "[CopyOnAssign] pub type Bag { i64 n; pub This Copy() { return Bag { n: this.n }; } \
                     pub This Borrow() { return Bag { n: this.n }; } pub This Detached() { return Bag { n: this.n }; } } \
                     unit Main() { return; }";
        let (db, root) = one_unit_assembly(valid, SyntaxGenerationId(160));
        let main = function_definitions(&db, root)[0];
        let findings = super::check_items(&db, &[main]).err().unwrap_or_default();
        assert!(findings.iter().all(|finding| finding.kind.code() != "E1512"), "{findings:?}");
    }

    #[test]
    fn repeated_assignments_to_one_binding_are_one_finding_at_the_declaration() {
        let source = "unit Main() { i64 n = 0; n = 1; n = 2; i64 m = 0; m = 3; return; }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(95));
        let key = function_definitions(&db, root)[0];
        let findings = super::check_items(&db, &[key]).expect_err("legality gate rejects the item");
        let immutable: Vec<_> = findings.iter().filter(|finding| finding.kind.code() == "E1214").collect();
        assert_eq!(immutable.len(), 2, "one finding per binding: {immutable:?}");
        let n = immutable[0];
        assert_eq!(n.related.len(), 2);
        assert!(n.related.iter().all(|(_, label)| *label == "assigned again"));
        let span = crate::node_span(&db, n.site).expect("span").expect("site span");
        assert_eq!(&source[span.start..span.end], "n", "anchored at the declaration name");
        let SemanticIssueKind::ImmutableAssignment { binding, .. } = &n.kind else { panic!("{:?}", n.kind) };
        assert_eq!(binding.form, beskid_analysis::analysis::diagnostic_kinds::BindingForm::TypedLocal);
        assert_eq!(&source[binding.declaration.start..binding.declaration.end], "i64 n = 0;");
        assert_eq!(binding.mut_at, source.find("i64 n").unwrap());
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

    const SHAPE: &str = "pub type Pair { i64 a; } enum Shape { Dot, Circle(i64 radius) } ";

    /// `name` judged against `Pair { i64 a }`: one edit from `a`, so `a` is suggested.
    fn pair_member(name: &str) -> UnknownMember {
        UnknownMember {
            name: name.into(),
            type_name: Some("Pair".into()),
            fields: vec!["a".into()],
            suggestion: Some("a".into()),
        }
    }

    #[test]
    fn an_unknown_field_names_the_type_lists_its_fields_and_suggests_the_closest() {
        let source = "pub type Rectangle { i64 width; i64 height; } i64 Main(Rectangle r) { return r.heigth; }";
        let Some(MemberReferenceKind::UnknownStructField(member)) = member_finding(source, 140, 0) else {
            panic!("`heigth` is unknown")
        };
        assert_eq!(member.type_name.as_deref(), Some("Rectangle"));
        assert_eq!(member.fields.iter().map(|field| field.as_ref()).collect::<Vec<_>>(), ["width", "height"]);
        assert_eq!(member.suggestion.as_deref(), Some("height"));
        assert_eq!(member.issue().message(), "type `Rectangle` has no field `heigth`");
    }

    #[test]
    fn unknown_members_and_arities_are_reported_against_the_resolved_declaration() {
        let cases = [
            (
                "i64 Main() { Pair p = Pair { b: 1 }; return 0; }",
                MemberReferenceKind::UnknownStructField(pair_member("b")),
            ),
            ("i64 Main(Pair p) { return p.b; }", MemberReferenceKind::UnknownStructField(pair_member("b"))),
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

    const MARKERS: &str = "[CompilerDerived] pub contract Transferable { } [CompilerDerived] pub contract Shared { } \
        [CompilerDerived] pub contract Moved { } [CompilerDerived] pub contract Copied { } ";

    /// `(code, message, help)` of every declaration, legality-gate and gate-obligation finding.
    fn transfer_report(source: &str, generation: u64) -> Vec<(String, String, String)> {
        transfer_report_at(source, generation, "Main.bd")
    }

    fn transfer_report_at(source: &str, generation: u64, relative: &str) -> Vec<(String, String, String)> {
        let (db, root) = unit_assembly_at(source, SyntaxGenerationId(generation), relative);
        let mut items = Vec::new();
        nodes_of_kind(&db, root, IndexedNodeKind::FunctionDefinition, &mut items);
        nodes_of_kind(&db, root, IndexedNodeKind::MethodDefinition, &mut items);
        let mut findings = super::transfer_declaration_findings(&db, root);
        findings.extend(super::check_items(&db, &items).err().unwrap_or_default());
        findings.extend(super::check_gate_obligations(&db, &items));
        findings
            .into_iter()
            .filter(|finding| finding.kind.code() != "E1510" || !finding.kind.message().contains("no declaration"))
            .map(|finding| {
                (finding.kind.code().to_string(), finding.kind.message(), finding.kind.help().unwrap_or_default())
            })
            .collect()
    }

    fn type_class(source: &str, generation: u64, name: &str) -> String {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        let mut types = Vec::new();
        nodes_of_kind(&db, root, IndexedNodeKind::TypeDefinition, &mut types);
        nodes_of_kind(&db, root, IndexedNodeKind::EnumDefinition, &mut types);
        let declaration = types
            .into_iter()
            .find(|key| {
                let syntax = db.syntax_unit(key.unit).expect("unit");
                let node = syntax.syntax_index(&db).node_at(syntax.expanded_program(&db), key.node).expect("node");
                node.of::<beskid_analysis::syntax::TypeDefinition>()
                    .map(|definition| definition.name.node.name.as_str())
                    .or_else(|| {
                        node.of::<beskid_analysis::syntax::EnumDefinition>()
                            .map(|definition| definition.name.node.name.as_str())
                    })
                    == Some(name)
            })
            .expect("declaration");
        crate::nominal_transfer_class(&db, declaration, &[]).expect("transfer class").to_string()
    }

    #[test]
    fn transfer_classes_follow_the_ten_rules() {
        let source = "[Immutable] pub type Job { string id; i64 size; } \
            [Isolated] pub type Index { i64[] offsets; Job head; } \
            [Copyable] pub type Pixel { u8 r; u8 g; u8 b; } \
            [Copyable] pub type Frame { i64 width; u8[] pixels; } \
            [NeverTransferable] pub type Handle { i64 id; } \
            pub type Cache { i64[] entries; } \
            [Immutable] pub type WorkerId { u64 processId; u64 sequence; } \
            pub enum Maybe<T> { Some(T value), None } \
            pub enum Mode { Fast, Slow } \
            [Isolated] pub type Node { i64 value; Maybe<Node> next; } \
            [Isolated] pub type Raw { pointer data; } \
            [Isolated] pub type Holder { Handle handle; } ";
        let expected = [
            ("Job", "Shared"),
            ("Index", "Moved"),
            ("Pixel", "Copied"),
            ("Frame", "Never (field `pixels` has class Moved)"),
            ("Handle", "Never (`[NeverTransferable]`)"),
            ("Cache", "Never (no marker attribute)"),
            ("WorkerId", "Shared"),
            ("Mode", "Shared"),
            ("Node", "Moved"),
            ("Raw", "Never (field `data` is Never (`pointer` is a native address))"),
            ("Holder", "Never (field `handle` is Never (`[NeverTransferable]`))"),
        ];
        for (offset, (name, class)) in expected.iter().enumerate() {
            assert_eq!(type_class(source, 6400 + offset as u64, name), *class, "{name}");
        }
    }

    #[test]
    fn marker_bounds_are_proven_from_the_transfer_class() {
        let source = format!(
            "{MARKERS}pub type Cache {{ i64[] entries; }} [Isolated] pub type Box<T> {{ T item; }} \
             [NeverTransferable] pub type Mutex {{ i64 id; }} \
             unit Own<T>(T value) where T: Moved {{ }} unit View<T>(T value) where T: Shared {{ }} \
             unit Keep<T>(T value) where T: Transferable {{ }} \
             unit Main(u8[] bytes, Cache cache, Box<string> named, Box<Mutex> locked) {{ \
             Own<u8[]>(bytes); Keep<i64>(21_i64); Keep<Box<string>>(named); View<u8[]>(bytes); Keep<Cache>(cache); \
             Keep<Box<Mutex>>(locked); }}"
        );
        let report = transfer_report(&source, 6420);
        let bounds = report.iter().filter(|(code, _, _)| code == "E1610").collect::<Vec<_>>();
        assert_eq!(bounds.len(), 3, "{report:?}");
        assert!(bounds.iter().any(|(_, _, help)| help == "class Moved, bound requires Shared"), "{report:?}");
        assert!(
            bounds.iter().any(|(_, message, help)| message.contains("`Cache`") && help == "no marker attribute"),
            "{report:?}"
        );
        assert!(bounds.iter().any(|(_, _, help)| help.contains("field `item`")), "{report:?}");
    }

    #[test]
    fn marker_attributes_are_judged_at_the_declaration() {
        let source = format!(
            "{MARKERS}[Copyable] pub type Pixel {{ u8 r; u8 g; u8 b; }} \
             [Isolated] pub type Image {{ Pixel[] rows; Pixel background; }} \
             [Copyable] pub type Frame {{ i64 width; u8[] pixels; }} \
             [Isolated] pub type Snapshot {{ i64 taken; }} \
             [Immutable] pub type Names {{ i64 taken; i64[] values; }} \
             [Immutable] pub type Raw {{ pointer data; i64 length; }} \
             [NeverTransferable] [Copyable] pub type Channel {{ i64 id; }} \
             [Immutable] [Isolated] pub type Both {{ i64 id; }} \
             pub type Token : Shared {{ i64 id; }} \
             [CompilerDerived] pub contract Sized {{ i64 Size(); }} \
             pub type Store {{ [ImmutableStorage] i64[] items; }}"
        );
        let report = transfer_report(&source, 6430);
        let codes = report.iter().map(|(code, _, _)| code.as_str()).collect::<Vec<_>>();
        for code in ["E1247", "E1244", "E1246", "E1510", "E1607", "E1003"] {
            assert!(codes.contains(&code), "{code} missing: {report:?}");
        }
        assert_eq!(codes.iter().filter(|code| **code == "E1247").count(), 2, "{report:?}");
        assert!(report.iter().any(|(code, message, _)| code == "E1247" && message.contains("`background`")));
        assert!(report.iter().any(|(code, message, _)| code == "E1247" && message.contains("`pixels`")));
        assert!(report.iter().any(|(code, message, _)| code == "E1244" && message.contains("`values`")));
        assert!(report.iter().any(|(code, message, _)| code == "E1246" && message.contains("field `data`")));
        assert!(report.iter().any(|(code, message, _)| code == "E1246" && message.contains("`[NeverTransferable]`")));
        assert!(report.iter().any(|(code, message, _)| code == "E1510" && message.contains("exclusive")));
        assert!(report.iter().any(|(code, message, _)| code == "E1510" && message.contains("ImmutableStorage")));
        assert!(report.iter().any(|(code, message, _)| code == "E1607" && message.contains("`Shared`")));
        assert!(report.iter().any(|(code, message, _)| code == "E1607" && message.contains("`Sized`")));
        assert!(report.iter().any(|(code, message, _)| code == "E1003" && message.contains("`Size`")));
    }

    #[test]
    fn immutable_fields_are_set_only_by_construction() {
        let source = "[Immutable] pub type Job { string id; i64 size; pub unit Grow() { this.size = this.size + 1; } } \
            unit Rename(Job job) { job.id = \"renamed\"; } \
            Job Make() { return Job { id: \"a\", size: 1 }; }";
        let report = transfer_report(source, 6440);
        let assignments = report.iter().filter(|(code, _, _)| code == "E1245").collect::<Vec<_>>();
        assert_eq!(assignments.len(), 2, "{report:?}");
        assert!(assignments.iter().any(|(_, message, _)| message.contains("`size`")));
        assert!(assignments.iter().any(|(_, message, _)| message.contains("`id`")));
    }

    #[test]
    fn glue_owner_handles_are_never_transferable() {
        let source = "[GlueHandle(Library: \"codec\")] [Immutable] pub type Codec { i64 id; }";
        let report = transfer_report(source, 6450);
        assert!(
            report.iter().any(|(code, message, _)| code == "E1246" && message.contains("GlueHandle")),
            "{report:?}"
        );
    }

    const AFFINE_PRELUDE: &str = "u8[] Make() { return [1_u8]; } i64 Len(u8[] data) { return 0; } \
        unit Store(move u8[] block) { return; } ";

    const CROSSING_TYPES: &str = "[Immutable] pub type Port<T> { i64 id; \
        pub i64 Send(move T value) where T: Transferable { return 0; } } \
        [Isolated] pub type Node { u8[] bytes; } [Isolated] pub type Holder { u8[] bytes; u8[] cache; \
        pub unit Put(mut this, move u8[] data) { this.cache = data; return; } } \
        [Isolated] pub type Block { u8[] bytes; } ";

    /// Affine findings of `body` in a `Core.Parallel` unit, so `Port.Send` is the corelib crossing.
    fn assert_crossing(body: &str, generation: u64, expected: &[&str]) {
        let source = format!("{MARKERS}{AFFINE_PRELUDE}{CROSSING_TYPES}{body}");
        let found = transfer_report_at(&source, generation, "Core/Parallel/Port.bd")
            .into_iter()
            .filter(|(code, ..)| ("E1234"..="E1243").contains(&code.as_str()))
            .collect::<Vec<_>>();
        let codes = found.iter().map(|(code, ..)| code.as_str()).collect::<Vec<_>>();
        assert_eq!(codes, expected, "{body}: {found:?}");
    }

    /// 0.6.7 owner ruling: ordinary local code never needs `move`; aliases, stores, returns,
    /// local `move` parameters and reuse after a local `move` compile without a finding.
    #[test]
    fn local_code_needs_no_move() {
        let cases: &[&str] = &[
            "unit Main(move u8[] buffer) { Store(buffer); i64 n = Len(buffer); return; }",
            "unit Main() { u8[] a = Make(); u8[] b = a; i64 n = Len(a); Store(b); Store(a); return; }",
            "unit Main(u8[] data) { i64 n = Len(data); u8[] alias = data; i64 m = Len(alias); return; }",
            "u8[] Main(move u8[] data) { return data; }",
            "Node Wrap(move u8[] state) { return Node { bytes: state }; }",
            "Node Wrap(Holder h) { return Node { bytes: h.bytes }; }",
            "u8[][] Wrap(move u8[] state) { return [state]; }",
            "unit Fill(Holder h, u8[][] xs) { h.bytes = xs[0]; return; }",
            "unit Keep(u8[][] xs) { Store(xs[0]); return; }",
            "Block First(Block[] blocks) { return blocks[0]; }",
            "unit Main(u8[] data) { let wrap = () => Node { bytes: data }; Node n = wrap(); return; }",
            "u8[] Swap(Holder h, move u8[] next) { return exchange(h.bytes, next); }",
            "unit Main(Holder h) { u8[] bytes = Make(); h.Put(bytes); i64 n = Len(bytes); return; }",
            "u8[] Fresh(u8[] seed) { u8[] made = Make(); return made; }",
        ];
        for (offset, body) in cases.iter().enumerate() {
            assert_crossing(body, 6600 + offset as u64, &[]);
        }
    }

    /// RB5 owner ruling: a plain (not `move`) parameter of Moved class is lent by its caller. It
    /// may be read and passed on, but storing it (field, element), putting it into a new value,
    /// returning it, or handing it to a parameter that keeps it (`move`, `exchange`, a `spawn`
    /// target) is E1241 with the help "copy it, or declare the parameter `move`". A local that
    /// holds the parameter is judged the same way.
    #[test]
    fn a_kept_borrowed_parameter_is_e1241() {
        let cases: &[(&str, &[&str])] = &[
            // The review repro: the callee keeps `b`, the caller still owns and sends it.
            ("unit Keep(Holder h, u8[] b) { h.bytes = b; return; }", &["E1241"]),
            ("unit Fill(u8[][] xs, u8[] b) { xs[0] = b; return; }", &["E1241"]),
            ("Node Wrap(u8[] state) { return Node { bytes: state }; }", &["E1241"]),
            ("u8[][] Wrap(u8[] state) { return [state]; }", &["E1241"]),
            ("u8[] Main(u8[] data) { return data; }", &["E1241"]),
            ("unit Main(u8[] data) { Store(data); return; }", &["E1241"]),
            ("u8[] Swap(Holder h, u8[] next) { return exchange(h.bytes, next); }", &["E1241"]),
            ("unit Keep(Holder h, u8[] b) { u8[] held = b; h.bytes = held; return; }", &["E1241"]),
            ("unit Keep(Holder h, u8[] b) { mut u8[] held = Make(); held = b; h.Put(held); return; }", &["E1241"]),
            ("unit Keep(Holder h, u8[] b) { h.Put(b); return; }", &["E1241"]),
            // A `move` parameter is owned by the callee and may be kept.
            ("unit Keep(Holder h, move u8[] b) { h.bytes = b; return; }", &[]),
            ("unit Keep(Holder h, move u8[] b) { h.Put(b); return; }", &[]),
            // A fresh value is owned by the callee.
            ("u8[] Fresh(u8[] seed) { u8[] made = Make(); return made; }", &[]),
            ("Node Fresh() { return Node { bytes: Make() }; }", &[]),
            // A plain parameter of a type parameter is not of Moved class in the generic body.
            ("pub type Cell<T> { T value; } unit Put<T>(Cell<T> c, T v) { c.value = v; return; }", &[]),
            // A `bulk` parameter receives the array its call packs: the callee owns it.
            ("u8[] Pack(bulk u8[] bytes) { return bytes; }", &[]),
        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            assert_crossing(body, 6800 + offset as u64, expected);
        }
        let source = format!("{MARKERS}{AFFINE_PRELUDE}{CROSSING_TYPES}unit Keep(Holder h, u8[] b) {{ h.bytes = b; return; }}");
        let report = transfer_report_at(&source, 6830, "Core/Parallel/Port.bd");
        let (_, message, help) = report.iter().find(|(code, ..)| code == "E1241").expect("E1241");
        assert_eq!(help, "copy it, or declare the parameter `move`");
        assert!(message.contains("the parameter `b` is borrowed"), "{message}");
    }

    /// RB5 for `this` and captures: `this` without `move this` is lent by the caller, and a
    /// borrowed parameter captured by an escaping lambda is kept by that lambda.
    #[test]
    fn kept_this_and_escaping_captures_are_e1241() {
        let cases: &[(&str, &[&str])] = &[
            ("[Isolated] pub type Cell { u8[] bytes; pub Cell Back() { return this; } }", &["E1241"]),
            ("[Isolated] pub type Cell { u8[] bytes; pub Cell Back(mut this) { return this; } }", &["E1241"]),
            ("[Isolated] pub type Cell { u8[] bytes; pub unit Into(Cell[] all) { all[0] = this; return; } }", &["E1241"]),
            ("[Isolated] pub type Cell { u8[] bytes; pub Cell Take(move this) { return this; } }", &[]),
            ("[Isolated] pub type Cell { u8[] bytes; pub i64 Size() { return Len(this.bytes); } }", &[]),
            ("pub type Thunk { () => i64 run; } Thunk Later(u8[] b) { return Thunk { run: () => Len(b) }; }", &["E1241"]),
            ("i64 Now(u8[] b) { let f = () => Len(b); return f(); }", &[]),
            ("pub type Thunk { () => i64 run; } Thunk Later(move u8[] b) { return Thunk { run: () => Len(b) }; }", &[]),
        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            assert_crossing(body, 6860 + offset as u64, expected);
        }
    }

    /// Call-site rule (RB5): handing an owning local to a parameter that may keep it (`move`,
    /// `mut`, a `spawn` target, the value of `exchange`) gives the value another name, so the
    /// local can no longer cross (E1241, copy it first). A plain argument does not: a plain
    /// parameter cannot be kept by the callee.
    #[test]
    fn a_kept_argument_is_another_name_at_a_crossing() {
        let cases: &[(&str, &[&str])] = &[
            ("unit Main(Port<u8[]> p, Holder h) { u8[] b = Make(); h.Put(b); p.Send(move b); return; }", &["E1241"]),
            ("unit Main(Port<u8[]> p) { u8[] b = Make(); Store(b); p.Send(move b); return; }", &["E1241"]),
            ("unit Main(Port<u8[]> p) { u8[] b = Make(); i64 n = Len(b); p.Send(move b); return; }", &[]),
            // A `mut` parameter shares the caller's binding and may keep it.
            (
                "unit Grow(mut u8[] b) { return; } unit Main(Port<u8[]> p) { u8[] b = Make(); Grow(b); p.Send(move b); return; }",
                &["E1241"],
            ),
            ("unit Keep(Holder h, mut u8[] b) { h.bytes = b; return; }", &[]),
            // Owner ruling: an argument of a parameter typed by a type parameter may be kept by the
            // generic body, so the binding must be fresh or copied to cross.
            (
                "unit Put<T>(T v) { return; } unit Main(Port<u8[]> p) { u8[] b = Make(); Put<u8[]>(b); p.Send(move b); return; }",
                &["E1241"],
            ),
            (
                "unit Put<T>(T[] v) { return; } unit Main(Port<u8[]> p) { u8[] b = Make(); Put<u8>(b); p.Send(move b); return; }",
                &[],
            ),
        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            assert_crossing(body, 6840 + offset as u64, expected);
        }
    }

    /// `move` outside a crossing is E1239 ("remove `move`"), and so is `move` of a value of a
    /// class other than Moved or the modifier on a parameter of such a type.
    #[test]
    fn move_outside_a_crossing_is_e1239() {
        let cases: &[(&str, &[&str])] = &[
            ("unit Main(move u8[] buffer) { Store(move buffer); return; }", &["E1239"]),
            ("unit Main() { u8[] a = Make(); u8[] b = move a; return; }", &["E1239"]),
            ("u8[] Take(Holder h) { return move h.bytes; }", &["E1239"]),
            ("u8[] Swap(Holder h, move u8[] next) { return exchange(h.bytes, move next); }", &["E1239"]),
            ("unit Main() { string name = \"a\"; string other = move name; return; }", &["E1239"]),
            ("i64 Size(move string name) { return 0; }", &["E1239"]),
            ("pub type Plain { i64 x; pub unit Take(move this) { return; } }", &["E1239"]),
        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            assert_crossing(body, 6640 + offset as u64, expected);
        }
    }

    /// At a port send the value crosses to another isolate: it is written with `move` (E1242),
    /// it is the only name for its value in the callable (E1241, copy it first), and the send
    /// consumes it (E1234 after it, E1240 in a loop without reassignment).
    #[test]
    fn a_port_send_consumes_and_needs_an_owning_value() {
        let cases: &[(&str, &[&str])] = &[
            ("unit Main(Port<u8[]> p, move u8[] buffer) { p.Send(move buffer); return; }", &[]),
            ("unit Main(Port<u8[]> p) { u8[] data = Make(); p.Send(move data); return; }", &[]),
            (
                "unit Main(Port<u8[]> p, move u8[] buffer) { p.Send(move buffer); i64 n = Len(buffer); return; }",
                &["E1234"],
            ),
            ("unit Main(Port<u8[]> p, move u8[] data) { p.Send(data); return; }", &["E1242"]),
            // External uniqueness: a borrowed parameter, an aliased local, a place-initialized
            // local or a capture of a repeatable closure is not the only name (E1241).
            ("unit Main(Port<u8[]> p, u8[] data) { p.Send(move data); return; }", &["E1241"]),
            ("unit Main(Port<u8[]> p, u8[] data) { p.Send(data); return; }", &["E1242"]),
            ("unit Main(Port<u8[]> p) { u8[] a = Make(); u8[] b = a; p.Send(move a); return; }", &["E1241"]),
            ("unit Main(Port<u8[]> p, move u8[] data) { let keep = data; p.Send(move data); return; }", &["E1241"]),
            (
                "unit Main(Port<u8[]> p) { u8[] a = Make(); Node n = Node { bytes: a }; p.Send(move a); return; }",
                &["E1241"],
            ),
            ("unit Main(Port<u8[]> p, Holder h) { u8[] b = h.bytes; p.Send(move b); return; }", &["E1241"]),
            ("unit Main(Port<u8[]> p) { u8[] a = Make(); i64 n = Len(a); p.Send(move a); return; }", &[]),
            ("unit Main(Port<u8[]> p, move u8[] data) { let send = () => p.Send(move data); return; }", &["E1241"]),
            // Branches are alternatives; a diverging arm keeps the name live after it.
            (
                "unit Main(Port<u8[]> p, move u8[] data, bool keep) { if keep { p.Send(move data); } else { i64 n = Len(data); } return; }",
                &[],
            ),
            (
                "unit Main(Port<u8[]> p, move u8[] data, bool keep) { if keep { p.Send(move data); } i64 n = Len(data); return; }",
                &["E1234"],
            ),
            (
                "unit Main(Port<u8[]> p, move u8[] data, bool fail) { if fail { p.Send(move data); return; } p.Send(move data); return; }",
                &[],
            ),
            // Loops: a send of a binding declared before the loop needs a reassignment.
            (
                "unit Main(Port<u8[]> p, move u8[] data, i64 rounds) { mut i64 i = 0; while i < rounds { p.Send(move data); i = i + 1; } return; }",
                &["E1240"],
            ),
            (
                "unit Main(Port<u8[]> p, i64 rounds) { mut u8[] data = Make(); mut i64 i = 0; while i < rounds { p.Send(move data); data = Make(); i = i + 1; } return; }",
                &[],
            ),
            (
                "unit Main(Port<u8[]> p) { mut u8[] data = Make(); p.Send(move data); data = Make(); i64 n = Len(data); return; }",
                &[],
            ),
            // Fields and elements: through a borrowed root E1241, through an owning root E1242.
            ("unit Main(Port<u8[]> p, Holder h) { p.Send(h.bytes); return; }", &["E1241"]),
            ("unit Main(Port<u8[]> p, move Holder h) { p.Send(h.bytes); return; }", &["E1241"]),
            ("unit Main(Port<u8[]> p, move Holder h) { p.Send(move h.bytes); return; }", &[]),
            ("unit Main(Port<u8[]> p, u8[][] xs) { p.Send(xs[0]); return; }", &["E1241"]),
            ("unit Main(Port<u8[]> p, move u8[][] xs) { p.Send(exchange(xs[0], Make())); return; }", &[]),
            // Shared values cross without `move`.
            ("unit Main(Port<string> p, string name) { p.Send(name); i64 n = 0; return; }", &[]),
            // Moved in `else`, used in `then`: alternatives.
            (
                "unit Main(Port<u8[]> p, move u8[] data, bool keep) { if keep { i64 n = Len(data); } else { p.Send(move data); } return; }",
                &[],
            ),
            // A `for` loop without reassignment, and one that rebinds before the next iteration.
            (
                "unit Main(Port<u8[]> p, move u8[] data, i64[] rounds) { for r in rounds { p.Send(move data); } return; }",
                &["E1240"],
            ),
            (
                "unit Main(Port<u8[]> p, i64[] rounds) { mut u8[] data = Make(); for r in rounds { p.Send(move data); data = Make(); } return; }",
                &[],
            ),
            // A send that `break`s out of the loop does not reach the next iteration.
            (
                "unit Main(Port<u8[]> p, move u8[] data, i64 rounds) { mut i64 i = 0; while i < rounds { p.Send(move data); break; } return; }",
                &[],
            ),
            // `move this.f` at a crossing through an owning receiver.
            (
                "[Isolated] pub type Owner { u8[] bytes; pub unit Flush(move this, Port<u8[]> p) { p.Send(move this.bytes); return; } }",
                &[],
            ),

        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            assert_crossing(body, 6660 + offset as u64, expected);
        }
        let help = transfer_report_at(
            &format!("{MARKERS}{AFFINE_PRELUDE}{CROSSING_TYPES}unit Main(Port<u8[]> p, u8[][] xs) {{ p.Send(xs[0]); return; }}"),
            6699,
            "Core/Parallel/Port.bd",
        )
        .into_iter()
        .find(|(code, ..)| code == "E1241")
        .map(|(_, _, help)| help)
        .unwrap_or_default();
        assert!(help.contains("copy `xs[0]` first") && help.contains("exchange(xs[0], <value>)"), "{help}");
    }

    #[test]
    fn a_corelib_scope_borrowed_view_stores_a_borrowed_value() {
        let view = "[Immutable] [ScopeBorrowed] pub type View<T> { [ImmutableStorage] T[] items; } \
            [Immutable] [ScopeBorrowed] pub type Bytes { [ImmutableStorage] u8[] items; } \
            pub type Plain { u8[] bytes; } ";
        let bodies = "pub i64 Over<T>(T[] values) where T: Transferable { View<T> view = View<T> { items: values }; return 0; } \
            i64 OverBytes(u8[] values) { Bytes view = Bytes { items: values }; return 0; } \
            i64 Owned(move u8[] values) { Bytes view = Bytes { items: values }; Store(values); return 0; } \
            Plain Wrap(move u8[] bytes) { return Plain { bytes: bytes }; } ";
        let affine = |relative: &str, generation: u64| {
            transfer_report_at(&format!("{MARKERS}{AFFINE_PRELUDE}{view}{bodies}"), generation, relative)
                .into_iter()
                .map(|(code, ..)| code)
                .filter(|code| ("E1234"..="E1243").contains(&code.as_str()) || code == "E1510")
                .collect::<Vec<_>>()
        };
        // In a `Core.*` module the view stores a borrowed value; construction is local code, so
        // nothing is consumed or reported (0.6.7 owner ruling).
        let core = affine("Core/Parallel/Seq.bd", 6940);
        assert!(core.is_empty(), "local construction is not a crossing: {core:?}");
        // Outside the corelib `[ScopeBorrowed]` (and `[ImmutableStorage]`) is E1510 and grants
        // nothing: the view is an ordinary value, so storing a plain parameter in it is E1241
        // (RB5), but no scope-borrowed escape is reported.
        let outside = transfer_report_at(&format!("{MARKERS}{AFFINE_PRELUDE}{view}{bodies}"), 6941, "Main.bd");
        let scope_borrowed = transfer_report_at(&format!("{MARKERS}{AFFINE_PRELUDE}{view}{bodies}"), 6942, "Main.bd")
            .into_iter()
            .filter(|(code, message, _)| code == "E1510" && message.contains("ScopeBorrowed"))
            .count();
        assert!(scope_borrowed > 0, "{outside:?}");
        assert!(!outside.iter().any(|(_, message, _)| message.contains("scope-borrowed")), "{outside:?}");
    }

    /// v0.6.7 ruling "Scope-borrowed values do not escape": a value of a corelib `[ScopeBorrowed]`
    /// type is not returned, stored, passed to `spawn` or captured by an escaping lambda (E1241).
    #[test]
    fn a_corelib_scope_borrowed_view_does_not_escape_its_scope() {
        let types = "[Immutable] [ScopeBorrowed] pub type Bytes { [ImmutableStorage] u8[] items; } \
            pub type Holder { Bytes view; } \
            pub type Loose { (i64) => u8 element; } \
            pub type Plan { [Captures(Bound: Transferable)] (i64) => u8 element; } \
            pub type Fiber<T> { i64 handle; pub unit Cancel() { return; } } \
            pub enum Slot { Full(Bytes view), Empty } \
            i64 Use(Bytes b) { return 0; } \
            u8 At(Bytes b, i64 i) { return 0_u8; } \
            u8 Apply((i64) => u8 f) { return f(0); } ";
        let escapes = |body: &str, generation: u64| {
            transfer_report_at(&format!("{MARKERS}{AFFINE_PRELUDE}{types}{body}"), generation, "Core/Parallel/Seq.bd")
                .into_iter()
                .filter(|(code, message, _)| code == "E1241" && message.contains("scope-borrowed"))
                .count()
        };
        let cases: &[(&str, usize)] = &[
            // Returned: a received view, a constructed view, an alias, a lambda result.
            ("Bytes Back(Bytes b) { return b; }", 1),
            ("Bytes Fresh(u8[] v) { return Bytes { items: v }; }", 1),
            ("Bytes Alias(Bytes b) { let other = b; return other; }", 1),
            ("unit Get(Bytes b) { let get = () => b; return; }", 1),
            // Stored into a field, an element, an array literal or an enum payload.
            ("unit Keep(Holder h, Bytes b) { h.view = b; return; }", 1),
            ("Holder Wrap(Bytes b) { return Holder { view: b }; }", 1),
            ("unit Fill(Bytes[] all, Bytes b) { all[0] = b; return; }", 1),
            ("i64 All(Bytes b) { Bytes[] all = [b]; return 0; }", 1),
            ("i64 Boxed(Bytes b) { Slot slot = Slot::Full(b); return 0; }", 1),
            // Captured by an escaping lambda: stored, unbounded field, spawned.
            ("Loose Build(Bytes b) { return Loose { element: (i64 i) => At(b, i) }; }", 1),
            ("unit Run(Bytes b) { let child = spawn (() => Use(b)); child.Cancel(); return; }", 1),
            ("Loose Nest(Bytes b) { let make = () => Loose { element: (i64 i) => At(b, i) }; return make(); }", 1),
            // Handed to a parameter that may keep it: a generic `T` or a `move` parameter.
            ("pub type Cell<T> { T v; } unit Put<T>(Cell<T> c, T v) { c.v = v; return; } i64 Keep(Cell<Bytes> c, Bytes b) { Put<Bytes>(c, b); return 0; }", 1),
            ("unit Take(move Bytes view) { return; } i64 Keep(Bytes b) { Take(b); return 0; }", 1),
            // Uses inside the scope stay legal.
            ("i64 Count(Bytes b) { return Use(b); }", 0),
            ("i64 Local(u8[] v) { Bytes view = Bytes { items: v }; return Use(view); }", 0),
            ("u8 Inner(Bytes b) { let f = (i64 i) => At(b, i); return f(0); }", 0),
            ("u8 Pass(Bytes b) { return Apply((i64 i) => At(b, i)); }", 0),
            ("u8 First(Bytes b) { return At(b, 0); }", 0),
            // A `[Captures]`-bounded plan field carries the borrow to the terminal (Core.Parallel).
            ("Plan Bounded(Bytes b) { return Plan { element: (i64 i) => At(b, i) }; }", 0),
        ];
        let mismatches = cases
            .iter()
            .enumerate()
            .filter_map(|(offset, (body, expected))| {
                let found = escapes(body, 8000 + offset as u64);
                (found != *expected).then(|| {
                    let report = transfer_report_at(
                        &format!("{MARKERS}{AFFINE_PRELUDE}{types}{body}"),
                        8100 + offset as u64,
                        "Core/Parallel/Seq.bd",
                    );
                    format!("{body}: expected {expected}, found {found}: {report:?}")
                })
            })
            .collect::<Vec<_>>();
        assert!(mismatches.is_empty(), "{mismatches:#?}");
        // Outside the corelib the attribute grants nothing and the type is ordinary (E1510).
        let outside = transfer_report_at(
            &format!("{MARKERS}{AFFINE_PRELUDE}{types}Bytes Back(Bytes b) {{ return b; }}"),
            8040,
            "Main.bd",
        );
        assert!(!outside.iter().any(|(_, message, _)| message.contains("scope-borrowed")), "{outside:?}");
    }

    #[test]
    fn destructive_reads_and_exchange_need_a_moved_place() {
        let holder =
            "[Isolated] pub type Index { u8[] offsets; } [Isolated] pub type Pair { u8[] left; Index right; } ";
        let cases: &[(&str, &[&str])] = &[
            ("unit Flush(Port<u8[]> p, Pair s) { p.Send(move s.left); return; }", &[]),
            ("unit Flush(Port<Index> p, Pair s) { p.Send(move s.right); return; }", &["E1243"]),
            ("u8[] Swap(Holder h, move u8[] next) { return exchange(h.bytes, next); }", &[]),
            (
                "pub type Named { string name; pub string Rename(mut this, string next) { return exchange(this.name, next); } }",
                &["E1239"],
            ),
        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            assert_crossing(&format!("{holder}{body}"), 6680 + offset as u64, expected);
        }
        let source = format!(
            "{AFFINE_PRELUDE}[Isolated] pub type Pair {{ u8[] left; pub u8[] TakeLeft(move this) {{ return move this.left; }} }}"
        );
        let (db, root) = one_unit_assembly(&source, SyntaxGenerationId(6690));
        let mut moves = Vec::new();
        nodes_of_kind(&db, root, IndexedNodeKind::MoveExpression, &mut moves);
        let form = crate::move_expression_form(&db, moves[0]).expect("query").expect("move form");
        assert!(
            matches!(form, crate::MoveExpressionForm::DestructiveRead { ref field, empty: crate::CanonicalEmpty::EmptyArray, .. } if field == "left"),
            "{form:?}"
        );
    }

    fn captures_codes(body: &str, generation: u64) -> Vec<String> {
        let source = format!(
            "{MARKERS}{AFFINE_PRELUDE}pub unit Run([Captures(Bound: Transferable)] () => i64 body) {{ return; }} \
             pub unit Share([Captures(Bound: Shared)] () => i64 body) {{ return; }} {body}"
        );
        transfer_report(&source, generation)
            .into_iter()
            .map(|(code, ..)| code)
            .filter(|code| ("E1233"..="E1248").contains(&code.as_str()) && code != "E1244" && code != "E1247")
            .collect()
    }

    #[test]
    fn captures_bounds_judge_bounded_lambda_arguments() {
        let cases: &[(&str, &[&str])] = &[
            (
                "[Immutable] pub type Config { string name; i64 retries; } [Copyable] pub type Pixel { u8 r; u8 g; u8 b; } \
                 unit Main(Config config, Pixel pixel) { Run(() => config.retries + i64(pixel.r)); return; }",
                &[],
            ),
            (
                "i64 Count() { mut i64 count = 0; Run(() => { count = count + 1; return count; }); return count; }",
                &["E1235"],
            ),
            (
                "pub type Cache { i64[] entries; } unit Main(Cache cache) { Run(() => Len([1_u8])); Run(() => cache.entries[0]); return; }",
                &["E1236"],
            ),
            ("unit Main(move u8[] data) { Run(() => Len(data)); return; }", &["E1237"]),
            ("unit Main(move u8[] data) { Run(() => Len(move data)); i64 n = Len(data); return; }", &["E1234"]),
            ("i64 Work() { return 1; } unit Main() { Run(Work); return; }", &[]),
            ("unit Main(() => i64 body) { Run(body); return; }", &["E1238"]),
            ("unit Main([Captures(Bound: Transferable)] () => i64 body) { Run(body); return; }", &[]),
            (
                "unit Main([Captures(Bound: Transferable)] () => i64 body) { let again = body; Run(again); return; }",
                &[],
            ),
            ("unit Main(move u8[] data) { Share(() => Len(move data)); return; }", &["E1236"]),
            (
                "pub contract Disposable { } pub unit Bad([Captures(Bound: Disposable)] () => i64 body) { return; }",
                &["E1248"],
            ),
            ("pub unit Bad([Captures(Bound: Transferable)] i64 count) { return; }", &["E1248"]),
            (
                "pub type Job { [Captures(Bound: Transferable)] () => i64 body; } \
                 unit Main(move u8[] data) { Job job = Job { body: () => Len(data) }; return; }",
                &["E1237"],
            ),
        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            let codes = captures_codes(body, 6700 + offset as u64);
            assert_eq!(codes, *expected, "{body}");
        }
    }

    #[test]
    fn moved_argument_for_a_repeatedly_captured_parameter_is_e1236() {
        // `Fold` captures its borrowed `T seed` in a bounded (repeatable) lambda, like the
        // `ParallelSeq.Reduce` seed: only Shared and Copied instantiations are admitted.
        let fold = "pub i64 Probe<T>(T value) { return 0; } \
                    pub unit Fold<T>(T seed) where T: Transferable { Run(() => Probe<T>(seed)); return; } ";
        let cases: &[(&str, &[&str])] = &[
            (
                "[Immutable] pub type Config { i64 retries; } unit Main(Config config) { Fold<Config>(config); return; }",
                &[],
            ),
            ("unit Main() { Fold<i64>(1_i64); return; }", &[]),
            ("unit Main(move u8[] data) { Fold<u8[]>(data); return; }", &["E1236"]),
        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            let codes = captures_codes(&format!("{fold}{body}"), 6760 + offset as u64);
            assert_eq!(codes, *expected, "{body}");
        }
    }

    #[test]
    fn moved_argument_for_a_parameter_forwarded_to_a_repeated_capture_is_e1236_at_the_wrapper_call() {
        // A generic wrapper forwarding its own `T seed` to `Fold` (like a helper over
        // `ParallelSeq.Reduce`) inherits the obligation: its call site is judged per
        // instantiation. Recursive and mutually recursive forwarding terminates.
        let prelude = "pub i64 Probe<T>(T value) { return 0; } \
                       pub unit Fold<T>(T seed) where T: Transferable { Run(() => Probe<T>(seed)); return; } \
                       pub unit Wrap<T>(T seed) where T: Transferable { Fold<T>(seed); return; } \
                       pub unit Twice<T>(i64 depth, T seed) where T: Transferable { if depth > 0 { Twice<T>(depth - 1, seed); } Wrap<T>(seed); return; } \
                       pub unit Ping<T>(T seed, i64 n) where T: Transferable { if n > 0 { Pong<T>(seed, n - 1); } return; } \
                       pub unit Pong<T>(T seed, i64 n) where T: Transferable { if n > 0 { Ping<T>(seed, n - 1); } return; } \
                       pub unit Keep<T>(T seed) where T: Transferable { Probe<T>(seed); return; } ";
        let cases: &[(&str, &[&str])] = &[
            ("unit Main(move u8[] data) { Wrap<u8[]>(data); return; }", &["E1236"]),
            ("unit Main(move u8[] data) { Twice<u8[]>(2, data); return; }", &["E1236"]),
            (
                "[Immutable] pub type Config { i64 retries; } unit Main(Config config) { Wrap<Config>(config); return; }",
                &[],
            ),
            ("unit Main() { Twice<i64>(2, 1_i64); return; }", &[]),
            ("unit Main(move u8[] data) { Ping<u8[]>(data, 2); return; }", &[]),
            ("unit Main(move u8[] data) { Keep<u8[]>(data); return; }", &[]),
        ];
        for (offset, (body, expected)) in cases.iter().enumerate() {
            let codes = captures_codes(&format!("{prelude}{body}"), 6780 + offset as u64);
            assert_eq!(codes, *expected, "{body}");
        }
    }

    #[test]
    fn move_parameters_over_transferable_type_parameters_are_judged_per_instantiation() {
        let source = format!(
            "{MARKERS}{AFFINE_PRELUDE}{CROSSING_TYPES}unit Ok1(Port<u8[]> p, move u8[] block) {{ p.Send(move block); return; }} \
             unit Bad1(Port<u8[]> p, move u8[] block) {{ p.Send(block); return; }} \
             unit Ok2(Port<string> p, string name) {{ p.Send(name); i64 n = 0; return; }} \
             unit Bad2(Port<string> p, string name) {{ p.Send(move name); return; }} \
             unit Generic<T>(Port<T> p, move T item) where T: Transferable {{ p.Send(move item); p.Send(item); return; }} \
             unit Unbounded<T>(move T item) {{ return; }}"
        );
        let found = transfer_report_at(&source, 6750, "Core/Parallel/Port.bd")
            .into_iter()
            .map(|(code, ..)| code)
            .filter(|code| ("E1234"..="E1243").contains(&code.as_str()))
            .collect::<Vec<_>>();
        let codes = &found;
        assert_eq!(found, ["E1242", "E1239", "E1234", "E1239"], "{codes:?}");
    }

    #[test]
    fn transfer_class_intrinsic_folds_and_isolate_bodies_are_bounded() {
        let source = format!(
            "{MARKERS}{AFFINE_PRELUDE}i64 A() {{ return __transfer_class<u8[]>(); }} \
             i64 B() {{ return __transfer_class<string>(); }} \
             i64 C<T>() where T: Transferable {{ return __transfer_class<T>(); }}"
        );
        let (db, root) = one_unit_assembly(&source, SyntaxGenerationId(6760));
        let mut calls = Vec::new();
        nodes_of_kind(&db, root, IndexedNodeKind::CallExpression, &mut calls);
        let folded = calls
            .into_iter()
            .filter_map(|call| crate::transfer_class_intrinsic(&db, call).expect("intrinsic query"))
            .collect::<Vec<_>>();
        assert_eq!(
            folded,
            [
                crate::TransferClassIntrinsic::Constant(1),
                crate::TransferClassIntrinsic::Constant(0),
                crate::TransferClassIntrinsic::PerInstantiation { name: "T".to_owned() },
            ]
        );
        let codes = captures_codes(
            "unit Main(move u8[] data) { i64 id = __isolate_spawn<i64>(() => Len(data), 0); return; }",
            6761,
        );
        assert!(codes.contains(&"E1237".to_owned()), "{codes:?}");
    }

    /// `(code, message, site text, related labels)` of every gate-obligation finding of the last
    /// function.
    fn conditional_report(source: &str, generation: u64) -> Vec<(String, String, String, Vec<String>)> {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        let key = *function_definitions(&db, root).last().expect("function");
        super::check_gate_obligations(&db, &[key])
            .into_iter()
            .map(|finding| {
                let span = crate::node_span(&db, finding.site).expect("span").expect("site span");
                // A single-operand operator chain keeps its pest pair span, which runs over the
                // whitespace before the next token (`n ?`); the site text is compared trimmed
                // until the parser trims that span (request to slice parse).
                (
                    finding.kind.code().to_string(),
                    finding.kind.message(),
                    source[span.start..span.end].trim_end().to_string(),
                    finding.related.iter().map(|(_, label)| label.to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn conditional_with_a_non_bool_condition_is_e1208_at_the_condition() {
        let report = conditional_report("i32 F(i32 n, i32 a, i32 b) { return n ? a : b; }", 7101);
        assert_eq!(report.len(), 1, "{report:?}");
        assert_eq!(report[0].0, "E1208");
        assert_eq!(report[0].2, "n", "anchored at the condition");
        let report = conditional_report("i32 F(i32 n, i32 a, i32 b) { return n + 1 ? a : b; }", 7102);
        assert_eq!(
            report.iter().map(|finding| (finding.0.as_str(), finding.2.as_str())).collect::<Vec<_>>(),
            vec![("E1208", "n + 1")]
        );
    }

    #[test]
    fn conditional_branches_of_two_typed_widths_are_e1206_at_the_else_branch_with_both_related() {
        let report = conditional_report("i64 F(bool c, i32 a, i64 b) { return c ? a : b; }", 7103);
        assert_eq!(report.len(), 1, "{report:?}");
        let (code, message, site, related) = &report[0];
        assert_eq!(code, "E1206");
        assert!(message.contains("i32") && message.contains("i64"), "{message}");
        assert_eq!(site, "b", "anchored at the else-branch");
        assert_eq!(related, &["this branch is `i32`", "this branch is `i64`"]);
        let report = conditional_report("string F(bool c, string a, bool b) { return c ? a : b; }", 7104);
        assert_eq!(report.iter().map(|finding| finding.0.as_str()).collect::<Vec<_>>(), vec!["E1206"]);
        assert_eq!(report[0].3, ["this branch is `string`", "this branch is `bool`"]);
    }

    #[test]
    fn conditional_branches_of_two_nominal_types_are_e1206_by_source_identity() {
        let report = conditional_report(
            "pub type Point { i64 x; } pub type Rect { i64 w; } \
             Point F(bool c, Point p, Rect r) { return c ? p : r; }",
            7105,
        );
        assert_eq!(report.len(), 1, "{report:?}");
        assert_eq!(report[0].0, "E1206");
        assert_eq!(report[0].2, "r");
        assert_eq!(report[0].3, ["this branch is `Point`", "this branch is `Rect`"]);
    }

    #[test]
    fn conditional_with_a_literal_branch_reports_no_branch_mismatch() {
        for (source, generation) in [
            ("i64 F(bool c) { return c ? 1 : 2_i64; }", 7106u64),
            ("i64 F(bool c, i64 n) { return c ? n : 2; }", 7107),
            ("i64 F(bool c, i64 n) { return c ? -1 : n; }", 7108),
            ("i32 F(bool c, i32 a, i32 b) { return c ? a : b; }", 7109),
            ("pub type Point { i64 x; } Point F(bool c, Point p, Point q) { return c ? p : q; }", 7110),
        ] {
            let report = conditional_report(source, generation);
            assert!(report.is_empty(), "{source}: {report:?}");
        }
    }

    #[test]
    fn conditional_has_the_type_of_its_non_literal_branch() {
        let source = "i64 F(bool c, i64 n) { i64 r = c ? 1 : n; return r; }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(7111));
        let key = function_definitions(&db, root)[0];
        let mut conditionals = Vec::new();
        nodes_of_kind(&db, key, IndexedNodeKind::ConditionalExpression, &mut conditionals);
        assert_eq!(conditionals.len(), 1);
        assert_eq!(node_type(&db, conditionals[0]).expect("query"), Some(SemanticTypeId::I64));
        assert_eq!(abi_type(&db, conditionals[0]).expect("query"), Some(SemanticTypeId::I64));
        let branch = crate::conditional_result_branch(&db, conditionals[0]).expect("query").expect("branch");
        let span = crate::node_span(&db, branch).expect("span").expect("branch span");
        assert_eq!(&source[span.start..span.end], "n");
        assert!(super::check_typing_obligations(&db, &[key]).is_empty());
    }

    /// `(code, message)` of every finding of `check_items` and the gate over every function.
    fn contract_value_report(source: &str, generation: u64) -> Vec<(String, String)> {
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
                .chain(super::check_typing_obligations(&db, &items))
                .map(|finding| (finding.kind.code().to_string(), finding.kind.message())),
        );
        findings.sort();
        findings.dedup();
        findings
    }

    const CONTRACT_GREETERS: &str = "pub contract Greeter { i64 Rank(); } \
        pub type Hello : Greeter { pub i64 Rank() { return 1_i64; } } \
        pub type Bye : Greeter { pub i64 Rank() { return 2_i64; } } \
        pub type Plain { i64 v; } ";

    #[test]
    fn equality_of_two_contract_values_is_valid_and_other_operators_are_not() {
        let valid = format!(
            "{CONTRACT_GREETERS} bool Same(Greeter a, Greeter b) {{ return a == b; }} \
             bool Other(Greeter a, Greeter b) {{ return a != b; }}"
        );
        assert_eq!(contract_value_report(&valid, 7201), Vec::<(String, String)>::new());
        for (body, generation) in [
            ("bool F(Greeter a, Greeter b) { return a < b; }", 7202u64),
            ("bool F(Greeter a, Hello b) { return a == b; }", 7203),
            ("i64 F(Greeter a, Greeter b) { return a + b; }", 7204),
        ] {
            let report = contract_value_report(&format!("{CONTRACT_GREETERS} {body}"), generation);
            assert!(!report.is_empty(), "`{body}` must be rejected");
        }
    }

    #[test]
    fn a_contract_value_converts_to_a_contract_its_contract_embeds_and_not_back() {
        let contracts = "pub contract Reader { i64 Read(); } pub contract Channel { Reader; i64 Flush(); } \
            pub type File : Channel { pub i64 Read() { return 1_i64; } pub i64 Flush() { return 2_i64; } } ";
        let valid = format!(
            "{contracts} i64 Use(Reader r) {{ return r.Read(); }} \
             i64 F(Channel c) {{ Reader r = c; mut Reader s = c; s = c; return r.Read() + Use(c); }}"
        );
        assert_eq!(contract_value_report(&valid, 7205), Vec::<(String, String)>::new());
        let invalid = format!("{contracts} i64 F(Reader r) {{ Channel c = r; return c.Flush(); }}");
        let report = contract_value_report(&invalid, 7206);
        assert!(report.iter().any(|(code, _)| code == "E1206"), "{report:?}");
    }

    #[test]
    fn conditional_branches_of_different_implementations_convert_to_a_contract_expectation() {
        let valid = format!(
            "{CONTRACT_GREETERS} Greeter F(bool c) {{ return c ? Hello {{}} : Bye {{}}; }} \
             i64 G(bool c) {{ Greeter g = c ? Hello {{}} : Bye {{}}; return g.Rank(); }} \
             i64 H(bool c) {{ Greeter() make = () => c ? Hello {{}} : Bye {{}}; return make().Rank(); }}"
        );
        assert_eq!(contract_value_report(&valid, 7207), Vec::<(String, String)>::new());
        let invalid = format!("{CONTRACT_GREETERS} Greeter F(bool c) {{ return c ? Hello {{}} : Plain {{ v: 1 }}; }}");
        let report = contract_value_report(&invalid, 7208);
        assert_eq!(report.iter().filter(|(code, _)| code == "E1206").count(), 1, "{report:?}");
        assert!(report.iter().any(|(_, message)| message.contains("Plain")), "{report:?}");
        // Without a contract expectation the branches must still have one type.
        let report = contract_value_report(
            &format!("{CONTRACT_GREETERS} i64 F(bool c) {{ let g = c ? Hello {{}} : Bye {{}}; return 0_i64; }}"),
            7209,
        );
        assert!(report.iter().any(|(code, _)| code == "E1206"), "{report:?}");
    }

    #[test]
    fn match_arms_and_lambda_bodies_convert_to_their_contract_result() {
        let valid = format!(
            "{CONTRACT_GREETERS} enum Pick {{ First, Second }} \
             Greeter F(Pick p) {{ return match p {{ Pick::First => Hello {{}}, Pick::Second => Bye {{}}, }}; }} \
             i64 G() {{ Greeter() make = () => Hello {{}}; return make().Rank(); }}"
        );
        assert_eq!(contract_value_report(&valid, 7210), Vec::<(String, String)>::new());
        let invalid =
            format!("{CONTRACT_GREETERS} i64 G() {{ Greeter() make = () => Plain {{ v: 1 }}; return 0_i64; }}");
        let report = contract_value_report(&invalid, 7211);
        assert!(report.iter().any(|(code, _)| code == "E1206"), "{report:?}");
    }

    #[test]
    fn a_type_converts_to_the_generic_contract_application_it_declares_and_no_other() {
        let contracts = "pub contract Repo<T> { T Get(); } \
            pub type Numbers : Repo<i64> { pub i64 Get() { return 9_i64; } } ";
        let valid = format!(
            "{contracts} i64 F() {{ mut Repo<i64> repo = Numbers {{}}; Repo<i64> other = repo; return other.Get(); }}"
        );
        assert_eq!(contract_value_report(&valid, 7231), Vec::<(String, String)>::new());
        let invalid = format!("{contracts} i64 F() {{ Repo<string> words = Numbers {{}}; return 0_i64; }}");
        let report = contract_value_report(&invalid, 7232);
        assert!(report.iter().any(|(code, _)| code == "E1206"), "{report:?}");
    }

    #[test]
    fn mixed_primitives_box_into_a_contract_element_array_literal() {
        let contracts = "pub contract Shown { string Show(); } \
            impl i64 : Shown { pub string Show() { return \"i\"; } } \
            impl bool : Shown { pub string Show() { return \"b\"; } } ";
        let valid = format!("{contracts} string F() {{ Shown[] mixed = [7_i64, false]; return mixed[0].Show(); }}");
        assert_eq!(contract_value_report(&valid, 7233), Vec::<(String, String)>::new());
        let invalid = "i64 F() { i64[] mixed = [7_i64, false]; return mixed[0]; }";
        let report = contract_value_report(invalid, 7234);
        assert!(!report.is_empty(), "a mixed literal without a contract element type stays rejected");
    }
}
