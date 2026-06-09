use crate::syntax_query::NodeRef;
use crate::syntax_query::traversal_core;

pub struct Descendants<'a> {
    stack: Vec<NodeRef<'a>>,
}

impl<'a> Descendants<'a> {
    pub fn new(start: NodeRef<'a>) -> Self {
        Self { stack: vec![start] }
    }
}

impl<'a> Iterator for Descendants<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        traversal_core::next_descendant(&mut self.stack, |node, children| {
            node.children(|child| children.push(child));
        })
    }
}
