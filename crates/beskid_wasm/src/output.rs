//! Publish the module last so readers always have the matching host available.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::Path,
};
use tempfile::NamedTempFile;

fn browser_host_source() -> String {
    use beskid_abi::runtime_source::browser_protocol::{Value, operations};
    use serde_json::json;
    let methods: Vec<_> = operations().iter()
        .filter(|operation| matches!(operation.capability, "http" | "websocket"))
        .map(|op| {
            let indices = |kind| op.parameters.iter().enumerate()
                .filter_map(|(index, &ty)| (ty == kind).then_some(index)).collect::<Vec<_>>();
            json!([op.service.symbol, {
                "id": op.capability, "parameters": vec!["i64"; op.parameters.len()],
                "results": vec!["i64"; usize::from(op.result != Value::Unit)],
                "maySuspend": op.suspends, "strings": indices(Value::String), "bytes": indices(Value::Bytes),
                "stringResult": op.result == Value::String, "operation": op.service.name.trim_start_matches("__")
            }])
        }).collect();
    format!(
        "const browserProtocolMethods = {};\nconst browserNetworkMethods = {};\nconst browserSynchronizationMethods = {};\nconst browserCompositionMethods = {};\n{}",
        json!(methods), crate::network::browser_methods(), crate::scheduler::browser_methods(), crate::composition::browser_methods(), concat!(
            include_str!("../browser_io.mjs"), include_str!("../browser_network.mjs"),
            include_str!("../browser_files.mjs"), include_str!("../browser_scheduler.mjs"),
            include_str!("../browser_composition.mjs"), include_str!("../browser.mjs")
        )
    )
}

fn stage(directory: &Path, path: &Path, bytes: &[u8]) -> Result<NamedTempFile> {
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = match fs::metadata(path) {
            Ok(metadata) => metadata.permissions(),
            Err(error) if error.kind() == ErrorKind::NotFound => fs::Permissions::from_mode(0o666),
            Err(error) => return Err(error.into()),
        };
        builder.permissions(permissions);
    }
    let mut file = builder.tempfile_in(directory)?;
    file.write_all(bytes)?;
    Ok(file)
}

pub(super) fn publish(path: &Path, bytes: &[u8], map: Option<(String, Vec<u8>)>) -> Result<()> {
    let directory = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    ensure!(
        !path.file_name().is_some_and(|p| p.eq_ignore_ascii_case("beskid.js")),
        "beskid.js is reserved for the browser host; choose another Wasm output filename"
    );
    let host = directory.join("beskid.js");
    let module = stage(directory, path, bytes)?;
    let host_source = browser_host_source();
    let source = host_source.as_bytes();
    // Concurrent builds may safely share a directory if they use identical host code.
    match fs::read(&host) {
        Ok(existing) if existing == source => {}
        Ok(_) => anyhow::bail!("{} contains a different host; choose a separate output directory", host.display()),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let staged = stage(directory, &host, source)?;
            if let Err(err) = staged.persist_noclobber(&host) {
                if err.error.kind() != ErrorKind::AlreadyExists {
                    return Err(err.error.into());
                }
                ensure!(
                    fs::read(&host)? == source,
                    "conflicting host at {}; choose a separate output directory",
                    host.display()
                );
            }
        }
        Err(err) => return Err(err.into()),
    }
    if let Some((name, content)) = map {
        let sidecar = directory.join(name);
        ensure!(path.file_name() != sidecar.file_name(), "output conflicts with its source map");
        let file = stage(directory, &sidecar, &content)?;
        if let Err(err) = file.persist_noclobber(&sidecar) {
            if err.error.kind() != ErrorKind::AlreadyExists { return Err(err.error.into()); }
            ensure!(fs::read(&sidecar)? == content, "conflicting source map at {}", sidecar.display());
        }
    }
    module.persist(path).with_context(|| format!("writing browser module {}", path.display()))?;
    Ok(())
}
