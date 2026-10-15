//! SER-04, SER-02 (installed half), MOD-06-01 and MOD-06-02: the installed CLI running real native
//! Mods (`language-meta--metaprogramming--serialization`, `compiler--compiler-mods--mod-host-bridge`,
//! `compiler--compiler-mods--typed-emitter-and-transforms`).
//!
//! Fixture Mods are embedded here (not read from the checkout) and built by the installed CLI in a
//! private home. Library-level arena and dispatch tests stay in `beskid_analysis` as related evidence.
use beskid_tests_release::installed::{Cli, Home, transcript};
use beskid_tests_release::project::write_app;
use std::path::{Path, PathBuf};
use std::process::Output;

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
fn path_text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

/// The reviewed compiler-sdk Mod fixture: an exact Collector naming one target and a Generator that
/// contributes a typed `GeneratedUnit()` function plus a generic `SerializedRecord<T>` record.
const NATIVE_MOD_SOURCE: &str = r#"use Beskid.Compiler.Collect;
use Beskid.Compiler.Emitter.Nodes as EmitterNodes;
use Beskid.Compiler.Emitter.Items;
use Beskid.Compiler.Emitter.Contribution;
use Beskid.Syntax.Nodes;

pub type ExactCollector : Collector {
    pub CollectTargetSet Collect(CollectRequest request) {
        return CollectTargetSet { targetIds: ["Native.GeneratedUnit"] };
    }
}

pub type ExactGenerator : Generator {
    pub GeneratedSyntaxContribution Generate(GenerationRequest request) {
        NodeSpan origin = NodeSpan { start: 0, end: 0, lineStart: 1, columnStart: 1, lineEnd: 1, columnEnd: 1 };
        FunctionDefinition definition = FunctionDefinition {
            attributes: EmitterNodes.EmptyAttributes(),
            visibility: EmitterNodes.PubVisibility(origin, 0),
            name: EmitterNodes.PubIdentifier("GeneratedUnit", origin, 0),
            generics: SpannedIdentifierList { items: [] },
            whereBounds: WhereBoundList { items: [] },
            parameters: SpannedParameterList { items: [] },
            parameterDocs: OptionalLeadingDocCommentList { items: [] },
            returnType: OptionalSpannedType::None,
            body: SpannedBlock { node: Block { statements: SpannedStatementList { items: [] } }, span: origin, id: 0 },
        };
        SpannedIdentifier generic = EmitterNodes.PubIdentifier("T", origin, 1);
        SpannedType fieldType = SpannedType { node: Type::Complex(SpannedPath { node: Path { segments: SpannedPathSegmentList { items: [SpannedPathSegment { node: PathSegment { name: generic, typeArgs: SpannedTypeList { items: [] } }, span: origin, id: 2 }] } }, span: origin, id: 3 }), span: origin, id: 4 };
        Field field = Field { attributes: EmitterNodes.EmptyAttributes(), visibility: EmitterNodes.PubVisibility(origin, 5), kind: FieldKind::Value, eventCapacity: Optionalusize::None, injectQualifier: OptionalInjectQualifier::None, name: EmitterNodes.PubIdentifier("Value", origin, 6), ty: fieldType };
        TypeDefinition record = TypeDefinition { attributes: EmitterNodes.EmptyAttributes(), visibility: EmitterNodes.PubVisibility(origin, 7), name: EmitterNodes.PubIdentifier("SerializedRecord", origin, 8), generics: SpannedIdentifierList { items: [generic] }, conformances: SpannedPathList { items: [] }, fields: SpannedFieldList { items: [SpannedField { node: field, span: origin, id: 9 }] }, fieldDocs: OptionalLeadingDocCommentList { items: [OptionalLeadingDocComment::None] }, methods: SpannedMethodDefinitionList { items: [] }, methodDocs: OptionalLeadingDocCommentList { items: [] }, associatedTypeBindings: SpannedAssociatedTypeBindingList { items: [] } };
        return Beskid.Compiler.Emitter.Contribution.From([
            Beskid.Compiler.Emitter.Items.FunctionItem(SpannedFunctionDefinition { node: definition, span: origin, id: 0 }),
            Beskid.Compiler.Emitter.Items.TypeItem(SpannedTypeDefinition { node: record, span: origin, id: 10 })
        ]);
    }
}

pub type ExactCollectorFactory : ModFactory<ExactCollector> {
    pub ExactCollector Create(CollectRequest request) {
        return ExactCollector {};
    }
}

