mod archive;
mod dependency_change;
mod filesystem;
mod lockfile;
mod prepare;
mod registry;
mod resolution;
mod shared_corelib;
pub use dependency_change::{
    DependencyChangePlan, DependencyChangeReport, commit_dependency_change, plan_dependency_change,
};
pub use resolution::{RefreshScope, ResolutionPolicy};

pub use lockfile::{
    PROJECT_LOCK_FILE_NAME, PortableLockPath, PortableLockPathBaseKind, ProjectLockDependencyEntry, ProjectLockSource,
    ProjectLockfile, WorkspacePrepareOptions, load_project_lock_dependencies, load_project_lock_dependencies_for_plan,
    load_project_lock_dependencies_from_path,
};
pub use prepare::{
    prepare_project_workspace, prepare_project_workspace_plan_with_options, prepare_project_workspace_with_options,
};
pub use shared_corelib::shared_corelib_package_of;
pub(crate) use shared_corelib::{SharedCorelib, verify_shared_corelib_for_lock};
