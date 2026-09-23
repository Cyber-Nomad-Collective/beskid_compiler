//! Synthetic [`MemberItemSpec`] rows for parameters, fields, and nested contract members (resolution item list).

use crate::syntax::{ContractNode, Node};
use crate::syntax::{SpanInfo, Spanned};

use super::items::ItemKind;

/// Child symbol surfaced alongside its parent for hover / API docs (`parent::child` name).
#[derive(Debug, Clone)]
pub struct MemberItemSpec {
    pub name: String,
    pub kind: ItemKind,
    pub span: SpanInfo,
}

/// Walk one syntax item and append specs for visible members (used when extending [`ItemInfo`] lists).
pub fn collect_member_items(item: &Spanned<Node>, parent_name: &str) -> Vec<MemberItemSpec> {
    let mut out = Vec::new();
    match &item.node {
        Node::Function(def) => {
            for parameter in &def.node.parameters {
                out.push(MemberItemSpec {
                    name: format!("{}::{}", parent_name, parameter.node.name.node.name),
                    kind: ItemKind::Parameter,
                    span: parameter.span,
                });
            }
        }
        Node::Method(def) => {
            for parameter in &def.node.parameters {
                out.push(MemberItemSpec {
                    name: format!("{}::{}", parent_name, parameter.node.name.node.name),
                    kind: ItemKind::Parameter,
                    span: parameter.span,
                });
            }
        }
        Node::TypeDefinition(def) => {
            for field in &def.node.fields {
                out.push(MemberItemSpec {
                    name: format!("{}::{}", parent_name, field.node.name.node.name),
                    kind: ItemKind::Field,
                    span: field.span,
                });
            }
            // Methods (and their own parameters) are NOT registered here: `Collector::collect_item`'s
            // dedicated `Node::TypeDefinition` block (resolve/collect.rs) already registers each
            // inline method with a proper receiver (`method_receiver: Some(receiver)`, giving it a
            // real `SymbolShape::Method`) plus its parameters via `collect_member_items_for_method`.
            // Registering them again here produced a second `ItemId` at the identical name+span with
            // no receiver and no symbol -- `item_id_for_span`/`item_id_for_name` then had two
            // candidates to choose between (or, for `item_id_for_span`, none at all, since its
            // ambiguous-match case returns `None`), and `record_signature`'s `canonical_item_id_for_span`
            // never recorded the plain member-item duplicate's signature (no symbol to canonicalize
            // through), silently orphaning it. Corelib inline `type X { method }` shapes were previously
            // untested at this level, so the collision was invisible until the contract-conformance
            // work in this change needed both to agree on one `ItemId`.
        }
        Node::EnumDefinition(def) => {
            for variant in &def.node.variants {
                let variant_name = format!("{}::{}", parent_name, variant.node.name.node.name);
                out.push(MemberItemSpec {
                    name: variant_name.clone(),
                    kind: ItemKind::EnumVariant,
                    span: variant.span,
                });
                for field in &variant.node.fields {
                    out.push(MemberItemSpec {
                        name: format!("{}::{}", variant_name, field.node.name.node.name),
                        kind: ItemKind::Field,
                        span: field.span,
                    });
                }
            }
        }
        Node::ContractDefinition(def) => {
            for node in &def.node.items {
                match &node.node {
                    ContractNode::MethodSignature(signature) => {
                        let method_name = format!("{}::{}", parent_name, signature.node.name.node.name);
                        out.push(MemberItemSpec {
                            name: method_name.clone(),
                            kind: ItemKind::ContractMethodSignature,
                            span: signature.span,
                        });
                        for parameter in &signature.node.parameters {
                            out.push(MemberItemSpec {
                                name: format!("{}::{}", method_name, parameter.node.name.node.name),
                                kind: ItemKind::Parameter,
                                span: parameter.span,
                            });
                        }
                    }
                    ContractNode::Embedding(embedding) => {
                        out.push(MemberItemSpec {
                            name: format!("{}::{}", parent_name, embedding.node.name.node.name),
                            kind: ItemKind::ContractEmbedding,
                            span: embedding.span,
                        });
                    }
                }
            }
        }
        Node::TestDefinition(def) => {
            for (index, statement) in def.node.statements.iter().enumerate() {
                out.push(MemberItemSpec {
                    name: format!("{}::statement#{}", parent_name, index + 1),
                    kind: ItemKind::Statement,
                    span: statement.span,
                });
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {

    #[test]
    fn statement_member_name_is_stable() {
        let name = format!("{}::statement#{}", "MyTest", 3);
        assert_eq!(name, "MyTest::statement#3");
    }
}
