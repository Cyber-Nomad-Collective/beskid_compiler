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

fn source_type(ty: &str) -> &str {
    match ty {
        "word" => "usize",
        "unit" => "void",
        "bool" => "u8",
        other => other,
    }
}

fn source_exports(root: &Path) -> BTreeMap<String, (Vec<String>, String)> {
    let mut exports = BTreeMap::new();
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
            let declaration = lines.next().expect("Export attribute must be followed by a declaration").trim();
            let declaration = declaration.strip_prefix("pub ").expect("Export must own a public function");
            let (result, remainder) = declaration.split_once(' ').expect("Export result type");
            let params_start = remainder.find('(').expect("Export parameter list") + 1;
            let params_end = remainder.rfind(')').expect("Export parameter list end");
            let params = remainder[params_start..params_end]
                .split(',')
                .filter_map(|parameter| {
                    let parameter = parameter.trim();
                    (!parameter.is_empty())
                        .then(|| source_type(parameter.split_whitespace().next().unwrap()).to_owned())
                })
                .collect::<Vec<_>>();
            assert!(
                exports.insert(symbol.to_owned(), (params, source_type(result).to_owned())).is_none(),
                "duplicate source export {symbol}"
            );
        }
    }
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
        if matches!(service.name.as_str(), "__args_count" | "__args_get") {
            continue;
        }
        let signature =
            (service.params.iter().map(|parameter| parameter.ty.clone()).collect::<Vec<_>>(), service.result.clone());
        for binding in &service.target_bindings {
            assert_eq!(binding.implementation, service.adapter, "target-specific service implementation drift");
        }
        assert!(
            declared.insert(service.adapter.clone(), signature).is_none(),
            "duplicate manifest-owned runtime symbol {}",
            service.adapter
        );
    }

    assert_eq!(
        source, declared,
        "runtime source exports and manifest provenance must have identical symbols/signatures"
    );

    let mut provenance = declared.keys().cloned().collect::<BTreeSet<_>>();
    provenance.extend(
        manifest
            .corelib_services
            .iter()
            .filter(|service| matches!(service.name.as_str(), "__args_count" | "__args_get"))
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
        declared.len() + 2 + manifest.assembly.len(),
        "runtime provenance is source exports plus generated Core.Args adapters plus assembly"
    );
}
