//! Ordered post-instantiation actions (`beskidLock`, `runCommand`, `openReadme`, …).

use std::path::PathBuf;
use std::process::Command;

use crate::error::{TemplateError, TemplateResult};
use crate::manifest::TemplatePostAction;

#[derive(Debug, Clone)]
pub struct PostActionContext {
    pub output_root: PathBuf,
    pub lock_root: PathBuf,
    pub beskid_exe: Option<PathBuf>,
    pub strict: bool,
}

pub fn run_post_actions(
    actions: &[TemplatePostAction],
    ctx: &PostActionContext,
) -> TemplateResult<()> {
    for action in actions {
        run_one(action, ctx)?;
    }
    Ok(())
}

fn run_one(action: &TemplatePostAction, ctx: &PostActionContext) -> TemplateResult<()> {
    match action.action_id.as_str() {
        "beskidLock" | "beskidFetch" => run_beskid_lock(ctx),
        "runCommand" => run_command(action, ctx),
        "openReadme" => open_readme(action, ctx),
        other => {
            if ctx.strict {
                return Err(TemplateError::Internal(format!(
                    "unknown post-action `{other}`"
                )));
            }
            eprintln!("warning: unknown post-action `{other}` (skipped)");
            Ok(())
        }
    }
}

fn beskid_command(ctx: &PostActionContext) -> PathBuf {
    ctx.beskid_exe
        .clone()
        .unwrap_or_else(|| std::env::current_exe().unwrap_or_else(|_| PathBuf::from("beskid")))
}

fn run_beskid_lock(ctx: &PostActionContext) -> TemplateResult<()> {
    let exe = beskid_command(ctx);
    let project = ctx.lock_root.join("Project.proj");
    if !project.is_file() {
        let workspace = ctx.lock_root.join("Workspace.proj");
        if !workspace.is_file() {
            eprintln!(
                "warning: skipping beskidLock — no Project.proj or Workspace.proj at {}",
                ctx.lock_root.display()
            );
            return Ok(());
        }
        let status = Command::new(&exe)
            .args(["lock", "--project", workspace.to_str().unwrap_or_default()])
            .status()
            .map_err(TemplateError::Io)?;
        if !status.success() {
            return Err(TemplateError::Internal(format!(
                "beskid lock failed with status {status}"
            )));
        }
        return Ok(());
    }

    let status = Command::new(&exe)
        .args(["lock", "--project", project.to_str().unwrap_or_default()])
        .status()
        .map_err(TemplateError::Io)?;
    if !status.success() {
        return Err(TemplateError::Internal(format!(
            "beskid lock failed with status {status}"
        )));
    }
    Ok(())
}

fn run_command(action: &TemplatePostAction, ctx: &PostActionContext) -> TemplateResult<()> {
    let command = action
        .args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TemplateError::Internal("runCommand requires args.command".into()))?;
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&ctx.output_root)
        .status()
        .map_err(TemplateError::Io)?;
    if !status.success() {
        return Err(TemplateError::Internal(format!(
            "runCommand failed with status {status}"
        )));
    }
    Ok(())
}

fn open_readme(action: &TemplatePostAction, ctx: &PostActionContext) -> TemplateResult<()> {
    let path = action
        .args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("README.md");
    let full = ctx.output_root.join(path);
    if full.is_file() {
        println!("README: {}", full.display());
    }
    Ok(())
}
