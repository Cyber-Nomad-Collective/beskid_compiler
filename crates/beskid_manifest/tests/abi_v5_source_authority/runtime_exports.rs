use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use beskid_manifest::load_v5_manifest_source;

fn runtime_sources(root: &Path) -> Vec<PathBuf> {
    fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
        let mut entries = fs::read_dir(directory).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("bd") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect(&root.join("runtime/beskid/src"), &mut files);
    files
}

fn source_type(ty: &str, nominals: &BTreeSet<String>) -> String {
    let ty = ty.trim();
    match ty {
        "word" => "usize".into(),
        "unit" => "void".into(),
        "bool" => "u8".into(),
        "char" => "u32".into(),
        "pointer" | "never" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" => ty.into(),
        "string" => "pointer".into(),
        _ => {
            if let Some(element) = ty.strip_suffix("[]") {
                source_type(element, nominals);
                return "pointer".into();
            }
            let base = ty.split('<').next().unwrap().trim();
            assert!(nominals.contains(base), "unrecognized source ABI type {ty}");
            if let Some((_, arguments)) = ty.split_once('<') {
                let arguments = arguments.strip_suffix('>').expect("closed generic source ABI type");
                for argument in split_parameters(arguments) {
                    source_type(argument, nominals);
                }
            }
            "pointer".into()
        }
    }
}

fn split_parameters(text: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut start = 0;
    let mut result = Vec::new();
    for (index, ch) in text.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.checked_sub(1).expect("balanced generic source ABI type"),
            ',' if depth == 0 => {
                result.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    assert_eq!(depth, 0, "balanced generic source ABI type");
    if !text[start..].trim().is_empty() {
        result.push(text[start..].trim());
    }
    result
}

fn source_nominals(root: &Path) -> BTreeSet<String> {
    let mut paths = runtime_sources(root);
    // These are the two exact public nominal dependencies of runtime exports.
    paths.push(root.join("corelib/packages/foundation/src/Core/Dynamic/Dynamic.bd"));
    paths.push(root.join("corelib/packages/foundation/src/Core/Results/Results.bd"));
    let mut nominals = BTreeSet::new();
    for path in paths {
        let source = fs::read_to_string(path).unwrap();
        for line in source.lines() {
            let line = line.trim().strip_prefix("pub ").unwrap_or(line.trim());
            let declaration = line.strip_prefix("type ").or_else(|| line.strip_prefix("enum "));
            if let Some(declaration) = declaration {
                let name = declaration.split(|ch: char| ch.is_whitespace() || ch == '<' || ch == '{').next().unwrap();
                if !name.is_empty() {
                    nominals.insert(name.to_owned());
                }
            }
        }
    }
    nominals
}

type Signature = (Vec<String>, String);

/// Consume one Beskid function signature, joining a wrapped parameter list but never the body.
fn read_signature<'a>(
    first: &str,
    lines: &mut impl Iterator<Item = &'a str>,
    path: &Path,
    nominals: &BTreeSet<String>,
) -> (String, Signature) {
    let mut declaration = first.trim().to_owned();
    while !declaration.contains(')') {
        let continuation = lines.next().expect("source parameter list must terminate");
        assert!(!continuation.contains("[Export("), "unterminated source signature in {}", path.display());
        declaration.push(' ');
        declaration.push_str(continuation.trim());
    }
    let declaration = declaration.strip_prefix("pub ").unwrap_or(&declaration);
    let params_start = declaration.find('(').expect("source parameter list") + 1;
    let params_end = declaration[params_start..].find(')').expect("source parameter list end") + params_start;
    let (result, name) =
        declaration[..params_start - 1].trim().rsplit_once(char::is_whitespace).expect("source result type and name");
    let params = split_parameters(&declaration[params_start..params_end])
        .into_iter()
        .map(|parameter| {
            let parameter = parameter.strip_prefix("mut ").unwrap_or(parameter);
            let (ty, _name) = parameter.rsplit_once(char::is_whitespace).expect("source parameter type and name");
            source_type(ty, nominals)
        })
        .collect::<Vec<_>>();
    (name.trim().to_owned(), (params, source_type(result, nominals)))
}

