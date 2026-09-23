//! Post-typecheck syntax walk that populates call dispatch kinds and numeric cast intents.

mod blocks;
mod calls;
mod casts;
mod expressions;
mod items;
mod resolve;
mod state;

#[allow(unused_imports)]
use state::PrepWalker;

#[cfg(test)]
#[path = "walker_tests.rs"]
mod tests;
