use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};

use sha2::{Digest, Sha256};
use zip::{ZipWriter, write::SimpleFileOptions};

use super::contracts::{Workspace, WorkspaceMember};

pub(super) fn build_member_artifact(
    workspace: &Workspace,
    member: &WorkspaceMember,
    version: &str,
) -> Result<Vec<u8>, &'static str> {
    let prefix = format!("{}/", member.relative_path);
    let mut entries = BTreeMap::new();
    for (path, bytes) in &workspace.entries {
        let Some(path) = path.strip_prefix(&prefix) else {
            continue;
        };
        if path == "Project.proj"
            || path.starts_with("src/")
            || path == "README.md"
            || path.starts_with("docs/")
            || path.starts_with(".beskid/docs/")
        {
            entries.insert(path.to_owned(), bytes.clone());
        }
    }
    if !entries.contains_key("Project.proj") || !entries.keys().any(|path| path.starts_with("src/")) {
        return Err("Workspace member must include Project.proj and source files.");
    }
    entries.insert("package.json".to_owned(), serde_json::to_vec(&serde_json::json!({"schema":"beskid.package.v1","id":member.package_name,"version":version,"packageKind":"library","dependencies":[]})).expect("JSON serialization succeeds"));
    let checksums = entries
        .iter()
        .map(|(path, bytes)| format!("{:x}  {path}", Sha256::digest(bytes)))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    entries.insert("checksums.sha256".to_owned(), checksums.into_bytes());
    let mut output = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut output);
        for (path, bytes) in entries {
            zip.start_file(path, SimpleFileOptions::default())
                .map_err(|_| "Workspace artifact could not be created.")?;
            zip.write_all(&bytes).map_err(|_| "Workspace artifact could not be created.")?;
        }
        zip.finish().map_err(|_| "Workspace artifact could not be created.")?;
    }
    Ok(output.into_inner())
}
