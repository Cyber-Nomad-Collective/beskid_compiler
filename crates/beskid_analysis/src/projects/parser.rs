use std::collections::HashMap;

use crate::projects::error::ProjectError;
use crate::projects::model::{
    Dependency, DependencySource, ProjectManifest, ProjectSection, Target, TargetKind,
    WorkspaceManifest, WorkspaceMember, WorkspaceOverride, WorkspaceRegistry, WorkspaceSection,
};
use crate::projects::validator::{validate_manifest, validate_workspace_manifest};

#[derive(Debug)]
struct ParsedBlock {
    kind: String,
    label: Option<String>,
    fields: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy)]
struct LineCtx<'a> {
    /// 1-based line index in the source file.
    line_1: usize,
    /// UTF-8 byte offset of `text` within the full source.
    line_start_byte: usize,
    text: &'a str,
}

struct PhysicalLines<'a> {
    iter: std::str::SplitInclusive<'a, char>,
    line_1: usize,
    byte_offset: usize,
}

impl<'a> PhysicalLines<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            iter: source.split_inclusive('\n'),
            line_1: 0,
            byte_offset: 0,
        }
    }

    fn next_line(&mut self) -> Option<LineCtx<'a>> {
        let chunk = self.iter.next()?;
        self.line_1 += 1;
        let start = self.byte_offset;
        let text = chunk.strip_suffix('\n').unwrap_or(chunk);
        self.byte_offset += chunk.len();
        Some(LineCtx {
            line_1: self.line_1,
            line_start_byte: start,
            text,
        })
    }
}

fn parse_err(
    ctx: &LineCtx<'_>,
    message: impl Into<String>,
    value_range: Option<(usize, usize)>,
) -> ProjectError {
    ProjectError::ParseAt {
        line: ctx.line_1,
        message: message.into(),
        start: value_range.map(|(s, _)| s),
        end: value_range.map(|(_, e)| e),
    }
}

fn trim_start_byte(s: &str) -> usize {
    s.as_bytes()
        .iter()
        .take_while(|b| b.is_ascii_whitespace())
        .count()
}

/// Byte span in the full source for the assignment value token on this line.
fn value_span_in_source(ctx: &LineCtx<'_>, value_trimmed: &str) -> Option<(usize, usize)> {
    let raw = ctx.text;
    let no_comment = strip_comment(raw);
    let t0 = trim_start_byte(no_comment);
    let eff = no_comment.get(t0..)?;
    let (left, right) = eff.split_once('=')?;
    let rhs = right.trim_start().trim_end();
    if rhs != value_trimmed {
        return None;
    }
    let after_eq = right.trim_start();
    let lead_after_eq = right.len() - after_eq.len();
    let idx_after_eq = left.len() + 1;
    let rhs_start_in_eff = idx_after_eq + lead_after_eq;
    let start = ctx.line_start_byte + t0 + rhs_start_in_eff;
    let end = start + value_trimmed.len();
    Some((start, end))
}

fn parse_block_header(line_ctx: &LineCtx<'_>) -> Result<(String, Option<String>), ProjectError> {
    let trimmed = strip_comment(line_ctx.text).trim();
    let (kind, label) =
        parse_block_header_text(trimmed).map_err(|message| parse_err(line_ctx, message, None))?;
    Ok((kind.to_string(), label))
}

fn parse_block_header_text(line: &str) -> Result<(&str, Option<String>), String> {
    if !line.ends_with('{') {
        return Err("expected block opening `{`".to_string());
    }

    let without_brace = line.trim_end_matches('{').trim();
    if without_brace.is_empty() {
        return Err("empty block header".to_string());
    }

    let mut parts = without_brace.split_whitespace();
    let kind = parts
        .next()
        .ok_or_else(|| "missing block kind".to_string())?;
    let rest = without_brace[kind.len()..].trim();

    if rest.is_empty() {
        return Ok((kind, None));
    }

    let label = parse_quoted(rest)?;
    Ok((kind, Some(label)))
}

fn allows_enum_literal(field: &str) -> bool {
    matches!(field, "kind" | "source" | "resolver")
}

