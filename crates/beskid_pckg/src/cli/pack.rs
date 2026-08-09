use super::versioning::{persist_pack_version_state, resolve_pack_version};
use super::{
    BTreeMap, CompressionMethod, Digest, PackArgs, PackProfile, PckgError, Sha256, SimpleFileOptions, Write, ZipWriter,
    build_package_json, collect_pack_entries, detect_pack_profile_with_override, fs, strip_template_pack_excludes,
    strip_tool_pack_excludes, zip_to_pckg_error,
};

pub(super) fn execute_pack(args: PackArgs) -> Result<(), PckgError> {
    let source = args.source.clone();
    let output = args.output.clone();
    let resolved_version = resolve_pack_version(&source, &args)?;
    let profile = detect_pack_profile_with_override(&source, args.package_kind_override())?;

    let mut entries = collect_pack_entries(&source)?;
    if profile.is_template() {
        strip_template_pack_excludes(&mut entries);
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

    let package_json = build_package_json(&args.package, &resolved_version, &profile)?;

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
