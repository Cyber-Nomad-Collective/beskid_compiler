//! Emit checked-in Beskid parser modules from `.pest` grammars.
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let mut args = env::args().skip(1);
    let grammar_path =
        args.next().map(PathBuf::from).expect("usage: emit_grammar <grammar.pest> <output.bd> [module_name]");
    let output_path =
        args.next().map(PathBuf::from).expect("usage: emit_grammar <grammar.pest> <output.bd> [module_name]");
    let module_name = args.next().unwrap_or_else(|| {
        grammar_path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("grammar").replace('-', "_")
    });

    let source =
        fs::read_to_string(&grammar_path).unwrap_or_else(|err| panic!("read {}: {err}", grammar_path.display()));
    let rules = beskid_pest_gen::parse_grammar_rules(&source)
        .unwrap_or_else(|err| panic!("parse {}: {err}", grammar_path.display()));
    let emitted = beskid_pest_gen::emit_combinator_module(&module_name, &rules);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).expect("create output parent");
    }
    fs::write(&output_path, emitted).unwrap_or_else(|err| {
        panic!("write {}: {err}", output_path.display());
    });
    eprintln!("wrote {} ({} rules) -> {}", grammar_path.display(), rules.len(), output_path.display());
}