fn source_exports(root: &Path) -> BTreeMap<String, Signature> {
    let mut exports = BTreeMap::new();
    let nominals = source_nominals(root);
    for path in runtime_sources(root) {
        let source = fs::read_to_string(&path).unwrap();
        let mut lines = source.lines();
        while let Some(line) = lines.next() {
            if !line.trim_start().starts_with("[Export(") {
                continue;
            }
            let Some(symbol_start) = line.find("Symbol:\"").map(|index| index + "Symbol:\"".len()) else {
                continue;
            };
            let symbol_end = line[symbol_start..].find('"').unwrap() + symbol_start;
            let symbol = &line[symbol_start..symbol_end];
            let first = lines.next().expect("Export attribute must be followed by a declaration");
            assert!(first.trim().starts_with("pub "), "Export must own a public function");
            let (_name, signature) = read_signature(first, &mut lines, &path, &nominals);
            assert!(exports.insert(symbol.to_owned(), signature).is_none(), "duplicate source export {symbol}");
        }
    }
    exports
}

/// Compiler-emitted checked clones keep the exact ABI of their named canonical source function.
fn checked_clone_exports(root: &Path) -> BTreeMap<String, Signature> {
    let nominals = source_nominals(root);
    let mut exports = BTreeMap::new();
    for clone in beskid_manifest::CHECKED_RUNTIME_CLONES {
        let path = root.join("runtime/beskid").join(clone.source_path);
        let source = fs::read_to_string(&path).unwrap();
        let mut lines = source.lines();
        let mut found = Vec::new();
        while let Some(line) = lines.next() {
            // Only top-level declarations; indented call sites and assignments are not owners.
            if line.starts_with(char::is_whitespace) {
                continue;
            }
            let declaration = line.strip_prefix("pub ").unwrap_or(line);
            let Some(head) = declaration.split_once('(').map(|(head, _)| head.trim()) else {
                continue;
            };
            if !head.contains('=')
                && head.rsplit_once(char::is_whitespace).is_some_and(|(_, name)| name == clone.function)
            {
                found.push(read_signature(line, &mut lines, &path, &nominals).1);
            }
        }
        let [signature] = found.as_slice() else {
            panic!("checked clone {} needs exactly one source function {} in {}", clone.export, clone.function, clone.source_path);
        };
        assert!(exports.insert(clone.export.to_owned(), signature.clone()).is_none(), "duplicate checked clone {}", clone.export);
    }
    exports
}

/// The canonical same-bundle C provider: every `BESKID_GLUE_PROVIDER_EXPORT` declaration or
/// definition in `runtime/Glue`, with C types projected to manifest ABI types.
fn c_provider_exports(root: &Path) -> BTreeMap<String, Signature> {
    let directory = root.join("runtime/Glue");
    let mut files = fs::read_dir(&directory).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    files.sort();
    let files = files
        .into_iter()
        .filter(|path| {
            let name = path.file_name().unwrap().to_str().unwrap();
            (name.ends_with(".c") || name.ends_with(".h")) && !name.ends_with("_test.c")
        })
        .collect::<Vec<_>>();
    let mut function_pointers = BTreeSet::new();
    for path in &files {
        for line in fs::read_to_string(path).unwrap().lines() {
            if let Some(rest) = line.trim().strip_prefix("typedef ") {
                if let Some(name) = rest.split_once("(*").and_then(|(_, tail)| tail.split_once(')')).map(|(name, _)| name) {
                    function_pointers.insert(name.trim().to_owned());
                }
            }
        }
    }
    let c_type = |text: &str| -> String {
        let text = text.replace("const ", "");
        let text = text.trim();
        if text.contains('*') {
            return "pointer".into();
        }
        let ty = text.split_whitespace().next().expect("canonical C provider type");
        match ty {
            "uint64_t" => "u64".into(),
            "int64_t" => "i64".into(),
            "uint32_t" => "u32".into(),
            "int32_t" => "i32".into(),
            "uint8_t" => "u8".into(),
            "size_t" => "usize".into(),
            "void" => "void".into(),
            other if function_pointers.contains(other) => "pointer".into(),
            other => panic!("unmapped canonical C provider type {other}"),
        }
    };
    let mut exports = BTreeMap::new();
    for path in &files {
        let source = fs::read_to_string(path).unwrap();
        for (offset, _) in source.match_indices("BESKID_GLUE_PROVIDER_EXPORT ") {
            if offset != 0 && !source[..offset].ends_with('\n') {
                continue;
            }
            let declaration = &source[offset + "BESKID_GLUE_PROVIDER_EXPORT ".len()..];
            let declaration = &declaration[..declaration.find(')').expect("C provider parameter list end")];
            let (head, params) = declaration.split_once('(').expect("C provider parameter list");
            let head = head.trim();
            let name_start = head.rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).map_or(0, |index| index + 1);
            let (result, name) = head.split_at(name_start);
            let params = params.trim();
            let params = if params.is_empty() || params == "void" {
                Vec::new()
            } else {
                params.split(',').map(c_type).collect::<Vec<_>>()
            };
            let signature = (params, c_type(result));
            if let Some(previous) = exports.insert(name.to_owned(), signature.clone()) {
                assert_eq!(previous, signature, "{name}: C provider declaration and definition disagree in {}", path.display());
            }
        }
    }
    assert!(!exports.is_empty(), "canonical C provider exports are absent");
    exports
}

