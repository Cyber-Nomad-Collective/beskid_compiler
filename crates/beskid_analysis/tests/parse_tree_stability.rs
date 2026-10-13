//! Grammar changes must keep the pest pair tree (rule names and byte spans) of existing sources
//! byte-identical. Mod SDK syntax projections and span-keyed facts depend on it.
//!
//! The golden file records, for every `.bd` source in the compiler repository (corelib
//! included), the SHA-256 of its bytes and the SHA-256 of its pair tree. A source whose bytes
//! still match the golden entry must produce the same tree digest. Sources whose bytes changed
//! are skipped; re-bless with `BESKID_BLESS_PARSE_TREES=1` only when the source edit is
//! intended and the grammar is unchanged.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use beskid_analysis::parser::{BeskidParser, Rule};
use pest::Parser;
use pest::iterators::Pair;
use sha2::{Digest, Sha256};

const GOLDEN: &str = "tests/fixtures/parse_tree_stability.tsv";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("compiler root")
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name == "obj" || name == "node_modules" || name.starts_with('.') {
                continue;
            }
            collect_sources(&path, out);
        } else if name.ends_with(".bd") {
            out.push(path);
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn write_pair(pair: Pair<Rule>, depth: usize, out: &mut String) {
    let span = pair.as_span();
    let _ = writeln!(out, "{depth} {:?} {} {}", pair.as_rule(), span.start(), span.end());
    for inner in pair.into_inner() {
        write_pair(inner, depth + 1, out);
    }
}

fn tree_digest(source: &str) -> String {
    match BeskidParser::parse(Rule::Program, source) {
        Ok(pairs) => {
            let mut text = String::new();
            for pair in pairs {
                write_pair(pair, 0, &mut text);
            }
            hex(&Sha256::digest(text.as_bytes()))
        }
        Err(_) => "parse-error".to_string(),
    }
}

/// Debug aid: `BESKID_DUMP_PARSE_TREE=<file> cargo test --test parse_tree_stability dump` prints
/// the pair tree of one source so that two grammar revisions can be diffed.
#[test]
fn dump_parse_tree_on_request() {
    let Some(path) = std::env::var_os("BESKID_DUMP_PARSE_TREE") else { return };
    let source = std::fs::read_to_string(&path).expect("source to dump");
    let pairs = BeskidParser::parse(Rule::Program, &source).expect("source parses");
    let mut text = String::new();
    for pair in pairs {
        write_pair(pair, 0, &mut text);
    }
    println!("{text}");
}

#[test]
fn existing_sources_keep_identical_parse_trees() {
    let root = repo_root();
    let mut sources = Vec::new();
    collect_sources(&root, &mut sources);
    sources.sort();

    let mut current = BTreeMap::new();
    for path in &sources {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(text) = String::from_utf8(bytes.clone()) else { continue };
        let relative = path.strip_prefix(&root).expect("relative").to_string_lossy().replace('\\', "/");
        current.insert(relative, (hex(&Sha256::digest(&bytes)), tree_digest(&text)));
    }

    let golden_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
    if std::env::var_os("BESKID_BLESS_PARSE_TREES").is_some() {
        let mut text = String::new();
        for (path, (content, tree)) in &current {
            let _ = writeln!(text, "{path}\t{content}\t{tree}");
        }
        std::fs::create_dir_all(golden_path.parent().expect("fixtures dir")).expect("create fixtures");
        std::fs::write(&golden_path, text).expect("write golden");
        return;
    }

    let golden = std::fs::read_to_string(&golden_path).expect("golden parse tree digests");
    let mut compared = 0usize;
    let mut changed = Vec::new();
    for line in golden.lines() {
        let mut fields = line.split('\t');
        let (Some(path), Some(content), Some(tree)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let Some((current_content, current_tree)) = current.get(path) else { continue };
        if current_content != content {
            continue;
        }
        compared += 1;
        if current_tree != tree {
            changed.push(path.to_string());
        }
    }
    assert!(compared > 500, "only {compared} golden sources were compared; is the corelib submodule checked out?");
    assert!(changed.is_empty(), "parse trees changed for unchanged sources:\n{}", changed.join("\n"));
}
