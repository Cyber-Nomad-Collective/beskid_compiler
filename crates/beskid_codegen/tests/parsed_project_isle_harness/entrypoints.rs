//! Contract admission and parsed projects reaching verified ISLE through production entry points.

use super::*;

#[test]
fn contract_specialization_module_admission_rejects_unproved_calls_and_hidden_members() {
    for (implementation, body) in [
        ("type Source { pub i64 Read() { return 1_i64; } }", "return reader.Read();"),
        ("type Source: Reader {}", "return reader.Read();"),
        ("type Source: Reader { i64 Read() { return 1_i64; } }", "return reader.Read();"),
        ("type Source: Reader { pub i32 Read() { return 1; } }", "return reader.Read();"),
        (
            "type Source: Reader { pub i64 Read() { return 1_i64; } pub i64 Reset() { return 2_i64; } }",
            "return reader.Reset();",
        ),
    ] {
        let source = format!(
            "contract Reader {{ i64 Read(); }} {implementation} i64 ReadOne(Reader reader) {{ {body} }} i64 Main() {{ return ReadOne(Source {{}}); }}"
        );
        let root = tempfile::tempdir().unwrap();
        let assembly = parse_production_units(root.path(), &[("Main.bd", "Main.bd", &source)]);
        let (target, isa) = x86_64_target_and_isa();
        let mut db = beskid_queries::BeskidDatabase::default();
        assert!(
            lower_syntax_assembly_entrypoint(&mut db, assembly, "Main", target, isa.as_ref()).is_err(),
            "invalid contract call must not emit: {source}"
        );
    }
}

#[test]
fn parsed_project_reaches_verified_isle_without_a_legacy_codegen_entrypoint() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        type Pair { i32 left, i32 right }
        i32 Add(i32 left, i32 right) { return left + right; }
        i32 Main() {
            Pair pair = Pair { left: 19, right: 23 };
            if pair.left < pair.right { return Add(pair.left, pair.right); }
            return 0;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    // The production entrypoint accepts parsed ProgramAssembly data and derives typed facts.
    let lowered = lower_verified_entrypoint(assembly, target.clone(), isa.as_ref());
    assert_eq!(lowered.artifact.functions.len(), 2, "reachable direct-call closure");

    let unsupported_source = "
        i32 Main() {
            i32 outer = 1;
            let task = spawn ((i32 inner) => outer + inner);
            return outer;
        }
    ";
    let unsupported = parse_production_units(project.path(), &[("Unsupported.bd", "Main", unsupported_source)]);
    // The generation-bound entry fact rejects the parameterized spawn before emitting its body.
    assert_unsupported_closed_failure(
        unsupported,
        target,
        isa.as_ref(),
        &["Unsupported.bd", "TargetRequiresArguments"],
    );
}

#[test]
fn parsed_direct_pointer_guard_with_unit_early_return_emits_verified_clif() {
    // Keep this shape aligned with the canonical scheduler's initialization guard:
    // `mut pointer table = SchedTable(); if table != NativePointer(0) { return; }`.
    // The recursive conversion helper is never executed; it supplies the exact direct-call
    // ABI shape so this test exercises statement lowering and imports rather than a host shim.
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pointer NativePointer(word value) { return NativePointer(value); }
        pointer SchedTable() { return NativePointer(0); }
        unit Main() {
            mut pointer table = SchedTable();
            if table != NativePointer(0) { return; }
            return;
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
        .expect("Main artifact");
    let clif = main.function.display().to_string();
    assert!(clif.contains("call"), "direct SchedTable/NativePointer calls must lower: {clif}");
    assert!(clif.contains("brif"), "the no-else pointer guard must branch in CLIF: {clif}");
}
