use std::fmt::Write as _;

use crate::model::{DispatchEntry, ManifestRoot};

use super::common::{
    dispatch_doc, dispatch_groups, dispatch_tag_imports, symbol_const_suffix, tag_const_name, write_generated_preamble,
};

pub fn render_abi_symbols(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &[]);

    let mut all_symbols: Vec<(String, String)> =
        manifest.kernel.iter().map(|entry| (symbol_const_suffix(&entry.symbol), entry.symbol.clone())).collect();
    for group in dispatch_groups(manifest) {
        for entry in group {
            all_symbols.push((symbol_const_suffix(&entry.dispatch_key), entry.dispatch_key.clone()));
        }
    }
    all_symbols.sort_by(|a, b| a.1.cmp(&b.1));
    all_symbols.dedup_by(|a, b| a.1 == b.1);

    for (const_suffix, symbol) in &all_symbols {
        if let Some(doc) = dispatch_doc(manifest, symbol) {
            writeln!(&mut out, "/// {doc}").unwrap();
        }
        writeln!(&mut out, "pub const SYM_{const_suffix}: &str = \"{symbol}\";").unwrap();
    }

    writeln!(&mut out).unwrap();
    writeln!(&mut out, "/// User-facing FFI layout band for callback registration tables (independent of").unwrap();
    writeln!(&mut out, "/// [`crate::BESKID_RUNTIME_ABI_VERSION`]).").unwrap();
    writeln!(&mut out, "pub const BESKID_USER_FFI_LAYOUT_BAND: u32 = 1;").unwrap();
    writeln!(&mut out).unwrap();

    writeln!(
        &mut out,
        "/// Kernel exports registered by the JIT / AOT linker for ABI v{}.",
        manifest.manifest.abi_version
    )
    .unwrap();
    writeln!(&mut out, "pub const RUNTIME_EXPORT_SYMBOLS: &[&str] = &[").unwrap();
    for entry in &manifest.kernel {
        writeln!(&mut out, "    SYM_{},", symbol_const_suffix(&entry.symbol)).unwrap();
    }
    writeln!(&mut out, "];").unwrap();
    out
}

pub fn render_dispatch_tags(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &[]);
    writeln!(
        &mut out,
        "//! Dispatch tag constants for soft runtime operations (ABI v{}).",
        manifest.manifest.abi_version
    )
    .unwrap();
    writeln!(&mut out).unwrap();

    render_dispatch_group(&mut out, "usize", &manifest.dispatch.usize);
    render_dispatch_group(&mut out, "ptr", &manifest.dispatch.ptr);
    render_dispatch_group(&mut out, "unit", &manifest.dispatch.unit);
    render_dispatch_group(&mut out, "i64", &manifest.dispatch.i64);

    writeln!(&mut out, "/// Total soft dispatch entries across all return groups.").unwrap();
    let total = manifest.dispatch.usize.len()
        + manifest.dispatch.ptr.len()
        + manifest.dispatch.unit.len()
        + manifest.dispatch.i64.len();
    writeln!(&mut out, "pub const DISPATCH_ENTRY_COUNT: usize = {total};").unwrap();
    out
}

