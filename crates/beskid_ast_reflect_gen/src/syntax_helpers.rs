//! Discover `Vec` / `Option` shapes in Rust syntax types and assign concrete Mod SDK helper types
//! (`IdentifierList`, `OptionalTypeReference`, …) so generated `.bd` can avoid `ReflectStub`.

use std::collections::{BTreeMap, BTreeSet};

use syn::{GenericArgument, PathArguments, Type, Visibility};

/// Relative directories under `crates/beskid_analysis/src` scanned for syntax surface types.
pub const SYNTAX_SCAN_SUBDIRS: &[&str] = &[
    "syntax/items",
    "syntax/types",
    "syntax/expressions",
    "syntax/statements",
    "syntax/common",
];

/// Files excluded from scanning (no `pub struct` / `pub enum` surface definitions).
pub const SYNTAX_SCAN_SKIP_FILES: &[&str] = &[
    "mod.rs",
    "parse_helpers.rs",
    "doc_attached_items.rs",
    "span.rs",
    "impl_block.rs",
];

/// Module prefix for emitted node modules (under compiler-sdk `src/`).
pub const SYNTAX_NODES_MODULE_PREFIX: &str = "Beskid.Syntax.Nodes";

/// Full Beskid path to the shared placeholder type (defined in `Syntax.bd`).
pub fn reflect_stub_path() -> &'static str {
    "Beskid.Syntax.ReflectStub"
}

/// Full Beskid path for a helper or node type under [`SYNTAX_NODES_MODULE_PREFIX`].
pub fn nodes_path(type_name: &str) -> String {
    format!("{SYNTAX_NODES_MODULE_PREFIX}.{type_name}")
}

pub(crate) fn peel_type(ty: &Type) -> &Type {
    let mut t = ty;
    loop {
        match t {
            Type::Reference(r) => t = &r.elem,
            Type::Paren(p) => t = &p.elem,
            Type::Group(g) => t = &g.elem,
            _ => break,
        }
    }
    t
}

pub(crate) fn path_last_ident(ty: &Type) -> Option<String> {
    let t = peel_type(ty);
    let Type::Path(tp) = t else {
        return None;
    };
    tp.path.segments.last().map(|s| s.ident.to_string())
}

pub(crate) fn spanned_inner_type(ty: &Type) -> Option<&Type> {
    let t = peel_type(ty);
    let Type::Path(tp) = t else {
        return None;
    };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Spanned" {
        return None;
    }
    let PathArguments::AngleBracketed(ab) = &seg.arguments else {
        return None;
    };
    match ab.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// Rust type name used for `{Name}List` / `Optional{Name}` helper keys (`Spanned<T>` → `T`).
pub(crate) fn list_element_rust_name(ty: &Type) -> Option<String> {
    let t = peel_type(ty);
    if let Some(inner) = spanned_inner_type(t) {
        return path_last_ident(inner);
    }
    path_last_ident(t)
}

pub(crate) fn option_payload_rust_name(ty: &Type) -> Option<String> {
    let t = peel_type(ty);
    if let Some(inner) = spanned_inner_type(t) {
        return path_last_ident(inner);
    }
    path_last_ident(t)
}

pub(crate) fn vec_element_type(ty: &Type) -> Option<&Type> {
    let t = peel_type(ty);
    let Type::Path(tp) = t else {
        return None;
    };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Vec" {
        return None;
    }
    let PathArguments::AngleBracketed(ab) = &seg.arguments else {
        return None;
    };
    match ab.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

