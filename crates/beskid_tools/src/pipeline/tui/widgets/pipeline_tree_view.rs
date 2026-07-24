//! Pipeline phase tree via ratkit `tree-view`.

use crate::shell::primitives::{TreeNode, TreeViewState};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};
use ratkit::widgets::{TreeView, TreeViewRef};

pub fn draw_pipeline_tree(
    frame: &mut Frame,
    area: Rect,
    nodes: &[TreeNode<String>],
    tree_state: &mut TreeViewState,
    title: &str,
) {
    let tree = TreeViewRef::new(nodes)
        .block(Block::default().borders(Borders::ALL).title(format!(" {title} ")))
        .highlight_style(Style::default().fg(Color::Cyan))
        .render_fn(|label, _| Line::from(label.clone()));
    frame.render_stateful_widget(tree, area, tree_state);
}

pub fn tree_click_at(
    area: Rect,
    mouse: crossterm::event::MouseEvent,
    nodes: &[TreeNode<String>],
    tree_state: &mut TreeViewState,
) {
    let mut tree = TreeView::new(nodes.to_vec())
        .block(Block::default().borders(Borders::ALL))
        .render_fn(|label, _| Line::from(label.clone()));
    let _ = tree.handle_mouse_event(mouse, tree_state, area);
}