pub fn render_dispatch_lookup(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &[]);
    writeln!(&mut out, "//! Maps dispatch routing keys to manifest dispatch tags and return groups.").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "use crate::generated::dispatch_tags::{{{}}};", dispatch_tag_imports(manifest)).unwrap();
    writeln!(&mut out).unwrap();
    writeln!(
        &mut out,
        "/// v3 [`RuntimeInteropEnvelope`](https://beskid-lang.org/platform-spec/language-meta/interop/interop-contracts/adr/0004-dispatch-envelope-layout/) header size in bytes."
    )
    .unwrap();
    writeln!(&mut out, "pub const DISPATCH_ENVELOPE_HEADER_SIZE: i32 = 16;").unwrap();
    writeln!(&mut out, "pub const DISPATCH_TYPE_DESC_OFFSET: i32 = 0;").unwrap();
    writeln!(&mut out, "pub const DISPATCH_TAG_OFFSET: i32 = 8;").unwrap();
    writeln!(&mut out, "pub const DISPATCH_PAD_OFFSET: i32 = 12;").unwrap();
    writeln!(&mut out, "pub const DISPATCH_PAYLOAD_OFFSET: i32 = 16;").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]").unwrap();
    writeln!(&mut out, "pub enum DispatchReturnGroup {{").unwrap();
    writeln!(&mut out, "    Unit,").unwrap();
    writeln!(&mut out, "    Ptr,").unwrap();
    writeln!(&mut out, "    Usize,").unwrap();
    writeln!(&mut out, "    I64,").unwrap();
    writeln!(&mut out, "}}").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "#[derive(Debug, Clone, Copy, PartialEq, Eq)]").unwrap();
    writeln!(&mut out, "pub struct DispatchRoute {{").unwrap();
    writeln!(&mut out, "    pub tag: i32,").unwrap();
    writeln!(&mut out, "    pub group: DispatchReturnGroup,").unwrap();
    writeln!(&mut out, "}}").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "/// Resolve a dispatch routing key to its manifest dispatch route, if any.").unwrap();
    writeln!(&mut out, "pub fn dispatch_route_for_symbol(symbol: &str) -> Option<DispatchRoute> {{").unwrap();
    writeln!(&mut out, "    match symbol {{").unwrap();
    for (group, entries) in dispatch_lookup_groups(manifest) {
        let group_ident = dispatch_group_ident(group);
        for entry in entries {
            writeln!(
                &mut out,
                "        \"{symbol}\" => Some(DispatchRoute {{ tag: TAG_{tag}, group: DispatchReturnGroup::{group_ident} }}),",
                symbol = entry.dispatch_key,
                tag = tag_const_name(&entry.name),
                group_ident = group_ident,
            )
            .unwrap();
        }
    }
    writeln!(&mut out, "        _ => None,").unwrap();
    writeln!(&mut out, "    }}").unwrap();
    writeln!(&mut out, "}}").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "/// True when `symbol` is a soft dispatch op (not a direct kernel export).").unwrap();
    writeln!(&mut out, "pub fn is_dispatch_symbol(symbol: &str) -> bool {{").unwrap();
    writeln!(&mut out, "    dispatch_route_for_symbol(symbol).is_some()").unwrap();
    writeln!(&mut out, "}}").unwrap();
    out
}

pub fn render_jit_kernel_registration(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &[]);
    writeln!(&mut out, "use beskid_abi::generated::symbols::{{").unwrap();
    for entry in &manifest.kernel {
        writeln!(&mut out, "    SYM_{},", symbol_const_suffix(&entry.symbol)).unwrap();
    }
    writeln!(&mut out, "}};").unwrap();
    writeln!(&mut out, "use cranelift_jit::JITBuilder;").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "/// Register kernel exports from [`beskid_abi::RUNTIME_EXPORT_SYMBOLS`].").unwrap();
    writeln!(&mut out, "pub fn register_kernel_exports(builder: &mut JITBuilder) {{").unwrap();
    for entry in &manifest.kernel {
        writeln!(
            &mut out,
            "    builder.symbol(SYM_{}, beskid_runtime::{} as *const u8);",
            symbol_const_suffix(&entry.symbol),
            entry.symbol
        )
        .unwrap();
    }
    writeln!(&mut out, "}}").unwrap();
    out
}

pub fn render_link_anchor(manifest: &ManifestRoot) -> String {
    let mut out = String::new();
    write_generated_preamble(&mut out, &[]);
    writeln!(&mut out, "/// Force the linker to retain kernel exports referenced by generated code.").unwrap();
    writeln!(&mut out, "pub fn anchor_kernel_exports() {{").unwrap();
    for entry in &manifest.kernel {
        writeln!(&mut out, "    let _ = beskid_runtime::{} as *const () as usize;", entry.symbol).unwrap();
    }
    writeln!(&mut out, "}}").unwrap();
    out
}

fn dispatch_lookup_groups(manifest: &ManifestRoot) -> [(&str, &[DispatchEntry]); 4] {
    [
        ("unit", &manifest.dispatch.unit),
        ("ptr", &manifest.dispatch.ptr),
        ("usize", &manifest.dispatch.usize),
        ("i64", &manifest.dispatch.i64),
    ]
}

fn dispatch_group_ident(group: &str) -> &'static str {
    match group {
        "unit" => "Unit",
        "ptr" => "Ptr",
        "usize" => "Usize",
        "i64" => "I64",
        _ => "I64",
    }
}

fn render_dispatch_group(out: &mut String, group: &str, entries: &[DispatchEntry]) {
    writeln!(out).unwrap();
    writeln!(out, "/// Number of `{group}` dispatch handlers declared in the manifest.").unwrap();
    writeln!(out, "pub const DISPATCH_{}_COUNT: usize = {};", group.to_uppercase(), entries.len()).unwrap();
    for entry in entries {
        writeln!(out, "/// `{symbol}` → dispatch key `{key}`", symbol = entry.name, key = entry.dispatch_key).unwrap();
        writeln!(out, "pub const TAG_{}: i32 = {};", tag_const_name(&entry.name), entry.tag).unwrap();
    }
}
