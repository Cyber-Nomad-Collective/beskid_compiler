//! Registered syntax authority must reach the production Rust backend, not fixture text.
#[path = "codegen_input/support.rs"]
#[allow(unused_imports, dead_code)]
mod support;
use beskid_abi::abi_v5::AbiManifestV5;
use beskid_codegen::{
    CodegenInput,
    backend::{Backend, BackendArtifact, RustSourceBackend},
    module_emission::SyntaxModuleItem,
};
use beskid_queries::{AstNodeKey, IndexedNodeKind, child_nodes, item_name, node_kind};
use std::sync::Arc;

fn items(db: &dyn beskid_queries::Db, root: AstNodeKey) -> Vec<SyntaxModuleItem> {
    fn visit(db: &dyn beskid_queries::Db, key: AstNodeKey, out: &mut Vec<SyntaxModuleItem>) {
        if node_kind(db, key).expect("canonical node kind") == Some(IndexedNodeKind::FunctionDefinition) {
            if let Some(name) = item_name(db, key).expect("canonical item fact") {
                if name.starts_with("Export") {
                    out.push(SyntaxModuleItem { key, symbol: name.to_string() });
                }
            }
        }
        for child in child_nodes(db, key).expect("registered child facts").unwrap_or_default().iter().copied() {
            visit(db, child, out);
        }
    }
    let mut out = Vec::new();
    visit(db, root, &mut out);
    out
}

#[test]
fn production_rust_backend_emits_from_registered_manual_export_inputs() {
    let source = include_str!("../../beskid_tests_interop/fixtures/glue/manual/src/ManualExport.bd");
    let (db, typed, root, target) = support::input_fixture_with_source(source);
    let selected = items(&db, root);
    assert_eq!(
        selected.len(),
        17,
        "the real fixture must retain its 17 typed source functions; ordinary u64 is not branded opaque ownership"
    );
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("registered input");
    let artifact = RustSourceBackend { native_library: "manual_exports".into() }
        .lower(&input, &selected)
        .expect("required production Rust Glue emitter");
    match artifact {
        BackendArtifact::RustSource(value) => {
            value.validate().expect("closed deterministic artifact");
            assert_eq!(value.manifest.bindings.len(), 17);
            for path in ["src/peer.rs", "vendor/beskid_glue/src/peer.rs", "vendor/beskid_serialization/src/lib.rs"] {
                assert!(value.files.iter().any(|file| file.path == path), "missing generated dependency {path}");
            }
            let consumer = tempfile::tempdir().unwrap();
            for file in &value.files {
                let path = consumer.path().join(&file.path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, &file.bytes).unwrap();
            }
            let built = std::process::Command::new(env!("CARGO"))
                .args(["check", "--offline", "--all-targets"])
                .current_dir(consumer.path())
                .env("CARGO_TARGET_DIR", consumer.path().join("target"))
                .output()
                .expect("build emitted standalone consumer");
            assert!(
                built.status.success(),
                "generated standalone peer must compile:\n{}\n{}",
                String::from_utf8_lossy(&built.stdout),
                String::from_utf8_lossy(&built.stderr)
            );
            let mut reordered = selected.clone();
            reordered.reverse();
            let BackendArtifact::RustSource(repeated) = (RustSourceBackend { native_library: "manual_exports".into() })
                .lower(&input, &reordered)
                .expect("ordered emission")
            else {
                panic!("wrong artifact")
            };
            assert_eq!(value, repeated, "selection order must not change generated files or identities");
            assert!(value.manifest.required_native_adapters.iter().any(|a| a.ends_with(":managed_return_v1")));
            let mut corrupt = value.clone();
            corrupt.files[0].bytes.push(0);
            assert!(corrupt.validate().is_err(), "tampered bytes must fail closure");
            let mut corrupt = value.clone();
            corrupt.manifest.bindings[0].symbol.push('x');
            assert!(corrupt.validate().is_err(), "substituted binding must fail closure");
        }
        other => panic!("required Rust backend emitted the wrong artifact: {other:?}"),
    }
}

