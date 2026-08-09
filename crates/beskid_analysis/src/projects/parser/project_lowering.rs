use bsol::{ValidatedBlock, ValidatedDocument};

use super::super::error::ProjectError;
use super::{
    fields_errors::{parse_at, reject_corelib_opt_out_keys, split_known_fields},
    intermediate::{PROJECT_ROOT_FIELDS, ParsedBlocks, ParsedProjectBlock},
    sections_builders::{
        lower_flat_block, lower_grammar_block, lower_link_block, lower_mod_block, lower_mod_generated_outputs,
        lower_schemas_block, lower_template_block,
    },
};

pub(super) fn lower_project_document(validated: ValidatedDocument) -> Result<ParsedBlocks, ProjectError> {
    let mut parsed = ParsedBlocks::default();
    for block in validated.blocks {
        match block.rule_id.as_str() {
            "root" => {
                if block.kind == "project" {
                    return Err(ProjectError::meta_contract(
                        "E1894",
                        "legacy `project { ... }` block is not supported; use a named root block matching `name` (for example `myapp { name = \"myapp\" ... }`)",
                    ));
                }
                if parsed.project.is_some() {
                    return Err(parse_at(block.span, "manifest must contain exactly one named project root block"));
                }
                parsed.project = Some(lower_project_root_block(block)?);
            }
            "target" => parsed.targets.push(lower_flat_block(block)),
            "dependency" => parsed.dependencies.push(lower_flat_block(block)),
            "link" => {
                if parsed.link.is_some() {
                    return Err(ProjectError::meta_contract("E1890", "duplicate `link` block at top level"));
                }
                parsed.link = Some(lower_link_block(block)?);
            }
            other => {
                return Err(parse_at(block.span, format!("unexpected `{other}` block in project manifest")));
            }
        }
    }
    Ok(parsed)
}

pub(super) fn lower_project_root_block(block: ValidatedBlock) -> Result<ParsedProjectBlock, ProjectError> {
    reject_corelib_opt_out_keys(&block.fields, &block.extras, block.span)?;
    let (fields, extras) = split_known_fields(block.fields, PROJECT_ROOT_FIELDS);
    let mod_block = block.nested.iter().find(|n| n.rule_id == "mod");
    let mod_section = mod_block.map(lower_mod_block).transpose()?;
    let mod_generated_outputs = mod_block.map(lower_mod_generated_outputs).transpose()?.unwrap_or_default();
    let grammar_section = block.nested.iter().find(|n| n.rule_id == "grammar").map(lower_grammar_block).transpose()?;
    let template_section =
        block.nested.iter().find(|n| n.rule_id == "template").map(lower_template_block).transpose()?;
    let schemas_section = block.nested.iter().find(|n| n.rule_id == "schemas").map(lower_schemas_block).transpose()?;
    Ok(ParsedProjectBlock {
        block_kind: block.kind,
        fields,
        extras,
        mod_section,
        mod_generated_outputs,
        grammar_section,
        template_section,
        schemas_section,
    })
}
