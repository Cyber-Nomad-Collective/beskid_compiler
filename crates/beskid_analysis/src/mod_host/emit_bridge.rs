//! Minimal typed materialization for mod generator contributions and SDK bridge shims.

use anyhow::{Context, Result};

use crate::services::parse_program_with_source_name;
use crate::syntax::{ContractDefinition, FunctionDefinition, Node, Spanned, TypeDefinition};
use crate::syntax_query::AstNode;

use super::types::ProgramItem;

pub fn materialize_program_item(source: &str) -> Result<Spanned<ProgramItem>> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        anyhow::bail!("expected non-empty program item source");
    }
    let program = parse_program_with_source_name("__emit_bridge__.bd", trimmed)
        .with_context(|| format!("failed to parse generated program item: {trimmed}"))?;
    program
        .node
        .items
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("expected at least one top-level item in: {trimmed}"))
}

pub fn materialize_program_items(
    sources: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<Vec<Spanned<ProgramItem>>> {
    sources
        .into_iter()
        .map(|source| materialize_program_item(source.as_ref()))
        .collect()
}

pub fn materialize_function_definition(source: &str) -> Result<Spanned<FunctionDefinition>> {
    match materialize_program_item(source)?.node {
        Node::Function(definition) => Ok(definition),
        other => anyhow::bail!("expected function definition, got {:?}", other.node_kind()),
    }
}

pub fn materialize_type_definition(source: &str) -> Result<Spanned<TypeDefinition>> {
    match materialize_program_item(source)?.node {
        Node::TypeDefinition(definition) => Ok(definition),
        other => anyhow::bail!("expected type definition, got {:?}", other.node_kind()),
    }
}

pub fn materialize_contract_definition(source: &str) -> Result<Spanned<ContractDefinition>> {
    match materialize_program_item(source)?.node {
        Node::ContractDefinition(definition) => Ok(definition),
        other => anyhow::bail!("expected contract definition, got {:?}", other.node_kind()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materializes_function_definition_from_source() {
        let item = materialize_function_definition("pub fn generated() { return; }")
            .expect("function materialize");
        assert_eq!(item.node.name.node.name, "generated");
    }

    #[test]
    fn materializes_type_definition_from_source() {
        let item =
            materialize_type_definition("type Account { i64 balance }").expect("type materialize");
        assert_eq!(item.node.name.node.name, "Account");
    }

    #[test]
    fn materializes_contract_definition_from_source() {
        let item = materialize_contract_definition("contract IStorage { unit Save(); }")
            .expect("contract materialize");
        assert_eq!(item.node.name.node.name, "IStorage");
    }
}
