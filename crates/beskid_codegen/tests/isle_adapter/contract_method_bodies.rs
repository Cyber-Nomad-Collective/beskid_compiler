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