fn parse_ident_token(raw: &str) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("expected identifier".to_string());
    }
    let mut chars = t.chars();
    let Some(first) = chars.next() else {
        return Err("expected identifier".to_string());
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(format!("invalid identifier start in `{t}`"));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("invalid identifier `{t}`"));
    }
    Ok(t.to_string())
}

fn parse_quoted_string_token(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if !(trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2) {
        return Err(format!(
            "expected quoted string (or unquoted enum for this field), found `{trimmed}`"
        ));
    }
    Ok(trimmed[1..trimmed.len() - 1].to_string())
}

fn parse_field_value(
    field: &str,
    raw_rhs: &str,
    ctx: &LineCtx<'_>,
) -> Result<String, ProjectError> {
    let trimmed = raw_rhs.trim();
    let span = value_span_in_source(ctx, trimmed);
    if allows_enum_literal(field) {
        let out = if trimmed.starts_with('"') {
            parse_quoted_string_token(trimmed)
        } else {
            parse_ident_token(trimmed)
        };
        return out.map_err(|message| {
            parse_err(
                ctx,
                message,
                span.or_else(|| value_span_in_source(ctx, trimmed)),
            )
        });
    }

    parse_quoted_string_token(trimmed).map_err(|message| {
        parse_err(
            ctx,
            message,
            span.or_else(|| value_span_in_source(ctx, trimmed)),
        )
    })
}

fn parse_assignment_line(ctx: &LineCtx<'_>) -> Result<(String, String), ProjectError> {
    let line = strip_comment(ctx.text).trim();
    if line.is_empty() {
        return Err(parse_err(ctx, "empty assignment line", None));
    }
    let (left, right) = line
        .split_once('=')
        .ok_or_else(|| parse_err(ctx, "expected key = value assignment", None))?;
    let key = left.trim();
    if key.is_empty() {
        return Err(parse_err(ctx, "assignment key cannot be empty", None));
    }
    let value = parse_field_value(key, right, ctx)?;
    Ok((key.to_string(), value))
}

fn parse_workspace_blocks(source: &str) -> Result<ParsedWorkspaceBlocks, ProjectError> {
    let mut lines = PhysicalLines::new(source);
    let mut parsed = ParsedWorkspaceBlocks::default();

    while let Some(line_ctx) = lines.next_line() {
        let trimmed = strip_comment(line_ctx.text).trim();
        if trimmed.is_empty() {
            continue;
        }

        let (kind, label) = parse_block_header(&line_ctx)?;

        let mut fields = HashMap::new();
        let mut closed = false;
        while let Some(body_ctx) = lines.next_line() {
            let body = strip_comment(body_ctx.text).trim();
            if body.is_empty() {
                continue;
            }
            if body == "}" {
                closed = true;
                break;
            }

            let (key, value) = parse_assignment_line(&body_ctx)?;
            fields.insert(key, value);
        }

        if !closed {
            return Err(ProjectError::ParseAt {
                line: line_ctx.line_1,
                message: format!("missing closing `}}` for `{kind}` block"),
                start: None,
                end: None,
            });
        }

        let block = ParsedBlock {
            kind,
            label,
            fields,
        };

        match block.kind.as_str() {
            "workspace" => parsed.workspace = Some(block),
            "member" => parsed.members.push(block),
            "override" => parsed.overrides.push(block),
            "registry" => parsed.registries.push(block),
            other => {
                return Err(ProjectError::ParseAt {
                    line: line_ctx.line_1,
                    message: format!("unknown block kind `{other}`"),
                    start: None,
                    end: None,
                });
            }
        }
    }

    Ok(parsed)
}

#[derive(Debug, Default)]
struct ParsedBlocks {
    project: Option<ParsedBlock>,
    targets: Vec<ParsedBlock>,
    dependencies: Vec<ParsedBlock>,
}

#[derive(Debug, Default)]
struct ParsedWorkspaceBlocks {
    workspace: Option<ParsedBlock>,
    members: Vec<ParsedBlock>,
    overrides: Vec<ParsedBlock>,
    registries: Vec<ParsedBlock>,
}

