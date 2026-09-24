//! Multi-unit projects, control flow, nested calls, methods, and closed lambda failures.

use super::*;

#[test]
fn multi_unit_parsed_project_lowers_through_codegen_input_isle_only() {
    let project = tempfile::tempdir().expect("project directory");
    let util_source = "pub i32 Double(i32 value) { return value + value; }";
    let main_source = "
        use Util;
        i32 Main() {
            return Util.Double(21);
        }
    ";
    let assembly =
        parse_production_units(project.path(), &[("Main.bd", "Main", main_source), ("Util.bd", "Util", util_source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(lowered.artifact.functions.len() >= 2, "reachable closure must include Main and imported Util.Double");
}

#[test]
fn parsed_extern_contract_call_comparison_lowers_as_an_if_condition() {
    let project = tempfile::tempdir().expect("project directory");
    let source = r#"
        [Extern(Abi:"C", Library:"libc.so.6")]
        pub contract LinuxPthread { i32 sched_yield(); }
        unit Main() {
            if LinuxPthread.sched_yield() != 0 { return; }
            return;
        }
    "#;
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let clif = main.function.display().to_string();
    assert!(clif.contains("sched_yield"), "the extern contract symbol must remain explicit: {clif}");
    assert!(clif.contains("icmp"), "the imported result must feed the comparison: {clif}");
}

#[test]
fn parsed_project_control_flow_while_break_continue_reaches_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            mut i32 i = 0;
            mut i32 sum = 0;
            while i < 5 {
                i = i + 1;
                if i == 2 { continue; }
                if i == 4 { break; }
                sum = sum + i;
            }
            return sum;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected while/if branching: {clif}");
    assert!(clif.matches("jump").count() >= 2, "expected loop transfer jumps: {clif}");
}

#[test]
fn parsed_project_if_else_reaches_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            i32 value = 3;
            if value < 2 {
                return 1;
            } else {
                return 7;
            }
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected if/else branching: {clif}");
}

#[test]
fn parsed_project_range_for_accumulator_reaches_verified_clif_without_hir_fallback() {
    // Mutable assignment inside a parsed range body is governed solely by generation-bound
    // syntax facts. Unsupported operations must fail closed.
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            mut i32 sum = 0;
            for i in range(0, 4) {
                sum = sum + i;
            }
            return sum;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("RangeFor.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let clif = main.function.display().to_string();
    assert!(clif.contains("brif"), "expected range-for branch: {clif}");
    assert!(clif.contains("iadd"), "expected accumulator addition: {clif}");
}

#[test]
fn parsed_project_nested_direct_calls_reach_verified_clif() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Inner(i32 value) { return value + 1; }
        i32 Mid(i32 value) { return Inner(value) + Inner(value); }
        i32 Main() { return Mid(20); }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 3, "reachable closure must include Main, Mid, and Inner");
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    let mid = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Mid#syntax_"))
        .expect("Mid artifact function");
    assert!(main.function.display().to_string().contains("call"), "Main must call Mid");
    assert!(mid.function.display().to_string().matches("call").count() >= 2, "Mid must nest two Inner calls");
}

#[test]
fn unsupported_lambda_fails_closed_without_legacy_fallback() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        i32 Main() {
            i32 outer = 1;
            let add = (i32 inner) => outer + inner;
            return outer;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Lambda.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    assert_unsupported_closed_failure(assembly, target, isa.as_ref(), &["Lambda.bd", "MissingRuleOrFact"]);
}

#[test]
fn parsed_project_inline_method_reaches_verified_clif_through_production_entrypoint() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        type Point { i32 x, i32 Ping() { return 7; } }
        i32 Main() { return Point { x: 1 }.Ping(); }
    ";
    let assembly = parse_production_units(project.path(), &[("Method.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(
        lowered.artifact.functions.len() >= 2,
        "reachable closure must include Main and the inline Point.Ping method"
    );
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact function");
    assert!(
        main.function.display().to_string().contains("call"),
        "Main must call the inline method through syntax ISLE"
    );
}

#[test]
fn parsed_project_capturing_lambda_keeps_generation_safe_capture_facts_and_fails_closed() {
    let project = tempfile::tempdir().expect("project directory");
    let source_path = project.path().join("Capture.bd");
    let source = "
        i32 Main() {
            i32 outer = 1;
            let apply = (i32 inner) => outer + inner;
            return outer;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Capture.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    with_db(|db| {
        let generation = assembly.generation;
        let unit = SourceUnitId::new(db, source_path.clone());
        let typed = beskid_queries::build_typed_program(
            db,
            beskid_queries::ProjectSession::new(
                db,
                project.path().to_path_buf(),
                source_path.clone(),
                "App".into(),
                "lock".into(),
            ),
            generation,
            Arc::clone(&assembly),
        )
        .expect("typed capture program");
        assert!(
            typed.runtime_intrinsic_capability.is_none(),
            "ordinary parsed projects must not mint trusted runtime intrinsic authority"
        );
        let root = AstNodeKey { unit, generation, node: AstNodeId(0) };
        let mut pending = vec![root];
        let mut lambda = None;
        while let Some(key) = pending.pop() {
            if matches!(node_kind(db, key), Ok(Some(beskid_queries::IndexedNodeKind::LambdaExpression))) {
                lambda = Some(key);
                break;
            }
            if let Ok(Some(children)) = child_nodes(db, key) {
                pending.extend(children.iter().copied());
            }
        }
        let lambda = lambda.expect("capturing lambda source node");
        let environment =
            closure_environment(db, lambda).expect("capture fact query").expect("generation-safe capture environment");
        assert_eq!(environment.captures.len(), 1, "outer parameter is captured");
        assert_eq!(environment.parameters.len(), 1, "inner lambda parameter");
    });

    assert_unsupported_closed_failure(assembly, target, isa.as_ref(), &["Capture.bd", "MissingRuleOrFact"]);
}
