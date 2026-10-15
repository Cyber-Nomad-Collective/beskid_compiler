//! Production Mod build must produce an executable contract, not a source/object surrogate.
use std::{fs, path::PathBuf, process::Command};

#[test]
fn v06_real_compiler_sdk_collector_generator_builds_executable_descriptor() {
    let root = tempfile::tempdir().unwrap();
    let compiler = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let project = root.path().join("mod");
    fs::create_dir_all(project.join("Src")).unwrap();
    fs::copy(compiler.join("crates/beskid_tests_mods/fixtures/mods/native_sdk/Src/Mod.bd"), project.join("Src/Mod.bd"))
        .unwrap();
    let manifest = project.join("NativeMod.bproj");
    // The compiler SDK ships in the implicit Core Corelib closure; a second explicit path dependency on the
    // checkout package would duplicate `corelib_compiler_sdk` and `corelib_foundation` in the lock.
    fs::write(&manifest, "NativeMod { name = \"NativeMod\" version = \"0.6.0\" type = Mod root = \"Src\" mod { capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot] } }\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["dev", "mod", "rebuild"])
        .arg(&manifest)
        .args(["--offline", "--plain"])
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual SDK build prerequisite failed before executable qualification:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let descriptors: Vec<_> = walkdir::WalkDir::new(project.join(".beskid/obj/mods"))
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name() == "mod.descriptor.json")
        .collect();
    assert_eq!(descriptors.len(), 1, "one authoritative executable artifact descriptor required");
    let descriptor: serde_json::Value = serde_json::from_slice(&fs::read(descriptors[0].path()).unwrap()).unwrap();
    assert_eq!(descriptor["schemaVersion"], 3, "object-only schema1 does not prove executable Mod contracts");
    // Schema 3 identifies build tools by digest and version output; no tool executable is copied.
    let files = descriptor["files"].as_object().expect("closed evidence inventory");
    let tools = files.keys().filter(|name| name.starts_with("tools/")).cloned().collect::<Vec<_>>();
    assert_eq!(
        tools,
        ["tools/adapter-compiler-version", "tools/compiler-version", "tools/linker-version"],
        "tool evidence is digest-only"
    );
    let artifact_bytes: u64 = walkdir::WalkDir::new(descriptors[0].path().parent().unwrap())
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.metadata().unwrap().len())
        .sum();
    let compiler_bytes = fs::metadata(env!("CARGO_BIN_EXE_beskid_cli")).unwrap().len();
    assert!(artifact_bytes < compiler_bytes, "the artifact ({artifact_bytes} B) must not embed the compiler ({compiler_bytes} B)");
    assert_eq!(descriptor["artifactKind"], "executable-shared-library");
    assert!(descriptor.get("objectFile").is_none(), "relocatable objects cannot be executable authority");
    assert_eq!(descriptor["hostAbiVersion"], 2);
    assert_enum_adapters_read_tags_at_their_width(&descriptors[0].path().parent().unwrap().join("adapter-plan.json"));
    let registrations = descriptor["registrations"].as_array().expect("exact native registrations");
    assert_eq!(registrations.len(), 2, "actual Collector and Generator must both be exported");
    for family in ["Collector", "Generator"] {
        assert!(
            registrations
                .iter()
                .any(|registration| registration["contractId"].as_str().is_some_and(|id| id.ends_with(family))),
            "missing {family}"
        );
    }
}

