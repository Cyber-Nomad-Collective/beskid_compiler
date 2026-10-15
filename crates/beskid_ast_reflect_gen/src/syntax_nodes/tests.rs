use std::path::PathBuf;

use super::emit::emit_type_bd;
use super::inventory::collect_declarations;
use super::{BTreeSet, inventory_syntax_type_names, reflect_sdk_node_kind_names, reflect_stub_path, syntax_helpers};

fn trim_field_token(tok: &str) -> &str {
    tok.trim_end_matches([',', ')', ';'])
}

/// True for legacy `f0`/`f1` positional placeholders (not `f32`/`f64` types).
fn is_fnumeric_placeholder_field(tok: &str) -> bool {
    let w = trim_field_token(tok);
    if w == "f32" || w == "f64" {
        return false;
    }
    let Some(rest) = w.strip_prefix('f') else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

#[test]
fn inventory_matches_reflect_sdk_node_kinds() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let reflect_rs = manifest.join("../beskid_analysis/src/compiler_sdk_reflect.rs");
    let inv: BTreeSet<String> = inventory_syntax_type_names(&analysis_src).expect("inventory").into_iter().collect();
    let kinds = reflect_sdk_node_kind_names(&reflect_rs).expect("reflect parse");
    let kinds_needing_shapes: BTreeSet<_> = kinds.iter().filter(|k| *k != "Node").cloned().collect();
    assert!(
        kinds_needing_shapes.is_subset(&inv),
        "every ReflectSdkNodeKind (except contract-only Node) must have a generated shape file; missing: {:?}",
        kinds_needing_shapes.difference(&inv).collect::<Vec<_>>()
    );
    // `AssignOp` is nested under `AssignExpression` (no standalone `NodeKind` today).
    let allowed_extra: BTreeSet<String> = [
        "AssignOp",
        "FieldKind",
        "InjectQualifier",
        "RegistrationLifetime",
        "ScopeHookKind",
        "WhereBound",
        "LeadingDocComment",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let extras: BTreeSet<_> = inv.difference(&kinds).cloned().collect();
    assert!(
        extras.is_subset(&allowed_extra),
        "unexpected syntax types not listed in ReflectSdkNodeKind: {:?}",
        extras.difference(&allowed_extra).collect::<Vec<_>>()
    );
}

#[test]
fn golden_syntax_nodes_inventory_matches_scan() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let names = inventory_syntax_type_names(&analysis_src).expect("inventory");
    let got = names.join("\n") + "\n";
    let expected = include_str!("../../tests/expected/syntax_nodes_inventory.txt");
    assert_eq!(
        got, expected,
        "update tests/expected/syntax_nodes_inventory.txt after adding/removing syntax surface types"
    );
}

/// Generated `Syntax/Nodes/*.bd` must not use legacy `f0`/`f1` positional field names.
#[test]
fn syntax_node_emission_avoids_fnumeric_field_names() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let files = syntax_helpers::load_syntax_files(&analysis_src).expect("load");
    let helpers = syntax_helpers::build_helper_paths(&files);
    let (decls, _) = collect_declarations(&analysis_src, Some(&helpers)).expect("collect");
    for (name, parsed) in &decls {
        let text = emit_type_bd(name, parsed);
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("///") {
                continue;
            }
            for tok in line.split_whitespace() {
                assert!(!is_fnumeric_placeholder_field(tok), "legacy fN field placeholder in emitted `{name}`: {line}");
            }
        }
    }
}

/// With list/optional helpers enabled, syntax node bodies must not reference the removed
/// `Syntax.ReflectStub` placeholder (opaque shapes still map to the same string elsewhere).
#[test]
fn syntax_node_emission_avoids_syntax_reflect_stub_path() {
    let stub = reflect_stub_path();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let files = syntax_helpers::load_syntax_files(&analysis_src).expect("load");
    let helpers = syntax_helpers::build_helper_paths(&files);
    let (decls, _) = collect_declarations(&analysis_src, Some(&helpers)).expect("collect");
    for (name, parsed) in &decls {
        let text = emit_type_bd(name, parsed);
        assert!(!text.contains(stub), "emitted `{name}` must not use {stub}; use concrete Nodes helpers instead");
    }
}

#[test]
fn enum_variant_directives_stay_in_the_enclosing_doc_block() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let files = syntax_helpers::load_syntax_files(&analysis_src).expect("load");
    let helpers = syntax_helpers::build_helper_paths(&files);
    let (decls, _) = collect_declarations(&analysis_src, Some(&helpers)).expect("collect");
    let host_body_item = decls.get("HostBodyItem").expect("HostBodyItem mirror");
    let text = emit_type_bd("HostBodyItem", host_body_item);
    let enum_start = text.find("pub enum HostBodyItem").expect("enum declaration");

    assert!(text.matches("@variant(").count() >= 4, "all variant summaries must be retained");
    for (offset, _) in text.match_indices("@variant(") {
        assert!(
            offset < enum_start,
            "@variant directives are valid only in the enclosing enum documentation, not variant documentation:\n{text}"
        );
    }
}

