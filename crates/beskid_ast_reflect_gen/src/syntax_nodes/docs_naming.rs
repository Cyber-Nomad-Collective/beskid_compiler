use super::model::{EnumVariantMirror, FieldMirror, ParsedType, TypeKind, VariantShape};
use super::{
    Attribute, BTreeSet, Expr, GenericParam, Generics, Lit, Meta, reflect_stub_path, rust_snake_to_beskid_field_camel,
};

/// Every `#[doc = "..."]` line (syn expands `///` into these), preserving inner newlines.
pub(super) fn doc_lines_from_attrs(attrs: &[Attribute]) -> Vec<String> {
    let mut out = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        let Meta::NameValue(nv) = &attr.meta else {
            continue;
        };
        let Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) = &nv.value else {
            continue;
        };
        for raw in s.value().lines() {
            let t = raw.trim_end();
            if t.is_empty() {
                continue;
            }
            out.push(t.trim_start().to_string());
        }
    }
    out
}

/// Suffix text for [`doc_variant_line`]: one line, no `@`, no raw newlines
/// (see `beskid_analysis/src/beskid_doc.pest` `ArgSuffix` / `VariantSuffix`).
fn sanitize_doc_directive_suffix(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| match c {
            '\n' | '\r' => ' ',
            '@' => ' ',
            _ => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `@variant(name) description` — `beskid_doc.pest` `VariantTag`.
fn doc_variant_line(ident: &str, description: &str) -> String {
    format!("@variant({}) {}", ident, sanitize_doc_directive_suffix(description))
}

/// `@par(name) description` — `beskid_doc.pest` `ParTag`.
fn doc_par_line(ident: &str, description: &str) -> String {
    format!("@par({}) {}", ident, sanitize_doc_directive_suffix(description))
}

pub(super) fn type_param_names_sorted(generics: &Generics) -> (BTreeSet<String>, Vec<String>) {
    let set: BTreeSet<String> = generics
        .params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Type(t) => Some(t.ident.to_string()),
            _ => None,
        })
        .collect();
    let names = set.iter().cloned().collect::<Vec<_>>();
    (set, names)
}

pub(super) fn field_sdk_doc_lines(attrs: &[Attribute], beskid_ty: &str, stub_note: Option<&str>) -> Vec<String> {
    let mut lines = doc_lines_from_attrs(attrs);
    let stub = reflect_stub_path();
    if beskid_ty == stub {
        let note = stub_note.unwrap_or("opaque or unmapped Rust type in this surface");
        lines.push(sanitize_doc_directive_suffix(&format!("ReflectStub in Mod SDK ({note}).")));
    }
    lines
}

fn variant_index_suffix(shape: &VariantShape) -> String {
    match shape {
        VariantShape::Unit => "unit (no payload)".into(),
        VariantShape::Tuple(fs) if fs.is_empty() => "empty tuple".into(),
        VariantShape::Tuple(fs) => {
            let inner = fs.iter().map(|f| format!("{}: {}", f.name, f.beskid_ty)).collect::<Vec<_>>().join(", ");
            format!("tuple ({inner})")
        }
        VariantShape::Struct(fs) => {
            let inner = fs.iter().map(|f| format!("{}: {}", f.name, f.beskid_ty)).collect::<Vec<_>>().join(", ");
            format!("struct {{ {inner} }}")
        }
    }
}

pub(super) fn unique_field_name(base: &str, used: &mut BTreeSet<String>) -> String {
    let esc = rust_snake_to_beskid_field_camel(base);
    if !used.contains(&esc) {
        used.insert(esc.clone());
        return esc;
    }
    let mut i = 2u32;
    loop {
        let cand = format!("{esc}{i}");
        if !used.contains(&cand) {
            used.insert(cand.clone());
            return cand;
        }
        i += 1;
    }
}