/// Compiler-emitted Dynamic descriptor getters: one box and one cell descriptor for every
/// primitive `beskid_rt_v5_dynamic_<label>_box` source export, plus the erased cell
/// descriptor owned by `DynamicErasedCellV1` in the canonical Records source.
fn dynamic_descriptor_exports(root: &Path, source: &BTreeMap<String, Signature>) -> BTreeMap<String, Signature> {
    let getter = || (Vec::new(), "pointer".to_owned());
    let mut exports = BTreeMap::new();
    for symbol in source.keys() {
        if let Some(label) = symbol.strip_prefix("beskid_rt_v5_dynamic_").and_then(|rest| rest.strip_suffix("_box")) {
            for role in ["box", "cell"] {
                exports.insert(format!("beskid_rt_v5_dynamic_{label}_{role}_descriptor"), getter());
            }
        }
    }
    let records = fs::read_to_string(root.join("runtime/beskid/src/Runtime/Dynamic/Records.bd")).unwrap();
    assert!(
        records.lines().any(|line| line.trim_start().starts_with("pub type DynamicErasedCellV1 ")),
        "canonical erased Dynamic cell declaration absent"
    );
    exports.insert("beskid_dynamic_v1_erased_cell_descriptor".to_owned(), getter());
    exports
}

#[test]
fn canonical_runtime_source_exports_exactly_match_manifest_provenance() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let manifest = load_v5_manifest_source(&manifest_source).expect("canonical ABI-v5 manifest");

    let source = source_exports(&root);
    let mut declared = BTreeMap::new();
    for export in &manifest.exports {
        declared.insert(
            export.symbol.clone(),
            (export.params.iter().map(|parameter| parameter.ty.clone()).collect(), export.result.clone()),
        );
    }
    for service in &manifest.corelib_services {
        if matches!(service.name.as_str(), "__args_count" | "__args_get" | "__thread_yield") {
            continue;
        }
        let signature =
            (service.params.iter().map(|parameter| parameter.ty.clone()).collect::<Vec<_>>(), service.result.clone());
        for binding in &service.target_bindings {
            assert_eq!(binding.implementation, service.adapter, "target-specific service implementation drift");
        }
        if let Some(export_signature) = declared.get(&service.adapter) {
            assert!(
                manifest.exports.iter().any(|export| export.symbol == service.adapter),
                "{} collides with another service rather than an explicitly owned export",
                service.name
            );
            assert_eq!(
                export_signature, &signature,
                "{} must exactly reuse its manifest-owned exported ABI",
                service.name
            );
        } else {
            declared.insert(service.adapter.clone(), signature);
        }
    }

    // Every manifest export has exactly one real producer: Beskid source, the canonical
    // same-bundle C provider, a compiler-emitted checked clone, or a Dynamic descriptor getter.
    let mut produced = BTreeMap::new();
    for (producer, exports) in [
        ("Beskid source", source.clone()),
        ("canonical C provider", c_provider_exports(&root)),
        ("checked clone", checked_clone_exports(&root)),
        ("Dynamic descriptor getter", dynamic_descriptor_exports(&root, &source)),
    ] {
        for (symbol, signature) in exports {
            assert!(produced.insert(symbol.clone(), signature).is_none(), "{symbol} has a second producer: {producer}");
        }
    }
    assert_eq!(
        produced, declared,
        "runtime export producers and manifest provenance must have identical symbols/signatures"
    );

    let mut provenance = declared.keys().cloned().collect::<BTreeSet<_>>();
    provenance.extend(
        manifest
            .corelib_services
            .iter()
            .filter(|service| matches!(service.name.as_str(), "__args_count" | "__args_get" | "__thread_yield"))
            .map(|service| service.adapter.clone()),
    );
    for assembly in &manifest.assembly {
        assert!(
            provenance.insert(assembly.symbol.clone()),
            "assembly symbol must not duplicate a source/service symbol"
        );
    }
    assert_eq!(
        provenance.len(),
        declared.len() + 3 + manifest.assembly.len(),
        "runtime provenance is source exports plus generated Core.Args and host-thread adapters plus assembly"
    );
}

