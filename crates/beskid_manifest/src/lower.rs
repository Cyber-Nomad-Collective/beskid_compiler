//! Lower a validated runtime Bsol document into [`ManifestRoot`].

use bsol::{ValidatedBlock, ValidatedDocument};

use crate::model::{
    DispatchEntry, DispatchTables, IntrinsicEntry, KernelEntry, ManifestMeta, ManifestProfiles,
    ManifestRoot, ProfileEntry,
};

pub fn lower_runtime_manifest(document: ValidatedDocument) -> Result<ManifestRoot, String> {
    let mut manifest = ManifestMeta { abi_version: 0 };
    let mut kernel = Vec::new();
    let mut dispatch_usize = Vec::new();
    let mut dispatch_ptr = Vec::new();
    let mut dispatch_unit = Vec::new();
    let mut dispatch_i64 = Vec::new();
    let mut intrinsic = Vec::new();
    let mut profiles = ManifestProfiles::default();

    for block in document.blocks {
        match block.rule_id.as_str() {
            "manifest" => {
                manifest.abi_version = parse_u32_field(&block, "abi_version")?;
            }
            "profile" => {
                let label = block
                    .label
                    .ok_or_else(|| "profile block requires label".to_string())?;
                let owners = block.lists.get("owners").cloned().unwrap_or_default();
                let entry = ProfileEntry { owners };
                match label.as_str() {
                    "minimal" => profiles.minimal = entry,
                    "std" => profiles.std = entry,
                    other => return Err(format!("unknown runtime profile `{other}`")),
                }
            }
            "kernel" => kernel.push(lower_kernel(block)?),
            "dispatch_usize" => dispatch_usize.push(lower_dispatch(block)?),
            "dispatch_ptr" => dispatch_ptr.push(lower_dispatch(block)?),
            "dispatch_unit" => dispatch_unit.push(lower_dispatch(block)?),
            "dispatch_i64" => dispatch_i64.push(lower_dispatch(block)?),
            "intrinsic" => intrinsic.push(lower_intrinsic(block)?),
            other => return Err(format!("unknown runtime block `{other}`")),
        }
    }

    if manifest.abi_version == 0 {
        return Err("missing required `manifest` block".to_string());
    }

    Ok(ManifestRoot {
        manifest,
        kernel,
        dispatch: DispatchTables {
            usize: dispatch_usize,
            ptr: dispatch_ptr,
            unit: dispatch_unit,
            i64: dispatch_i64,
        },
        intrinsic,
        profiles,
    })
}

fn lower_kernel(block: ValidatedBlock) -> Result<KernelEntry, String> {
    Ok(KernelEntry {
        symbol: required_field(&block, "symbol")?,
        name: required_field(&block, "name")?,
        params: list_field(&block, "params"),
        returns: required_field(&block, "returns")?,
        injected: bool_field(&block, "injected").unwrap_or(false),
        beskid_path: list_field(&block, "beskid_path"),
    })
}

fn lower_dispatch(block: ValidatedBlock) -> Result<DispatchEntry, String> {
    Ok(DispatchEntry {
        dispatch_key: required_field(&block, "dispatch_key")?,
        name: required_field(&block, "name")?,
        tag: parse_u32_field(&block, "tag")?,
        params: list_field(&block, "params"),
        returns: required_field(&block, "returns")?,
        injected: bool_field(&block, "injected").unwrap_or(true),
        beskid_path: list_field(&block, "beskid_path"),
        owner: block
            .fields
            .get("owner")
            .cloned()
            .unwrap_or_else(|| "language".to_string()),
    })
}

fn lower_intrinsic(block: ValidatedBlock) -> Result<IntrinsicEntry, String> {
    Ok(IntrinsicEntry {
        symbol: required_field(&block, "symbol")?,
        path: list_field(&block, "path"),
        params: list_field(&block, "params"),
        returns: required_field(&block, "returns")?,
        injected: bool_field(&block, "injected").unwrap_or(false),
    })
}

fn required_field(block: &ValidatedBlock, key: &str) -> Result<String, String> {
    block
        .fields
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing required field `{key}`"))
}

fn list_field(block: &ValidatedBlock, key: &str) -> Vec<String> {
    block.lists.get(key).cloned().unwrap_or_default()
}

fn bool_field(block: &ValidatedBlock, key: &str) -> Option<bool> {
    block.fields.get(key).map(|value| value == "true")
}

fn parse_u32_field(block: &ValidatedBlock, key: &str) -> Result<u32, String> {
    let value = required_field(block, key)?;
    value
        .parse::<u32>()
        .map_err(|_| format!("`{key}` must be a u32, found `{value}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bsol::{load_profile, parse_bsol_document, validate};

    #[test]
    fn lower_runtime_manifest_smoke() {
        let src = r#"manifest { abi_version = 4 }
profile "minimal" { owners = [language] }
kernel {
  symbol = alloc
  name = Alloc
  params = [usize, ptr]
  returns = ptr
  injected = true
  beskid_path = [__alloc]
}
"#;
        let doc = parse_bsol_document(src).expect("parse");
        let profile = load_profile("runtime.v1").expect("profile");
        let validated = validate(&doc, &profile).expect("validate");
        let root = lower_runtime_manifest(validated).expect("lower");
        assert_eq!(root.manifest.abi_version, 4);
        assert_eq!(root.kernel.len(), 1);
        assert_eq!(root.profiles.minimal.owners, vec!["language".to_string()]);
    }
}
