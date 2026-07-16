//! Host-independent verifier for explicit ABI-v5 runtime symbol lists.

use std::io::{self, Read};
use std::path::Path;
use std::process::ExitCode;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_provenance::{parse_symbol_list, RuntimeProvenanceAudit};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("runtime provenance audit: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    match arguments.as_slice() {
        [flag, target] if flag == "--audit" => {
            let audit = audit_for(target)?;
            println!("{}", audit.to_json().map_err(|error| error.to_string())?);
            Ok(())
        }
        [flag, target] if flag == "--fixture" => {
            let audit = audit_for(target)?;
            println!("target={}", audit.target);
            for symbol in audit.allowed_exports {
                println!("defined={symbol}");
            }
            for symbol in audit.allowed_imports {
                println!("undefined={symbol}");
            }
            Ok(())
        }
        [flag, path] if flag == "--verify" => {
            let source = read_symbol_list(path)?;
            let symbols = parse_symbol_list(&source).map_err(|error| error.to_string())?;
            audit_for(&symbols.target)?
                .verify(&symbols)
                .map_err(|error| error.to_string())
        }
        _ => Err("usage: beskid_runtime_provenance (--audit <target> | --fixture <target> | --verify <symbol-list>|-)".into()),
    }
}

fn audit_for(triple: &str) -> Result<RuntimeProvenanceAudit, String> {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == triple)
        .ok_or_else(|| format!("unsupported ABI-v5 target `{triple}`"))?;
    RuntimeProvenanceAudit::canonical(target)
        .map_err(|error| format!("invalid ABI-v5 manifest: {error:?}"))
}

fn read_symbol_list(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut source = String::new();
        io::stdin()
            .read_to_string(&mut source)
            .map_err(|error| format!("read standard input: {error}"))?;
        return Ok(source);
    }
    std::fs::read_to_string(Path::new(path)).map_err(|error| format!("read `{path}`: {error}"))
}
