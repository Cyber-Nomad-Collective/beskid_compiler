//! Pipeline phase tree via `tui-tree-widget`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders};
use tui_tree_widget::{Tree, TreeItem, TreeState};

pub fn draw_pipeline_tree(
    frame: &mut Frame,
    area: Rect,
    items: &[TreeItem<'_, String>],
    tree_state: &mut TreeState<String>,
) {
    if let Ok(tree) = Tree::new(items).map(|tree| {
        tree.block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Build "),
        )
        .highlight_style(Style::default().fg(Color::Cyan))
    }) {
        frame.render_stateful_widget(tree, area, tree_state);
    }
}
