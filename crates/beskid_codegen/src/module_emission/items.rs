use beskid_isle::{AstNodeKey, DirectCallee};
use beskid_queries::{GenericSpecializationInstance, SourceUnitId, item_name};

use crate::CodegenInput;

/// One syntax item declared and defined through the HIR-free ISLE boundary.
#[derive(Debug, Clone)]
pub struct SyntaxModuleItem {
    pub key: AstNodeKey,
    pub symbol: String,
}

/// One fully declared syntax item after generic source declarations have been expanded into
/// exact ABI specializations. The callee key is the same structural identity produced by ISLE
/// call facts, keeping declaration and import selection generation-safe.
#[derive(Debug, Clone)]
pub(super) struct ResolvedSyntaxModuleItem {
    pub(super) key: AstNodeKey,
    pub(super) symbol: String,
    pub(super) callee: DirectCallee,
    pub(super) specialization: Option<GenericSpecializationInstance>,
}

pub(super) fn syntax_item_symbol(input: &CodegenInput<'_>, key: AstNodeKey) -> Option<String> {
    let name = item_name(input.database(), key).ok().flatten()?;
    let unit = input
        .typed_program()
        .assembly
        .units()
        .iter()
        .find(|unit| SourceUnitId::new(input.database(), unit.path.clone()) == key.unit)?;
    let logical = unit
        .logical_name
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
        .collect::<String>();
    Some(format!("{name}#syntax_{logical}_{}", key.node.0))
}
