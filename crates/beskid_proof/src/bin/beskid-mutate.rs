//! Deterministic mutation generator for the internal-error sweep.
//!
//!   beskid-mutate plan  --seed N [--sample N] [--ext bd] <root>...
//!       prints one `label<TAB>kind` line per planned mutant; labels are paths relative to the
//!       root they were found under, prefixed with the root's index (`0:src/Main.bd`).
//!   beskid-mutate apply --seed N --kind K --label L <file>
//!       prints the mutant of <file> on stdout; exit status 3 when the kind yields no mutant.

use std::fs;
use std::path::{Path, PathBuf};

use beskid_proof::mutate::{MutationKind, mutant_for, plan};

fn usage() -> ! {
    eprintln!(
        "usage: beskid-mutate plan --seed N [--sample N] [--ext bd] <root>...\n       beskid-mutate apply --seed N --kind <delete-token|duplicate-line|swap-tokens> --label L <file>"
    );
    std::process::exit(2);
}

fn walk(root: &Path, dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // `file_type` does not follow symlinks: linked directories and files are never mutated.
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_dir() {
            if !matches!(name.as_str(), "obj" | ".git" | "target" | "node_modules" | ".beskid") {
                walk(root, &path, ext, out);
            }
        } else if file_type.is_file() && path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else { usage() };
    let (mut seed, mut sample, mut ext, mut kind, mut label) = (1u64, None, "bd".to_string(), None, None);
    let mut positional = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => seed = args.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()),
            "--sample" => sample = Some(args.next().and_then(|v| v.parse::<usize>().ok()).unwrap_or_else(|| usage())),
            "--ext" => ext = args.next().unwrap_or_else(|| usage()),
            "--kind" => kind = Some(args.next().and_then(|v| MutationKind::parse(&v)).unwrap_or_else(|| usage())),
            "--label" => label = Some(args.next().unwrap_or_else(|| usage())),
            _ => positional.push(arg),
        }
    }
    match command.as_str() {
        "plan" => {
            if positional.is_empty() {
                usage();
            }
            let mut files = Vec::new();
            for (index, root) in positional.iter().enumerate() {
                let root = PathBuf::from(root);
                let mut found = Vec::new();
                walk(&root, &root, &ext, &mut found);
                for path in found {
                    let Ok(text) = fs::read_to_string(&path) else { continue };
                    let relative = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                    files.push((format!("{index}:{relative}"), text));
                }
            }
            for planned in plan(&files, seed, sample) {
                println!("{}\t{}", planned.label, planned.kind.name());
            }
        }
        "apply" => {
            let (Some(kind), Some(label)) = (kind, label) else { usage() };
            let [file] = positional.as_slice() else { usage() };
            let text = fs::read_to_string(file).unwrap_or_else(|error| {
                eprintln!("cannot read {file}: {error}");
                std::process::exit(2);
            });
            match mutant_for(&text, seed, &label, kind) {
                Some(mutant) => print!("{mutant}"),
                None => std::process::exit(3),
            }
        }
        _ => usage(),
    }
}
