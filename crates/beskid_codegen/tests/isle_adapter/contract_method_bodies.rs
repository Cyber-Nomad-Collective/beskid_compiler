//! Method bodies written by contract implementors (type-body methods, `impl` blocks, `This`)
//! lower through ISLE the same way free functions do.

use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, build_typed_program, find_nodes_of_kind, isa,
    item_fixture_with_root, item_name, lower_syntax_program, parse_program_with_source_name, settings,
};

/// Every method and free function under `roots`, named by its declared name.
fn module_items(input: &CodegenInput<'_>, roots: &[AstNodeKey]) -> Vec<SyntaxModuleItem> {
    let mut items = Vec::new();
    for root in roots {
        for kind in [beskid_queries::IndexedNodeKind::MethodDefinition, beskid_queries::IndexedNodeKind::FunctionDefinition]
        {
            for key in find_nodes_of_kind(input.database(), *root, kind) {
                let name = item_name(input.database(), key).ok().flatten().expect("item name");
                items.push(SyntaxModuleItem { key, symbol: name.to_string() });
            }
        }
    }
    items
}

fn lower_single_unit(source: &str) -> Result<(), String> {
    let (input, isa, root) = item_fixture_with_root(source);
    let items = module_items(&input, &[root]);
    lower_syntax_program(&input, isa.as_ref(), &items).map(|_| ()).map_err(|error| format!("{error:?}"))
}

/// A multi-unit project: `sources[0]` is the entry. Returns the input and one root per unit.
fn multi_unit_fixture(
    sources: &[(&str, &str)],
) -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, Vec<AstNodeKey>) {
    let mut db = Box::new(BeskidDatabase::default());
    let root = tempfile::tempdir().expect("project").keep();
    let sources: Vec<_> = sources.iter().map(|(path, source)| (root.join(path), (*source).to_owned())).collect();
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
    let generation = SyntaxGenerationId(233);
    let project = ProjectSession::new(&*db, root.clone(), sources[0].0.clone(), "App".into(), "lock".into());
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
    let roots: Vec<AstNodeKey> = sources
        .iter()
        .map(|(path, _)| AstNodeKey { unit: SourceUnitId::new(&*db, path.clone()), generation, node: AstNodeId(0) })
        .collect();
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(
        leaked,
        typed,
        Arc::from(roots.clone()),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe multi-unit input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, roots)
}

#[test]
fn this_used_as_a_value_inside_a_generic_type_body_method_lowers() {
    lower_single_unit(
        r#"
type Cursor<T> {
    T[] source,
    i64 index,
    i64 length,

    bool Done() {
        return this.index >= this.length;
    }

    Cursor<T> Same() {
        return this;
    }
}

unit Main(Cursor<i64> cursor) {
    cursor.Done();
    cursor.Same();
    return;
}
"#,
    )
    .expect("`this` as a value inside a generic type-body method must lower");
}

#[test]
fn bare_field_comparison_and_interpolation_after_early_return_lower_inside_a_method() {
    lower_single_unit(
        r#"
type Probe {
    i64 count,
    string label,

    bool Changed(i64 other) {
        return count != other && label != "";
    }

    string Describe(bool early) {
        if early {
            return "none";
        }
        return "count=${count}, label=${label}.";
    }
}

unit Main(Probe probe) {
    probe.Changed(1);
    probe.Describe(false);
    return;
}
"#,
    )
    .expect("bare fields in `!=` and in interpolation after an early return must lower");
}

#[test]
fn cross_unit_call_to_an_impl_block_method_lowers() {
    let (input, isa, roots) = multi_unit_fixture(&[
        (
            "Main.bd",
            r#"
use Lib.Counter;

unit Main(Counter counter) {
    counter.Get();
    return;
}
"#,
        ),
        (
            "Lib/Counter.bd",
            r#"
pub type Counter {
    i64 value,
}

impl Counter {
    pub i64 Get() {
        return value;
    }
}
"#,
        ),
    ]);
    let items = module_items(&input, &roots);
    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("a cross-unit call to an impl-block method must lower");
}

#[test]
fn this_in_an_implementing_method_signature_lowers_as_the_receiver_type() {
    lower_single_unit(
        r#"
contract Step {
    This Next();
}

type Walker : Step {
    i64 steps,

    This Next() {
        return Walker { steps: steps + 1 };
    }
}

unit Main(Walker walker) {
    walker.Next();
    return;
}
"#,
    )
    .expect("`This` in an implementing method's own signature must lower as the receiver type");
}



