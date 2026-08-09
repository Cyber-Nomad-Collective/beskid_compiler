use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Args;

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};

#[derive(Args, Debug)]
pub struct DocArgs {
    /// Beskid source file (same resolution as `analyze` when combined with `--project`).
    /// Project-backed docs use the entry import closure (same scope as `beskid build`), not a full workspace scan.
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Output directory for `api.json` and `index.md`
    #[arg(long, default_value = "doc-out")]
    pub out: PathBuf,
}

#[derive(Clone, Debug)]
pub(super) struct DocEntry {
    pub(super) qualified_name: String,
    pub(super) kind: String,
    pub(super) doc_markdown: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct LocationJson {
    pub(super) file: String,
    pub(super) start_line: usize,
    pub(super) start_column: usize,
    pub(super) end_line: usize,
    pub(super) end_column: usize,
}

#[derive(Default, Debug)]
pub(super) struct TreeNode {
    pub(super) children: BTreeMap<String, TreeNode>,
    pub(super) entries: Vec<usize>,
}