#[test]
fn v06_qualified_mod_worker_generates_callable_and_generic_record_for_real_host() {
    let root = tempfile::tempdir().unwrap();
    let compiler = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let project = root.path().join("mod");
    fs::create_dir_all(project.join("Src")).unwrap();
    fs::copy(compiler.join("crates/beskid_tests_mods/fixtures/mods/native_sdk/Src/Mod.bd"), project.join("Src/Mod.bd"))
        .unwrap();
    fs::write(project.join("NativeMod.bproj"), "NativeMod { name = \"NativeMod\" version = \"0.6.0\" type = Mod root = \"Src\" mod { capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot] } }\n").unwrap();
    let host = root.path().join("host");
    fs::create_dir_all(host.join("Src")).unwrap();
    fs::write(host.join("Src/Main.bd"),"use Testing.Assert;\nunit Main() { GeneratedUnit(); SerializedRecord<u32> value = SerializedRecord<u32> { Value: 7 }; Assert.True(value.Value == 7, \"generated generic field\"); }\n").unwrap();
    let manifest = host.join("Host.bproj");
    fs::write(&manifest,format!("Host {{ name = \"Host\" version = \"0.6.0\" root = \"Src\" }}\ndependency \"NativeMod\" {{ source = path path = {:?} }}\ntarget \"main\" {{ kind = App entry = \"Main.bd\" }}\n",project.to_string_lossy())).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["run", "--project"])
        .arg(&manifest)
        .args(["--offline", "--plain"])
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "qualified native Mod must execute Collector/Generator and merge typed function/generic field before native host execution:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The Corelib Pest Mod contributes its regex combinator parsers as typed `FunctionDefinition`
/// syntax only. A host that depends on it receives the merged parsers at a fresh registered
/// generation, compiles them through the canonical path and runs them natively.
#[test]
fn v06_corelib_pest_mod_contributes_typed_combinators_for_real_host() {
    let root = tempfile::tempdir().unwrap();
    let compiler = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let source = compiler.join("corelib/mods/corelib_pest_gen/Src");
    let project = root.path().join("pest");
    fs::create_dir_all(project.join("Src")).unwrap();
    for file in ["Mod.bd", "Targets.bd", "Emit.bd", "PestSyntax.bd", "PestGrammar.bd"] {
        fs::copy(source.join(file), project.join("Src").join(file)).unwrap();
    }
    let emit = fs::read_to_string(project.join("Src/Emit.bd")).unwrap();
    assert!(emit.contains("Items.FunctionItem("), "the Pest Mod contributes typed function items");
    // Foundation and the compiler SDK ship in the implicit Core Corelib closure.
    fs::write(project.join("PestMod.bproj"), "PestMod { name = \"PestMod\" version = \"0.6.0\" type = Mod root = \"Src\" mod { capabilities = [read_project_sources, emit_syntax] } }\n").unwrap();
    let host = root.path().join("host");
    fs::create_dir_all(host.join("Src")).unwrap();
    fs::write(
        host.join("Src/Main.bd"),
        "use Testing.Assert;\nunit Main() {\n    Assert.True(Core.Text.Parser.IsOk(ParseDigit(Core.Text.Cursor.From(\"7\"))), \"generated digit parser accepts a digit\");\n    Assert.True(!Core.Text.Parser.IsOk(ParseDigit(Core.Text.Cursor.From(\"x\"))), \"generated digit parser rejects a letter\");\n    Assert.True(Core.Text.Parser.IsOk(ParsePat(Core.Text.Cursor.From(\"a(b|c)*[0-9]+\"))), \"generated pattern parser accepts a pattern\");\n    Assert.True(!Core.Text.Parser.IsOk(ParsePat(Core.Text.Cursor.From(\"\"))), \"generated pattern parser rejects an empty pattern\");\n    Core.Text.Cursor.TextCursor rest = Core.Text.Parser.RestOnOk(ParseLowerRun(Core.Text.Cursor.From(\"abc1\")), Core.Text.Cursor.From(\"\"));\n    Assert.True(Core.Text.Cursor.Position(rest) == 3, \"generated repetition consumes the whole run\");\n}\n",
    )
    .unwrap();
    let manifest = host.join("Host.bproj");
    fs::write(&manifest, format!("Host {{ name = \"Host\" version = \"0.6.0\" root = \"Src\" }}\ndependency \"PestMod\" {{ source = path path = {:?} }}\ntarget \"main\" {{ kind = App entry = \"Main.bd\" }}\n", project.to_string_lossy())).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["run", "--project"])
        .arg(&manifest)
        .args(["--offline", "--plain"])
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the Pest Mod must contribute typed combinator parsers that compile and run in the host:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Every enum the producer's adapter plan transports (all SDK syntax enums reachable from the
/// Generator contract) reads its canonical `i32` tag at exactly that width, and no variant payload
/// overlaps the tag bytes. A pointer-wide tag read would include a scalar payload packed beside
/// the tag and reject that variant as unknown.
fn assert_enum_adapters_read_tags_at_their_width(plan: &std::path::Path) {
    let plan: serde_json::Value = serde_json::from_slice(&fs::read(plan).expect("adapter plan evidence")).unwrap();
    let types = plan["types"].as_array().expect("adapter types");
    // A unit payload has no storage; its encoder never reads the (placeholder) offset.
    let is_unit = |id: u64| types.get(id as usize).is_some_and(|ty| ty["kind"].get("Scalar") == Some(&serde_json::json!(0)));
    let mut enums = 0;
    let mut variants = 0;
    for ty in types {
        let Some(body) = ty["kind"].get("Enum") else { continue };
        enums += 1;
        let name = ty["name"].as_str().unwrap();
        let tag_offset = body["tag_offset"].as_u64().unwrap();
        assert_eq!(body["tag_bytes"], 4, "enum `{name}` must read its i32 tag at width 4");
        for variant in body["variants"].as_array().unwrap() {
            variants += 1;
            for field in variant["fields"].as_array().unwrap() {
                if is_unit(field["type_id"].as_u64().unwrap()) {
                    continue;
                }
                let offset = field["offset"].as_u64().unwrap();
                assert!(
                    offset >= tag_offset + 4,
                    "`{name}::{}` payload at {offset} overlaps the tag at {tag_offset}",
                    variant["name"]
                );
            }
        }
    }
    assert!(enums >= 20 && variants >= 100, "the plan transports the SDK syntax enums ({enums} enums, {variants} variants)");
}
