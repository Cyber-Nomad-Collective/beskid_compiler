use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use tempfile::NamedTempFile;
use unicode_normalization::UnicodeNormalization;
use zip::ZipArchive;

use crate::projects::error::ProjectError;

const MAX_ENTRY_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 10_000;
const MAX_ZIP_NAME_BYTES: usize = 4_096;
const MAX_ZIP_NAME_COMPONENTS: usize = 256;
const MAX_RETAINED_PREFIX_KEY_BYTES: usize = 64 * 1024 * 1024;
const ZIP_EOCD_BYTES: usize = 22;
const MAX_ZIP_COMMENT_BYTES: usize = u16::MAX as usize;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArchivePathKind {
    Directory,
    File,
}

pub(super) fn extract_zip_to_dir(mut file: File, output_dir: &Path) -> Result<(), ProjectError> {
    validate_zip_eocd(&mut file)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|err| ProjectError::Validation(format!("invalid registry artifact ZIP: {err}")))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(ProjectError::Validation("registry artifact ZIP exceeds the 10,000 entry limit".into()));
    }

    // Inspect all names and declared sizes before writing any entry. The copy
    // loop also counts actual output because ZIP metadata is untrusted.
    let mut planned_paths = BTreeMap::new();
    let mut case_folded_paths = BTreeMap::new();
    let mut retained_prefix_key_bytes = 0_usize;
    let mut declared_total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| ProjectError::Validation(format!("failed to read registry artifact entry: {err}")))?;
        if entry.name().len() > MAX_ZIP_NAME_BYTES {
            return Err(ProjectError::Validation(
                "registry artifact ZIP entry exceeds the 4,096 UTF-8 name-byte limit".into(),
            ));
        }
        let path = checked_entry_name(&entry)?;
        if path.components().count() > MAX_ZIP_NAME_COMPONENTS {
            return Err(ProjectError::Validation("registry artifact ZIP entry exceeds the 256 component limit".into()));
        }
        plan_archive_path(
            &mut planned_paths,
            &mut case_folded_paths,
            &mut retained_prefix_key_bytes,
            &path,
            entry.is_dir(),
        )?;
        if !entry.is_dir() {
            check_output_limits(entry.size(), &mut declared_total)?;
        } else if entry.size() != 0 || entry.crc32() != 0 {
            return Err(ProjectError::Validation(format!(
                "registry artifact directory entry must have no payload and a zero CRC: {}",
                path.display()
            )));
        }
        validate_destination(output_dir, &path, entry.is_dir())?;
    }

    let mut actual_total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| ProjectError::Validation(format!("failed to read registry artifact entry: {err}")))?;
        let path = checked_entry_name(&entry)?;
        let target = validate_destination(output_dir, &path, entry.is_dir())?;
        if entry.is_dir() {
            let mut probe = [0_u8; 1];
            if entry.read(&mut probe).map_err(|error| {
                ProjectError::Validation(format!("registry artifact directory entry failed validation: {error}"))
            })? != 0
            {
                return Err(ProjectError::Validation(format!(
                    "registry artifact directory entry has an unexpected payload: {}",
                    path.display()
                )));
            }
            create_checked_directories(output_dir, &path)?;
            continue;
        }

        let parent = path.parent().expect("checked ZIP entry has a parent");
        create_checked_directories(output_dir, parent)?;
        validate_destination(output_dir, &path, false)?;

        // Persisting a temporary file replaces an existing symlink itself,
        // rather than following it as File::create would. Parent paths are
        // checked again immediately before the replacement.
        let mut scratch = NamedTempFile::new_in(output_dir.join(parent)).map_err(|source| {
            ProjectError::MaterializationCopy { from: output_dir.to_path_buf(), to: target.clone(), source }
        })?;
        copy_entry_with_limits(&mut entry, scratch.as_file_mut(), &target, &mut actual_total)?;
        validate_destination(output_dir, &path, false)?;
        scratch.persist(&target).map_err(|error| ProjectError::MaterializationCopy {
            from: output_dir.to_path_buf(),
            to: target,
            source: error.error,
        })?;
    }

    Ok(())
}

