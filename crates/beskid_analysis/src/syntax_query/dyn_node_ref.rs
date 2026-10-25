use crate::syntax::SpanInfo;
use crate::syntax_query::AstNode;

#[derive(Clone, Copy)]
pub struct DynNodeRef<'a>(pub &'a dyn AstNode);

impl<'a> DynNodeRef<'a> {
    pub fn children(self, mut push: impl FnMut(DynNodeRef<'a>)) {
        self.0.children(&mut push);
    }

    pub fn children_iter(self) -> impl Iterator<Item = DynNodeRef<'a>> {
        let mut items = Vec::new();
        self.children(|node| items.push(node));
        items.into_iter()
    }

    pub fn of<T: AstNode + 'static>(&self) -> Option<&'a T> {
        self.0.as_any().downcast_ref::<T>()
    }

    pub fn node_kind(self) -> crate::syntax_query::NodeKind {
        self.0.node_kind()
    }

    pub fn span(self) -> Option<SpanInfo> {
        self.0.span()
    }

    /// Address identity: whether both references point at the same node in one syntax tree.
    pub fn is_same_node(self, other: DynNodeRef<'_>) -> bool {
        std::ptr::eq(std::ptr::from_ref(self.0).cast::<()>(), std::ptr::from_ref(other.0).cast::<()>())
    }
}

impl<'a, T: AstNode + 'a> From<&'a T> for DynNodeRef<'a> {
    fn from(value: &'a T) -> Self {
        DynNodeRef(value)
    }
}