#[test]
fn this_rooted_field_chain_lowers_through_the_receiver() {
    lower_single_unit(
        r#"
type Inner {
    i64 value,
}

type Outer {
    Inner inner,

    i64 Read() {
        return this.inner.value;
    }

    bool Positive() {
        return this.inner.value > 0 && inner.value > 0;
    }
}

unit Main(Outer outer) {
    outer.Read();
    outer.Positive();
    return;
}
"#,
    )
    .expect("`this.field.field` must project through the implicit receiver");
}

#[test]
fn bounded_generic_receivers_dispatch_to_each_instantiation() {
    let (input, isa, root) = item_fixture_with_root(
        "contract Sized { i64 Size(); } \
         type Box : Sized { i64 n, pub i64 Size() { return this.n; } } \
         type Bag : Sized { i64 m, pub i64 Size() { return 100; } } \
         i64 SizeOf<T>(T value) where T: Sized { return value.Size(); } \
         i64 Main() { return SizeOf<Box>(Box { n: 2 }) + SizeOf(Bag { m: 1 }); }",
    );
    let items = module_items(&input, &[root]);
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("a where-bounded receiver dispatches statically in every specialization");
    beskid_codegen::validate_artifact(&artifact).expect("bounded dispatch artifact");
    let specializations =
        artifact.functions.iter().filter(|function| function.name.starts_with("SizeOf#generic_")).count();
    assert_eq!(specializations, 2, "one specialization per receiver type");
}

/// One contract, implemented by every primitive the bound dispatch test covers. Each `Size`
/// reads its receiver by value, so a wrong receiver ABI fails CLIF verification.
const PRIMITIVE_SIZED: &str = "contract Sized { i64 Size(); } \
     impl i32 : Sized { pub i64 Size() { return i64(this) + 1; } } \
     impl i64 : Sized { pub i64 Size() { return this * 2; } } \
     impl u8 : Sized { pub i64 Size() { return i64(this); } } \
     impl f64 : Sized { pub i64 Size() { return 8; } } \
     impl bool : Sized { pub i64 Size() { if this { return 1; } return 0; } } \
     impl string : Sized { pub i64 Size() { return 3; } } \
     i64 SizeOf<T>(T value) where T: Sized { return value.Size(); } ";

#[test]
fn bounded_generic_receivers_dispatch_to_primitive_implementations() {
    let (input, isa, root) = item_fixture_with_root(&format!(
        "{PRIMITIVE_SIZED}i64 Main() {{ return SizeOf<i32>(3_i32) + SizeOf<i64>(4_i64) + SizeOf<u8>(5_u8) \
         + SizeOf<f64>(1.5_f64) + SizeOf(true) + SizeOf(\"abc\"); }}"
    ));
    let items = module_items(&input, &[root]);
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("a where-bounded receiver dispatches to a primitive implementation in every specialization");
    beskid_codegen::validate_artifact(&artifact).expect("primitive bounded dispatch artifact");
    let specializations =
        artifact.functions.iter().filter(|function| function.name.starts_with("SizeOf#generic_")).count();
    assert_eq!(specializations, 6, "one specialization per primitive receiver");
}

#[test]
fn primitive_implementation_methods_take_their_receiver_by_value() {
    let (input, _, root) = item_fixture_with_root(&format!("{PRIMITIVE_SIZED}unit Main() {{ return; }}"));
    for (receiver, abi) in [
        ("i32", beskid_queries::SemanticTypeId::I32),
        ("i64", beskid_queries::SemanticTypeId::I64),
        ("u8", beskid_queries::SemanticTypeId::U8),
        ("f64", beskid_queries::SemanticTypeId::F64),
        ("bool", beskid_queries::SemanticTypeId::BOOL),
        ("string", beskid_queries::SemanticTypeId::STRING),
    ] {
        let method = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
            .into_iter()
            .find(|key| beskid_queries::method_receiver_abi_type(input.database(), *key) == abi)
            .unwrap_or_else(|| panic!("the `{receiver}` implementation method takes a `{receiver}` receiver"));
        let signature = beskid_queries::item_abi_signature(input.database(), method)
            .expect("item signature query")
            .expect("a non-generic primitive implementation method has an item signature");
        assert_eq!(signature.parameters.as_ref(), &[abi], "`{receiver}` receiver is the first parameter");
        assert_eq!(signature.result, beskid_queries::SemanticTypeId::I64);
    }
}

