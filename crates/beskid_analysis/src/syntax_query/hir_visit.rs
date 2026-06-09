use crate::syntax_query::HirNodeRef;

pub trait HirVisit {
    fn enter(&mut self, _node: HirNodeRef<'_>) {}
    fn exit(&mut self, _node: HirNodeRef<'_>) {}
}