#[test]
fn checked_exports_validate_scalars_and_resolve_managed_inputs_before_effects() {
    let source = r#"
        [Export(Abi:"C",Symbol:"count_views")] pub i64 ExportCount(string first, string second) { return 7; }
        [Export(Abi:"C",Symbol:"bool_echo")] pub bool ExportBool(bool value) { return value; }
        [Export(Abi:"C",Symbol:"char_echo")] pub char ExportChar(char value) { return value; }
        [Export(Abi:"C",Symbol:"unit_call")] pub unit ExportUnit() { return; }
    "#;
    let (db, typed, root, target) = support::input_fixture_with_source(source);
    let selected = items(&db, root);
    assert_eq!(selected.len(), 4);
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .unwrap();
    let BackendArtifact::RustSource(artifact) =
        (RustSourceBackend { native_library: "checked_exports".into() }).lower(&input, &selected).unwrap()
    else {
        panic!("wrong backend");
    };
    for (path, expected) in [
        ("vendor/beskid_glue/src/peer.rs", include_bytes!("../../beskid_glue/src/peer.rs").as_slice()),
        ("vendor/beskid_glue/src/codec.rs", include_bytes!("../../beskid_glue/src/codec.rs").as_slice()),
        ("vendor/beskid_serialization/src/lib.rs", include_bytes!("../../beskid_serialization/src/lib.rs").as_slice()),
    ] {
        assert_eq!(
            artifact.files.iter().find(|file| file.path == path).unwrap().bytes,
            expected,
            "generated protocol closure must contain exact canonical source"
        );
    }
    let peer =
        std::str::from_utf8(&artifact.files.iter().find(|file| file.path == "src/peer.rs").unwrap().bytes).unwrap();
    assert!(peer.contains("pub trait Service"));
    assert!(peer.contains("Value::Boolean(v)"));
    assert!(peer.contains("Value::Scalar(v)"));
    assert!(peer.contains("catch_unwind"));
    assert!(peer.contains("serve_stdio"));
    assert!(peer.contains("values.len()!="));
    let cargo =
        std::str::from_utf8(&artifact.files.iter().find(|file| file.path == "Cargo.toml").unwrap().bytes).unwrap();
    assert!(cargo.contains("vendor/beskid_glue"));
    assert!(!cargo.contains("beskid_aot"));
    let native =
        std::str::from_utf8(&artifact.files.iter().find(|file| file.path == "native/adapters.c").unwrap().bytes)
            .unwrap();
    let header =
        std::str::from_utf8(&artifact.files.iter().find(|file| file.path == "include/beskid_glue.h").unwrap().bytes)
            .unwrap();
    for binding in &artifact.manifest.bindings {
        let symbol = beskid_codegen::glue::artifact::checked_invocation_symbol(binding);
        assert!(native.contains(&format!("int32_t {symbol}(")));
        assert!(header.contains(&format!("int32_t {symbol}(")));
    }
    assert!(native.contains("if(arg_0_0>1) return 1;"));
    assert!(native.contains("UINT32_C(0xd800)"));
    assert!(!native.contains("int64_t count_views("), "managed-input scalar return must have only checked transport");
    let binding = artifact.manifest.bindings.iter().find(|binding| binding.symbol == "count_views").unwrap();
    let wrapper = native
        .split(&format!("int32_t {}(", beskid_codegen::glue::artifact::checked_invocation_symbol(binding)))
        .nth(1)
        .unwrap();
    let admission = wrapper.find("beskid_glue_v1_owner_validate_binding").unwrap();
    let second_allocation = wrapper.find("beskid_glue_v1_input_utf8(arg_1_0").unwrap();
    let first_resolve = wrapper.find("managed_0=beskid_rt_v5_gc_resolve_handle(root_0)").unwrap();
    let invocation = wrapper.find(&format!("result={}(", binding.body_symbol)).unwrap();
    assert!(admission < second_allocation && second_allocation < first_resolve && first_resolve < invocation);
    assert!(wrapper.contains("gc_unroot_handle(root_0)"));
    assert!(wrapper.contains("gc_unroot_handle(root_1)"));
}

#[test]
fn production_rust_backend_rejects_foreign_item_generation() {
    let source = "[Export(Abi:\"C\", Symbol:\"foreign_i32\")] pub i32 ExportI32(i32 value) { return value; }";
    let (db, typed, root, target) = support::input_fixture_with_source(source);
    let mut selected = items(&db, root);
    selected[0].key.generation = beskid_queries::SyntaxGenerationId(typed.generation.0 + 1);
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("registered input");
    let error = RustSourceBackend { native_library: "manual_exports".into() }
        .lower(&input, &selected)
        .expect_err("stale selected item must fail before emission");
    assert!(matches!(error, beskid_codegen::backend::BackendError::StaleRustGlueItem { .. }));
}