pub type ExactGeneratorFactory : ModFactory<ExactGenerator> {
    pub ExactGenerator Create(CollectRequest request) {
        return ExactGenerator {};
    }
}
"#;

/// A Mod whose Generator tries to answer with source text. The 0.6 SDK has no code-output
/// contribution (`GeneratedSyntaxContribution` carries typed items and bindings only), so the
/// attempt cannot reach the host: it is rejected while the Mod is compiled, before any lowering.
const CODE_OUTPUT_MOD_SOURCE: &str = r#"use Beskid.Compiler.Collect;
use Beskid.Compiler.Emitter.Contribution;

pub type TextCollector : Collector {
    pub CollectTargetSet Collect(CollectRequest request) {
        return CollectTargetSet { targetIds: ["Text.GeneratedUnit"] };
    }
}

pub type TextGenerator : Generator {
    pub GeneratedSyntaxContribution Generate(GenerationRequest request) {
        return Beskid.Compiler.Emitter.Contribution.FromCodeOutputs([
            Beskid.Compiler.Emitter.Contribution.CodeOutput("Generated", "Generated.bd",
                Beskid.Compiler.Emitter.Contribution.CodeBody("beskid", "pub i32 GeneratedUnit() { return 1; }"))
        ]);
    }
}

pub type TextCollectorFactory : ModFactory<TextCollector> {
    pub TextCollector Create(CollectRequest request) {
        return TextCollector {};
    }
}

pub type TextGeneratorFactory : ModFactory<TextGenerator> {
    pub TextGenerator Create(CollectRequest request) {
        return TextGenerator {};
    }
}
"#;

/// A Mod whose Generator binds a typed item to a semantic handle the host never issued (a stale or
/// forged handle). The host must reject the contribution before lowering.
const FORGED_HANDLE_MOD_SOURCE: &str = r#"use Beskid.Compiler.Collect;
use Beskid.Compiler.Emitter.Nodes as EmitterNodes;
use Beskid.Compiler.Emitter.Items;
use Beskid.Compiler.Emitter.Contribution;
use Beskid.Compiler.Semantic;
use Beskid.Syntax.Nodes;

pub type ForgedCollector : Collector {
    pub CollectTargetSet Collect(CollectRequest request) {
        return CollectTargetSet { targetIds: ["Forged.GeneratedUnit"] };
    }
}

pub type ForgedGenerator : Generator {
    pub GeneratedSyntaxContribution Generate(GenerationRequest request) {
        NodeSpan origin = NodeSpan { start: 0, end: 0, lineStart: 1, columnStart: 1, lineEnd: 1, columnEnd: 1 };
        FunctionDefinition definition = FunctionDefinition {
            attributes: EmitterNodes.EmptyAttributes(),
            visibility: EmitterNodes.PubVisibility(origin, 0),
            name: EmitterNodes.PubIdentifier("GeneratedUnit", origin, 0),
            generics: SpannedIdentifierList { items: [] },
            whereBounds: WhereBoundList { items: [] },
            parameters: SpannedParameterList { items: [] },
            parameterDocs: OptionalLeadingDocCommentList { items: [] },
            returnType: OptionalSpannedType::None,
            body: SpannedBlock { node: Block { statements: SpannedStatementList { items: [] } }, span: origin, id: 0 },
        };
        GeneratedSyntaxContribution typed = Beskid.Compiler.Emitter.Contribution.From([
            Beskid.Compiler.Emitter.Items.FunctionItem(SpannedFunctionDefinition { node: definition, span: origin, id: 0 })
        ]);
        return Beskid.Compiler.Emitter.Contribution.BindTarget(typed, 0_u32, Beskid.Compiler.Semantic.HandleValue(9223372036854775807_u64));
    }
}

pub type ForgedCollectorFactory : ModFactory<ForgedCollector> {
    pub ForgedCollector Create(CollectRequest request) {
        return ForgedCollector {};
    }
}

pub type ForgedGeneratorFactory : ModFactory<ForgedGenerator> {
    pub ForgedGenerator Create(CollectRequest request) {
        return ForgedGenerator {};
    }
}
"#;

fn mod_manifest(name: &str) -> String {
    format!("{name} {{ name = \"{name}\" version = \"0.6.0\" type = Mod root = \"Src\" mod {{ capabilities = [read_project_sources, emit_syntax, query_semantic_snapshot] }} }}\n")
}

