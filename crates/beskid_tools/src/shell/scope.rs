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

    pub fn label(&self) -> String {
        match self {
            Self::User => "user".into(),
            Self::Project { manifest, .. } => format!("project:{}", manifest.display()),
            Self::Workspace { manifest, .. } => format!("workspace:{}", manifest.display()),
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
}

pub fn user_board_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".beskid")
        .join("data")
        .join("boards")
        .join("default.board.bsol")
}

pub fn user_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".beskid")
        .join("data")
}