pub(super) fn validate_zip_eocd(file: &mut File) -> Result<(), ProjectError> {
    // ZipArchive stores entries by name, so its len() can be smaller than the
    // number of central-directory records. Bound the raw declared count before
    // ZipArchive parses or allocates for those records.
    let archive_bytes = file
        .metadata()
        .map_err(|error| ProjectError::Validation(format!("cannot inspect registry artifact ZIP: {error}")))?
        .len();
    let tail_bytes = archive_bytes.min((ZIP_EOCD_BYTES + MAX_ZIP_COMMENT_BYTES) as u64) as usize;
    if tail_bytes < ZIP_EOCD_BYTES {
        return Err(ProjectError::Validation("registry artifact ZIP EOCD is missing or misplaced".into()));
    }
    file.seek(SeekFrom::End(-(tail_bytes as i64)))
        .map_err(|error| ProjectError::Validation(format!("cannot seek registry artifact ZIP EOCD: {error}")))?;
    let mut tail = vec![0_u8; tail_bytes];
    file.read_exact(&mut tail)
        .map_err(|error| ProjectError::Validation(format!("cannot read registry artifact ZIP EOCD: {error}")))?;

    // The ZIP reader may reject a forged EOCD in a real EOCD's comment and
    // backtrack to the real one. Never validate one header while it parses
    // another: more than one EOF-anchored EOCD candidate is ambiguous.
    let candidates = (0..=tail_bytes - ZIP_EOCD_BYTES)
        .filter(|&start| {
            &tail[start..start + 4] == b"PK\x05\x06"
                && start + ZIP_EOCD_BYTES + u16::from_le_bytes([tail[start + 20], tail[start + 21]]) as usize
                    == tail_bytes
        })
        .take(2)
        .count();
    if candidates > 1 {
        return Err(ProjectError::Validation("registry artifact ZIP EOCD is ambiguous".into()));
    }

    for start in (0..=tail_bytes - ZIP_EOCD_BYTES).rev() {
        if &tail[start..start + 4] != b"PK\x05\x06" {
            continue;
        }
        let comment_bytes = u16::from_le_bytes([tail[start + 20], tail[start + 21]]) as usize;
        if start + ZIP_EOCD_BYTES + comment_bytes != tail_bytes {
            continue;
        }
        let disk_number = u16::from_le_bytes([tail[start + 4], tail[start + 5]]);
        let central_disk = u16::from_le_bytes([tail[start + 6], tail[start + 7]]);
        let entries_on_disk = u16::from_le_bytes([tail[start + 8], tail[start + 9]]);
        let entries_total = u16::from_le_bytes([tail[start + 10], tail[start + 11]]);
        let central_bytes = u32::from_le_bytes(tail[start + 12..start + 16].try_into().expect("fixed EOCD field"));
        let central_offset = u32::from_le_bytes(tail[start + 16..start + 20].try_into().expect("fixed EOCD field"));
        let eocd_offset = archive_bytes - tail_bytes as u64 + start as u64;
        let has_zip64_locator = if start >= 20 {
            &tail[start - 20..start - 16] == b"PK\x06\x07"
        } else if eocd_offset >= 20 {
            // With a maximum-length comment the locator falls before the
            // bounded EOCD tail. Read only its fixed 20-byte slot.
            file.seek(SeekFrom::Start(eocd_offset - 20)).map_err(|error| {
                ProjectError::Validation(format!("cannot seek registry artifact ZIP64 locator: {error}"))
            })?;
            let mut locator = [0_u8; 20];
            file.read_exact(&mut locator).map_err(|error| {
                ProjectError::Validation(format!("cannot read registry artifact ZIP64 locator: {error}"))
            })?;
            &locator[..4] == b"PK\x06\x07"
        } else {
            false
        };
        if entries_on_disk == u16::MAX
            || entries_total == u16::MAX
            || central_bytes == u32::MAX
            || central_offset == u32::MAX
            || has_zip64_locator
        {
            return Err(ProjectError::Validation("registry artifact ZIP64 format is unsupported".into()));
        }
        if disk_number != 0 || central_disk != 0 || entries_on_disk != entries_total {
            return Err(ProjectError::Validation("registry artifact multi-disk ZIP is unsupported".into()));
        }
        if entries_total as usize > MAX_ZIP_ENTRIES {
            return Err(ProjectError::Validation("registry artifact ZIP exceeds the 10,000 entry limit".into()));
        }
        // Registry artifacts use a single contiguous central directory
        // immediately before EOCD, so all readers agree on its extent.
        if central_offset as u64 + central_bytes as u64 != eocd_offset {
            return Err(ProjectError::Validation(
                "registry artifact ZIP EOCD has an invalid central-directory range".into(),
            ));
        }
        validate_central_directory_records(file, central_offset as u64, central_bytes as u64, entries_total as usize)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| ProjectError::Validation(format!("cannot reset registry artifact ZIP: {error}")))?;
        return Ok(());
    }
    Err(ProjectError::Validation("registry artifact ZIP EOCD is missing or misplaced".into()))
}

