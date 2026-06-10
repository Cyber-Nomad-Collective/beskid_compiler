//! Shell scope: user, project, or workspace context.

use std::path::{Path, PathBuf};

use beskid_analysis::projects::discovery::{discover_project_file, discover_workspace_file};

/// Where the shell is rooted for contextual commands and board config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellScope {
    User,
    Project { root: PathBuf, manifest: PathBuf },
    Workspace { root: PathBuf, manifest: PathBuf },
}

impl ShellScope {
    pub fn resolve(start: &Path) -> Self {
        if let Some(manifest) = discover_workspace_file(start) {
            let root = manifest
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| start.to_path_buf());
            return Self::Workspace { root, manifest };
        }
        if let Some(manifest) = discover_project_file(start) {
            let root = manifest
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| start.to_path_buf());
            return Self::Project { root, manifest };
        }
        Self::User
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
            Self::User => "user".into(),
            Self::Project { manifest, .. } | Self::Workspace { manifest, .. } => manifest
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("project")
                .to_string(),
        }
    }

    pub fn root_dir(&self) -> Option<&Path> {
        match self {
            Self::User => None,
            Self::Project { root, .. } | Self::Workspace { root, .. } => Some(root.as_path()),
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
