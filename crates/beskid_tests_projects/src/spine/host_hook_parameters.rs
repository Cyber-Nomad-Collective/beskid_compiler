//! Host hooks with injected parameters lower through the real prepare spine.
//!
//! The codegen unit fixtures run only the hook lowering step; this conformance runs the whole
//! spine (expansion, Mod host, composition, query gate) and attaches the frozen composition plan
//! to the codegen input, so the plan and the codegen syntax are the products of one prepare.

use std::fs;

use beskid_analysis::services::{FrontEndOptions, PrepareOptions, resolve_input};
use beskid_queries::{configure_db_for_project, prepare_compilation_diagnostics};

use crate::projects::fixture_harness::{enter_mod_host, mod_host_for};
use crate::projects::with_cwd;
use beskid_tests_support::{temp_case_dir, write_project_manifest as write_manifest};

/// `Note` has a caller-information parameter: typing fills it at every call, which adds syntax
/// nodes after the composition plan was resolved (the 0.6.7-preview.3 failure: E4041
/// "composition plan disagrees with the validated snapshot"). `Early` puts such a call before the
/// host, the hook bodies put one before the generated hook argument types.
const SERVICES: &str = r#"
unit Note(string text, [CallerLineNumber] i64 line = 0) { return; }
unit Early() { Note("early"); }
pub contract Greeter { string Greet(string name); }
type PoliteGreeter : Greeter { pub string Greet(string name) { return "Good morning, ${name}."; } }
type Reception
{
    inject Greeter greeter;
    pub string Welcome(string name) { return this.greeter.Greet(name); }
}
"#;

/// Prepare `source` as the entry of a target of `kind` and lower `entrypoint` from the prepared
/// front end with its composition authority attached (attaching it validates the whole plan,
/// whichever entrypoint is lowered).
/// What to lower from the prepared front end.
enum Lowering<'a> {
    /// One function entrypoint, in the process database (`beskid run`).
    Function(&'a str),
    /// The selected `test` entries, in a database of their own (`beskid test`).
    Tests(&'a [&'a str]),
}

fn lower_through_spine(case: &str, kind: &str, source: &str, lowering: Lowering<'_>) {
    let root = temp_case_dir(case);
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("source root");
    write_manifest(
        &root,
        &format!(
            r#"
project {{
  name = "HostHooks"
  version = "0.1.0"
}}

target "main" {{
  kind = {kind}
  entry = "Main.bd"
}}
"#
        ),
    );
    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write entry");
    with_cwd(&root, || {
        let resolved = resolve_input(
            Some(&entry),
            Some(&root),
            None,
            None,
            beskid_analysis::projects::WorkspacePrepareOptions::default(),
        )
        .expect("resolve");
        configure_db_for_project(&root);
        let mod_host = enter_mod_host();
        let (prepared, _diagnostics, _fixes) = prepare_compilation_diagnostics(
            &resolved,
            PrepareOptions {
                front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                ..PrepareOptions::with_mod_host(mod_host_for(&mod_host, &resolved))
            },
            None,
        )
        .expect("executable gate prepare");
        let front = prepared.into_executable().expect("executable front end");
        let target = beskid_engine::host_runtime_target().expect("host ABI-v5 target");
        let functions = match lowering {
            Lowering::Function(entrypoint) => {
                beskid_engine::services::lower_prepared_syntax_entrypoint(&front, entrypoint, target)
                    .unwrap_or_else(|error| panic!("{case}: hook parameters lower with the frozen plan: {error:#}"))
                    .functions
                    .into_iter()
                    .map(|function| function.name)
                    .collect::<Vec<_>>()
            }
            Lowering::Tests(names) => {
                let names = names.iter().map(|name| (*name).to_string()).collect::<Vec<_>>();
                beskid_aot::prepared_syntax::lower_frozen_syntax_entrypoints(&front, &names, target)
                    .unwrap_or_else(|error| panic!("{case}: hook parameters lower with the frozen plan: {error}"))
                    .into_artifact()
                    .functions
                    .into_iter()
                    .map(|function| function.name)
                    .collect::<Vec<_>>()
            }
        };
        assert!(
            functions.iter().any(|name| name.contains("__BeskidHook_AppHost_")),
            "{case}: the generated hook functions are emitted: {functions:?}"
        );
    });
    let _ = fs::remove_dir_all(root);
}

fn app(case: &str, hooks: &str) {
    lower_through_spine(
        case,
        "App",
        &format!(
            r#"{SERVICES}
host AppHost()
{{
    registry {{ single PoliteGreeter for Greeter; single Reception; }}
{hooks}
}}
i32 Main()
{{
    launch AppHost();
    with Visit() {{ }}
    return 0;
}}
"#
        ),
        Lowering::Function("Main"),
    );
}

#[test]
fn startup_takes_a_contract_and_a_concrete_service() {
    app(
        "host_hooks_startup",
        r#"
    startup(Greeter greeter, Reception reception)
    {
        Note(greeter.Greet("Ada"));
        Note(reception.Welcome("Bob"));
    }
    scope Visit() { }
"#,
    );
}

#[test]
fn scope_init_and_dispose_take_a_contract_and_a_concrete_service() {
    app(
        "host_hooks_scope",
        r#"
    scope Visit()
    {
        init(Greeter greeter, Reception reception) { Note(greeter.Greet("Ada")); Note(reception.Welcome("Bob")); }
        dispose(Greeter greeter, Reception reception) { Note(greeter.Greet("Ada")); Note(reception.Welcome("Bob")); }
    }
"#,
    );
}

#[test]
fn global_init_and_dispose_take_a_contract_and_a_concrete_service() {
    app(
        "host_hooks_global",
        r#"
    init(Greeter greeter) { Note(greeter.Greet("Ada")); }
    dispose(Reception reception) { Note(reception.Welcome("Bob")); }
    scope Visit() { }
"#,
    );
}

/// The course's host-hooks lesson: a Test target whose test launches a host with a `startup`
/// parameter, lowered as `beskid test` lowers it.
#[test]
fn a_test_target_launches_a_host_whose_startup_takes_a_service() {
    for (case, parameter, call) in [
        ("host_hooks_test_contract", "Greeter greeter", "greeter.Greet(\"Ada\")"),
        ("host_hooks_test_concrete", "Reception reception", "reception.Welcome(\"Ada\")"),
    ] {
        lower_through_spine(
            case,
            "Test",
            &format!(
                r#"{SERVICES}
host AppHost()
{{
    registry {{ single PoliteGreeter for Greeter; single Reception; }}
    startup({parameter}) {{ Note({call}); }}
}}
test StartupGreets {{ launch AppHost(); }}
"#
            ),
            Lowering::Tests(&["StartupGreets"]),
        );
    }
}

/// The run-time fixture `beskid_e2e_tests/fixtures/composition_hooks` prepares and lowers (its
/// output is checked by `composition_hooks_run_in_order_on_every_exit`). This harness has no
/// Corelib, so a local `WriteLine` stands in for `Core.Output`.
#[test]
fn the_composition_hooks_run_time_fixture_lowers() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../beskid_e2e_tests/fixtures/composition_hooks/Src/Main.bd");
    let source = fs::read_to_string(&fixture).expect("read the composition_hooks fixture");
    assert!(source.contains("use Core.Output;"), "the fixture writes through Core.Output");
    let source = source.replace("use Core.Output;", "unit WriteLine(string text) { return; }").replace("Output.WriteLine(", "WriteLine(");
    lower_through_spine("host_hooks_e2e_fixture", "App", &source, Lowering::Function("Main"));
}
