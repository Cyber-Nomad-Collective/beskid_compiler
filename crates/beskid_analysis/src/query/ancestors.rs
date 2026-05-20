//! Iterator over ancestors from a node toward the snapshot root.

use crate::query::syntax_snapshot::SyntaxSnapshot;
use crate::query::DynNodeRef;

/// Root-to-parent chain toward (but not including) the start node; order is immediate parent first.
pub struct Ancestors<'a> {
    snapshot: &'a SyntaxSnapshot<'a>,
    next_id: Option<u32>,
}

impl<'a> Ancestors<'a> {
    pub fn new(snapshot: &'a SyntaxSnapshot<'a>, start: DynNodeRef<'a>) -> Self {
        let next_id = snapshot.id_of(start).and_then(|id| snapshot.parent_id(id));
        Self { snapshot, next_id }
    }
}

impl<'a> Iterator for Ancestors<'a> {
    type Item = DynNodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next_id?;
        let node = self.snapshot.node_at(id)?;
        self.next_id = self.snapshot.parent_id(id);
        Some(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::syntax_snapshot::SyntaxSnapshot;
    use crate::services::parse_program;

    #[test]
    fn ancestors_walk_up_to_program() {
        let src = "pub fn f() { }";
        let program = parse_program(src).expect("parse");
        let snap = SyntaxSnapshot::from_program(&program, 1);
        let root = snap.node_at(snap.root_id()).expect("root");
        assert_eq!(root.node_kind(), crate::query::NodeKind::Program);

        let mut func_id = None;
        for id in 0..snap.len() as u32 {
            if snap.kind_of(id) == Some(crate::query::NodeKind::FunctionDefinition) {
                func_id = Some(id);
                break;
            }
        }
        let func_id = func_id.expect("function");
        let start = snap.node_at(func_id).expect("func node");
        let ancestors: Vec<_> = Ancestors::new(&snap, start).collect();
        assert!(!ancestors.is_empty());
        assert_eq!(
            ancestors.last().map(|n| n.node_kind()),
            Some(crate::query::NodeKind::Program)
        );
    }
}
