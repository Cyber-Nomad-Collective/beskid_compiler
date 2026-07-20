//! Panes-backed hi shell layout (board.v2).

mod editor;
mod editor_overlay;
mod emit;
pub mod load;
mod lower;
mod model;
pub mod overlays;
pub mod pages;
mod parse;
pub mod resolve;
mod templates;

pub use editor::{HiLayoutState, LayoutEditCommand, LayoutEditorState, LayoutOverlayTab};
pub use editor_overlay::{LayoutEditorOverlay, LayoutOverlayAction};
pub use model::{BoardNode, BoardV2Doc, NodeKind};
pub use pages::{EMBEDDED_HI_PAGES, PagesDoc, switch_page};
pub use parse::{EMBEDDED_HI_V2, parse_v2};
pub use resolve::ResolvedPanels;
pub use templates::{LAYOUT_TEMPLATES, LayoutTemplate, template_by_id};

pub fn load_for_scope(scope: &crate::shell::scope::ShellScope) -> Result<HiLayoutState, String> {
    let (doc, runtime) = load::load_for_scope(scope)?;
    let pages = pages::load_for_scope(scope)?;
    Ok(HiLayoutState::new(doc, runtime, pages))
}
