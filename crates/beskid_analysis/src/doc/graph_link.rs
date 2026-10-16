//! Wire `api.json` `parentId` / `memberIds` into a library tree and tag cross-package symbols.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::resolve::Resolution;

use super::api_snapshot::{ApiDocItem, ApiLocation};

/// Per-package roots for doc linking: absolute `match_root` for analysis, artifact prefix for packed paths.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiDocPackageRoots {
    pub package: String,
    /// Absolute path prefix of compilation units (materialized or plan source root).
    pub match_root: PathBuf,
    /// Path from package root (`project_root`) to `source_root` in `.bpk` entries (e.g. `src`).
    pub artifact_source_prefix: String,
    /// Absolute `.generated` sibling of `match_root`; discovery compiles `*.g.bd` units from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_match_root: Option<PathBuf>,
    /// Path from package root to the generated root in `.bpk` entries (e.g. `.generated`).
    #[serde(default)]
    pub artifact_generated_prefix: String,
}

impl ApiDocPackageRoots {
    /// Source root plus its `.generated` sibling, each with the `.bpk` prefix it maps to.
    pub fn match_roots(&self) -> impl Iterator<Item = (&Path, &str)> {
        std::iter::once((self.match_root.as_path(), self.artifact_source_prefix.as_str())).chain(
            self.generated_match_root.as_deref().map(|root| (root, self.artifact_generated_prefix.as_str())),
        )
    }
}

/// Package roots for `declaringPackage` assignment and artifact-relative path emission.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiDocLinkContext {
    pub publishing_package: String,
    pub packages: Vec<ApiDocPackageRoots>,
}

impl ApiDocLinkContext {
    pub fn declaring_package_for_file(&self, file: &str) -> Option<String> {
        let path = Path::new(file);
        let mut best: Option<(usize, String)> = None;
        for entry in &self.packages {
            for (root, _) in entry.match_roots() {
                if path.starts_with(root) {
                    let len = root.as_os_str().len();
                    if best.as_ref().is_none_or(|(l, _)| len > *l) {
                        best = Some((len, entry.package.clone()));
                    }
                }
            }
        }
        let (_, package) = best?;
        if package == self.publishing_package { None } else { Some(package) }
    }
}

const KIND_MODULE: &str = "module";
const MODULE_LEVEL_KINDS: &[&str] = &["module", "type", "enum", "contract", "function", "test"];

fn path_key(segments: &[String]) -> String {
    segments.join("\0")
}

fn qualified_name_to_segments(qualified_name: &str) -> Vec<String> {
    if qualified_name.is_empty() {
        return Vec::new();
    }
    qualified_name.split("::").map(str::to_string).collect()
}

fn module_path_for_row(item: &ApiDocItem) -> Vec<String> {
    if item.kind == KIND_MODULE {
        let from_qn = qualified_name_to_segments(&item.qualified_name);
        if !from_qn.is_empty() {
            return from_qn;
        }
    }
    if !item.module_path.is_empty() {
        return item.module_path.clone();
    }
    if item.parent_id.is_some() {
        return Vec::new();
    }
    let segments = qualified_name_to_segments(&item.qualified_name);
    if segments.len() <= 1 {
        return segments;
    }
    segments[..segments.len() - 1].to_vec()
}

fn next_synthetic_id(items: &[ApiDocItem]) -> usize {
    items.iter().filter_map(|i| i.id).max().unwrap_or(0).saturating_add(1)
}

fn stub_location_from(items: &[ApiDocItem]) -> ApiLocation {
    items.first().map(|i| i.location.clone()).unwrap_or(ApiLocation {
        file: String::new(),
        start_line: 1,
        start_column: 1,
        end_line: 1,
        end_column: 1,
    })
}

/// Populate `declaringPackage` from [`ApiDocLinkContext`].
pub fn assign_declaring_packages(items: &mut [ApiDocItem], ctx: &ApiDocLinkContext) {
    for item in items.iter_mut() {
        item.declaring_package = ctx.declaring_package_for_file(&item.location.file);
    }
}

