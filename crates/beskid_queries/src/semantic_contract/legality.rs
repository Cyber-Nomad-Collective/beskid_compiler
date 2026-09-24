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
//! `call_arity_mismatch` for E1204, `member_reference_legality` for E1211/E1301/E1302/E1307,
//! `match_exhaustiveness` for E1304; `unresolved_imports` for E1105, judged once per unit that owns
//! a judged item): it is never string classification of an opaque `SemanticError::unavailable`.
//! `check_items` collects every finding of the pass rather than stopping at the first, so
//! `beskid test` and `beskid build` report every violation the requested items carry in one run.

use super::*;
use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::syntax::{FunctionDefinition, MethodDefinition};
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};

mod calls;
mod imports;
mod members;

pub use calls::{UnresolvedCallKind, UnresolvedCallTarget, unresolved_call_target};
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

fn collect_call_sites(index: &SyntaxIndex, id: beskid_analysis::syntax::AstNodeId, calls: &mut Vec<beskid_analysis::syntax::AstNodeId>) {
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
    immutable.then(|| ImmutableLocalAssignment { assignment: AstNodeKey { node: assignment, ..key }, name: Arc::from(name) })
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
        if let Ok(Some(finding)) = unresolved_type_reference(db, item) {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::TypeUnknownType { name: finding.name.to_string() },
                site: finding.site,
                related: Vec::new(),
            });
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
                UnresolvedCallKind::UnknownValue { name } => SemanticIssueKind::ResolveUnknownValue { name: name.to_string() },
                UnresolvedCallKind::UnknownModulePath { path } => {
                    SemanticIssueKind::ResolveUnknownModulePath { path: path.to_string() }
                }
                UnresolvedCallKind::MissingTypeArguments => SemanticIssueKind::TypeMissingTypeArguments,
            };
            findings.push(SemanticFinding { kind, site: finding.call, related: Vec::new() });
        }
        if let Ok(Some(finding)) = member_reference_legality(db, item) {
            let kind = match finding.kind {
                MemberReferenceKind::UnknownStructField { name } => {
                    SemanticIssueKind::TypeUnknownStructField { name: name.to_string() }
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
                kind: SemanticIssueKind::MatchNonExhaustive { enum_name: finding.enum_name.to_string() },
                site: finding.site,
                related: Vec::new(),
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

    fn function_definitions(db: &dyn Db, key: AstNodeKey) -> Vec<AstNodeKey> {
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

    fn one_unit_assembly(source: &str, generation: SyntaxGenerationId) -> (BeskidDatabase, AstNodeKey) {
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let source_path = directory.join("Main.bd");
        std::fs::write(&source_path, source).expect("source");
        let program = parse_program_with_source_name(source_path.to_str().expect("path"), source).expect("parse source");
        let entry = SourceUnitId::new(&db, source_path.clone());
        let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root: directory }, dependencies: Vec::new() },
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
            ("i64 Main() { Pair p = Pair { b: 1 }; return 0; }", MemberReferenceKind::UnknownStructField { name: "b".into() }),
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

    #[test]
    fn direct_call_with_matching_arguments_is_not_reported() {
        let source = "unit Helper(i32 a, i32 b) { return; } unit Main() { Helper(1, 2); return; }";
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(42));
        let main_item = function_definitions(&db, root)[1];

        assert_eq!(super::call_arity_mismatch(&db, main_item).expect("call_arity_mismatch query"), None);
    }
}