pub(crate) fn option_inner_type(ty: &Type) -> Option<&Type> {
    let t = peel_type(ty);
    let Type::Path(tp) = t else {
        return None;
    };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(ab) = &seg.arguments else {
        return None;
    };
    match ab.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

fn is_vec_u8(ty: &Type) -> bool {
    matches!(
        vec_element_type(ty),
        Some(inner) if matches!(peel_type(inner), Type::Path(p) if p.path.is_ident("u8"))
    )
}

/// Maps `Vec<…>` / `Option<…>` Rust shapes to concrete `Beskid.Syntax.Nodes.*` paths.
#[derive(Debug, Clone, Default)]
pub struct HelperPaths {
    /// Element Rust type name (e.g. `Identifier`) → full Beskid path for `{Element}List`.
    pub list_by_element: BTreeMap<String, String>,
    /// Inner Beskid type **name** only (`Identifier` or `IdentifierList`) → full path for `Optional{Inner}`.
    pub optional_by_inner: BTreeMap<String, String>,
    /// Helper type basename (e.g. `IdentifierList`) → full Beskid path of list **element** type.
    pub list_helpers: BTreeMap<String, String>,
    /// Helper basename (e.g. `OptionalTypeReference`) → full Beskid path of `Some` payload type.
    pub opt_helpers: BTreeMap<String, String>,
    /// Helpers only, deterministic emit order (`*List` before `Optional*` when names sort that way).
    pub helper_emit_order: Vec<String>,
}

pub fn list_helper_name(element: &str, decl_names: &BTreeSet<String>) -> String {
    let mut n = format!("{element}List");
    if decl_names.contains(&n) {
        n = format!("Sdk{element}List");
    }
    n
}

pub fn optional_helper_name(inner_type_name: &str, decl_names: &BTreeSet<String>) -> String {
    let mut n = format!("Optional{inner_type_name}");
    if decl_names.contains(&n) {
        n = format!("SdkOptional{inner_type_name}");
    }
    n
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum RawNeed {
    /// `Vec<El>` where `El` is the Rust type name of the element.
    List(String),
    /// `Option` wrapping a Rust node type name **or** a `{El}List` helper name.
    Opt(String),
}

fn accumulate_from_type(ty: &Type, decl_names: &BTreeSet<String>, needs: &mut BTreeSet<RawNeed>) {
    let t = peel_type(ty);
    if is_vec_u8(t) {
        return;
    }
    if let Some(inner) = vec_element_type(t) {
        if let Some(el) = list_element_rust_name(inner) {
            needs.insert(RawNeed::List(el.clone()));
            accumulate_from_type(inner, decl_names, needs);
        }
        return;
    }
    if let Some(inner) = option_inner_type(t) {
        if let Some(inner_vec) = vec_element_type(inner) {
            if is_vec_u8(inner) {
                return;
            }
            if let Some(el) = list_element_rust_name(inner_vec) {
                needs.insert(RawNeed::List(el.clone()));
                let list_h = list_helper_name(&el, decl_names);
                needs.insert(RawNeed::Opt(list_h));
                accumulate_from_type(inner_vec, decl_names, needs);
            }
            return;
        }
        if let Some(nm) = option_payload_rust_name(inner) {
            needs.insert(RawNeed::Opt(nm.clone()));
            accumulate_from_type(inner, decl_names, needs);
        }
        return;
    }
    let Type::Path(tp) = t else {
        return;
    };
    for seg in &tp.path.segments {
        if let PathArguments::AngleBracketed(ab) = &seg.arguments {
            for arg in &ab.args {
                if let GenericArgument::Type(inner) = arg {
                    accumulate_from_type(inner, decl_names, needs);
                }
            }
        }
    }
}

fn walk_item(item: &syn::Item, decl_names: &BTreeSet<String>, needs: &mut BTreeSet<RawNeed>) {
    match item {
        syn::Item::Struct(s) if matches!(s.vis, Visibility::Public(_)) => {
            if !s.generics.params.is_empty() {
                return;
            }
            for f in s.fields.iter() {
                accumulate_from_type(&f.ty, decl_names, needs);
            }
        }
        syn::Item::Enum(e) if matches!(e.vis, Visibility::Public(_)) => {
            if !e.generics.params.is_empty() {
                return;
            }
            for v in &e.variants {
                match &v.fields {
                    syn::Fields::Named(n) => {
                        for f in &n.named {
                            accumulate_from_type(&f.ty, decl_names, needs);
                        }
                    }
                    syn::Fields::Unnamed(u) => {
                        for f in &u.unnamed {
                            accumulate_from_type(&f.ty, decl_names, needs);
                        }
                    }
                    syn::Fields::Unit => {}
                }
            }
        }
        _ => {}
    }
}

fn collect_rs_files(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            collect_rs_files(&p, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(p);
        }
    }
    Ok(())
}

/// Load and parse all syntax surface `.rs` files (same roots as [`crate::syntax_nodes`]).
pub fn load_syntax_files(
    analysis_src: &std::path::Path,
) -> Result<Vec<(String, syn::File)>, std::io::Error> {
    let mut files = Vec::new();
    for sub in SYNTAX_SCAN_SUBDIRS {
        let dir = analysis_src.join(sub);
        if !dir.is_dir() {
            continue;
        }
        collect_rs_files(&dir, &mut files)?;
    }
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if SYNTAX_SCAN_SKIP_FILES.contains(&name) {
            continue;
        }
        let rel_path = path
            .strip_prefix(analysis_src)
            .ok()
            .and_then(|p| p.to_str())
            .map(|s| s.replace('\\', "/"))
            .unwrap_or_else(|| name.to_string());
        let src = std::fs::read_to_string(&path)?;
        let file = syn::parse_file(&src).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: {e}", path.display()),
            )
        })?;
        out.push((rel_path, file));
    }
    Ok(out)
}

pub fn decl_names_from_files(files: &[(String, syn::File)]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (_, file) in files {
        for item in &file.items {
            match item {
                syn::Item::Struct(s) if matches!(s.vis, Visibility::Public(_)) => {
                    if s.generics.params.is_empty() {
                        names.insert(s.ident.to_string());
                    }
                }
                syn::Item::Enum(e) if matches!(e.vis, Visibility::Public(_))
                    && e.generics.params.is_empty() =>
                {
                    names.insert(e.ident.to_string());
                }
                _ => {}
            }
        }
    }
    names
}