pub(super) fn tuple_variant_field_names(field_count: usize) -> Vec<String> {
    match field_count {
        0 => Vec::new(),
        1 => vec!["payload".to_string()],
        n => (0..n).map(|i| format!("variant_field_{i}")).collect(),
    }
}
fn emit_reflect_stub_doc_lines(type_name: &str, parsed: &ParsedType, stub: &str) -> Vec<String> {
    let mut out = Vec::new();
    match parsed.kind {
        TypeKind::Struct => {
            for f in &parsed.fields {
                if f.beskid_ty == stub {
                    let why = f.stub_note.as_deref().unwrap_or("Mod SDK placeholder (no per-field expansion).");
                    out.push(format!(
                        "/// `{type_name}.{}`: ReflectStub — {}.",
                        f.name,
                        sanitize_doc_directive_suffix(why)
                    ));
                }
            }
        }
        TypeKind::Enum => {
            for v in &parsed.variants {
                let fields: &[FieldMirror] = match &v.shape {
                    VariantShape::Unit => &[],
                    VariantShape::Tuple(fs) | VariantShape::Struct(fs) => fs.as_slice(),
                };
                for f in fields {
                    if f.beskid_ty == stub {
                        let why = f.stub_note.as_deref().unwrap_or("Mod SDK placeholder (no per-field expansion).");
                        out.push(format!(
                            "/// `{type_name}::{}::{}`: ReflectStub — {}.",
                            v.name,
                            f.name,
                            sanitize_doc_directive_suffix(why)
                        ));
                    }
                }
            }
        }
    }
    out
}

pub(super) fn emit_type_doc_block(type_name: &str, parsed: &ParsedType) -> String {
    let stub = reflect_stub_path();
    let rust_loc = format!("crates/beskid_analysis/src/{}", parsed.source_rel_path);
    let mut lines = vec![format!("/// Generated syntax node mirror: `{rust_loc}` — `{type_name}`.")];
    if !parsed.rust_doc_lines.is_empty() {
        lines.push("///".into());
        lines.push("/// **Rust documentation** (from mirrored type):".into());
        for d in &parsed.rust_doc_lines {
            lines.push(format!("/// {d}"));
        }
    }
    if !parsed.type_param_names.is_empty() {
        lines.push("///".into());
        lines.push("/// **Rust type parameters**:".into());
        for p in &parsed.type_param_names {
            lines.push(format!(
                "/// {}",
                doc_par_line(
                    p,
                    "Generic parameter is not expanded to a concrete Beskid type in this surface; affected payloads use ReflectStub.",
                )
            ));
        }
    }
    lines.push("///".into());
    lines.push(
        "/// Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only)."
            .into(),
    );
    lines.push(crate::syntax_traversal::shape_traversal_doc_line().into());
    match parsed.kind {
        TypeKind::Struct => {
            if parsed.fields.is_empty() {
                lines.push("/// Marker struct with no fields.".into());
            } else {
                let names = parsed.fields.iter().map(|f| format!("`{}`", f.name)).collect::<Vec<_>>().join(", ");
                lines.push(format!("/// Struct fields (see declaration): {names}."));
            }
        }
        TypeKind::Enum => {
            for v in &parsed.variants {
                lines.push(format!("/// {}", doc_variant_line(&v.name, &variant_index_suffix(&v.shape))));
            }
        }
    }
    let stub_lines = emit_reflect_stub_doc_lines(type_name, parsed, stub);
    if !stub_lines.is_empty() {
        lines.push("///".into());
        lines.push("/// **ReflectStub** fields (opaque in this shape):".into());
        lines.extend(stub_lines);
    }
    lines.join("\n") + "\n"
}

pub(super) fn push_field_doc_lines(indent: &str, rust_doc_lines: &[String], out: &mut Vec<String>) {
    for d in rust_doc_lines {
        let t = d.trim();
        if !t.is_empty() {
            out.push(format!("{indent}/// {t}"));
        }
    }
}

pub(super) fn format_variant_lines(v: &EnumVariantMirror) -> Vec<String> {
    match &v.shape {
        VariantShape::Unit => vec![format!("    {},", v.name)],
        VariantShape::Tuple(fields) | VariantShape::Struct(fields) => {
            if fields.is_empty() {
                return vec![format!("    {},", v.name)];
            }
            let mut out = vec![format!("    {}(", v.name)];
            for f in fields {
                push_field_doc_lines("        ", &f.rust_doc_lines, &mut out);
                out.push(format!("        {} {},", f.beskid_ty, f.name));
            }
            out.push("    ),".to_string());
            out
        }
    }
}