/// Write a Mod project and a host application depending on it; returns the host manifest.
fn mod_and_host(home: &Home, mod_name: &str, mod_source: &str, host_main: &str) -> (PathBuf, PathBuf) {
    let project = home.work().join(mod_name);
    std::fs::create_dir_all(project.join("Src")).unwrap();
    std::fs::write(project.join("Src/Mod.bd"), mod_source).unwrap();
    std::fs::write(project.join(format!("{mod_name}.bproj")), mod_manifest(mod_name)).unwrap();
    let host = home.work().join("Host");
    std::fs::create_dir_all(host.join("Src")).unwrap();
    std::fs::write(host.join("Src/Main.bd"), host_main).unwrap();
    let manifest = host.join("Host.bproj");
    std::fs::write(
        &manifest,
        format!("Host {{ name = \"Host\" version = \"0.6.0\" root = \"Src\" }}\ndependency \"{mod_name}\" {{ source = path path = \"../{mod_name}\" }}\ntarget \"main\" {{ kind = App entry = \"Main.bd\" }}\n"),
    )
    .unwrap();
    (project, manifest)
}

fn run(cli: &Cli, home: &Home, args: &[&str]) -> Output {
    cli.run(cli.command(home).args(args))
}

fn find_descriptor(root: &Path) -> PathBuf {
    fn walk(at: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(at) else { return };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.file_name().is_some_and(|name| name == "mod.descriptor.json") {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&root.join(".beskid/obj/mods"), &mut found);
    assert_eq!(found.len(), 1, "exactly one Mod artifact descriptor exists under {}: {found:?}", root.display());
    found.remove(0)
}

// ---------------------------------------------------------------------------------------------
// SER-04 Source output rejected before lowering

#[test]
fn ser04_installed_build_rejects_generator_code_string_output_before_lowering() {
    let cli = Cli::qualified();
    let home = Home::new();
    let (_mod, manifest) = mod_and_host(&home, "TextMod", CODE_OUTPUT_MOD_SOURCE, "i32 Main() { return GeneratedUnit() - 1; }\n");
    let executable = home.work().join(beskid_tests_release::installed::executable_name("host-out"));
    let output = run(cli, &home, &["build", "--offline", "--plain", "--project", path_text(&manifest), "--output", path_text(&executable)]);
    assert_eq!(output.status.code(), Some(1), "a Generator answering with source text must fail before lowering:\n{}", transcript(&output));
    let diagnostic = text(&output.stderr);
    let lowered = diagnostic.to_ascii_lowercase();
    assert!(lowered.contains("textmod"), "the diagnostic names the Mod:\n{diagnostic}");
    assert!(
        diagnostic.contains("FromCodeOutputs") && lowered.contains("contribution"),
        "the installed SDK exposes no code-output contribution, so the attempt is rejected at the SDK surface:\n{diagnostic}"
    );
    assert!(!lowered.contains("generate from mods ("), "no Generator ran on a code-output contribution:\n{diagnostic}");
    assert!(!executable.exists(), "no executable is produced");
    assert!(!home.work().join("Host/Src/Generated.bd").exists() && !home.work().join("Host/Generated").exists(), "no source text is written into the host");
}

#[test]
fn ser04_installed_build_rejects_forged_semantic_handle_before_lowering() {
    let cli = Cli::qualified();
    let home = Home::new();
    let (_mod, manifest) = mod_and_host(&home, "ForgedMod", FORGED_HANDLE_MOD_SOURCE, "i32 Main() { GeneratedUnit(); return 0; }\n");
    let executable = home.work().join(beskid_tests_release::installed::executable_name("host-out"));
    let output = run(cli, &home, &["build", "--offline", "--plain", "--project", path_text(&manifest), "--output", path_text(&executable)]);
    assert_eq!(output.status.code(), Some(1), "a contribution bound to a handle the host never issued must fail before lowering:\n{}", transcript(&output));
    let diagnostic = text(&output.stderr).to_ascii_lowercase();
    assert!(diagnostic.contains("forgedmod"), "the diagnostic names the Mod:\n{diagnostic}");
    // The Mod project itself is compiled (and lowered) before it runs; the host program is not.
    assert!(diagnostic.contains("stale") || diagnostic.contains("foreign"), "the host rejects the never-issued handle:\n{diagnostic}");
    assert!(!executable.exists(), "no executable is produced");
}

// ---------------------------------------------------------------------------------------------
// SER-02 Unsupported shape diagnostic at the field span (installed `check`)

#[test]
fn ser02_installed_check_rejects_pointer_field_at_declaration_span() {
    let cli = Cli::qualified();
    let home = Home::new();
    let source = "use Core.Serialization.Contracts;\n\n[Serialize]\npub type Handle {\n    pub i32 count,\n    pub pointer raw,\n}\n\ni32 Main() { return 0; }\n";
    let manifest = write_app(&home.work(), "Pointer", source);
    let output = run(cli, &home, &["check", "--plain", "--offline", "--project", path_text(&manifest)]);
    assert_eq!(output.status.code(), Some(1), "a pointer field under [Serialize] fails check:\n{}", transcript(&output));
    let combined = format!("{}{}", text(&output.stdout), text(&output.stderr));
    let lowered = combined.to_ascii_lowercase();
    assert!(lowered.contains("pointer"), "the diagnostic names the unsupported shape:\n{combined}");
    let field_line = source.lines().position(|line| line.contains("pub pointer raw")).unwrap() + 1;
    assert!(combined.contains(&format!("Main.bd:{field_line}")) || combined.contains(&format!("line {field_line}")) || combined.contains(&format!(":{field_line}:")), "the diagnostic points at the field declaration line {field_line}:\n{combined}");
    assert!(!home.work().join("Pointer/obj").join("main").exists(), "nothing is lowered");
}

// ---------------------------------------------------------------------------------------------
// Serialization Mod routes independent of user imports

/// The Serialization Mod's generated adapters name canonical declarations of the whole
/// `corelib_serialization` package (`Core.Serialization.Reader.Reader`, descriptors, errors). A
/// program whose only `use` is `Core.Serialization.Contracts` must build and run: the package that
/// declares the Mod joins the compilation whole, whatever the program imported.
#[test]
fn ser_contracts_only_import_builds_runs_and_round_trips_bsol() {
    let cli = Cli::qualified();
    let home = Home::new();
    let declaration = "use Core.Serialization.Contracts;\n\n[Serialize]\npub type Point {\n    pub i32 x,\n    pub i32 y,\n}\n\n";
    let plain = format!("{declaration}i32 Main() {{\n    Point point = Point {{ x: 3_i32, y: 4_i32 }};\n    if (point.x + point.y == 7_i32) {{ return 0; }}\n    return 1;\n}}\n");
    let manifest = write_app(&home.work(), "ContractsOnly", &plain);
    let output = run(cli, &home, &["run", "--offline", "--plain", "--project", path_text(&manifest)]);
    assert!(
        output.status.success(),
        "a [Serialize] record in a program importing only Contracts must compile its generated adapters and run:\n{}",
        transcript(&output)
    );

    // BSOL round trip through fully qualified paths; the only `use` is still Contracts.
    let round_trip = format!(
        "{declaration}i32 Main() {{\n    Point original = Point {{ x: 3_i32, y: 4_i32 }};\n    Core.Serialization.Registry.ShapeRegistry table = Core.Serialization.Registry.Create();\n    if (!Core.Results.IsOk(Core.Serialization.Registry.RegisterCompiled<Point>(table, 1048576_i64, 8388608_i64, 100000_i64))) {{ return 1; }}\n    match Core.Bsol.Serialization.WriteTyped<Point>(original, Core.Serialization.Descriptors.CompiledShape<Point>().id, table, Core.Serialization.Limits.DefaultLimits()) {{\n        Core.Results.Result::Error(_) => {{ return 2; }},\n        Core.Results.Result::Ok(text) => {{\n            match Core.Bsol.Serialization.Read<Point>(text, Core.Serialization.Limits.DefaultLimits()) {{\n                Core.Results.Result::Error(_) => {{ return 3; }},\n                Core.Results.Result::Ok(decoded) => {{\n                    if (decoded.x == 3_i32 && decoded.y == 4_i32) {{ return 0; }}\n                    return 4;\n                }},\n            }};\n        }},\n    }};\n    return 5;\n}}\n"
    );
    let manifest = write_app(&home.work(), "ContractsRoundTrip", &round_trip);
    let output = run(cli, &home, &["run", "--offline", "--plain", "--project", path_text(&manifest)]);
    assert!(
        output.status.success(),
        "the generated adapters of a Contracts-only program must round-trip the record through BSOL (exit 1 register, 2 write, 3 read, 4 value):\n{}",
        transcript(&output)
    );
}

// ---------------------------------------------------------------------------------------------
// MOD-06-01 Required native failure, real native generator

#[test]
fn mod06_01_installed_run_executes_real_generator_and_lowers_contribution() {
    let cli = Cli::qualified();
    let home = Home::new();
    let host_main = "use Testing.Assert;\nunit Main() { GeneratedUnit(); SerializedRecord<u32> value = SerializedRecord<u32> { Value: 7 }; Assert.True(value.Value == 7, \"generated generic field\"); }\n";
    let (_mod, manifest) = mod_and_host(&home, "NativeMod", NATIVE_MOD_SOURCE, host_main);
    let output = run(cli, &home, &["run", "--offline", "--plain", "--project", path_text(&manifest)]);
    assert!(
        output.status.success(),
        "the real native Collector and Generator must execute and their typed contribution must lower and run:\n{}",
        transcript(&output)
    );
}

#[test]
fn mod06_01_installed_build_fails_on_tampered_mod_descriptor_without_stub() {
    let cli = Cli::qualified();
    let home = Home::new();
    let host_main = "use Core.Output;\nunit Main() { GeneratedUnit(); Output.WriteLine(\"stub-ran\"); }\n";
    let (mod_root, manifest) = mod_and_host(&home, "NativeMod", NATIVE_MOD_SOURCE, host_main);
    let warm = run(cli, &home, &["run", "--offline", "--plain", "--project", path_text(&manifest)]);
    assert!(warm.status.success(), "the untampered Mod must build and run first:\n{}", transcript(&warm));
    // Corrupt the executable Mod image while keeping its descriptor digest consistent: the artifact still
    // qualifies, so loading it is the first point where the host can observe the missing object/symbol.
    let descriptor_path = find_descriptor(&mod_root);
    let mut descriptor: serde_json::Value = serde_json::from_slice(&std::fs::read(&descriptor_path).unwrap()).unwrap();
    let executable_file = descriptor["executableFile"].as_str().expect("executableFile").to_owned();
    let image = descriptor_path.parent().unwrap().join(&executable_file);
    let original = std::fs::read(&image).unwrap();
    let old_digest = format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&original));
    let corrupted = vec![0x4bu8; original.len().max(64)];
    let new_digest = format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&corrupted));
    std::fs::write(&image, &corrupted).unwrap();
    let files = descriptor["files"].as_object_mut().expect("files inventory");
    let mut relabeled = 0;
    for value in files.values_mut() {
        if value.as_str() == Some(old_digest.as_str()) {
            *value = serde_json::Value::String(new_digest.clone());
            relabeled += 1;
        }
    }
    assert!(relabeled >= 1, "the descriptor inventory records the image digest");
    std::fs::write(&descriptor_path, serde_json::to_vec_pretty(&descriptor).unwrap()).unwrap();
    let output = run(cli, &home, &["run", "--offline", "--plain", "--project", path_text(&manifest)]);
    assert!(!output.status.success(), "a required Mod whose object cannot load must fail compilation:\n{}", transcript(&output));
    assert!(!text(&output.stdout).contains("stub-ran"), "no program runs on stub results");
    let diagnostic = text(&output.stderr).to_ascii_lowercase();
    assert!(diagnostic.contains("nativemod") || diagnostic.contains("mod"), "the failure names the native Mod:\n{diagnostic}");
    assert!(std::fs::read(&image).unwrap() == corrupted, "the host does not silently rebuild over a qualified artifact it could not load");
}

