#[cfg(test)]
mod std_env_lock;

#[cfg(test)]
pub(crate) use std_env_lock::std_dependency_env_lock;

#[cfg(test)]
mod compile_plan;
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
#[cfg(test)]
mod resolution;
#[cfg(test)]
mod workspace_manifest;
