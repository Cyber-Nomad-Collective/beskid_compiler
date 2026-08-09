use super::{BTreeSet, Path, fs};

/// Parse `ReflectSdkNodeKind` variant names from `compiler_sdk_reflect.rs` (set equality tests).
pub fn reflect_sdk_node_kind_names(reflect_rs: &Path) -> Result<BTreeSet<String>, std::io::Error> {
    let text = fs::read_to_string(reflect_rs)?;
    let mut out = BTreeSet::new();
    let mut in_enum = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("pub enum ReflectSdkNodeKind") {
            in_enum = true;
            continue;
        }
        if in_enum {
            if t.starts_with('}') && !t.starts_with("},") {
                break;
            }
            if let Some(name) = t.strip_suffix(',') {
                let name = name.trim();
                if name.is_empty() || name.starts_with("//") {
                    continue;
                }
                if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    out.insert(name.to_string());
                }
            }
        }
    }
    Ok(out)
}
