use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs::File, io, path::PathBuf};

pub(super) struct Binaryen {
    bin: PathBuf,
}

impl Binaryen {
    pub(super) fn discover() -> Result<Self> {
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../binaryen-kit.json"))?;
        let host = if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            "x86_64-apple-darwin"
        } else {
            beskid_abi::runtime_kit::host_runtime_triple()?
        };
        let version = manifest["version"].as_u64().context("Binaryen kit version")?;
        let prefix = beskid_abi::runtime_kit::installed_runtime_prefix()
            .context("install the browser compiler kit or set BESKID_RUNTIME_PREFIX to its prefix")?;
        let kit = prefix.join("lib/beskid-wasm").join(version.to_string()).join(host);
        let files = manifest["platforms"][host]["files"].as_object().context("unknown Binaryen kit host")?;
        for (name, hash) in files {
            let path = kit.join(name);
            let mut file =
                File::open(&path).with_context(|| format!("missing browser compiler tool {}", path.display()))?;
            let mut digest = Sha256::new();
            io::copy(&mut file, &mut digest)?;
            ensure!(
                Some(format!("{:x}", digest.finalize()).as_str()) == hash.as_str(),
                "browser compiler kit checksum mismatch for {}",
                path.display()
            );
        }
        Ok(Self { bin: kit.join("bin").canonicalize()? })
    }

    pub(super) fn tool(&self, name: &str) -> PathBuf {
        self.bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    }
}
