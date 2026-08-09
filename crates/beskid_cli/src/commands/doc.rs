//! `beskid doc` — emit `api.json` and `index.md` API documentation for resolved sources.

mod execution;
mod links;
mod model;
mod snapshot;
mod structure_tree;

pub use execution::execute;
pub use model::DocArgs;

#[cfg(test)]
use model::DocEntry;
#[cfg(test)]
use structure_tree::render_structure_tree;

#[cfg(test)]
mod member_doc_tests;
#[cfg(test)]
mod tests;
