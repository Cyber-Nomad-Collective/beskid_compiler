use super::{
    BTreeSet, GenericArgument, HelperPaths, PathArguments, SYNTAX_NODES_MODULE_PREFIX, Type, list_element_rust_name,
    option_payload_rust_name, peel_type, vec_element_type,
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
            let p = peel_type(inner);
            if is_primitive_option_inner(p) {
                return map_rust_type(p, stub_path, helpers, type_params);
            }
            if let Some(h) = helpers {
                if let Some(vel) = vec_element_type(p) {
                    let vp = peel_type(vel);
                    if matches!(vp, Type::Path(pp) if pp.path.is_ident("u8")) {
                        return TypeMirror { beskid_ty: "string".into(), stub_note: None };
                    }
                    if let Some(el) = list_element_rust_name(vel)
                        && let Some(list_path) = h.list_by_element.get(&el)
                    {
                        let list_base = list_path.rsplit('.').next().unwrap();
                        if let Some(opt_path) = h.optional_by_inner.get(list_base) {
                            return TypeMirror { beskid_ty: opt_path.clone(), stub_note: None };
                        }
                    }
                }
                if let Some(nm) = option_payload_rust_name(p)
                    && let Some(opt_path) = h.optional_by_inner.get(&nm)
                {
                    return TypeMirror { beskid_ty: opt_path.clone(), stub_note: None };
                }
            }
        }
        return TypeMirror {
            beskid_ty: stub_path.into(),
            stub_note: Some("Option with non-primitive inner type is collapsed to ReflectStub in Mod SDK".into()),
        };
    }
    if let ("Vec", PathArguments::AngleBracketed(ab)) = (ident.as_str(), args) {
        if let Some(GenericArgument::Type(inner)) = ab.args.first() {
            let pi = peel_type(inner);
            if matches!(pi, Type::Path(p) if p.path.is_ident("u8")) {
                return TypeMirror { beskid_ty: "string".into(), stub_note: None };
            }
            if let Some(h) = helpers
                && let Some(el) = list_element_rust_name(pi)
                && let Some(p) = h.list_by_element.get(&el)
            {
                return TypeMirror { beskid_ty: p.clone(), stub_note: None };
            }
        }
        return TypeMirror {
            beskid_ty: stub_path.into(),
            stub_note: Some("Vec subtrees are not expanded field-for-field in Mod SDK (ReflectStub)".into()),
        };
    }
    if let ("Box", PathArguments::AngleBracketed(ab)) = (ident.as_str(), args) {
        if let Some(GenericArgument::Type(inner)) = ab.args.first() {
            return map_rust_type(inner, stub_path, helpers, type_params);
        }
        return TypeMirror { beskid_ty: stub_path.into(), stub_note: Some("Box without inner type".into()) };
    }
    if let ("Spanned", PathArguments::AngleBracketed(ab)) = (ident.as_str(), args) {
        if let Some(GenericArgument::Type(inner)) = ab.args.first() {
            return map_rust_type(inner, stub_path, helpers, type_params);
        }
        return TypeMirror { beskid_ty: stub_path.into(), stub_note: Some("Spanned without inner type".into()) };
    }

    match ident.as_str() {
        "bool" => TypeMirror { beskid_ty: "bool".into(), stub_note: None },
        "char" => TypeMirror { beskid_ty: "string".into(), stub_note: None },
        "String" => TypeMirror { beskid_ty: "string".into(), stub_note: None },
        "usize" | "isize" | "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" => {
            TypeMirror { beskid_ty: "i64".into(), stub_note: None }
        }
        "f32" | "f64" => TypeMirror { beskid_ty: "f64".into(), stub_note: None },
        _ => {
            let fq = path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>().join("::");
            if fq.contains("LeadingDocComment") {
                return TypeMirror {
                    beskid_ty: stub_path.into(),
                    stub_note: Some("LeadingDocComment is host-only and not modeled as a syntax node".into()),
                };
            }
            if fq.contains("SpanInfo") {
                return TypeMirror { beskid_ty: format!("{}.NodeSpan", SYNTAX_NODES_MODULE_PREFIX), stub_note: None };
            }
            TypeMirror { beskid_ty: format!("{}.{}", SYNTAX_NODES_MODULE_PREFIX, ident), stub_note: None }
        }
    }
}

fn is_primitive_option_inner(ty: &Type) -> bool {
    match ty {
        Type::Path(tp) => {
            tp.path.is_ident("bool")
                || tp.path.is_ident("char")
                || tp.path.is_ident("String")
                || matches!(
                    tp.path.get_ident().map(|i| i.to_string()).as_deref(),
                    Some(
                        "usize" | "isize" | "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64"
                    )
                )
        }
        _ => false,
    }
}
