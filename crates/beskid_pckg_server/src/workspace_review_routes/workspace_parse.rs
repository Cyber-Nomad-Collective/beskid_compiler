use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

use zip::ZipArchive;

use super::contracts::{MAX_WORKSPACE_ENTRY_BYTES, MAX_WORKSPACE_UNCOMPRESSED_BYTES, Workspace, WorkspaceMember};

pub(super) fn parse_workspace(bytes: &[u8]) -> Result<Workspace, &'static str> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Workspace bundle is not a valid ZIP archive.")?;
    if !(1..=10_000).contains(&archive.len()) {
        return Err("Workspace bundle is empty or too large.");
    }
    let mut entries = BTreeMap::new();
    let mut uncompressed_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|_| "Workspace bundle is not a valid ZIP archive.")?;
        if entry.is_dir() {
            continue;
        }
        let entry_size = entry.size();
        if entry_size > MAX_WORKSPACE_ENTRY_BYTES {
            return Err("Workspace bundle contains an entry that exceeds the size limit.");
        }
        uncompressed_bytes = uncompressed_bytes.saturating_add(entry_size);
        if uncompressed_bytes > MAX_WORKSPACE_UNCOMPRESSED_BYTES {
            return Err("Workspace bundle exceeds the uncompressed size limit.");
        }
        let path = entry.name().replace('\\', "/");
        if path.is_empty()
            || path.starts_with('/')
            || path.split('/').any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            return Err("Workspace bundle contains an unsafe entry path.");
        }
        let mut contents = Vec::with_capacity(entry_size as usize);
        entry.read_to_end(&mut contents).map_err(|_| "Workspace bundle could not be read.")?;
        if entries.insert(path, contents).is_some() {
            return Err("Workspace bundle contains duplicate entries.");
        }
    }
    let project =
        std::str::from_utf8(entries.get("Workspace.proj").ok_or("Workspace bundle is missing 'Workspace.proj'.")?)
            .map_err(|_| "Workspace.proj must be UTF-8.")?;
    let name = quoted_assignment(project, "name").ok_or("Workspace.proj is missing a workspace name.")?;
    let configured =
        entries.get("workspace.package.json").and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok());
    let mut members = Vec::new();
    let lines = project.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let Some(after) = trimmed.strip_prefix("member \"") else {
            continue;
        };
        let Some((member_id, after_id)) = after.split_once('"') else {
            return Err("Workspace.proj has an invalid member declaration.");
        };
        let mut member_block = after_id.to_owned();
        for candidate in lines.iter().skip(index + 1) {
            member_block.push('\n');
            member_block.push_str(candidate);
            if candidate.contains('}') {
                break;
            }
        }
        let relative_path = quoted_assignment(&member_block, "path").ok_or("Workspace member is missing its path.")?;
        let package_name = configured
            .as_ref()
            .and_then(|value| value.get("members")?.get(member_id)?.get("package")?.as_str())
            .map(str::to_owned)
            .or_else(|| {
                entries
                    .get(&format!("{relative_path}/Project.proj"))
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .and_then(|manifest| quoted_assignment(manifest, "name"))
            })
            .ok_or("Workspace member is missing a package name.")?;
        if relative_path.contains("..") || package_name.trim().is_empty() {
            return Err("Workspace member is invalid.");
        }
        members.push(WorkspaceMember { member_id: member_id.to_owned(), relative_path, package_name });
    }
    if members.is_empty() {
        return Err("Workspace bundle has no members.");
    }
    Ok(Workspace { name, entries, members })
}

fn quoted_assignment(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix(key)?.trim_start().strip_prefix('=')?.trim();
        rest.strip_prefix('"')?.split_once('"').map(|(value, _)| value.to_owned())
    })
}