pub fn parse_manifest(source: &str) -> Result<ProjectManifest, ProjectError> {
    let parsed = parse_blocks(source)?;
    let manifest = build_manifest(parsed)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn parse_workspace_manifest(source: &str) -> Result<WorkspaceManifest, ProjectError> {
    let parsed = parse_workspace_blocks(source)?;
    let manifest = build_workspace_manifest(parsed)?;
    validate_workspace_manifest(&manifest)?;
    Ok(manifest)
}

fn parse_blocks(source: &str) -> Result<ParsedBlocks, ProjectError> {
    let mut lines = PhysicalLines::new(source);
    let mut parsed = ParsedBlocks::default();

    while let Some(line_ctx) = lines.next_line() {
        let trimmed = strip_comment(line_ctx.text).trim();
        if trimmed.is_empty() {
            continue;
        }

        let (kind, label) = parse_block_header(&line_ctx)?;

        let mut fields = HashMap::new();
        let mut closed = false;
        while let Some(body_ctx) = lines.next_line() {
            let body = strip_comment(body_ctx.text).trim();
            if body.is_empty() {
                continue;
            }
            if body == "}" {
                closed = true;
                break;
            }

            let (key, value) = parse_assignment_line(&body_ctx)?;
            fields.insert(key, value);
        }

        if !closed {
            return Err(ProjectError::ParseAt {
                line: line_ctx.line_1,
                message: format!("missing closing `}}` for `{kind}` block"),
                start: None,
                end: None,
            });
        }

        let block = ParsedBlock {
            kind,
            label,
            fields,
        };

        match block.kind.as_str() {
            "project" => parsed.project = Some(block),
            "target" => parsed.targets.push(block),
            "dependency" => parsed.dependencies.push(block),
            other => {
                return Err(ProjectError::ParseAt {
                    line: line_ctx.line_1,
                    message: format!("unknown block kind `{other}`"),
                    start: None,
                    end: None,
                });
            }
        }
    }

    Ok(parsed)
}

fn build_manifest(parsed: ParsedBlocks) -> Result<ProjectManifest, ProjectError> {
    let project = parsed
        .project
        .ok_or_else(|| ProjectError::Validation("missing required `project` block".to_string()))?;

    let project_section = ProjectSection {
        name: required_field(&project.fields, "name")?,
        version: required_field(&project.fields, "version")?,
        root: project
            .fields
            .get("root")
            .cloned()
            .unwrap_or_else(|| "Src".to_string()),
        root_namespace: project.fields.get("root_namespace").cloned(),
    };

    let mut targets = Vec::with_capacity(parsed.targets.len());
    for target in parsed.targets {
        let kind = match required_field(&target.fields, "kind")?.as_str() {
            "App" => TargetKind::App,
            "Lib" => TargetKind::Lib,
            "Test" => TargetKind::Test,
            other => {
                return Err(ProjectError::Validation(format!(
                    "target `{}` has unsupported kind `{other}` (expected App, Lib, or Test, e.g. `kind = Lib`)",
                    target.label.as_deref().unwrap_or("<unnamed>")
                )));
            }
        };

        targets.push(Target {
            name: target.label.ok_or_else(|| {
                ProjectError::Validation("target block must include a label".to_string())
            })?,
            kind,
            entry: required_field(&target.fields, "entry")?,
        });
    }

    let mut dependencies = Vec::with_capacity(parsed.dependencies.len());
    for dependency in parsed.dependencies {
        let source = match required_field(&dependency.fields, "source")?.as_str() {
            "path" => DependencySource::Path,
            "git" => DependencySource::Git,
            "registry" => DependencySource::Registry,
            other => {
                return Err(ProjectError::Validation(format!(
                    "dependency `{}` has unsupported source `{other}` (expected path, git, or registry, e.g. `source = path`)",
                    dependency.label.as_deref().unwrap_or("<unnamed>")
                )));
            }
        };

        dependencies.push(Dependency {
            name: dependency.label.ok_or_else(|| {
                ProjectError::Validation("dependency block must include a label".to_string())
            })?,
            source,
            path: dependency.fields.get("path").cloned(),
            url: dependency.fields.get("url").cloned(),
            rev: dependency.fields.get("rev").cloned(),
            version: dependency.fields.get("version").cloned(),
            registry: dependency.fields.get("registry").cloned(),
        });
    }

    Ok(ProjectManifest {
        project: project_section,
        targets,
        dependencies,
    })
}

fn build_workspace_manifest(
    parsed: ParsedWorkspaceBlocks,
) -> Result<WorkspaceManifest, ProjectError> {
    let workspace = parsed.workspace.ok_or_else(|| {
        ProjectError::Validation("missing required `workspace` block".to_string())
    })?;

    let workspace_section = WorkspaceSection {
        name: required_field(&workspace.fields, "name")?,
        resolver: workspace
            .fields
            .get("resolver")
            .cloned()
            .unwrap_or_else(|| "v1".to_string()),
    };

    let mut members = Vec::with_capacity(parsed.members.len());
    for member in parsed.members {
        members.push(WorkspaceMember {
            name: member.label.ok_or_else(|| {
                ProjectError::Validation("member block must include a label".to_string())
            })?,
            path: required_field(&member.fields, "path")?,
        });
    }

    let mut overrides = Vec::with_capacity(parsed.overrides.len());
    for dependency_override in parsed.overrides {
        overrides.push(WorkspaceOverride {
            dependency: dependency_override.label.ok_or_else(|| {
                ProjectError::Validation("override block must include a label".to_string())
            })?,
            version: required_field(&dependency_override.fields, "version")?,
        });
    }

    let mut registries = Vec::with_capacity(parsed.registries.len());
    for registry in parsed.registries {
        registries.push(WorkspaceRegistry {
            name: registry.label.ok_or_else(|| {
                ProjectError::Validation("registry block must include a label".to_string())
            })?,
            url: required_field(&registry.fields, "url")?,
        });
    }

    Ok(WorkspaceManifest {
        workspace: workspace_section,
        members,
        overrides,
        registries,
    })
}

fn required_field(fields: &HashMap<String, String>, key: &str) -> Result<String, ProjectError> {
    fields
        .get(key)
        .cloned()
        .ok_or_else(|| ProjectError::Validation(format!("missing required field `{key}`")))
}

fn parse_quoted(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if !(trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2) {
        return Err(format!("expected quoted label, found `{trimmed}`"));
    }

    Ok(trimmed[1..trimmed.len() - 1].to_string())
}

fn strip_comment(input: &str) -> &str {
    let bytes = input.as_bytes();
    let mut in_quotes = false;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                in_quotes = !in_quotes;
                i += 1;
            }
            b'#' if !in_quotes => {
                return &input[..i];
            }
            b'/' if !in_quotes && i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                return &input[..i];
            }
            _ => {
                i += 1;
            }
        }
    }

    input
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::model::{DependencySource, TargetKind};

    fn minimal_project(kind: &str, source_field: &str) -> String {
        format!(
            r#"project {{
  name = "p"
  version = "0.1.0"
}}
target "t" {{
  kind = {kind}
  entry = "Main.bd"
}}
dependency "d" {{
  source = {source_field}
  path = "../x"
}}
"#
        )
    }

    #[test]
    fn parse_kind_lib_unquoted() {
        let src = minimal_project("Lib", "path");
        let m = parse_manifest(&src).expect("parse");
        assert_eq!(m.targets[0].kind, TargetKind::Lib);
        assert_eq!(m.dependencies[0].source, DependencySource::Path);
    }

    #[test]
    fn parse_kind_and_source_quoted_legacy() {
        let src = minimal_project("\"Lib\"", "\"path\"");
        let m = parse_manifest(&src).expect("parse");
        assert_eq!(m.targets[0].kind, TargetKind::Lib);
        assert_eq!(m.dependencies[0].source, DependencySource::Path);
    }

    #[test]
    fn name_must_stay_quoted() {
        let src = r#"project {
  name = MyApp
  version = "0.1.0"
}
target "t" { kind = Lib entry = "e.bd" }
"#;
        let err = parse_manifest(src).expect_err("name unquoted");
        assert!(matches!(err, ProjectError::ParseAt { .. }));
    }

    #[test]
    fn invalid_kind_reports_parse_at() {
        let src = minimal_project("Blob", "path");
        let err = parse_manifest(&src).expect_err("bad kind");
        match err {
            ProjectError::Validation(msg) => assert!(msg.contains("Blob")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn workspace_resolver_unquoted() {
        let src = r#"workspace {
  name = "w"
  resolver = v1
}
member "m" {
  path = "pkg"
}
"#;
        let w = parse_workspace_manifest(src).expect("parse workspace");
        assert_eq!(w.workspace.resolver, "v1");
    }
}