fn validate_central_directory_records(
    file: &mut File,
    offset: u64,
    length: u64,
    declared_count: usize,
) -> Result<(), ProjectError> {
    let end = offset + length;
    let mut position = offset;
    let mut actual_count = 0_usize;
    let mut extra_buffer = vec![0_u8; u16::MAX as usize];
    let mut raw_names = BTreeSet::new();
    while position < end {
        if end - position < 46 {
            return Err(ProjectError::Validation("registry artifact ZIP central-directory record is truncated".into()));
        }
        file.seek(SeekFrom::Start(position)).map_err(|error| {
            ProjectError::Validation(format!("cannot seek registry artifact ZIP central-directory record: {error}"))
        })?;
        let mut header = [0_u8; 46];
        file.read_exact(&mut header).map_err(|error| {
            ProjectError::Validation(format!("cannot read registry artifact ZIP central-directory record: {error}"))
        })?;
        if &header[..4] != b"PK\x01\x02" {
            return Err(ProjectError::Validation(
                "registry artifact ZIP central-directory contains an unsupported record".into(),
            ));
        }
        let name_bytes = u16::from_le_bytes([header[28], header[29]]) as u64;
        let extra_bytes = u16::from_le_bytes([header[30], header[31]]) as u64;
        let comment_bytes = u16::from_le_bytes([header[32], header[33]]) as u64;
        let disk_start = u16::from_le_bytes([header[34], header[35]]);
        let compressed_bytes = u32::from_le_bytes(header[20..24].try_into().expect("fixed central field"));
        let uncompressed_bytes = u32::from_le_bytes(header[24..28].try_into().expect("fixed central field"));
        let local_offset = u32::from_le_bytes(header[42..46].try_into().expect("fixed central field"));
        if disk_start == u16::MAX
            || compressed_bytes == u32::MAX
            || uncompressed_bytes == u32::MAX
            || local_offset == u32::MAX
        {
            return Err(ProjectError::Validation("registry artifact ZIP64 format is unsupported".into()));
        }
        if disk_start != 0 {
            return Err(ProjectError::Validation("registry artifact multi-disk ZIP is unsupported".into()));
        }
        let record_bytes = 46 + name_bytes + extra_bytes + comment_bytes;
        let next = position.checked_add(record_bytes).ok_or_else(|| {
            ProjectError::Validation("registry artifact ZIP central-directory record length overflows".into())
        })?;
        if next > end {
            return Err(ProjectError::Validation("registry artifact ZIP central-directory record is truncated".into()));
        }
        if name_bytes as usize > MAX_ZIP_NAME_BYTES {
            return Err(ProjectError::Validation(
                "registry artifact ZIP entry exceeds the 4,096 UTF-8 name-byte limit".into(),
            ));
        }
        file.seek(SeekFrom::Start(position + 46)).map_err(|error| {
            ProjectError::Validation(format!("cannot seek registry artifact ZIP central-directory name: {error}"))
        })?;
        let mut raw_name = vec![0_u8; name_bytes as usize];
        file.read_exact(&mut raw_name).map_err(|error| {
            ProjectError::Validation(format!("cannot read registry artifact ZIP central-directory name: {error}"))
        })?;
        if !raw_names.insert(raw_name) {
            return Err(ProjectError::Validation("registry artifact contains a duplicate ZIP entry name".into()));
        }
        if has_zip64_extra(file, position + 46 + name_bytes, extra_bytes, &mut extra_buffer)? {
            return Err(ProjectError::Validation("registry artifact ZIP64 format is unsupported".into()));
        }
        validate_local_header(file, local_offset as u64, offset, &mut extra_buffer)?;
        position = next;
        actual_count += 1;
        if actual_count > MAX_ZIP_ENTRIES {
            return Err(ProjectError::Validation("registry artifact ZIP exceeds the 10,000 entry limit".into()));
        }
    }
    if actual_count != declared_count {
        return Err(ProjectError::Validation(
            "registry artifact ZIP central-directory count disagrees with EOCD".into(),
        ));
    }
    Ok(())
}