// ---------------------------------------------------------------------------------------------
// MOD-06-02 Instantiated facts authority

#[test]
fn mod06_02_installed_build_of_instantiated_generic_helpers_fails_before_lowering_when_facts_are_missing() {
    let cli = Cli::qualified();
    let home = Home::new();
    // `Opaque` carries no serialization facts; a generic `[Serialize]` helper instantiated with it has
    // no substituted shape to authorize its encoder, so compilation must stop with a diagnostic.
    let source = "use Core.Serialization.Contracts;\n\npub type Opaque {\n    pub i32 inner,\n}\n\n[Serialize]\npub type Pair<T> {\n    pub T left,\n    pub T right,\n}\n\n[Serialize]\npub type Holder {\n    pub Pair<Opaque> nested,\n}\n\ni32 Main() { return 0; }\n";
    let manifest = write_app(&home.work(), "Facts", source);
    let executable = home.work().join(beskid_tests_release::installed::executable_name("facts-out"));
    let output = run(cli, &home, &["build", "--plain", "--offline", "--project", path_text(&manifest), "--output", path_text(&executable)]);
    assert_eq!(output.status.code(), Some(1), "a generic helper without facts for its instantiation fails before lowering:\n{}", transcript(&output));
    let combined = format!("{}{}", text(&output.stdout), text(&output.stderr));
    assert!(combined.contains("Opaque") || combined.contains("nested"), "the diagnostic names the unauthorized instantiation:\n{combined}");
    assert!(!executable.exists(), "no executable is produced");
}
