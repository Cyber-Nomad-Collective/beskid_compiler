use crate::hir::{HirContractNode, HirItem};
use crate::syntax::{SpanInfo, Spanned};

use super::items::ItemKind;

#[derive(Debug, Clone)]
pub struct MemberItemSpec {
    pub name: String,
    pub kind: ItemKind,
    pub span: SpanInfo,
}

pub fn collect_member_items(item: &Spanned<HirItem>, parent_name: &str) -> Vec<MemberItemSpec> {
    let mut out = Vec::new();
    match &item.node {
        HirItem::FunctionDefinition(def) => {
            for parameter in &def.node.parameters {
                out.push(MemberItemSpec {
                    name: format!("{}::{}", parent_name, parameter.node.name.node.name),
                    kind: ItemKind::Parameter,
                    span: parameter.span,
                });
            }
        }
        HirItem::MethodDefinition(def) => {
            for parameter in &def.node.parameters {
                out.push(MemberItemSpec {
                    name: format!("{}::{}", parent_name, parameter.node.name.node.name),
                    kind: ItemKind::Parameter,
                    span: parameter.span,
                });
            }
        }
        HirItem::TypeDefinition(def) => {
            for field in &def.node.fields {
                out.push(MemberItemSpec {
                    name: format!("{}::{}", parent_name, field.node.name.node.name),
                    kind: ItemKind::Field,
                    span: field.span,
                });
            }
        }
        HirItem::EnumDefinition(def) => {
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
        HirItem::ContractDefinition(def) => {
            for node in &def.node.items {
                match &node.node {
                    HirContractNode::MethodSignature(signature) => {
                        let method_name =
                            format!("{}::{}", parent_name, signature.node.name.node.name);
                        out.push(MemberItemSpec {
                            name: method_name.clone(),
                            kind: ItemKind::ContractMethodSignature,
                            span: signature.span,
                        });
                        for parameter in &signature.node.parameters {
                            out.push(MemberItemSpec {
                                name: format!(
                                    "{}::{}",
                                    method_name, parameter.node.name.node.name
                                ),
                                kind: ItemKind::Parameter,
                                span: parameter.span,
                            });
                        }
                    }
                    HirContractNode::Embedding(embedding) => {
                        out.push(MemberItemSpec {
                            name: format!("{}::{}", parent_name, embedding.node.name.node.name),
                            kind: ItemKind::ContractEmbedding,
                            span: embedding.span,
                        });
                    }
                }
            }
        }
        HirItem::TestDefinition(def) => {
            for (index, statement) in def.node.body.node.statements.iter().enumerate() {
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
    use super::*;

    #[test]
    fn statement_member_name_is_stable() {
        let name = format!("{}::statement#{}", "MyTest", 3);
        assert_eq!(name, "MyTest::statement#3");
    }
}
