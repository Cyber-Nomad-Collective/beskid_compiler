//! Function-level ECMA-426 mappings use byte offsets in the final Wasm binary.
use anyhow::{Context, Result, ensure};
use beskid_codegen::artifact::BrowserCoverageFunction;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use wasm_encoder::{Encode, Section};
use wasmparser::{ExternalKind, KnownCustom, Name, Parser, Payload, TypeRef};

fn vlq(value: i64, output: &mut String) {
    const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = (value.unsigned_abs() << 1) | u64::from(value < 0);
    loop {
        let digit = encoded & 31;
        encoded >>= 5;
        output.push(DIGITS[(digit + if encoded > 0 { 32 } else { 0 }) as usize] as char);
        if encoded == 0 {
            break;
        }
    }
}

pub(super) fn attach(
    bytes: &mut Vec<u8>,
    origins: &HashMap<String, BrowserCoverageFunction>,
    links: &BTreeMap<String, String>,
) -> Result<(String, Vec<u8>)> {
    let mut symbols = HashMap::new();
    let mut indices = HashMap::new();
    let mut bodies = Vec::new();
    let mut next = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload? {
            Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    if matches!(import?.ty, TypeRef::Func(_)) {
                        next += 1;
                    }
                }
            }
            Payload::ExportSection(section) => {
                for export in section {
                    let export = export?;
                    if export.kind == ExternalKind::Func {
                        indices.insert(export.name, export.index);
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                bodies.push((next, body.get_operators_reader()?.original_position()));
                next += 1;
            }
            Payload::CustomSection(section) => {
                if let KnownCustom::Name(names) = section.as_known() {
                    for group in names {
                        if let Name::Function(names) = group? {
                            for name in names {
                                let name = name?;
                                symbols.insert(name.index, name.name);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for (symbol, export) in links {
        if let Some(index) = indices.get(export.as_str()) {
            symbols.insert(*index, symbol);
        }
    }
    let mut files = HashMap::new();
    let mut paths = Vec::new();
    let mut contents = Vec::new();
    let mut mappings = String::new();
    let mut previous = [0i64; 4];
    let mut mapped = false;
    for (index, offset) in bodies {
        let origin = symbols.get(&index).and_then(|name| origins.get(*name));
        let location = origin.and_then(|origin| Some((origin, origin.byte_start?)));
        if location.is_none() && !mapped {
            continue;
        }
        if !mappings.is_empty() {
            mappings.push(',');
        }
        vlq(offset as i64 - previous[0], &mut mappings);
        previous[0] = offset as i64;
        mapped = location.is_some();
        if let Some((origin, start)) = location {
            let file = *files.entry(&origin.path).or_insert_with(|| {
                let id = paths.len();
                let filename = origin.path.file_name().unwrap_or_default().to_string_lossy();
                paths.push(format!("sources/{id}/{filename}"));
                contents.push(origin.source.as_ref());
                id as i64
            });
            let prefix = origin.source.get(..start).context("invalid source byte offset for Wasm map")?;
            let line = prefix.bytes().filter(|&b| b == b'\n').count() as i64;
            let column = prefix.rsplit('\n').next().unwrap_or("").encode_utf16().count() as i64;
            for (slot, value) in previous[1..].iter_mut().zip([file, line, column]) {
                vlq(value - *slot, &mut mappings);
                *slot = value;
            }
        }
    }
    ensure!(!paths.is_empty(), "browser debug artifact has no mapped source functions");
    let content = serde_json::to_vec(&serde_json::json!({
        "version": 3, "sources": paths, "sourcesContent": contents, "names": [], "mappings": mappings
    }))?;
    let name = format!("beskid-{:x}.wasm.map", Sha256::digest(&content));
    let mut data = Vec::new();
    name.encode(&mut data);
    wasm_encoder::CustomSection { name: "sourceMappingURL".into(), data: data.into() }.append_to(bytes);
    Ok((name, content))
}