/// Attribute every module to the package whose items it holds.
///
/// A module path such as `Core` or `Http` can be opened by several packages, and modules the
/// library tree synthesizes carry a placeholder location, so the file-based attribution of
/// [`assign_declaring_packages`] does not hold for modules. A module that holds an item of the
/// publishing package is its own (no `declaringPackage`) and points at the first such item; a
/// module that holds only items of one dependency belongs to that dependency. Run after
/// [`link_api_doc_library_tree`] and [`assign_declaring_packages`].
pub fn attribute_modules_to_members(items: &mut [ApiDocItem]) {
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if let (Some(_), Some(parent)) = (item.id, item.parent_id) {
            children.entry(parent).or_default().push(index);
        }
    }
    let mut modules: Vec<usize> = (0..items.len()).filter(|&index| items[index].kind == KIND_MODULE).collect();
    // Deepest first, so nested modules are attributed before the modules that contain them.
    modules.sort_by_key(|&index| std::cmp::Reverse(module_path_for_row(&items[index]).len()));

    for index in modules {
        let Some(id) = items[index].id else { continue };
        let Some(members) = children.get(&id) else { continue };
        if let Some(&own) = members.iter().find(|&&member| items[member].declaring_package.is_none()) {
            if items[index].declaring_package.is_some() {
                items[index].location = items[own].location.clone();
                items[index].declaring_package = None;
            }
            continue;
        }
        let first = &items[members[0]];
        let package = first.declaring_package.clone();
        if members.iter().all(|&member| items[member].declaring_package == package) {
            let location = first.location.clone();
            let module = &mut items[index];
            if module.declaring_package != package {
                module.location = location;
                module.declaring_package = package;
            }
        }
    }
}

/// Rebuild `memberIds` from `parentId` edges (emission order preserved per parent).
pub fn fill_member_ids_from_parents(items: &mut [ApiDocItem]) {
    let mut by_parent: HashMap<usize, Vec<usize>> = HashMap::new();
    for it in items.iter() {
        if let (Some(child_id), Some(pid)) = (it.id, it.parent_id) {
            by_parent.entry(pid).or_default().push(child_id);
        }
    }
    for v in by_parent.values_mut() {
        v.sort_unstable();
    }
    for it in items.iter_mut() {
        if let Some(id) = it.id {
            it.member_ids = by_parent.remove(&id).unwrap_or_default();
        } else {
            it.member_ids.clear();
        }
    }
}