#[test]
fn descriptor_syscalls_preserve_source_and_manifest_abi_on_every_target() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = load_v5_manifest_source(&fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap()).unwrap();
    let source = source_exports(&root);
    for (service_name, symbol, parameters) in [
        ("__syscall_read", "syscall_read", vec!["i32", "pointer", "usize"]),
        ("__syscall_read_bytes", "syscall_read_bytes", vec!["i32", "pointer", "usize"]),
        ("__syscall_write_bytes", "syscall_write_bytes", vec!["i32", "pointer", "usize"]),
        ("__syscall_write", "syscall_write", vec!["i64", "pointer"]),
    ] {
        let expected = (parameters.iter().map(|ty| (*ty).to_owned()).collect::<Vec<_>>(), "i64".to_owned());
        let service = manifest.corelib_services.iter().find(|service| service.name == service_name).unwrap();
        assert_eq!(service.adapter, symbol);
        assert_eq!(
            (service.params.iter().map(|parameter| parameter.ty.clone()).collect::<Vec<_>>(), service.result.clone()),
            expected,
            "{service_name}: descriptor ABI must not widen to HANDLE or return a source array"
        );
        assert_eq!(source.get(symbol), Some(&expected), "{symbol}: canonical source disagrees with the descriptor ABI");
        assert_eq!(service.target_bindings.len(), 3);
        for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let binding = service.target_bindings.iter().find(|binding| binding.target == target).unwrap();
            assert_eq!(binding.implementation, symbol, "{target}: canonical descriptor entry");
            assert!(binding.os_imports.is_empty(), "OS descriptor adaptation belongs to the native worker");
        }
    }
}

#[test]
fn network_lifecycle_services_carry_absolute_deadlines_on_every_target() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = load_v5_manifest_source(&fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap()).unwrap();
    let source = source_exports(&root);
    for (service_name, symbol, parameters, result) in [
        ("__network_open", "beskid_rt_v5_network_open", vec!["i64", "pointer", "i64", "i64", "i64", "pointer"], "i32"),
        ("__network_accept", "beskid_rt_v5_network_accept", vec!["usize", "i64", "pointer"], "i32"),
        ("__network_receive", "beskid_rt_v5_network_receive", vec!["usize", "pointer", "pointer", "u8", "i64"], "i64"),
        ("__network_send", "beskid_rt_v5_network_send", vec!["usize", "pointer", "pointer", "u8", "i64"], "i64"),
        (
            "__network_dns_resolve",
            "beskid_rt_v5_network_dns_resolve",
            vec!["pointer", "i64", "i64", "i64", "pointer"],
            "i32",
        ),
    ] {
        let expected = (parameters.iter().map(|ty| (*ty).to_owned()).collect::<Vec<_>>(), result.to_owned());
        let service = manifest.corelib_services.iter().find(|service| service.name == service_name).unwrap();
        assert_eq!(service.adapter, symbol);
        assert_eq!(
            (service.params.iter().map(|parameter| parameter.ty.clone()).collect::<Vec<_>>(), service.result.clone()),
            expected,
            "{service_name}: lifecycle ABI must carry the absolute monotonic deadline"
        );
        assert_eq!(source.get(symbol), Some(&expected), "{symbol}: canonical source disagrees with the lifecycle ABI");
        for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let binding = service.target_bindings.iter().find(|binding| binding.target == target).unwrap();
            assert_eq!(binding.implementation, symbol, "{target}: canonical lifecycle entry");
        }
    }
}

#[test]
fn source_abi_reader_handles_applied_nominals_without_unknown_pointer_fallback() {
    let nominals = ["Result", "DynamicValueV1", "DynamicErrorV1"].into_iter().map(str::to_owned).collect();
    assert_eq!(source_type("Result<DynamicValueV1, DynamicErrorV1>", &nominals), "pointer");
    assert_eq!(
        split_parameters("Result<DynamicValueV1, DynamicErrorV1> value, u8[] bytes"),
        ["Result<DynamicValueV1, DynamicErrorV1> value", "u8[] bytes"]
    );
    assert!(std::panic::catch_unwind(|| source_type("UnregisteredRecord", &nominals)).is_err());
    assert!(std::panic::catch_unwind(|| source_type("Result<UnregisteredRecord, DynamicErrorV1>", &nominals)).is_err());
}
