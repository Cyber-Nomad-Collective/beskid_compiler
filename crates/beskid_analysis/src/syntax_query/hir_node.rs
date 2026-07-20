use std::any::Any;

use crate::syntax::Spanned;
use crate::syntax_query::{HirNodeKind, HirNodeRef};

pub trait HirNode: Any {
    fn as_any(&self) -> &dyn Any;
    fn children<'a>(&'a self, _push: &mut dyn FnMut(HirNodeRef<'a>)) {}
    fn node_kind(&self) -> HirNodeKind;
}

impl<T: HirNode + 'static> HirNode for Spanned<T> {
    fn as_any(&self) -> &dyn Any {
        self.node.as_any()
    }

    fn children<'a>(&'a self, push: &mut dyn FnMut(HirNodeRef<'a>)) {
        self.node.children(push);
    }

    fn node_kind(&self) -> HirNodeKind {
        self.node.node_kind()
    }
}