#[test]
fn v06_sdk_function_mirror_retains_where_bounds_and_source_wrappers() {
    let analysis_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_analysis/src");
    let files = syntax_helpers::load_syntax_files(&analysis_src).unwrap();
    let helpers = syntax_helpers::build_helper_paths(&files);
    let (decls, _) = collect_declarations(&analysis_src, Some(&helpers)).unwrap();
    let text = emit_type_bd("FunctionDefinition", decls.get("FunctionDefinition").unwrap());
    assert!(text.contains("whereBounds"), "generic contract bounds were erased: {text}");
    assert!(text.contains("SpannedIdentifier"), "source spans were erased from canonical child fields: {text}");
}

#[test]
fn v06_sdk_numeric_source_widths_and_signedness_are_exact() {
    let analysis_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_analysis/src");
    let files = syntax_helpers::load_syntax_files(&analysis_src).unwrap();
    let helpers = syntax_helpers::build_helper_paths(&files);
    let parameters = super::BTreeSet::new();
    for scalar in ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64"] {
        let ty: syn::Type = syn::parse_str(scalar).unwrap();
        let mapped = super::type_mapping::map_rust_type(&ty, reflect_stub_path(), Some(&helpers), &parameters);
        assert_eq!(mapped.beskid_ty, scalar, "SDK erased canonical Rust scalar {scalar}");
    }
}

#[test]
fn v06_sdk_containers_preserve_absence_bytes_and_nested_documentation() {
    let file: syn::File = syn::parse_quote! {
        pub struct Fixture {
            pub count: Option<usize>,
            pub bytes: Vec<u8>,
            pub docs: Vec<Option<LeadingDocComment>>,
            pub name: Spanned<Identifier>,
        }
    };
    let helpers = syntax_helpers::build_helper_paths(&[("fixture.rs".into(), file)]);
    for (source, expected) in [
        ("Option<usize>", "Beskid.Syntax.Nodes.Optionalusize"),
        ("Vec<u8>", "Beskid.Syntax.Nodes.u8List"),
        ("Vec<Option<LeadingDocComment>>", "Beskid.Syntax.Nodes.OptionalLeadingDocCommentList"),
        ("Spanned<Identifier>", "Beskid.Syntax.Nodes.SpannedIdentifier"),
    ] {
        let ty = syn::parse_str(source).unwrap();
        let mapped = super::type_mapping::map_rust_type(&ty, reflect_stub_path(), Some(&helpers), &BTreeSet::new());
        assert_eq!(mapped.beskid_ty, expected);
        assert!(mapped.stub_note.is_none());
    }
    assert_eq!(helpers.opt_helpers["Optionalusize"], "u64");
    assert_eq!(helpers.list_helpers["u8List"], "u8");
    assert_eq!(helpers.list_helpers["OptionalLeadingDocCommentList"], "Beskid.Syntax.Nodes.OptionalLeadingDocComment");
    let wrapper =
        syntax_helpers::emit_spanned_record("SpannedIdentifier", &helpers.spanned_helpers["SpannedIdentifier"]);
    assert!(wrapper.contains("Identifier node"));
    assert!(wrapper.contains("NodeSpan span"));
    assert!(wrapper.contains("u32 id"));
}

#[test]
fn v06_native_syntax_factories_preserve_concrete_fields_variants_and_wrappers() {
    let analysis = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../beskid_analysis/src");
    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("beskid-sdk-factories-{}-{unique}", std::process::id()));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    super::emit_syntax_sdk(&root, &analysis).unwrap();
    let source = std::fs::read_to_string(root.join("Syntax/NativeFactories.bd"))
        .expect("native syntax construction must use generated real source factories");
    assert!(source.contains("IdentifierValue(string name)"));
    assert!(source.contains("return Beskid.Syntax.Nodes.Identifier { name: name }"));
    assert!(source.contains("FunctionDefinitionValue("));
    assert!(source.contains("whereBounds"), "factory must not erase generic constraints");
    assert!(source.contains("PrimitiveTypeU64Value()"));
    // A reserved-word escaped variant keeps its spelling in the expression only; the callable
    // itself is PascalCase.
    assert!(source.contains("pub Beskid.Syntax.Nodes.Type TypeThisValue() {"));
    assert!(source.contains("return Beskid.Syntax.Nodes.Type::_This;"));
    assert!(!source.contains("Type_ThisValue"));
    assert!(
        source
            .lines()
            .filter_map(|line| line.strip_prefix("pub "))
            .all(|line| line.split_whitespace().nth(1).is_some_and(|callable| !callable.contains('_'))),
        "every generated factory must be named in PascalCase"
    );
    assert!(source.contains("return Beskid.Syntax.Nodes.PrimitiveType::U64;"));
    assert!(source.contains("SpannedIdentifierValue("));
    assert!(source.contains("NodeSpan span, u32 id"));
    assert!(source.contains("SpannedIdentifierListValue(Beskid.Syntax.Nodes.SpannedIdentifier[] items)"));
    assert!(!source.contains("SpannedIdentifierListConsValue("));
    assert!(source.contains("OptionalSpannedTypeSomeValue("));
    assert!(!source.contains("ReflectStub"));
    assert!(!source.contains("void "));
}