#[test]
fn registered_managed_exports_emit_normalized_adapters_and_compiled_shapes() {
    let source = "[Export(Abi:\"C\", Symbol:\"copied_utf8\")] pub string ExportUTF8(string value) { return value; } [Export(Abi:\"C\", Symbol:\"copied_bytes\")] pub u8[] ExportBytes(u8[] value) { return value; }";
    let (db, typed, root, target) = support::input_fixture_with_source(source);
    let selected = items(&db, root);
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("registered input");
    let BackendArtifact::RustSource(artifact) = RustSourceBackend { native_library: "managed_exports".into() }
        .lower(&input, &selected)
        .expect("required managed adapters")
    else {
        panic!("wrong artifact")
    };
    let native = artifact
        .files
        .iter()
        .find(|file| file.path == "native/adapters.c")
        .expect("normalized managed native adapters must actually be emitted");
    let native = std::str::from_utf8(&native.bytes).expect("generated native source");
    assert!(native.contains("beskid_glue_v1_owner_copy"));
    assert!(
        native.contains("beskid_glue_artifact_v1_initialize"),
        "generated wrappers require private producer/loader-issued initialization"
    );
    assert!(
        !native.contains("beskid_glue_v1_bind_compiled_shapes"),
        "a DLL must not qualify its own JSON by presenting a matching hash"
    );
    assert!(native.contains("__beskid_glue_body_"));
    assert!(native.contains("copied_utf8") && native.contains("copied_bytes"));
    assert!(native.contains("beskid_glue_v1_input_utf8") && native.contains("beskid_glue_v1_input_bytes"));
    assert!(native.contains("beskid_glue_v1_result_utf8") && native.contains("beskid_glue_v1_result_bytes"));
    let release = beskid_codegen::glue::artifact::owned_release_symbol("managed_exports");
    assert!(native.contains(&format!("int32_t {release}(uint64_t token)")));
    assert!(native.contains("beskid_glue_v1_owner_release_token(library,token)"));
    let rust = artifact.files.iter().find(|file| file.path == "src/lib.rs").unwrap();
    let rust = std::str::from_utf8(&rust.bytes).unwrap();
    assert!(rust.contains(&release));
    assert!(!rust.contains("beskid_owned_release"));
    assert_ne!(release, beskid_codegen::glue::artifact::owned_release_symbol("another_library"));
    assert!(artifact.files.iter().any(|file| file.path == "native/compiled-shapes.json"));
}

#[test]
fn generated_peer_executes_typed_dispatch_and_denies_before_effects() {
    let source = r#"[Export(Abi:"C",Symbol:"echo_i8")] pub i8 ExportI8(i8 value) { return value; }"#;
    let (db, typed, root, target) = support::input_fixture_with_source(source);
    let selected = items(&db, root);
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .unwrap();
    let BackendArtifact::RustSource(artifact) =
        (RustSourceBackend { native_library: "dispatch_fixture".into() }).lower(&input, &selected).unwrap()
    else {
        panic!("wrong backend")
    };
    let consumer = tempfile::tempdir().unwrap();
    for file in &artifact.files {
        let path = consumer.path().join(&file.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &file.bytes).unwrap();
    }
    let gate = consumer.path().join("src/bin/peer_dispatch_gate.rs");
    std::fs::create_dir_all(gate.parent().unwrap()).unwrap();
    std::fs::write(gate, r#"
#[path = "../peer.rs"] mod peer;
use beskid_serialization::{DataValue as V, SequenceKind};
struct Service { calls: usize }
impl peer::Service for Service {
    fn binding_0(&mut self, value: i8) -> Result<i8, V> { self.calls += 1; Ok(value) }
}
fn argument(value: V) -> V { V::Sequence { kind: SequenceKind::Array, values: vec![value] } }
fn main() {}
#[test]
fn exact_width_range_and_arity_precede_service_effects() {
    let mut service = Service { calls: 0 };
    for value in [V::Signed { value: 128, width: 8 }, V::Signed { value: 1, width: 16 }, V::Unsigned { value: 1, width: 8 }, V::Boolean(true)] {
        assert!(peer::dispatch(&mut service, 0, argument(value)).is_err());
    }
    assert!(peer::dispatch(&mut service, 0, V::Sequence {kind: SequenceKind::Array, values: vec![]}).is_err());
    assert!(peer::dispatch(&mut service, 1, argument(V::Signed {value: 7, width: 8})).is_err());
    assert_eq!(service.calls, 0);
    assert_eq!(peer::dispatch(&mut service, 0, argument(V::Signed {value: -128, width: 8})), Ok(V::Signed {value: -128, width: 8}));
    assert_eq!(service.calls, 1);
}
"#).unwrap();
    let built = std::process::Command::new(env!("CARGO"))
        .args(["test", "--offline", "--bin", "peer_dispatch_gate"])
        .current_dir(consumer.path())
        .env("CARGO_TARGET_DIR", consumer.path().join("target"))
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "generated typed peer dispatch must execute:\n{}\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
}
