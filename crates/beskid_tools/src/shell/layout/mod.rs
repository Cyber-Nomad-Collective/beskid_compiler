//! Panes-backed hi shell layout (board.v2).

mod editor;
mod emit;
mod load;
mod lower;
mod model;
mod parse;
pub mod resolve;

pub use editor::{HiLayoutState, LayoutEditCommand, LayoutEditorState};
pub use model::{BoardNode, BoardV2Doc, NodeKind};
pub use parse::EMBEDDED_HI_V2;
pub use resolve::ResolvedPanels;

pub fn load_for_scope(
    scope: &crate::shell::scope::ShellScope,
) -> Result<HiLayoutState, String> {
    let (doc, runtime) = load::load_for_scope(scope)?;
    Ok(HiLayoutState::new(doc, runtime))
}
