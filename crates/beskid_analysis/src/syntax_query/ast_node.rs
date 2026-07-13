use std::any::Any;

use crate::syntax::{SpanInfo, Spanned};
use crate::syntax_query::{DynNodeRef, NodeKind};

pub trait AstNode: Any {
    fn as_any(&self) -> &dyn Any;
    fn children<'a>(&'a self, _push: &mut dyn FnMut(DynNodeRef<'a>)) {}
    fn node_kind(&self) -> NodeKind;
    fn span(&self) -> Option<SpanInfo> {
        None
    }
}

impl<T: AstNode + 'static> AstNode for Spanned<T> {
    fn as_any(&self) -> &dyn Any {
        self.node.as_any()
    }

    fn children<'a>(&'a self, push: &mut dyn FnMut(DynNodeRef<'a>)) {
        self.node.children(push);
    }

    fn node_kind(&self) -> NodeKind {
        self.node.node_kind()
    }

    fn span(&self) -> Option<SpanInfo> {
        Some(self.span)
    }
}

pub type NodeRef<'a> = DynNodeRef<'a>;