fn validate_local_header(
    file: &mut File,
    offset: u64,
    central_start: u64,
    extra_buffer: &mut [u8],
) -> Result<(), ProjectError> {
    if offset + 30 > central_start {
        return Err(ProjectError::Validation("registry artifact ZIP local header is outside file data".into()));
    }
    file.seek(SeekFrom::Start(offset)).map_err(|error| {
        ProjectError::Validation(format!("cannot seek registry artifact ZIP local header: {error}"))
    })?;
    let mut header = [0_u8; 30];
    file.read_exact(&mut header).map_err(|error| {
        ProjectError::Validation(format!("cannot read registry artifact ZIP local header: {error}"))
    })?;
    if &header[..4] != b"PK\x03\x04" {
        return Err(ProjectError::Validation("registry artifact ZIP local header has an invalid signature".into()));
    }
    let compressed_bytes = u32::from_le_bytes(header[18..22].try_into().expect("fixed local field"));
    let uncompressed_bytes = u32::from_le_bytes(header[22..26].try_into().expect("fixed local field"));
    if compressed_bytes == u32::MAX || uncompressed_bytes == u32::MAX {
        return Err(ProjectError::Validation("registry artifact ZIP64 format is unsupported".into()));
    }
    let name_bytes = u16::from_le_bytes([header[26], header[27]]) as u64;
    let extra_bytes = u16::from_le_bytes([header[28], header[29]]) as u64;
    let extra_start = offset + 30 + name_bytes;
    if extra_start + extra_bytes > central_start {
        return Err(ProjectError::Validation("registry artifact ZIP local extra fields leave file data".into()));
    }
    if has_zip64_extra(file, extra_start, extra_bytes, extra_buffer)? {
        return Err(ProjectError::Validation("registry artifact ZIP64 format is unsupported".into()));
    }
    Ok(())
}

