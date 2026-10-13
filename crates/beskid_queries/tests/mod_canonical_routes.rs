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
    resolved_app_with_main(
        root,
        db,
        "use Core.Results;\n\ni32 Main() {\n    Result<i32, string> value = Result::Ok(1);\n    return 0;\n}\n",
    )
}

fn resolved_app_with_main(root: &std::path::Path, db: &mut BeskidDatabase, main: &str) -> ResolvedInput {
    let app = root.join("app");
    std::fs::create_dir_all(app.join("Src")).unwrap();
    std::fs::write(
        app.join("Routes.bproj"),
        "Routes {\n  name = \"Routes\"\n  version = \"0.1.0\"\n}\n\ntarget \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
    )
    .unwrap();
    std::fs::write(app.join("Src/Main.bd"), main).unwrap();
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
    plan_route(db, project, assembly, "Core.Results.Result")
}

fn plan_route(
    db: &BeskidDatabase,
    project: beskid_queries::ProjectSession,
    assembly: &beskid_analysis::projects::ProgramAssembly,
    selector: &str,
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
        .plan_canonical_paths(issuer, &root, &[selector.into()])
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

/// The Serialization Mod's generated adapters route to canonical declarations across the whole
/// `corelib_serialization` package, the package that declares the Mod. A program that imports only
/// `Core.Serialization.Contracts` still holds every unit of that package in its assembly, so the
/// Mod authority plans `Core.Serialization.Reader.Reader` (and the other serialization routes)
/// although the program never imports `Core.Serialization.Reader`.
#[test]
fn mod_companion_package_routes_do_not_depend_on_user_imports() {
    isolated_corelib_toolchain();
    let temp = tempfile::tempdir().unwrap();
    let mut db = BeskidDatabase::default();
    let resolved = resolved_app_with_main(
        temp.path(),
        &mut db,
        "use Core.Serialization.Contracts;\n\n[Serialize]\npub type Point {\n    pub i32 x,\n    pub i32 y,\n}\n\ni32 Main() {\n    Point point = Point { x: 1, y: 2 };\n    return point.x - 1;\n}\n",
    );
    let assembly = Arc::new(resolved.assembly.clone().expect("assembly"));
    for logical in [
        "Core/Serialization/Contracts.bd",
        "Core/Serialization/Reader.bd",
        "Core/Serialization/Descriptors.bd",
        "Core/Serialization/Decoding.bd",
        "Core/Serialization/Registry.bd",
    ] {
        assert!(
            assembly.units.iter().any(|unit| unit.path.ends_with(logical)),
            "the companion package unit {logical} is in the assembly although Main imports only Contracts"
        );
    }
    let plan = resolved.compile_plan.clone().expect("compile plan");
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &resolved.source_path, "routes".into())
            .unwrap();
    build_mod_authority_typed_program(&mut db, project, Arc::clone(&assembly)).expect("Mod authority program");
    for selector in [
        "Core.Serialization.Contracts.Serializable",
        "Core.Serialization.Reader.Reader",
        "Core.Serialization.Reader.ReadField",
        "Core.Serialization.Descriptors.CompiledShape",
        "Core.Serialization.Errors.SerializationError",
        "Core.Serialization.Limits.SerializationLimits",
    ] {
        let segments = plan_route(&db, project, &assembly, selector)
            .unwrap_or_else(|error| {
                let units = assembly
                    .units
                    .iter()
                    .filter(|unit| unit.path.to_string_lossy().contains("Serialization/"))
                    .map(|unit| format!("{} (origin {})", unit.path.display(), unit.origin_path.display()))
                    .collect::<Vec<_>>();
                panic!("canonical route `{selector}` must be planned: {error}\nserialization units: {units:#?}")
            });
        let name = selector.rsplit('.').next().unwrap();
        assert_eq!(segments.last().map(String::as_str), Some(name), "{selector}: {segments:?}");
    }
}

/// A program that never reaches `corelib_serialization` does not activate the Serialization Mod,
/// so its assembly gains none of that package's units.
#[test]
fn unreached_mod_companion_package_stays_out_of_the_assembly() {
    isolated_corelib_toolchain();
    let temp = tempfile::tempdir().unwrap();
    let mut db = BeskidDatabase::default();
    let resolved = resolved_app(temp.path(), &mut db);
    let assembly = resolved.assembly.clone().expect("assembly");
    assert!(
        !assembly.units.iter().any(|unit| unit.path.to_string_lossy().contains("Core/Serialization/")),
        "no serialization unit joins an assembly that never reaches corelib_serialization"
    );
}
