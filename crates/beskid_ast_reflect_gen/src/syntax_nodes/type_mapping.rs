use super::{
    BTreeSet, GenericArgument, HelperPaths, PathArguments, SYNTAX_NODES_MODULE_PREFIX, Type, list_element_rust_name,
    option_payload_rust_name,
};

#[derive(Debug, Clone)]
pub(super) struct TypeMirror {
    /// Target Beskid type string for this Rust type.
    pub(super) beskid_ty: String,
    pub(super) stub_note: Option<String>,
}
pub(super) fn map_rust_type(
    ty: &Type,
    stub_path: &str,
    helpers: Option<&HelperPaths>,
    type_params: &BTreeSet<String>,
) -> TypeMirror {
    match ty {
        Type::Path(tp) => map_path_type(tp, stub_path, helpers, type_params),
        Type::Reference(r) => map_rust_type(&r.elem, stub_path, helpers, type_params),
        Type::Paren(p) => map_rust_type(&p.elem, stub_path, helpers, type_params),
        Type::Tuple(t) if t.elems.is_empty() => {
            TypeMirror { beskid_ty: "()".into(), stub_note: Some("Rust unit type `()` has no Mod SDK mapping".into()) }
        }
        Type::Tuple(_) => TypeMirror {
            beskid_ty: stub_path.into(),
            stub_note: Some("Rust tuple type is not expanded in Mod SDK".into()),
        },
        Type::Slice(_) | Type::Array(_) => TypeMirror {
            beskid_ty: stub_path.into(),
            stub_note: Some("Rust slice/array type is not mapped field-for-field".into()),
        },
        _ => TypeMirror {
            beskid_ty: stub_path.into(),
            stub_note: Some("Rust type shape not represented in generated Mod SDK nodes".into()),
        },
    }
}

fn map_path_type(
    tp: &syn::TypePath,
    stub_path: &str,
    helpers: Option<&HelperPaths>,
    type_params: &BTreeSet<String>,
) -> TypeMirror {
    let path = &tp.path;
    let Some(seg) = path.segments.last() else {
        return TypeMirror { beskid_ty: stub_path.into(), stub_note: Some("empty Rust path".into()) };
    };
    let ident = seg.ident.to_string();
    let args = &seg.arguments;

    if type_params.contains(&ident) && matches!(args, PathArguments::None) {
        return TypeMirror {
            beskid_ty: stub_path.into(),
            stub_note: Some(format!("Rust generic type parameter `{ident}` (mirrored as ReflectStub in Mod SDK)")),
        };
    }

    if let ("Option", PathArguments::AngleBracketed(ab)) = (ident.as_str(), args) {
        if let Some(GenericArgument::Type(inner)) = ab.args.first() {
            if let Some(h) = helpers {
                if let Some(name) = option_payload_rust_name(inner) {
                    if let Some(path) = h.optional_by_inner.get(&name) {
                        return TypeMirror { beskid_ty: path.clone(), stub_note: None };
                    }
                }
            }
        }
        return TypeMirror { beskid_ty: stub_path.into(), stub_note: Some("missing canonical optional schema".into()) };
    }
    if let ("Vec", PathArguments::AngleBracketed(ab)) = (ident.as_str(), args) {
        if let Some(GenericArgument::Type(inner)) = ab.args.first() {
            if let Some(h) = helpers {
                if let Some(name) = list_element_rust_name(inner) {
                    if let Some(path) = h.list_by_element.get(&name) {
                        return TypeMirror { beskid_ty: path.clone(), stub_note: None };
                    }
                }
            }
        }
        return TypeMirror { beskid_ty: stub_path.into(), stub_note: Some("missing canonical list schema".into()) };
    }
    if let ("Box", PathArguments::AngleBracketed(ab)) = (ident.as_str(), args) {
        if let Some(GenericArgument::Type(inner)) = ab.args.first() {
            return map_rust_type(inner, stub_path, helpers, type_params);
        }
        return TypeMirror { beskid_ty: stub_path.into(), stub_note: Some("Box without inner type".into()) };
    }
    if let ("Spanned", PathArguments::AngleBracketed(ab)) = (ident.as_str(), args) {
        if let Some(GenericArgument::Type(inner)) = ab.args.first() {
            if let Some(name) = list_element_rust_name(inner) {
                return TypeMirror {
                    beskid_ty: format!("{SYNTAX_NODES_MODULE_PREFIX}.Spanned{name}"),
                    stub_note: None,
                };
            }
        }
        return TypeMirror { beskid_ty: stub_path.into(), stub_note: Some("Spanned without inner type".into()) };
    }

    match ident.as_str() {
        "bool" => TypeMirror { beskid_ty: "bool".into(), stub_note: None },
        "char" => TypeMirror { beskid_ty: "string".into(), stub_note: None },
        "String" => TypeMirror { beskid_ty: "string".into(), stub_note: None },
        "usize" => TypeMirror { beskid_ty: "u64".into(), stub_note: None },
        "isize" => TypeMirror { beskid_ty: "i64".into(), stub_note: None },
        "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" => {
            TypeMirror { beskid_ty: ident, stub_note: None }
        }
        _ => {
            let fq = path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>().join("::");
            if fq.contains("SpanInfo") {
                return TypeMirror { beskid_ty: format!("{}.NodeSpan", SYNTAX_NODES_MODULE_PREFIX), stub_note: None };
            }
            TypeMirror { beskid_ty: format!("{}.{}", SYNTAX_NODES_MODULE_PREFIX, ident), stub_note: None }
        }
    }
}
