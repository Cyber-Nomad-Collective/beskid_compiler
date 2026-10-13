//! A Mod invocation's semantic authority plans canonical Corelib routes in a real compilation.
use std::sync::Arc;

use beskid_analysis::mod_host::{ModSemanticAuthority, ModSyntaxAuthority, ModSyntaxRequest, ModSyntaxResponse};
use beskid_analysis::services::{PrepareOptions, ResolvedInput, resolve_input};
use beskid_queries::{
    BeskidDatabase, ModSemanticQueryAuthority, assemble_resolved_input_with_db, build_mod_authority_typed_program,
    build_typed_program, configure_db_for_project, project_session_for_planned_syntax_assembly,
};

/// Resolve Corelib from the shared sealed installation under test, never the developer's home.
fn isolated_corelib_toolchain() -> &'static std::path::Path {
    static HOME: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        let home = tempfile::tempdir().expect("create isolated BESKID_HOME");
        let corelib = beskid_tests_mod_host::shared_corelib_root().expect("provision the shared Corelib under test");
        // SAFETY: runs exactly once, before any test in this binary resolves a Corelib project.
        unsafe {
            std::env::set_var("BESKID_HOME", home.path().join("home"));
            std::env::set_var("BESKID_CONFIG_DIR", home.path().join("config"));
            std::env::set_var("BESKID_CORELIB_ROOT", &corelib);
        }
        home
    })
    .path()
}

fn resolved_app(root: &std::path::Path, db: &mut BeskidDatabase) -> ResolvedInput {
    let app = root.join("app");
    std::fs::create_dir_all(app.join("Src")).unwrap();
    std::fs::write(
        app.join("Routes.bproj"),
        "Routes {\n  name = \"Routes\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        app.join("Src/Main.bd"),
        "use Core.Results;\n\ni32 Main() {\n    Result<i32, string> value = Result::Ok(1);\n    return 0;\n}\n",
    )
    .unwrap();
    configure_db_for_project(&app);
    let resolved = resolve_input(
        Some(&app.join("Src/Main.bd")),
        Some(&app),
        Some("App"),
        None,
        beskid_analysis::projects::WorkspacePrepareOptions::default(),
    )
    .expect("resolve project");
    assemble_resolved_input_with_db(db, &resolved, &PrepareOptions::without_mods()).expect("assemble project")
}

fn plan_result_route(
    db: &BeskidDatabase,
    project: beskid_queries::ProjectSession,
    assembly: &beskid_analysis::projects::ProgramAssembly,
) -> Result<Vec<String>, String> {
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(db, project, assembly).map_err(|error| error.0)?;
    let issuer = 41;
    let ModSyntaxResponse::Node(Some(root)) =
        authority.syntax_query(issuer, &ModSyntaxRequest::Root).map_err(|error| error.0)?
    else {
        return Err("compilation root missing".into());
    };
    let paths = authority
        .plan_canonical_paths(issuer, &root, &["Core.Results.Result".into()])
        .map_err(|error| error.0)?;
    Ok(paths.into_iter().next().map(|path| path.segments).unwrap_or_default())
}

/// The Mod authority's program carries the Corelib source attestation the compilation uses, so
/// `Core.Results.Result` has a caller-visible route; a bare typed program attests no Corelib unit
/// and the same request finds no declaration.
#[test]
fn mod_authority_plans_the_canonical_core_results_route() {
    isolated_corelib_toolchain();
    let temp = tempfile::tempdir().unwrap();

    let mut db = BeskidDatabase::default();
    let resolved = resolved_app(temp.path(), &mut db);
    let assembly = Arc::new(resolved.assembly.clone().expect("assembly"));
    let plan = resolved.compile_plan.clone().expect("compile plan");
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &resolved.source_path, "routes".into())
            .unwrap();
    build_mod_authority_typed_program(&mut db, project, Arc::clone(&assembly)).expect("Mod authority program");
    let segments = plan_result_route(&db, project, &assembly).expect("canonical Core.Results.Result route");
    assert_eq!(segments.last().map(String::as_str), Some("Result"), "{segments:?}");
    assert!(segments.len() >= 2, "a module-qualified route: {segments:?}");

    let bare_root = tempfile::tempdir().unwrap();
    let mut bare_db = BeskidDatabase::default();
    let bare = resolved_app(bare_root.path(), &mut bare_db);
    let bare_assembly = Arc::new(bare.assembly.clone().expect("assembly"));
    let bare_plan = bare.compile_plan.clone().expect("compile plan");
    let bare_project = project_session_for_planned_syntax_assembly(
        &mut bare_db,
        &bare_assembly,
        &bare_plan,
        &bare.source_path,
        "routes".into(),
    )
    .unwrap();
    build_typed_program(&mut bare_db, bare_project, bare_assembly.generation, Arc::clone(&bare_assembly)).unwrap();
    assert!(
        plan_result_route(&bare_db, bare_project, &bare_assembly).is_err(),
        "without Corelib attestation the route is absent"
    );
}
