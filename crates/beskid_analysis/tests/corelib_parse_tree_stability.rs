//! Grammar edits must keep the parse trees of existing sources byte-identical: the Mod SDK
//! authorizes syntax trees by their exact shape and spans.
//!
//! The fixture records, for every Corelib `.bd` source, the SHA-256 of the source text and the
//! SHA-256 of its pest parse tree (rule name, start byte, end byte of every pair, in pre-order).
//! A source whose text still matches its recorded hash must still produce the recorded tree.
//! Sources that changed since the fixture was written are not checked; re-bless the fixture with
//! `BESKID_BLESS_PARSE_TREES=1` only from a grammar revision whose trees are known to be correct.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use beskid_analysis::{BeskidParser, Rule};
use pest::Parser;
use sha2::{Digest, Sha256};

const FIXTURE: &str = "tests/fixtures/corelib_parse_trees.txt";

fn corelib_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib")
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if path.is_dir() {
            if name == "obj" || name == "target" || name.starts_with('.') {
                continue;
            }
            collect_sources(&path, out);
        } else if name.ends_with(".bd") {
            out.push(path);
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn tree_digest(source: &str) -> String {
    match BeskidParser::parse(Rule::Program, source) {
        Ok(pairs) => {
            let mut hasher = Sha256::new();
            for pair in pairs.flatten() {
                let span = pair.as_span();
                hasher.update(format!("{:?}:{}:{};", pair.as_rule(), span.start(), span.end()).as_bytes());
            }
            hex(&hasher.finalize())
        }
        Err(_) => "parse-error".to_string(),
    }
}

fn current_trees() -> BTreeMap<String, (String, String)> {
    let root = corelib_root();
    let mut sources = Vec::new();
    collect_sources(&root, &mut sources);
    assert!(!sources.is_empty(), "no Corelib sources under {}", root.display());
    sources
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
            let relative = path.strip_prefix(&root).expect("source under root").to_string_lossy().replace('\\', "/");
            let source_hash = hex(&Sha256::digest(source.as_bytes()));
            (relative, (source_hash, tree_digest(&source)))
        })
        .collect()
}

#[test]
fn corelib_parse_trees_stay_byte_identical() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let current = current_trees();

    if std::env::var_os("BESKID_BLESS_PARSE_TREES").is_some() {
        let mut text = String::new();
        for (path, (source_hash, tree_hash)) in &current {
            let _ = writeln!(text, "{path} {source_hash} {tree_hash}");
        }
        std::fs::create_dir_all(fixture_path.parent().expect("fixture dir")).expect("create fixture dir");
        std::fs::write(&fixture_path, text).expect("write fixture");
        return;
    }

    let recorded = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", fixture_path.display()));
    let mut checked = 0usize;
    let mut changed = Vec::new();
    for line in recorded.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split(' ');
        let (Some(path), Some(source_hash), Some(tree_hash), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            panic!("malformed fixture line {line:?}");
        };
        let Some((current_source, current_tree)) = current.get(path) else {
            continue;
        };
        if current_source != source_hash {
            continue;
        }
        checked += 1;
        if current_tree != tree_hash {
            changed.push(format!("{path}: recorded {tree_hash}, now {current_tree}"));
        }
    }
    assert!(checked > 0, "no Corelib source still matches the fixture; re-bless from a known-good grammar");
    assert!(changed.is_empty(), "parse trees changed for unchanged sources:\n{}", changed.join("\n"));
}