fn has_zip64_extra(file: &mut File, offset: u64, length: u64, buffer: &mut [u8]) -> Result<bool, ProjectError> {
    let length = length as usize;
    let bytes = &mut buffer[..length];
    if bytes.is_empty() {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| ProjectError::Validation(format!("cannot seek registry artifact ZIP extra field: {error}")))?;
    file.read_exact(bytes)
        .map_err(|error| ProjectError::Validation(format!("cannot read registry artifact ZIP extra field: {error}")))?;
    let mut position = 0_usize;
    while position < length {
        if length - position < 4 {
            return Err(ProjectError::Validation("registry artifact ZIP extra field is truncated".into()));
        }
        let field_id = u16::from_le_bytes([bytes[position], bytes[position + 1]]);
        let field_bytes = u16::from_le_bytes([bytes[position + 2], bytes[position + 3]]) as usize;
        position += 4 + field_bytes;
        if position > length {
            return Err(ProjectError::Validation("registry artifact ZIP extra field is truncated".into()));
        }
        if field_id == 0x0001 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn plan_archive_path(
    paths: &mut BTreeMap<PathBuf, ArchivePathKind>,
    case_folded_paths: &mut BTreeMap<String, PathBuf>,
    retained_prefix_key_bytes: &mut usize,
    path: &Path,
    is_dir: bool,
) -> Result<(), ProjectError> {
    let mut prefix = PathBuf::new();
    let mut folded_prefix = String::new();
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err(ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()));
        };
        prefix.push(segment);
        let normalized_segment = segment.to_string_lossy().nfkc().collect::<String>();
        let folded_segment =
            unicase::UniCase::unicode(normalized_segment.as_str()).to_folded_case().nfkc().collect::<String>();
        validate_portable_zip_segment(&folded_segment)?;
        if !folded_prefix.is_empty() {
            folded_prefix.push('/');
        }
        folded_prefix.push_str(&folded_segment);
        if let Some(existing) = case_folded_paths.get(&folded_prefix)
            && existing != &prefix
        {
            return Err(ProjectError::Validation(format!(
                "registry artifact ZIP entries have a case-insensitive path alias: {} and {}",
                existing.display(),
                prefix.display()
            )));
        }
        let kind =
            if is_dir || index + 1 < components.len() { ArchivePathKind::Directory } else { ArchivePathKind::File };
        match paths.get(&prefix) {
            Some(existing) if *existing != kind || kind == ArchivePathKind::File => {
                return Err(ProjectError::Validation(format!(
                    "registry artifact ZIP entries conflict at {}",
                    prefix.display()
                )));
            }
            Some(_) => {}
            None => {
                let retained_bytes =
                    prefix.to_string_lossy().len().checked_add(folded_prefix.len()).ok_or_else(|| {
                        ProjectError::Validation("registry artifact planned prefix-key budget overflows".into())
                    })?;
                *retained_prefix_key_bytes =
                    (*retained_prefix_key_bytes).checked_add(retained_bytes).ok_or_else(|| {
                        ProjectError::Validation("registry artifact planned prefix-key budget overflows".into())
                    })?;
                if *retained_prefix_key_bytes > MAX_RETAINED_PREFIX_KEY_BYTES {
                    return Err(ProjectError::Validation(
                        "registry artifact exceeds the 64 MiB cumulative planned prefix-key limit".into(),
                    ));
                }
                case_folded_paths.insert(folded_prefix.clone(), prefix.clone());
                paths.insert(prefix.clone(), kind);
            }
        }
    }
    Ok(())
}