/// The Corelib `Formatting` implementations themselves (digit loop for `u64`, `word` conversion,
/// UTF-8 array literals for `char`, interpolation of `this`) lower for every primitive through a
/// `where T: Formatting` bound.
#[test]
fn corelib_formatting_implementations_lower_for_every_primitive() {
    // The single-unit fixture has no `Core.String`; only the final UTF-8 materialization is stubbed.
    let formatting =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/src/Core/Text/Formatting.bd"))
            .replace("use Core.String;\n", "")
            .replace("return String.FromUtf8CodeUnits(bytes);", "return \"?\";");
    assert!(!formatting.contains("String."), "the stub replaced every Core.String use");
    let (input, isa, root) = item_fixture_with_root(&format!(
        "{formatting}\nstring Show<T>(T value) where T: Formatting {{ return value.Format(); }}\n\
         string Main() {{ return Show<i8>(-1_i8) + Show<i16>(2_i16) + Show<i32>(3_i32) + Show<i64>(4_i64) \
         + Show<u8>(5_u8) + Show<u16>(6_u16) + Show<u32>(7_u32) + Show<u64>(8_u64) + Show<word>(word(9_i64)) \
         + Show<f32>(1.5_f32) + Show<f64>(2.5_f64) + Show<bool>(true) + Show<char>('x') + Show<string>(\"s\"); }}\n"
    ));
    let items = module_items(&input, &[root]);
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("every Corelib Formatting implementation lowers through a bound");
    beskid_codegen::validate_artifact(&artifact).expect("Formatting artifact");
    let specializations = artifact.functions.iter().filter(|function| function.name.starts_with("Show#generic_")).count();
    assert_eq!(specializations, 14, "one specialization per primitive");
}

/// Direct calls on primitive values (`n.Format()`, `3.Format()`, a literal string receiver) call
/// the primitive's implementation statically, the receiver passed by value.
#[test]
fn primitive_values_call_their_implementation_methods_directly() {
    let (input, isa, root) = item_fixture_with_root(
        "contract Shown { string Show(); } \
         impl i32 : Shown { pub string Show() { return \"${this}\"; } } \
         impl i64 : Shown { pub string Show() { return \"${this}\"; } } \
         impl f64 : Shown { pub string Show() { return \"${this}\"; } } \
         impl bool : Shown { pub string Show() { if this { return \"yes\"; } return \"no\"; } } \
         impl string : Shown { pub string Show() { return this; } } \
         string Main(f64 x) { i64 n = 3; bool flag = true; string s = \"t\"; \
         return n.Show() + x.Show() + flag.Show() + s.Show() + 3.Show() + (n).Show() + \"lit\".Show(); }",
    );
    let items = module_items(&input, &[root]);
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("direct calls on primitive values lower to their implementation methods");
    beskid_codegen::validate_artifact(&artifact).expect("direct primitive call artifact");
}

/// `for` variables over primitive arrays and `match` bindings of primitive payloads call their
/// primitive implementation directly.
#[test]
fn loop_variables_and_match_bindings_call_their_primitive_implementation_directly() {
    let (input, isa, root) = item_fixture_with_root(
        "contract Shown { string Show(); } \
         impl i64 : Shown { pub string Show() { return \"${this}\"; } } \
         impl f64 : Shown { pub string Show() { return \"${this}\"; } } \
         impl bool : Shown { pub string Show() { if this { return \"yes\"; } return \"no\"; } } \
         enum Reading { Count(i64 value), Ratio(f64 value), Flag(bool value), Empty } \
         string Main(i64[] nums, f64[] ratios, Reading reading) { mut string out = \"\"; \
         for x in nums { out = out + x.Show(); } \
         for r in ratios { out = out + (r).Show(); } \
         string described = match reading { Reading::Count(v) => v.Show(), Reading::Ratio(v) => v.Show(), \
         Reading::Flag(v) => (v).Show(), Reading::Empty => \"none\", }; \
         return out + described; }",
    );
    let items = module_items(&input, &[root]);
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("loop variables and match bindings call their primitive implementations");
    beskid_codegen::validate_artifact(&artifact).expect("loop and binding receiver artifact");
}
