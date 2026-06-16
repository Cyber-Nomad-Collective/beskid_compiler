//! Shell scope: user, project, or workspace context.

use std::path::{Path, PathBuf};

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use beskid_analysis::projects::discovery::{
    DEFAULT_DESCENDANT_SEARCH_DEPTH, discover_project_file, discover_project_file_descendant,
    discover_workspace_file, discover_workspace_file_descendant,
};

/// Where the shell is rooted for contextual commands and board config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellScope {
    User,
    Project { root: PathBuf, manifest: PathBuf },
    Workspace { root: PathBuf, manifest: PathBuf },
}

impl ShellScope {
    pub fn resolve(start: &Path) -> Self {
        if let Some(scope) = Self::from_workspace_manifest(discover_workspace_file(start), start) {
            return scope;
        }
        if let Some(scope) = Self::from_project_manifest(discover_project_file(start), start) {
            return scope;
        }
        if let Some(scope) = Self::from_workspace_manifest(
            discover_workspace_file_descendant(start, DEFAULT_DESCENDANT_SEARCH_DEPTH),
            start,
        ) {
            return scope;
        }
        if let Some(scope) = Self::from_project_manifest(
            discover_project_file_descendant(start, DEFAULT_DESCENDANT_SEARCH_DEPTH),
            start,
        ) {
            return scope;
        }
        Self::User
    }

    /// Re-resolve from `path` (cwd on launch, after picker, or before scoped commands).
    pub fn resolve_cwd(path: &Path) -> Self {
        Self::resolve(path)
    }

    pub fn is_user(&self) -> bool {
        matches!(self, Self::User)
    }

    pub fn has_project(&self) -> bool {
        !self.is_user()
    }

    fn from_workspace_manifest(manifest: Option<PathBuf>, start: &Path) -> Option<Self> {
        let manifest = manifest?;
        let root = manifest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| start.to_path_buf());
        Some(Self::Workspace { root, manifest })
    }

    fn from_project_manifest(manifest: Option<PathBuf>, start: &Path) -> Option<Self> {
        let manifest = manifest?;
        let root = manifest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| start.to_path_buf());
        Some(Self::Project { root, manifest })
    }

    /// Re-resolve when CLI params name an explicit project/workspace path.
    pub fn resolve_for_cli(scope: &Self, params: &str) -> Self {
        let trimmed = params.trim();
        if !trimmed.is_empty() {
            let first = trimmed.split_whitespace().next().unwrap_or(trimmed);
            let path = Path::new(first);
            if path.exists() {
                return Self::resolve(path);
            }
        }
        scope.clone()
    }

    pub fn label(&self) -> String {
        match self {
            Self::User => "user".into(),
            Self::Project { manifest, .. } => format!("project:{}", manifest.display()),
            Self::Workspace { manifest, .. } => format!("workspace:{}", manifest.display()),
        }
    }

    /// Short scope label for the pinned top bar.
    pub fn chrome_title(&self) -> String {
        match self {
            Self::User => "no project".into(),
            Self::Project { manifest, .. } => format!(
                "{} · project",
                manifest
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("project")
            ),
            Self::Workspace { manifest, .. } => format!(
                "{} · workspace",
                manifest
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("workspace")
            ),
        }
    }

    /// Shared empty-state copy when compiler panels need an open scope.
    pub fn no_project_lines(palette_hint: &str) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "No project or workspace is open.",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from("Open a `.bws` or `.bproj` manifest, or run from a directory that contains one."),
            Line::from(""),
            Line::from(vec![
                Span::styled(palette_hint.to_string(), Style::default().fg(Color::Cyan)),
                Span::raw("  Open workspace / Open project"),
            ]),
        ]
    }

    pub fn root_dir(&self) -> Option<&Path> {
        match self {
            Self::User => None,
            Self::Project { root, .. } | Self::Workspace { root, .. } => Some(root.as_path()),
        }
    }

    pub fn manifest_path(&self) -> Option<&Path> {
        match self {
            Self::User => None,
            Self::Project { manifest, .. } | Self::Workspace { manifest, .. } => {
                Some(manifest.as_path())
            }
        }
    }

    /// Append `--project <manifest>` when a workspace or project scope is open.
    pub fn append_project_argv(&self, argv: &mut Vec<String>) {
        if let Some(manifest) = self.manifest_path() {
            argv.push("--project".into());
            argv.push(manifest.display().to_string());
        }
    }

    pub fn board_config_path(&self) -> PathBuf {
        match self {
            Self::User => user_board_path(),
            Self::Project { root, .. } | Self::Workspace { root, .. } => {
                root.join(".beskid").join("board.bsol")
            }
        }
    }

    pub fn pages_config_path(&self) -> PathBuf {
        match self {
            Self::User => user_pages_path(),
            Self::Project { root, .. } | Self::Workspace { root, .. } => {
                root.join(".beskid").join("pages.bsol")
            }
        }
    }
}

pub fn user_board_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".beskid")
        .join("data")
        .join("boards")
        .join("default.board.bsol")
}

pub fn user_pages_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".beskid")
        .join("data")
        .join("pages")
        .join("default.pages.bsol")
}

pub fn user_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".beskid")
        .join("data")
}