fn validate_portable_zip_segment(segment: &str) -> Result<(), ProjectError> {
    let invalid_win32_character = segment
        .chars()
        .any(|character| character < ' ' || matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'));
    let stem = segment.split('.').next().unwrap_or_default().trim_end_matches(' ');
    let reserved_device = matches!(stem, "con" | "prn" | "aux" | "nul")
        || stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9');
    if segment.ends_with(['.', ' ']) || invalid_win32_character || reserved_device {
        return Err(ProjectError::Validation(format!(
            "registry artifact contains a non-portable ZIP path component: {segment}"
        )));
    }
    Ok(())
}

fn check_output_limits(entry_bytes: u64, total: &mut u64) -> Result<(), ProjectError> {
    if entry_bytes > MAX_ENTRY_UNCOMPRESSED_BYTES {
        return Err(ProjectError::Validation(
            "registry artifact entry exceeds the 512 MiB uncompressed size limit".into(),
        ));
    }
    *total = total
        .checked_add(entry_bytes)
        .ok_or_else(|| ProjectError::Validation("registry artifact uncompressed size overflows its budget".into()))?;
    if *total > MAX_TOTAL_UNCOMPRESSED_BYTES {
        return Err(ProjectError::Validation(
            "registry artifact exceeds the 1 GiB total uncompressed size limit".into(),
        ));
    }
    Ok(())
}

fn copy_entry_with_limits(
    entry: &mut zip::read::ZipFile<'_>,
    destination: &mut File,
    destination_path: &Path,
    total: &mut u64,
) -> Result<(), ProjectError> {
    let mut entry_bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = entry.read(&mut buffer).map_err(|source| ProjectError::MaterializationCopy {
            from: destination_path.to_path_buf(),
            to: destination_path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        let read = read as u64;
        entry_bytes = entry_bytes.checked_add(read).ok_or_else(|| {
            ProjectError::Validation("registry artifact entry uncompressed size overflows its budget".into())
        })?;
        if entry_bytes > MAX_ENTRY_UNCOMPRESSED_BYTES {
            return Err(ProjectError::Validation(
                "registry artifact entry exceeds the 512 MiB uncompressed size limit".into(),
            ));
        }
        *total = total.checked_add(read).ok_or_else(|| {
            ProjectError::Validation("registry artifact total uncompressed size overflows its budget".into())
        })?;
        if *total > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(ProjectError::Validation(
                "registry artifact exceeds the 1 GiB total uncompressed size limit".into(),
            ));
        }
        destination.write_all(&buffer[..read as usize]).map_err(|source| ProjectError::MaterializationCopy {
            from: destination_path.to_path_buf(),
            to: destination_path.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

fn checked_entry_name(entry: &zip::read::ZipFile<'_>) -> Result<PathBuf, ProjectError> {
    let path = entry
        .enclosed_name()
        .ok_or_else(|| ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()))?;
    let mode_type = entry.unix_mode().map(|mode| mode & 0o170000).unwrap_or(0);
    if path.as_os_str().is_empty()
        || entry.is_symlink()
        || !matches!(mode_type, 0 | 0o040000 | 0o100000)
        || mode_type == 0o040000 && !entry.is_dir()
        || mode_type == 0o100000 && entry.is_dir()
    {
        return Err(ProjectError::Validation("registry artifact contains a symlink or special ZIP entry".into()));
    }
    Ok(path)
}

fn validate_destination(output_dir: &Path, relative: &Path, is_dir: bool) -> Result<PathBuf, ProjectError> {
    for ancestor in output_dir.ancestors() {
        validate_existing_path(ancestor, true, false)?;
    }

    let components = relative.components().collect::<Vec<_>>();
    let mut target = output_dir.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err(ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()));
        };
        target.push(segment);
        validate_existing_path(&target, is_dir || index + 1 < components.len(), true)?;
    }
    Ok(target)
}

fn validate_existing_path(path: &Path, is_dir: bool, allow_missing: bool) -> Result<(), ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_redirecting_path(&metadata) => Err(ProjectError::Validation(format!(
            "registry artifact extraction path contains a symlink: {}",
            path.display()
        ))),
        Ok(metadata) if is_dir && !metadata.is_dir() || !is_dir && !metadata.is_file() => {
            Err(ProjectError::Validation(format!(
                "registry artifact extraction path has the wrong file type: {}",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if allow_missing && error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ProjectError::Validation(format!(
            "registry artifact extraction path cannot be inspected: {}: {error}",
            path.display()
        ))),
    }
}

fn is_redirecting_path(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

pub(super) fn verify_materialized_tree(expected: &Path, actual: &Path) -> Result<(), ProjectError> {
    let expected_metadata = fs::symlink_metadata(expected)
        .map_err(|error| tampered_materialization(actual, &format!("staged package cannot be inspected: {error}")))?;
    let actual_metadata = fs::symlink_metadata(actual)
        .map_err(|error| tampered_materialization(actual, &format!("cached package cannot be inspected: {error}")))?;
    if is_redirecting_path(&actual_metadata) {
        return Err(tampered_materialization(actual, "contains a symlink or reparse point"));
    }
    if expected_metadata.is_dir() {
        if !actual_metadata.is_dir() {
            return Err(tampered_materialization(actual, "a directory was replaced"));
        }
        let mut actual_names = BTreeSet::new();
        for entry in fs::read_dir(actual)
            .map_err(|error| tampered_materialization(actual, &format!("cannot list cached directory: {error}")))?
        {
            let entry = entry
                .map_err(|error| tampered_materialization(actual, &format!("cannot list cached directory: {error}")))?;
            actual_names.insert(entry.file_name());
        }
        for entry in fs::read_dir(expected)
            .map_err(|error| tampered_materialization(actual, &format!("cannot list staged directory: {error}")))?
        {
            let entry = entry
                .map_err(|error| tampered_materialization(actual, &format!("cannot list staged directory: {error}")))?;
            let name = entry.file_name();
            if !actual_names.remove(&name) {
                return Err(tampered_materialization(&actual.join(&name), "is missing"));
            }
            verify_materialized_tree(&entry.path(), &actual.join(name))?;
        }
        if let Some(extra) = actual_names.first() {
            return Err(tampered_materialization(&actual.join(extra), "is an extra path"));
        }
        return Ok(());
    }
    if !expected_metadata.is_file() || !actual_metadata.is_file() || expected_metadata.len() != actual_metadata.len() {
        return Err(tampered_materialization(actual, "has changed type or size"));
    }
    let mut expected_file = File::open(expected)
        .map_err(|error| tampered_materialization(actual, &format!("cannot read staged file: {error}")))?;
    let mut actual_file = File::open(actual)
        .map_err(|error| tampered_materialization(actual, &format!("cannot read cached file: {error}")))?;
    let mut expected_buffer = [0_u8; 64 * 1024];
    let mut actual_buffer = [0_u8; 64 * 1024];
    let mut remaining = expected_metadata.len();
    while remaining > 0 {
        let chunk = usize::try_from(remaining.min(expected_buffer.len() as u64)).unwrap();
        expected_file
            .read_exact(&mut expected_buffer[..chunk])
            .map_err(|error| tampered_materialization(actual, &format!("cannot read staged file: {error}")))?;
        actual_file
            .read_exact(&mut actual_buffer[..chunk])
            .map_err(|error| tampered_materialization(actual, &format!("cannot read cached file: {error}")))?;
        if expected_buffer[..chunk] != actual_buffer[..chunk] {
            return Err(tampered_materialization(actual, "contains changed bytes"));
        }
        remaining -= chunk as u64;
    }
    Ok(())
}

fn tampered_materialization(path: &Path, reason: &str) -> ProjectError {
    ProjectError::Validation(format!(
        "tampered registry materialization at {} ({reason}); clean and rebuild the generated package cache",
        path.display()
    ))
}

fn create_checked_directories(output_dir: &Path, relative: &Path) -> Result<(), ProjectError> {
    let mut directory = output_dir.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(ProjectError::Validation("registry artifact contains an unsafe ZIP entry path".into()));
        };
        directory.push(segment);
        match fs::symlink_metadata(&directory) {
            Ok(_) => validate_existing_path(&directory, true, false)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&directory)
                    .map_err(|source| ProjectError::MaterializationCreateDir { path: directory.clone(), source })?;
                validate_existing_path(&directory, true, false)?;
            }
            Err(error) => {
                return Err(ProjectError::Validation(format!(
                    "registry artifact extraction path cannot be inspected: {}: {error}",
                    directory.display()
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{MAX_ENTRY_UNCOMPRESSED_BYTES, check_output_limits, plan_archive_path};

    #[test]
    fn declared_archive_total_must_not_exceed_one_gib_even_when_entries_fit() {
        let mut total = 0;
        check_output_limits(MAX_ENTRY_UNCOMPRESSED_BYTES, &mut total).unwrap();
        check_output_limits(MAX_ENTRY_UNCOMPRESSED_BYTES, &mut total).unwrap();

        let error = check_output_limits(1, &mut total).expect_err("the third entry must exceed the total budget");

        assert!(error.to_string().contains("1 GiB"), "unexpected error: {error}");
    }

    #[test]
    fn retained_prefix_keys_must_not_exceed_sixty_four_mib() {
        let mut paths = BTreeMap::new();
        let mut folded_paths = BTreeMap::new();
        let mut retained_bytes = 0;
        let tail = std::iter::repeat_n("abcdefghijklmnop", 199).collect::<Vec<_>>().join("/");
        let mut rejected = false;
        for index in 0..220 {
            let path = PathBuf::from(format!("D{index:03}/{tail}"));
            match plan_archive_path(&mut paths, &mut folded_paths, &mut retained_bytes, &path, false) {
                Ok(()) => {}
                Err(error) => {
                    assert!(error.to_string().contains("64 MiB"), "unexpected error: {error}");
                    rejected = true;
                    break;
                }
            }
        }
        assert!(rejected, "planned prefix keys exceeded 64 MiB without rejection");
    }
}