fn discover_raw_needs(
    files: &[(String, syn::File)],
    decl_names: &BTreeSet<String>,
) -> BTreeSet<RawNeed> {
    let mut needs = BTreeSet::new();
    for (_, file) in files {
        for item in &file.items {
            walk_item(item, decl_names, &mut needs);
        }
    }
    needs
}

fn resolve_optional_payload_path(
    inner_key: &str,
    list_by_element: &BTreeMap<String, String>,
) -> String {
    for fullp in list_by_element.values() {
        if fullp.rsplit('.').next() == Some(inner_key) {
            return fullp.clone();
        }
    }
    nodes_path(inner_key)
}

fn helper_emit_order(helper_names: &BTreeSet<String>) -> Vec<String> {
    let mut lists: Vec<_> = helper_names
        .iter()
        .filter(|n| n.ends_with("List"))
        .cloned()
        .collect();
    lists.sort();
    let mut opts: Vec<_> = helper_names
        .iter()
        .filter(|n| n.starts_with("Optional") || n.starts_with("SdkOptional"))
        .cloned()
        .collect();
    opts.sort();
    let mut rest: Vec<_> = helper_names
        .iter()
        .filter(|n| !lists.contains(n) && !opts.contains(n))
        .cloned()
        .collect();
    rest.sort();
    let mut out = lists;
    out.extend(opts);
    out.extend(rest);
    out
}

/// Build helper type names, Beskid paths, and emit order.
pub fn build_helper_paths(files: &[(String, syn::File)]) -> HelperPaths {
    let decl_names = decl_names_from_files(files);
    let raw = discover_raw_needs(files, &decl_names);

    let mut list_by_element: BTreeMap<String, String> = BTreeMap::new();
    for n in &raw {
        if let RawNeed::List(el) = n {
            let hname = list_helper_name(el, &decl_names);
            list_by_element.insert(el.clone(), nodes_path(&hname));
        }
    }

    let mut optional_by_inner: BTreeMap<String, String> = BTreeMap::new();
    for n in &raw {
        if let RawNeed::Opt(inner) = n {
            let hname = optional_helper_name(inner, &decl_names);
            optional_by_inner.insert(inner.clone(), nodes_path(&hname));
        }
    }

    let mut helper_names: BTreeSet<String> = BTreeSet::new();
    let mut list_helpers: BTreeMap<String, String> = BTreeMap::new();
    for (el, list_path) in &list_by_element {
        let h = list_path.rsplit('.').next().unwrap().to_string();
        helper_names.insert(h.clone());
        list_helpers.insert(h, nodes_path(el));
    }

    let mut opt_helpers: BTreeMap<String, String> = BTreeMap::new();
    for (inner_key, fullp) in &optional_by_inner {
        let h = fullp.rsplit('.').next().unwrap().to_string();
        helper_names.insert(h.clone());
        let payload = resolve_optional_payload_path(inner_key, &list_by_element);
        opt_helpers.insert(h, payload);
    }

    HelperPaths {
        list_by_element,
        optional_by_inner,
        list_helpers,
        opt_helpers,
        helper_emit_order: helper_emit_order(&helper_names),
    }
}

pub fn emit_list_enum(helper_name: &str, element_path: &str) -> String {
    let elem = element_path.rsplit('.').next().unwrap_or("T");
    let tail_ty = format!("{SYNTAX_NODES_MODULE_PREFIX}.{helper_name}");
    format!(
        r#"// Generated by beskid_ast_reflect_gen (syntax helpers). Do not hand-edit.

/// Cons-list encoding for Rust `Vec<{elem}>` (`beskid_doc.pest` `@variant`; field prose is plain text).
///
/// @variant(Empty) Empty list (length 0).
/// @variant(Cons) Non-empty list: `head` is first (`{element_path}`), `tail` is the recursive remainder (`{tail_ty}`).
pub enum {helper_name} {{
    Empty,
    Cons(
        {element_path} head,
        {tail_ty} tail,
    ),
}}
"#
    )
}

pub fn emit_optional_enum(helper_name: &str, inner_path: &str) -> String {
    format!(
        r#"// Generated by beskid_ast_reflect_gen (syntax helpers). Do not hand-edit.

/// Rust `Option<…>` encoding where the inner type is `{inner_path}` (`beskid_doc.pest` `@variant`).
///
/// @variant(None) Absent (`None` in Rust).
/// @variant(Some) Present; `payload` holds the inner value (`{inner_path}`).
pub enum {helper_name} {{
    None,
    Some({inner_path} payload),
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::{SYNTAX_NODES_MODULE_PREFIX, reflect_stub_path};

    #[test]
    fn syntax_node_module_prefix_uses_public_beskid_syntax_surface() {
        assert_eq!(SYNTAX_NODES_MODULE_PREFIX, "Beskid.Syntax.Nodes");
        assert_eq!(reflect_stub_path(), "Beskid.Syntax.ReflectStub");
    }
}
