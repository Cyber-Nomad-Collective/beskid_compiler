use super::docs_naming::{
    doc_lines_from_attrs, field_sdk_doc_lines, tuple_variant_field_names, type_param_names_sorted, unique_field_name,
};
use super::model::{EnumVariantMirror, FieldMirror, ParsedType, TypeKind, VariantShape};
use super::type_mapping::map_rust_type;
use super::{
    Attribute, BTreeMap, BTreeSet, Fields, HelperPaths, Item, Meta, Path, Visibility, reflect_stub_path, syntax_helpers,
};

/// Discover all `pub struct` / `pub enum` names under the syntax scan dirs.
pub fn inventory_syntax_type_names(analysis_src: &Path) -> Result<Vec<String>, std::io::Error> {
    let (decls, _) = collect_declarations(analysis_src, None)?;
    Ok(decls.keys().filter(|k| !crate::syntax_traversal::is_host_only_type(k)).cloned().collect())
}

pub(super) fn collect_declarations(
    analysis_src: &Path,
    helpers: Option<&HelperPaths>,
) -> Result<(BTreeMap<String, ParsedType>, Vec<String>), std::io::Error> {
    let files = syntax_helpers::load_syntax_files(analysis_src)?;
    let mut map = BTreeMap::new();
    let skipped_generics = Vec::new();
    for (rel_path, file) in &files {
        for item in &file.items {
            if let Some((ident, parsed)) = parse_top_level_item(item, rel_path.as_str(), helpers) {
                map.insert(ident, parsed);
            }
        }
    }
    Ok((map, skipped_generics))
}
fn parse_top_level_item(
    item: &Item,
    source_rel_path: &str,
    helpers: Option<&HelperPaths>,
) -> Option<(String, ParsedType)> {
    let stub = reflect_stub_path();
    match item {
        Item::Struct(s) if matches!(s.vis, Visibility::Public(_)) => {
            let (type_param_set, type_param_names) = type_param_names_sorted(&s.generics);
            let rust_doc_lines = doc_lines_from_attrs(&s.attrs);
            let fields = struct_fields(&s.fields, stub, helpers, &type_param_set);
            Some((
                s.ident.to_string(),
                ParsedType {
                    kind: TypeKind::Struct,
                    source_rel_path: source_rel_path.to_string(),
                    rust_doc_lines,
                    type_param_names,
                    fields,
                    variants: Vec::new(),
                },
            ))
        }
        Item::Enum(e) if matches!(e.vis, Visibility::Public(_)) => {
            let (type_param_set, type_param_names) = type_param_names_sorted(&e.generics);
            let rust_doc_lines = doc_lines_from_attrs(&e.attrs);
            let variants = e.variants.iter().map(|v| parse_enum_variant(v, stub, helpers, &type_param_set)).collect();
            Some((
                e.ident.to_string(),
                ParsedType {
                    kind: TypeKind::Enum,
                    source_rel_path: source_rel_path.to_string(),
                    rust_doc_lines,
                    type_param_names,
                    fields: Vec::new(),
                    variants,
                },
            ))
        }
        _ => None,
    }
}

fn parse_enum_variant(
    v: &syn::Variant,
    stub: &str,
    helpers: Option<&HelperPaths>,
    type_params: &BTreeSet<String>,
) -> EnumVariantMirror {
    let name = v.ident.to_string();
    let shape = match &v.fields {
        Fields::Unit => VariantShape::Unit,
        Fields::Unnamed(uf) => {
            let fields: Vec<_> = uf.unnamed.iter().filter(|f| !field_has_ast_skip(&f.attrs)).collect();
            let count = fields.len();
            let names = tuple_variant_field_names(count);
            let mut used = BTreeSet::new();
            let mut out = Vec::new();
            for (f, base_name) in fields.into_iter().zip(names) {
                let tm = map_rust_type(&f.ty, stub, helpers, type_params);
                let beskid_name = unique_field_name(&base_name, &mut used);
                let rust_src = format!("{name}::{base_name}");
                let rust_doc_lines = field_sdk_doc_lines(&f.attrs, &tm.beskid_ty, tm.stub_note.as_deref());
                out.push(FieldMirror {
                    name: beskid_name,
                    rust_field_source: rust_src,
                    beskid_ty: tm.beskid_ty,
                    stub_note: tm.stub_note,
                    rust_doc_lines,
                });
            }
            VariantShape::Tuple(out)
        }
        Fields::Named(nf) => {
            let mut used = BTreeSet::new();
            let mut out = Vec::new();
            for f in &nf.named {
                if field_has_ast_skip(&f.attrs) {
                    continue;
                }
                let base = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(|| "field".into());
                let tm = map_rust_type(&f.ty, stub, helpers, type_params);
                let beskid_name = unique_field_name(&base, &mut used);
                let rust_src = format!("{name}::{base}");
                let rust_doc_lines = field_sdk_doc_lines(&f.attrs, &tm.beskid_ty, tm.stub_note.as_deref());
                out.push(FieldMirror {
                    name: beskid_name,
                    rust_field_source: rust_src,
                    beskid_ty: tm.beskid_ty,
                    stub_note: tm.stub_note,
                    rust_doc_lines,
                });
            }
            VariantShape::Struct(out)
        }
    };
    EnumVariantMirror { name, rust_doc_lines: doc_lines_from_attrs(&v.attrs), shape }
}

fn struct_fields(
    fields: &Fields,
    stub: &str,
    helpers: Option<&HelperPaths>,
    type_params: &BTreeSet<String>,
) -> Vec<FieldMirror> {
    match fields {
        Fields::Named(named) => {
            let mut used = BTreeSet::new();
            named
                .named
                .iter()
                .filter(|f| !field_has_ast_skip(&f.attrs))
                .map(|f| {
                    let base = f.ident.as_ref().expect("named field").to_string();
                    let tm = map_rust_type(&f.ty, stub, helpers, type_params);
                    let name = unique_field_name(&base, &mut used);
                    let rust_doc_lines = field_sdk_doc_lines(&f.attrs, &tm.beskid_ty, tm.stub_note.as_deref());
                    FieldMirror {
                        name,
                        rust_field_source: base,
                        beskid_ty: tm.beskid_ty,
                        stub_note: tm.stub_note,
                        rust_doc_lines,
                    }
                })
                .collect()
        }
        Fields::Unnamed(unnamed) => {
            let fields: Vec<_> = unnamed.unnamed.iter().filter(|f| !field_has_ast_skip(&f.attrs)).collect();
            let mut used = BTreeSet::new();
            fields
                .into_iter()
                .enumerate()
                .map(|(i, f)| {
                    let base = format!("field_{i}");
                    let tm = map_rust_type(&f.ty, stub, helpers, type_params);
                    let name = unique_field_name(&base, &mut used);
                    let rust_src = format!("tuple field {i} (`{base}`)");
                    let rust_doc_lines = field_sdk_doc_lines(&f.attrs, &tm.beskid_ty, tm.stub_note.as_deref());
                    FieldMirror {
                        name,
                        rust_field_source: rust_src,
                        beskid_ty: tm.beskid_ty,
                        stub_note: tm.stub_note,
                        rust_doc_lines,
                    }
                })
                .collect()
        }
        Fields::Unit => Vec::new(),
    }
}

fn field_has_ast_skip(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("ast") {
            return false;
        }
        match &a.meta {
            Meta::List(list) => list.tokens.to_string().contains("skip"),
            _ => false,
        }
    })
}
