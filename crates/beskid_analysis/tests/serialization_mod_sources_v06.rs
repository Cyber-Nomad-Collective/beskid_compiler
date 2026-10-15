//! The real Serialization Mod must parse before any native callback can be built.
use beskid_analysis::services::parse_program_with_source_name_and_diagnostics;
use std::path::PathBuf;

#[test]
fn canonical_serialization_mod_sources_parse_without_recovery() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/mods/serialization_mod/Src");
    let mut pending = vec![root];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "bd") {
                sources.push(path);
            }
        }
    }
    sources.sort();
    assert!(sources.iter().any(|path| path.file_name().is_some_and(|name| name == "Driver.bd")));
    let mut failures = Vec::new();
    for path in sources {
        let source = std::fs::read_to_string(&path).unwrap();
        match parse_program_with_source_name_and_diagnostics(path.to_str().unwrap(), &source) {
            Err(error) => failures.push(format!("{}: {error}", path.display())),
            Ok(parsed) if parsed.recovered || !parsed.diagnostics.is_empty() => {
                failures.push(format!("{} required parser recovery: {:?}", path.display(), parsed.diagnostics));
            }
            Ok(_) => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn serialization_library_loads_canonical_mod_dependency_closure() {
    use beskid_analysis::projects::{build_project_graph, ProjectGraphNode, ProjectKind};
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/serialization/corelib_serialization.bproj");
    let graph = build_project_graph(&manifest).expect("canonical Serialization dependency graph");
    let mut dependencies = std::collections::BTreeMap::new();
    for node in graph.dag.graph().node_weights() {
        if let ProjectGraphNode::ResolvedPathDependency { project_name, project_kind, manifest_path, .. } = node {
            dependencies.insert(project_name.as_str(), (*project_kind, manifest_path));
        }
    }
    let (kind, path) = dependencies.get("serialization_mod")
        .expect("Serialization consumers must transitively load the Serialization Mod");
    assert_eq!(*kind, ProjectKind::Mod);
    assert_eq!(path.canonicalize().unwrap(), manifest.parent().unwrap()
        .join("../../mods/serialization_mod/serialization_mod.bproj").canonicalize().unwrap());
    assert!(dependencies.contains_key("corelib_compiler_sdk"));
    assert!(dependencies.contains_key("corelib_foundation"));
}
