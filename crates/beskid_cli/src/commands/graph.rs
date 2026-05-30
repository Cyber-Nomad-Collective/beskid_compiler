//! `beskid graph` — render workspace/project graphs as Mermaid (TUI or raw).

use std::fs;
use std::io::{self, IsTerminal, Write, stdout};
use std::path::PathBuf;

use anyhow::Result;
use beskid_graph::GraphKind;
use beskid_queries::{get_graph_document, get_graph_document_simple, GraphFetchRequest, with_db};
use clap::Args;
use graphs_tui::{RenderOptions, render_mermaid_to_tui};

use crate::frontend::resolve_input_with_pipeline;
use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};

#[derive(Args, Debug)]
pub struct GraphArgs {
    /// Input Beskid file or project context
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Graph kind: project, workspace, module, imports, host
    #[arg(long, default_value = "project")]
    pub kind: String,

    /// Emit raw Mermaid syntax instead of terminal TUI
    #[arg(long)]
    pub mermaid: bool,

    /// Force terminal TUI even when stdout is not a TTY
    #[arg(long)]
    pub tui: bool,

    /// Write Mermaid output to a file
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Disable resolve progress UI
    #[arg(long)]
    pub plain: bool,
}

pub fn execute(args: GraphArgs) -> Result<()> {
    let kind = GraphKind::parse(&args.kind).ok_or_else(|| {
        anyhow::anyhow!("unknown graph kind `{}` (use project|workspace|module|imports|host)", args.kind)
    })?;

    let resolved = resolve_input_with_pipeline(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
        None,
    )?;

    let manifest_path = resolved
        .compile_plan
        .as_ref()
        .map(|p| p.manifest_path.clone())
        .or_else(|| args.project.project.clone())
        .ok_or_else(|| anyhow::anyhow!("could not resolve project manifest"))?;

    let workspace_manifest = resolved
        .workspace_summary
        .as_ref()
        .map(|ws| ws.workspace_manifest_path.clone());

    let request = GraphFetchRequest {
        kind,
        manifest_path,
        workspace_manifest,
        compile_plan: resolved.compile_plan.clone(),
        entry_path: Some(resolved.source_path.clone()),
        entry_source: Some(resolved.source.clone()),
    };

    let doc = with_db(|db| get_graph_document(db, &request))
        .or_else(|_| get_graph_document_simple(&request))?;

    for warning in &doc.spec.warnings {
        eprintln!("warning [{}]: {}", warning_code(warning.code), warning.message);
    }

    let use_tui = (args.tui || stdout().is_terminal()) && !args.mermaid && args.out.is_none();

    if let Some(out_path) = &args.out {
        fs::write(out_path, &doc.mermaid)?;
        eprintln!("Wrote graph to {}", out_path.display());
        return Ok(());
    }

    if use_tui {
        let result = render_mermaid_to_tui(&doc.mermaid, RenderOptions::default())?;
        for warning in &result.warnings {
            eprintln!("layout warning: {warning}");
        }
        io::stdout().write_all(result.output.as_bytes())?;
        io::stdout().write_all(b"\n")?;
    } else {
        print!("{}", doc.mermaid);
        if !doc.mermaid.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn warning_code(code: beskid_graph::GraphWarningCode) -> &'static str {
    code.as_str()
}
