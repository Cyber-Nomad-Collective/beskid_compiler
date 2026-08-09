use bsol::ValidatedDocument;

use super::super::{
    error::ProjectError,
    model::{WorkspaceManifest, WorkspaceMember, WorkspaceOverride, WorkspaceRegistry, WorkspaceSection},
};
use super::{
    fields_errors::{parse_at, required_field, split_known_fields},
    intermediate::{MEMBER_FIELDS, ParsedWorkspaceBlocks, WORKSPACE_ROOT_FIELDS},
    sections_builders::lower_flat_block,
};

pub(super) fn lower_workspace_document(validated: ValidatedDocument) -> Result<ParsedWorkspaceBlocks, ProjectError> {
    let mut parsed = ParsedWorkspaceBlocks::default();
    for block in validated.blocks {
        match block.rule_id.as_str() {
            "workspace" => parsed.workspace = Some(lower_flat_block(block)),
            "member" => parsed.members.push(lower_flat_block(block)),
            "override" => parsed.overrides.push(lower_flat_block(block)),
            "registry" => parsed.registries.push(lower_flat_block(block)),
            other => {
                return Err(parse_at(block.span, format!("unexpected `{other}` block in workspace manifest")));
            }
        }
    }
    Ok(parsed)
}

pub(super) fn build_workspace_manifest(parsed: ParsedWorkspaceBlocks) -> Result<WorkspaceManifest, ProjectError> {
    let workspace =
        parsed.workspace.ok_or_else(|| ProjectError::Validation("missing required `workspace` block".to_string()))?;

    let (workspace_fields, workspace_extras) = split_known_fields(workspace.fields, WORKSPACE_ROOT_FIELDS);
    let workspace_section = WorkspaceSection {
        name: required_field(&workspace_fields, "name")?,
        resolver: workspace_fields.get("resolver").cloned().unwrap_or_else(|| "v1".to_string()),
        extras: workspace_extras,
    };

    let mut members = Vec::with_capacity(parsed.members.len());
    for member in parsed.members {
        let (member_fields, member_extras) = split_known_fields(member.fields, MEMBER_FIELDS);
        members.push(WorkspaceMember {
            name: member
                .label
                .ok_or_else(|| ProjectError::Validation("member block must include a label".to_string()))?,
            path: required_field(&member_fields, "path")?,
            extras: member_extras,
        });
    }

    let mut overrides = Vec::with_capacity(parsed.overrides.len());
    for dependency_override in parsed.overrides {
        overrides.push(WorkspaceOverride {
            dependency: dependency_override
                .label
                .ok_or_else(|| ProjectError::Validation("override block must include a label".to_string()))?,
            version: required_field(&dependency_override.fields, "version")?,
        });
    }

    let mut registries = Vec::with_capacity(parsed.registries.len());
    for registry in parsed.registries {
        registries.push(WorkspaceRegistry {
            name: registry
                .label
                .ok_or_else(|| ProjectError::Validation("registry block must include a label".to_string()))?,
            url: required_field(&registry.fields, "url")?,
        });
    }

    Ok(WorkspaceManifest { workspace: workspace_section, members, overrides, registries })
}
