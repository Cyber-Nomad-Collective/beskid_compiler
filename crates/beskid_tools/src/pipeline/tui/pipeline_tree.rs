//! Structured pipeline tree for ratkit [`TreeView`] rendering.

use crate::shell::primitives::TreeNode;

#[derive(Debug, Clone)]
struct PipelineNode {
    label: String,
    detail: Option<String>,
    children: Vec<PipelineNode>,
}

impl PipelineNode {
    fn display_label(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{} ({detail})", self.label),
            None => self.label.clone(),
        }
    }

    fn to_tree_node(&self) -> TreeNode<String> {
        if self.children.is_empty() {
            TreeNode::new(self.display_label())
        } else {
            TreeNode::with_children(
                self.display_label(),
                self.children
                    .iter()
                    .map(PipelineNode::to_tree_node)
                    .collect(),
            )
        }
    }
}

/// Mutable pipeline phase tree (roots are top-level compiler stages).
#[derive(Debug, Default)]
pub struct PipelineTree {
    roots: Vec<PipelineNode>,
    /// Path from root index through child indices for each open nesting depth.
    stack: Vec<Vec<usize>>,
}

impl PipelineTree {
    pub fn phase_start(&mut self, depth: usize, label: impl Into<String>) {
        let label = label.into();
        let node = PipelineNode {
            label,
            detail: None,
            children: Vec::new(),
        };
        if depth == 0 {
            self.roots.push(node);
            self.stack = vec![vec![self.roots.len() - 1]];
            return;
        }
        self.stack.truncate(depth);
        if let Some(parent_path) = self.stack.get(depth - 1).cloned()
            && let Some(parent) = self.node_at_path_mut(&parent_path)
        {
            parent.children.push(node);
            let mut path = parent_path;
            path.push(parent.children.len() - 1);
            self.stack.push(path);
        }
    }

    pub fn phase_end(
        &mut self,
        depth: usize,
        label: impl Into<String>,
        duration: impl Into<String>,
    ) {
        let duration = duration.into();
        if let Some(path) = self.stack.get(depth).cloned()
            && let Some(node) = self.node_at_path_mut(&path)
        {
            node.label = label.into();
            node.detail = Some(duration);
        }
        if self.stack.len() > depth {
            self.stack.truncate(depth);
        }
    }

    pub fn work_unit(&mut self, depth: usize, done: u64, total: u64, label: impl Into<String>) {
        let label = format!("[{done}/{total}] {}", label.into());
        let node = PipelineNode {
            label,
            detail: None,
            children: Vec::new(),
        };
        let parent_path = if depth == 0 {
            if self.roots.is_empty() {
                self.roots.push(PipelineNode {
                    label: "Work".into(),
                    detail: None,
                    children: vec![node],
                });
                return;
            }
            vec![self.roots.len() - 1]
        } else if depth <= self.stack.len() {
            self.stack[depth - 1].clone()
        } else {
            return;
        };
        if let Some(parent) = self.node_at_path_mut(&parent_path) {
            if let Some(last) = parent.children.last_mut()
                && last.detail.is_none()
                && last.label.starts_with('[')
            {
                last.label = node.label;
                return;
            }
            parent.children.push(node);
        }
    }

    pub fn tree_nodes(&self) -> Vec<TreeNode<String>> {
        self.roots.iter().map(PipelineNode::to_tree_node).collect()
    }

    pub fn open_paths(&self) -> Vec<Vec<usize>> {
        let mut paths = Vec::new();
        for (index, root) in self.roots.iter().enumerate() {
            let path = vec![index];
            paths.push(path.clone());
            collect_open_paths(root, path, &mut paths);
        }
        paths
    }

    fn node_at_path_mut(&mut self, path: &[usize]) -> Option<&mut PipelineNode> {
        let mut node = self.roots.get_mut(*path.first()?)?;
        for &idx in path.iter().skip(1) {
            node = node.children.get_mut(idx)?;
        }
        Some(node)
    }
}

fn collect_open_paths(node: &PipelineNode, prefix: Vec<usize>, paths: &mut Vec<Vec<usize>>) {
    for (index, child) in node.children.iter().enumerate() {
        let mut path = prefix.clone();
        path.push(index);
        paths.push(path.clone());
        collect_open_paths(child, path, paths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_phases_build_tree() {
        let mut tree = PipelineTree::default();
        tree.phase_start(0, "Semantic analysis");
        tree.phase_start(1, "Type check");
        tree.phase_end(1, "Type check", "12ms");
        tree.phase_end(0, "Semantic analysis", "45ms");
        let nodes = tree.tree_nodes();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].children.len(), 1);
    }
}
