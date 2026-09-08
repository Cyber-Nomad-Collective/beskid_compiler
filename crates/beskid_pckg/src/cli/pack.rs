use super::versioning::{persist_pack_version_state, resolve_pack_version};
use super::{
    BTreeMap, CompressionMethod, Digest, PackArgs, PackProfile, PckgError, Sha256, SimpleFileOptions, Write, ZipWriter,
    build_package_json, collect_pack_entries, detect_pack_profile_with_override, fs, prepare_artifact_dependencies,
    prepare_template_pack_entries, strip_tool_pack_excludes, zip_to_pckg_error,
};

pub(super) fn execute_pack(args: PackArgs) -> Result<(), PckgError> {
    let source = args.source.clone();
    let output = args.output.clone();
    let resolved_version = resolve_pack_version(&source, &args)?;
    let profile = detect_pack_profile_with_override(&source, args.package_kind_override())?;

    let mut entries = collect_pack_entries(&source)?;
    if profile.is_template() {
        prepare_template_pack_entries(&mut entries)?;
    }
    if profile.is_tool() {
        strip_tool_pack_excludes(&mut entries);
    }
    if entries.is_empty() {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: "no files found to package".to_string(),
            body: None,
        });
    }
    let dependencies = prepare_artifact_dependencies(&source, &mut entries)?;

    if matches!(&profile, PackProfile::Library) {
        for (name, bytes) in &entries {
            if name == ".beskid/docs/api.json" {
                let root = crate::api_doc::ApiDocRoot::from_json_slice(bytes).map_err(|e| PckgError::Api {
                    status: reqwest::StatusCode::BAD_REQUEST,
                    message: format!("invalid `.beskid/docs/api.json` in package sources: {e}"),
                    body: None,
                })?;
                crate::api_doc::validate_packed_api_doc(&root).map_err(|e| PckgError::Api {
                    status: reqwest::StatusCode::BAD_REQUEST,
                    message: format!("invalid `.beskid/docs/api.json` in package sources: {e}"),
                    body: None,
                })?;
            }
        }
    }

    let has_api_docs = entries.iter().any(|(name, _)| name == ".beskid/docs/api.json");
    let package_json = build_package_json(&args.package, &resolved_version, &profile, has_api_docs, &dependencies)?;

    let mut checksums = BTreeMap::new();
    for (name, content) in &entries {
        checksums.insert(name.clone(), sha256_hex(content));
    }
    checksums.insert("package.json".to_string(), sha256_hex(package_json.as_bytes()));

    let checksums_sha =
        checksums.iter().map(|(path, digest)| format!("{digest}  {path}")).collect::<Vec<_>>().join("\n") + "\n";

    let file = fs::File::create(&output)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for (name, content) in entries {
        writer.start_file(name, options).map_err(zip_to_pckg_error)?;
        writer.write_all(&content)?;
    }

    writer.start_file("package.json", options).map_err(zip_to_pckg_error)?;
    writer.write_all(package_json.as_bytes())?;

    writer.start_file("checksums.sha256", options).map_err(zip_to_pckg_error)?;
    writer.write_all(checksums_sha.as_bytes())?;

    writer.finish().map_err(zip_to_pckg_error)?;
    persist_pack_version_state(&source, &args, &resolved_version)?;
    println!("Resolved package version: {resolved_version}");
    println!("Packed artifact at {}", output.display());

    Ok(())
}
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    format!("{hash:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::PackArgsPackageKind;
    use std::io::Read;

    #[test]
    fn skip_docs_packs_prepared_library_api_docs() {
        let source = std::env::temp_dir().join(format!("beskid_skip_docs_pack_{}", std::process::id()));
        let _ = fs::remove_dir_all(&source);
        let docs = source.join(".beskid/docs");
        fs::create_dir_all(&docs).expect("docs directory");
        fs::write(
            source.join("demo.bproj"),
            "demo { name = \"demo\" version = \"0.4.0\" root = \"src\" readme = \"README.md\" }\n\ntarget \"DemoLib\" { kind = Lib }\n",
        )
        .expect("project manifest");
        fs::write(source.join("README.md"), "# Demo\n").expect("readme");
        fs::create_dir_all(source.join("src")).expect("source directory");
        fs::write(source.join("src/Demo.bd"), "pub i32 Value() { return 1; }\n").expect("source");
        let prepared_api = br#"{"schemaVersion":2,"generator":"beskid test","source":"src/Demo.bd","items":[]}"#;
        fs::write(docs.join("api.json"), prepared_api).expect("prepared api.json");
        fs::write(docs.join("index.md"), "# API\n").expect("prepared index");
        let output = source.join("demo.bpk");

        execute_pack(PackArgs {
            package: "demo".into(),
            version: None,
            source: source.clone(),
            output: output.clone(),
            version_state_file: source.join("pack-version-state.json"),
            package_kind: PackArgsPackageKind::Auto,
            skip_docs: true,
        })
        .expect("pack prepared docs without regenerating them");

        let mut archive = zip::ZipArchive::new(fs::File::open(output).expect("artifact")).expect("zip artifact");
        let mut packed_api = Vec::new();
        archive
            .by_name(".beskid/docs/api.json")
            .expect("prepared api.json remains in artifact")
            .read_to_end(&mut packed_api)
            .expect("read packed api.json");
        assert_eq!(packed_api, prepared_api);
        let package_json: serde_json::Value =
            serde_json::from_reader(archive.by_name("package.json").expect("package manifest"))
                .expect("parse package manifest");
        assert_eq!(package_json["documentation"]["apiJson"], ".beskid/docs/api.json");
        let _ = fs::remove_dir_all(source);
    }
}