/// Ensure module hierarchy and attach module-level symbols to owning modules.
pub fn link_api_doc_library_tree(items: &mut Vec<ApiDocItem>, _resolution: &Resolution) {
    let fallback_location = stub_location_from(items);
    let mut module_by_path: HashMap<String, usize> = HashMap::new();

    for item in items.iter() {
        if item.kind != KIND_MODULE {
            continue;
        }
        let Some(id) = item.id else { continue };
        let path = module_path_for_row(item);
        if path.is_empty() {
            continue;
        }
        module_by_path.insert(path_key(&path), id);
    }

    // A synthesized module points at its first contained item, never at an unrelated one.
    let mut needed_paths: Vec<Vec<String>> = Vec::new();
    let mut first_location: HashMap<String, ApiLocation> = HashMap::new();
    for item in items.iter() {
        let path = module_path_for_row(item);
        if path.is_empty() {
            continue;
        }
        for len in 1..=path.len() {
            first_location.entry(path_key(&path[..len])).or_insert_with(|| item.location.clone());
            needed_paths.push(path[..len].to_vec());
        }
    }

    needed_paths.sort_by_key(|p| p.len());
    needed_paths.dedup_by(|a, b| a == b);

    let mut next_id = next_synthetic_id(items);
    for path in needed_paths {
        let key = path_key(&path);
        if module_by_path.contains_key(&key) {
            continue;
        }
        let qualified_name = path.join("::");
        let name = path.last().cloned().unwrap_or_default();
        let id = next_id;
        next_id += 1;
        let location = first_location.get(&key).cloned().unwrap_or_else(|| fallback_location.clone());
        module_by_path.insert(key, id);
        items.push(ApiDocItem {
            id: Some(id),
            qualified_name: qualified_name.clone(),
            symbol_key: None,
            name,
            kind: KIND_MODULE.to_string(),
            visibility: Some("public".to_string()),
            location,
            parent_id: None,
            member_ids: Vec::new(),
            module_path: path,
            display_name: None,
            signature: None,
            field_type: None,
            return_type: None,
            parameters: Vec::new(),
            generic_parameters: Vec::new(),
            doc_markdown: None,
            doc: None,
            declaring_package: None,
            controls: vec![],
            tier: None,
        });
    }

    for item in items.iter_mut() {
        if item.kind == KIND_MODULE {
            let path = module_path_for_row(item);
            item.module_path = path.clone();
            if path.len() > 1 {
                let parent_path = &path[..path.len() - 1];
                if let Some(&pid) = module_by_path.get(&path_key(parent_path)) {
                    item.parent_id = Some(pid);
                }
            } else {
                item.parent_id = None;
            }
        }
    }

    for item in items.iter_mut() {
        if item.parent_id.is_some() {
            continue;
        }
        if !MODULE_LEVEL_KINDS.contains(&item.kind.as_str()) {
            continue;
        }
        if item.kind == KIND_MODULE {
            continue;
        }
        let path = module_path_for_row(item);
        if path.is_empty() {
            continue;
        }
        if let Some(&mid) = module_by_path.get(&path_key(&path)) {
            item.parent_id = Some(mid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::{ModuleGraph, Resolution, ResolutionTables};

    fn sample_location() -> ApiLocation {
        ApiLocation { file: "src/A.bd".into(), start_line: 1, start_column: 1, end_line: 1, end_column: 1 }
    }

    fn empty_resolution() -> Resolution {
        Resolution {
            items: vec![],
            module_graph: ModuleGraph::new_root(),
            tables: ResolutionTables::new(),
            span_index: Default::default(),
            warnings: vec![],
            builtin_items: HashMap::new(),
            module_imports: HashMap::new(),
            symbols: Default::default(),
            by_symbol: HashMap::new(),
        }
    }

    #[test]
    fn links_nested_modules_and_type_under_module() {
        let mut items = vec![
            ApiDocItem {
                id: Some(1),
                qualified_name: "App".into(),
                symbol_key: None,
                name: "App".into(),
                kind: KIND_MODULE.into(),
                visibility: Some("public".into()),
                location: sample_location(),
                parent_id: None,
                member_ids: vec![],
                module_path: vec!["App".into()],
                display_name: Some("App".into()),
                signature: None,
                field_type: None,
                return_type: None,
                parameters: vec![],
                generic_parameters: vec![],
                doc_markdown: None,
                doc: None,
                declaring_package: None,
                controls: vec![],
                tier: None,
            },
            ApiDocItem {
                id: Some(2),
                qualified_name: "App::Widgets".into(),
                symbol_key: None,
                name: "Widgets".into(),
                kind: KIND_MODULE.into(),
                visibility: Some("public".into()),
                location: sample_location(),
                parent_id: None,
                member_ids: vec![],
                module_path: vec!["App".into()],
                display_name: Some("Widgets".into()),
                signature: None,
                field_type: None,
                return_type: None,
                parameters: vec![],
                generic_parameters: vec![],
                doc_markdown: None,
                doc: None,
                declaring_package: None,
                controls: vec![],
                tier: None,
            },
            ApiDocItem {
                id: Some(3),
                qualified_name: "App::Widgets::Button".into(),
                symbol_key: None,
                name: "Button".into(),
                kind: "type".into(),
                visibility: Some("public".into()),
                location: sample_location(),
                parent_id: None,
                member_ids: vec![],
                module_path: vec!["App".into(), "Widgets".into()],
                display_name: Some("Button".into()),
                signature: None,
                field_type: None,
                return_type: None,
                parameters: vec![],
                generic_parameters: vec![],
                doc_markdown: None,
                doc: None,
                declaring_package: None,
                controls: vec![],
                tier: None,
            },
        ];

        link_api_doc_library_tree(&mut items, &empty_resolution());
        fill_member_ids_from_parents(&mut items);

        let widgets = items.iter().find(|i| i.id == Some(2)).expect("widgets");
        assert_eq!(widgets.parent_id, Some(1));
        let button = items.iter().find(|i| i.id == Some(3)).expect("button");
        assert_eq!(button.parent_id, Some(2));

        let roots: Vec<_> = items.iter().filter(|i| i.parent_id.is_none() && i.kind == KIND_MODULE).collect();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, Some(1));
    }

    fn row(id: usize, qualified_name: &str, kind: &str, file: &str) -> ApiDocItem {
        let name = qualified_name.rsplit("::").next().unwrap_or(qualified_name).to_string();
        ApiDocItem {
            id: Some(id),
            qualified_name: qualified_name.into(),
            symbol_key: None,
            name: name.clone(),
            kind: kind.into(),
            visibility: Some("public".into()),
            location: ApiLocation { file: file.into(), start_line: id, start_column: 1, end_line: id, end_column: 1 },
            parent_id: None,
            member_ids: vec![],
            module_path: vec![],
            display_name: Some(name),
            signature: None,
            field_type: None,
            return_type: None,
            parameters: vec![],
            generic_parameters: vec![],
            doc_markdown: None,
            doc: None,
            declaring_package: None,
            controls: vec![],
            tier: None,
        }
    }

    #[test]
    fn own_modules_are_not_attributed_to_the_dependency_of_the_first_item() {
        let foundation = "/w/obj/beskid/deps/src/corelib_foundation-1a2b/src";
        let ctx = ApiDocLinkContext {
            publishing_package: "corelib_http".into(),
            packages: vec![
                ApiDocPackageRoots {
                    package: "corelib_http".into(),
                    match_root: "/w/http/src".into(),
                    artifact_source_prefix: "src".into(),
                    generated_match_root: Some("/w/http/.generated".into()),
                    artifact_generated_prefix: ".generated".into(),
                },
                ApiDocPackageRoots {
                    package: "corelib_foundation".into(),
                    match_root: foundation.into(),
                    artifact_source_prefix: "src".into(),
                    generated_match_root: None,
                    artifact_generated_prefix: ".generated".into(),
                },
            ],
        };
        let array_iterator = format!("{foundation}/Query/ArrayIterator.bd");
        // Sorted by qualified name, a dependency item comes first and seeds the stub location.
        let mut items = vec![
            row(1, "ArrayIterator", "type", &array_iterator),
            row(2, "Http::Client::Send", "function", "/w/http/src/Http/Client.bd"),
            row(3, "Core::Option", "type", &format!("{foundation}/Core/Option.bd")),
            row(4, "Core::Http", "function", "/w/http/src/Core/Http.bd"),
            row(5, "Query::Range", "function", &format!("{foundation}/Query/Range.bd")),
        ];

        link_api_doc_library_tree(&mut items, &empty_resolution());
        assign_declaring_packages(&mut items, &ctx);
        attribute_modules_to_members(&mut items);

        let module = |name: &str| items.iter().find(|i| i.kind == KIND_MODULE && i.qualified_name == name).unwrap();
        for own in ["Http", "Http::Client", "Core"] {
            let module = module(own);
            assert_eq!(module.declaring_package, None, "{own} holds corelib_http items");
            assert!(module.location.file.starts_with("/w/http/src/"), "{own} location: {}", module.location.file);
        }
        let query = module("Query");
        assert_eq!(query.declaring_package.as_deref(), Some("corelib_foundation"));
        assert_eq!(query.location.file, format!("{foundation}/Query/Range.bd"), "a module points at its own first item");
    }
}
