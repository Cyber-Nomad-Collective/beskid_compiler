use crate::commands::doc::{self, DocArgs};
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};
use beskid_pckg::PckgArgs;
use beskid_pckg::cli::PckgCommand;
use miette::Report;
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub(super) fn ensure_corelib_ready() -> anyhow::Result<()> {
    let provisioned = beskid_tools::ensure_bundled_corelib()?;
    if provisioned.updated {
        println!("corelib: updated to {} at {}", provisioned.version, provisioned.root.display());
    }
    Ok(())
}

pub(super) fn anyhow_to_miette(error: anyhow::Error) -> Report {
    match error.downcast::<Report>() {
        Ok(report) => report,
        Err(error) => beskid_tools::diagnostics::report_from_anyhow(&error),
    }
}

pub(super) fn maybe_generate_docs_for_pack(args: &PckgArgs) -> anyhow::Result<()> {
    let PckgCommand::Pack(pack_args) = &args.command else {
        return Ok(());
    };
    if pack_args.skip_docs {
        return Ok(());
    }

    let source_root = absolutize_source_root(&pack_args.source)?;
    if matches!(beskid_pckg::detect_pack_profile(&source_root)?, beskid_pckg::PackProfile::Template(_)) {
        return Ok(());
    }
    let (input, project) = resolve_doc_entrypoint(&source_root)?;
    let out = source_root.join(".beskid").join("docs");

    let doc_args = DocArgs {
        input,
        project: ProjectResolveArgs { project, target: None, workspace_member: None },
        lockfile: LockfilePolicyArgs { frozen: false, locked: false },
        out,
    };
    doc::execute(doc_args)?;
    Ok(())
}

fn absolutize_source_root(source: &Path) -> anyhow::Result<PathBuf> {
    if source.is_absolute() {
        return Ok(source.to_path_buf());
    }
    Ok(env::current_dir()?.join(source))
}

fn resolve_doc_entrypoint(source_root: &Path) -> anyhow::Result<(Option<PathBuf>, Option<PathBuf>)> {
    if let Ok(Some(project_manifest)) = beskid_analysis::projects::discover_project_manifest_in_dir(source_root) {
        return Ok((None, Some(project_manifest)));
    }

    for candidate in
        [source_root.join("main.bd"), source_root.join("src").join("main.bd"), source_root.join("index.bd")]
    {
        if candidate.exists() {
            return Ok((Some(candidate), None));
        }
    }

    let mut bd_files: Vec<PathBuf> = WalkDir::new(source_root)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("bd"))
        .collect();
    bd_files.sort();
    if bd_files.len() == 1 {
        return Ok((Some(bd_files.remove(0)), None));
    }

    anyhow::bail!(
        "cannot infer docs entrypoint for package source {} (expected a `.bproj` manifest, main.bd/src/main.bd, or a single .bd file)",
        source_root.display()
    )
}
