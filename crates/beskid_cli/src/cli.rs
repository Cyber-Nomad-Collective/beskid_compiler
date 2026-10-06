//! Root Clap model and subcommand dispatch for the `beskid` executable.

pub(crate) mod app;
mod dev;
mod docs;

#[cfg(test)]
mod tests;

pub use app::{Cli, Commands, run};
