//! Completion inside `use` paths: child modules of the segments typed so far.
//!
//! `use C` offers top-level modules (the project's own and every dependency package's),
//! `use Core.` offers the child modules of `Core`, `use Core.String.` offers the submodules of
//! `Core.String`. Children come from the effective source roots, using the same layout the
//! module resolver uses (`A/B.bd`, `A/B/B.bd`, `A/B/mod.bd`; directories nest modules), so the
//! answer needs no analysis facts and works while a buffer does not parse.

use std::collections::BTreeSet;
use std::path::Path;

use beskid_analysis::projects::EffectiveCompilationRoots;
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, TextEdit};

use crate::position::offset_range_to_lsp;

/// Where the cursor sits inside a `use` path.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UsePathContext {
    /// Complete segments before the segment being typed.
    pub prefix: Vec<String>,
    /// Characters of the segment being typed, before the cursor.
    pub partial: String,
    pub replacement_start: usize,
    pub replacement_end: usize,
}

/// Recognize `use a.b.c|` (optionally `pub use`) on the cursor line.
pub(crate) fn use_path_context(text: &str, offset: usize) -> Option<UsePathContext> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let before = text[line_start..offset].trim_start();
    let rest = before.strip_prefix("pub").map(str::trim_start).unwrap_or(before);
    let path = rest.strip_prefix("use")?;
    if !path.starts_with(char::is_whitespace) {
        return None;
    }
    let path = path.trim_start();
    if !path.chars().all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '.') {
        return None;
    }
    let mut segments: Vec<String> = path.split('.').map(str::to_owned).collect();
    let partial = segments.pop().unwrap_or_default();
    if segments.iter().any(String::is_empty) {
        return None;
    }
    let replacement_start = offset - partial.len();
    let tail =
        text[offset..].chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').map(char::len_utf8).sum::<usize>();
    Some(UsePathContext { prefix: segments, partial, replacement_start, replacement_end: offset + tail })
}

fn is_module_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && name != "mod" && name != "obj" && name != "target"
}

/// Child module names of `prefix` across every root.
pub(crate) fn module_children(roots: &EffectiveCompilationRoots, prefix: &[String]) -> BTreeSet<String> {
    let mut children = BTreeSet::new();
    for root in std::iter::once(&roots.host).chain(roots.dependencies.iter()) {
        let mut directory = root.source_root.clone();
        for segment in prefix {
            directory.push(segment);
        }
        collect_children(&directory, prefix.last().map(String::as_str), &mut children);
    }
    children
}

fn collect_children(directory: &Path, homonym: Option<&str>, children: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if is_module_name(name) {
                children.insert(name.to_owned());
            }
        } else if let Some(stem) = name.strip_suffix(".bd") {
            // `String/String.bd` is the module itself, and `.g.bd` files are generated.
            if is_module_name(stem) && !stem.ends_with(".g") && Some(stem) != homonym {
                children.insert(stem.to_owned());
            }
        }
    }
}

/// Completion items for the `use` path at `offset`, or `None` when the cursor is not in one.
pub(crate) fn use_path_items(
    text: &str,
    offset: usize,
    roots: &EffectiveCompilationRoots,
) -> Option<Vec<CompletionItem>> {
    let context = use_path_context(text, offset)?;
    let partial = context.partial.to_lowercase();
    let range = offset_range_to_lsp(text, context.replacement_start, context.replacement_end);
    Some(
        module_children(roots, &context.prefix)
            .into_iter()
            .filter(|name| name.to_lowercase().starts_with(&partial))
            .map(|name| CompletionItem {
                kind: Some(CompletionItemKind::MODULE),
                detail: Some("module".to_owned()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text: name.clone() })),
                label: name,
                ..CompletionItem::default()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use beskid_analysis::projects::RootEntry;
    use tempfile::TempDir;

    use super::*;

    fn roots() -> (TempDir, EffectiveCompilationRoots) {
        let temp = TempDir::new().expect("roots");
        let host = temp.path().join("host");
        let core = temp.path().join("core");
        fs::create_dir_all(host.join("Util")).unwrap();
        fs::write(host.join("Main.bd"), "").unwrap();
        fs::write(host.join("Util/Strings.bd"), "").unwrap();
        fs::create_dir_all(core.join("Core/String")).unwrap();
        fs::create_dir_all(core.join("Core/Collections")).unwrap();
        fs::write(core.join("Core/Results.bd"), "").unwrap();
        fs::write(core.join("Core/String/String.bd"), "").unwrap();
        for module in ["Core", "Search", "Transform"] {
            fs::write(core.join(format!("Core/String/{module}.bd")), "").unwrap();
        }
        fs::write(core.join("Core/Collections/Array.bd"), "").unwrap();
        fs::write(core.join("Core/Collections/mod.bd"), "").unwrap();
        fs::write(core.join("Core/Collections/Array.g.bd"), "").unwrap();
        let roots = EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: host },
            dependencies: vec![RootEntry { dependency_name: Some("Corelib".into()), source_root: core }],
        };
        (temp, roots)
    }

    fn labels(text: &str, roots: &EffectiveCompilationRoots) -> Vec<String> {
        let offset = text.find('|').expect("cursor marker");
        let text = text.replace('|', "");
        use_path_items(&text, offset, roots).expect("inside a use path").into_iter().map(|item| item.label).collect()
    }

    #[test]
    fn first_segment_offers_project_modules_and_dependency_packages() {
        let (_temp, roots) = roots();
        let all = labels("use |", &roots);
        assert!(all.iter().any(|l| l == "Core") && all.iter().any(|l| l == "Util") && all.iter().any(|l| l == "Main"));
        assert_eq!(labels("use C|", &roots), vec!["Core"]);
        assert_eq!(labels("pub use ut|", &roots), vec!["Util"]);
    }

    #[test]
    fn second_segment_lists_child_modules_of_core() {
        let (_temp, roots) = roots();
        let text = "use Core.Results;\nuse Core.|";
        assert_eq!(labels(text, &roots), vec!["Collections", "Results", "String"]);
        assert_eq!(labels("use Core.S|", &roots), vec!["String"]);
    }

    #[test]
    fn later_segments_list_submodules_without_the_module_itself() {
        let (_temp, roots) = roots();
        assert_eq!(labels("use Core.String.|", &roots), vec!["Core", "Search", "Transform"]);
        assert_eq!(labels("use Core.Collections.|", &roots), vec!["Array"]);
        assert_eq!(labels("use Util.|", &roots), vec!["Strings"]);
    }

    #[test]
    fn replacement_covers_the_whole_segment_under_the_cursor() {
        let (_temp, roots) = roots();
        let text = "use Core.Str;";
        let items = use_path_items(text, 12, &roots).expect("use path");
        let Some(CompletionTextEdit::Edit(edit)) = &items[0].text_edit else { panic!("edit") };
        assert_eq!((edit.range.start.character, edit.range.end.character), (9, 12));
    }

    #[test]
    fn other_positions_are_not_use_paths() {
        for text in ["i32 Main() { Core.| }", "user.|", "use|", "use Core. String.|"] {
            let offset = text.find('|').unwrap();
            assert!(use_path_context(&text.replace('|', ""), offset).is_none(), "{text}");
        }
    }
}
