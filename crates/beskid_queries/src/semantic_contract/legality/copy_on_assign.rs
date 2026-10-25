//! E1512: a `[CopyOnAssign]` type must declare the copy hook `This Copy()` -- a borrowed
//! receiver, no parameters, returning the type itself. Without it a copy-position read of the
//! type has no hook to call, which lowering would otherwise meet as an internal error.

use super::typing::nodes_of_kind;
use super::*;
use crate::semantic_contract::transfer::{indexed_site, recognized_attribute_use};
use beskid_analysis::syntax::{MethodDefinition, Type, TypeDefinition};
use beskid_analysis::syntax_query::DynNodeRef;

/// Every `[CopyOnAssign]` type of the unit rooted at `root` whose `Copy` hook is missing or has
/// the wrong shape, reported at the attribute.
pub fn copy_on_assign_findings(db: &dyn Db, root: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(root.unit).filter(|syntax| syntax.accepts_key(db, root)) else {
        return Vec::new();
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let mut findings = Vec::new();
    for node in nodes_of_kind(index, root.node, NodeKind::TypeDefinition) {
        let key = AstNodeKey { node, ..root };
        let Some(definition) = index.node_at(program, node).and_then(|node| node.of::<TypeDefinition>()) else {
            continue;
        };
        let Some(attribute) = recognized_attribute_use(db, key, &definition.attributes, "CopyOnAssign") else {
            continue;
        };
        let Some(problem) = copy_hook_problem(definition) else { continue };
        findings.push(SemanticFinding {
            kind: SemanticIssueKind::CopyOnAssignWithoutCopy {
                type_name: definition.name.node.name.clone(),
                problem: problem.to_string(),
            },
            site: indexed_site(db, key, DynNodeRef::from(attribute)),
            related: Vec::new(),
        });
    }
    findings
}

/// What is wrong with the `Copy` hook of `definition`, or `None` when it is valid.
fn copy_hook_problem(definition: &TypeDefinition) -> Option<&'static str> {
    let methods: Vec<&MethodDefinition> =
        definition.methods.iter().map(|method| &method.node).filter(|method| method.name.node.name == "Copy").collect();
    let [method] = methods.as_slice() else {
        return Some(if methods.is_empty() {
            "it declares no `Copy` method"
        } else {
            "it declares more than one `Copy`"
        });
    };
    if method.receiver.as_ref().is_some_and(|receiver| receiver.node.mutable || receiver.node.movable) {
        return Some("`Copy` must take a borrowed receiver, not `mut this` or `move this`");
    }
    if !method.parameters.is_empty() {
        return Some("`Copy` must take no parameters");
    }
    let returns_own_type = match method.return_type.as_ref().map(|ty| &ty.node) {
        Some(Type::This) => true,
        Some(Type::Complex(path)) => {
            path.node.segments.last().is_some_and(|segment| segment.node.name.node.name == definition.name.node.name)
        }
        _ => false,
    };
    (!returns_own_type).then_some("`Copy` must return the type itself")
}
