use crate::syntax::{Program, Spanned};
use crate::syntax_query::NodeRef;

pub fn render_program_tree(program: &Spanned<Program>) -> String {
    let mut out = String::new();
    render_tree_node(NodeRef::from(&program.node), 0, &mut out);
    out
}

fn render_tree_node(node: NodeRef, indent: usize, out: &mut String) {
    let prefix = "  ".repeat(indent);
    let kind = node.node_kind();

    let extra = if let Some(ident) = node.of::<crate::syntax::Identifier>() {
        format!(" ({})", ident.name)
    } else if let Some(lit) = node.of::<crate::syntax::Literal>() {
        format!(" ({lit:?})")
    } else {
        String::new()
    };

    out.push_str(&format!("{}{:?}{}\n", prefix, kind, extra));
    node.children(|child| {
        render_tree_node(child, indent + 1, out);
    });
}
