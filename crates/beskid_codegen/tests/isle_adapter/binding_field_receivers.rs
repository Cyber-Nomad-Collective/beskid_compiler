//! Slice `binding2` reproducers: method calls whose receiver is a nominal field of the implicit
//! method receiver, and array literals nested directly inside struct literal fields.

use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, build_typed_program, find_function_definition,
    find_function_definitions, find_node, isa, item_fixture_with_root, lower_syntax_program,
    parse_program_with_source_name, settings,
};

/// Build a multi-unit program whose entry unit lives at `entry_path` and whose dependencies are
/// resolved through the import closure, mirroring the corelib package layout.
fn imported_fixture(
    entry_path: &str,
    entry_source: &str,
    dependencies: &[(&str, &str)],
) -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let root = tempfile::tempdir().expect("project").keep();
    let entry_path = root.join(entry_path);
    let mut sources = vec![(entry_path.clone(), entry_source.to_owned())];
    sources.extend(dependencies.iter().map(|(path, source)| (root.join(path), (*source).to_owned())));
    for (path, source) in &sources {
        std::fs::create_dir_all(path.parent().expect("source parent")).expect("create source parent");
        std::fs::write(path, source).expect("write source");
    }
    let units = sources
        .iter()
        .map(|(path, source)| SourceUnit {
            logical_name: path.display().to_string(),
            origin_path: path.clone(),
            path: path.clone(),
            source: source.clone(),
            program: parse_program_with_source_name(path.to_str().expect("UTF-8 path"), source).expect("parse source"),
        })
        .collect::<Vec<_>>();
    let generation = SyntaxGenerationId(191);
    let project = ProjectSession::new(&*db, root.clone(), entry_path, "App".into(), "lock".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root },
            dependencies: Vec::new(),
        },
        Arc::from(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let entry_unit = typed.entry;
    let root = AstNodeKey { unit: entry_unit, generation, node: AstNodeId(0) };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("generation-safe imported input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

fn imported_unit_root(input: &CodegenInput<'_>, logical_name: &str) -> AstNodeKey {
    let unit = input
        .typed_program()
        .assembly
        .units
        .iter()
        .find(|unit| unit.logical_name.ends_with(logical_name))
        .expect("imported source unit");
    AstNodeKey {
        unit: SourceUnitId::new(input.database(), unit.path.clone()),
        generation: input.typed_program().generation,
        node: AstNodeId(0),
    }
}

const RESULT_SOURCE: &str = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }";

const LISTENER_SOURCE: &str = r#"
use Core.Results;
pub type TcpListener {
    word handle,
    pub Result<i64, string> LocalAddress() { return Result::Ok(7_i64); }
    pub Result<unit, string> Close() { return Result::Ok(()); }
}
"#;

#[test]
fn method_call_on_an_imported_nominal_field_of_the_implicit_receiver_lowers() {
    let server_source = r#"
use Core.Results;
use Network.Tcp.TcpListener;
pub type HttpServer {
    TcpListener listener,
    pub Result<i64, string> LocalAddress() {
        Result<i64, string> address = listener.LocalAddress();
        return match address {
            Result::Ok(value) => Result::Ok(value),
            Result::Error(_) => Result::Error("network"),
        };
    }
}
unit Main(HttpServer server) { server.LocalAddress(); return; }
"#;
    let (input, isa, root) = imported_fixture(
        "Http/Server.bd",
        server_source,
        &[("Network/Tcp/TcpListener.bd", LISTENER_SOURCE), ("Core/Results/Results.bd", RESULT_SOURCE)],
    );
    let server_method = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("HttpServer.LocalAddress method");
    let main = find_function_definition(input.database(), root).expect("Main function");
    let listener_root = imported_unit_root(&input, "Network/Tcp/TcpListener.bd");
    let listener_method = find_node(input.database(), listener_root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("TcpListener.LocalAddress method");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: server_method, symbol: "HttpServer_LocalAddress".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
            SyntaxModuleItem { key: listener_method, symbol: "TcpListener_LocalAddress".into() },
        ],
    )
    .expect("a method call on a nominal field of the implicit receiver lowers through the field load");

    let method = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("HttpServer_LocalAddress"))
        .expect("lowered HttpServer.LocalAddress");
    let clif = method.function.display().to_string();
    assert!(clif.contains("load.i64"), "receiver field must be loaded before the call: {clif}");
    assert!(clif.contains("call fn"), "the field receiver call must be a direct call: {clif}");
}

#[test]
fn unit_result_method_call_on_an_imported_nominal_field_of_the_implicit_receiver_lowers() {
    let server_source = r#"
use Core.Results;
use Network.Tcp.TcpListener;
pub type HttpServer {
    TcpListener listener,
    pub Result<unit, string> Close() {
        Result<unit, string> closed = listener.Close();
        return match closed {
            Result::Ok(_) => Result::Ok(()),
            Result::Error(_) => Result::Error("closed"),
        };
    }
}
unit Main(HttpServer server) { server.Close(); return; }
"#;
    let (input, isa, root) = imported_fixture(
        "Http/Server.bd",
        server_source,
        &[("Network/Tcp/TcpListener.bd", LISTENER_SOURCE), ("Core/Results/Results.bd", RESULT_SOURCE)],
    );
    let server_method = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("HttpServer.Close method");
    let main = find_function_definition(input.database(), root).expect("Main function");
    let listener_root = imported_unit_root(&input, "Network/Tcp/TcpListener.bd");
    let listener_methods = super::support::find_nodes_of_kind(
        input.database(),
        listener_root,
        beskid_queries::IndexedNodeKind::MethodDefinition,
    );
    assert_eq!(listener_methods.len(), 2, "TcpListener declares LocalAddress and Close");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: server_method, symbol: "HttpServer_Close".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
            SyntaxModuleItem { key: listener_methods[0], symbol: "TcpListener_LocalAddress".into() },
            SyntaxModuleItem { key: listener_methods[1], symbol: "TcpListener_Close".into() },
        ],
    )
    .expect("a unit-result method call on a nominal field of the implicit receiver lowers");
}

#[test]
fn array_literal_of_struct_literals_inside_a_struct_literal_field_lowers() {
    let (input, isa, root) = item_fixture_with_root(
        r#"
type Header { string name, string value, }
type Request { string method, Header[] headers, }
Request Build() {
    return Request { method: "GET", headers: [Header { name: "Host", value: "loopback.test" }] };
}
unit Main() { Build(); return; }
"#,
    );
    let functions = find_function_definitions(input.database(), root);
    let build = functions[0];
    let main = functions[1];

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: build, symbol: "Build".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("an array literal of struct literals lowers as a struct literal field");

    let build = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Build"))
        .expect("lowered Build function");
    let clif = build.function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_array_allocate_rooted"), "{clif}");
    assert!(clif.contains("beskid_rt_v5_array_write_barrier"), "{clif}");
}

#[test]
fn scalar_array_literal_inside_a_struct_literal_field_lowers() {
    let (input, isa, root) = item_fixture_with_root(
        r#"
type Packet { i64 kind, i64[] words, }
Packet Build() { return Packet { kind: 1_i64, words: [2_i64, 3_i64] }; }
unit Main() { Build(); return; }
"#,
    );
    let functions = find_function_definitions(input.database(), root);
    let build = functions[0];
    let main = functions[1];

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: build, symbol: "Build".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("a scalar array literal lowers as a struct literal field");
}
