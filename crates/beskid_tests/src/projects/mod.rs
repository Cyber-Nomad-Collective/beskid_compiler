//! Project and workspace manifest tests: compile plans, corelib layout, resolution, lockfiles.

#[cfg(test)]
mod test_cwd;

#[cfg(test)]
mod std_env_lock;

#[cfg(test)]
pub(crate) use std_env_lock::std_dependency_env_lock;

#[cfg(test)]
mod assembly;
#[cfg(test)]
mod compile_plan;
#[cfg(test)]
mod composition;
#[cfg(test)]
mod corelib;
#[cfg(test)]
mod discovery;
#[cfg(test)]
mod graph;
#[cfg(test)]
mod lockfile;
#[cfg(test)]
mod manifest;
// Disabled until manifest `readme` fields land in project/workspace sections.
// #[cfg(test)]
// mod readme;
#[cfg(test)]
mod mod_manifest;
#[cfg(test)]
mod templates;
#[cfg(test)]
mod resolution;
#[cfg(test)]
mod workspace_manifest;
