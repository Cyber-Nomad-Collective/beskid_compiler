use std::collections::HashMap;

use crate::projects::error::ProjectError;
use crate::projects::model::{
    Dependency, DependencySource, ProjectKind, ProjectManifest, ProjectModSection, ProjectSection,
    ProjectTemplateSection, Target, TargetKind, WorkspaceManifest, WorkspaceMember,
    WorkspaceOverride, WorkspaceRegistry, WorkspaceSection,
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
    matches!(field, "kind" | "source" | "resolver" | "type")
}

#[derive(Debug, Clone)]
enum ModFieldValue {
    StringList(Vec<String>),
    U32(u32),
    String(String),
}

fn split_comma_list_items(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut in_string = false;
    for (i, c) in inner.char_indices() {
        match c {
            '"' => {
                in_string = !in_string;
            }
            ',' if !in_string => {
                parts.push(inner.get(start..i).unwrap_or(""));
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(inner.get(start..).unwrap_or(""));
    parts
}

fn parse_bracket_string_list(raw: &str) -> Result<Vec<String>, String> {
    let t = raw.trim();
    if !(t.starts_with('[') && t.ends_with(']')) {
        return Err(format!("expected `[...]` list, found `{t}`"));
    }
    let inner = t[1..t.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in split_comma_list_items(inner) {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let token = if p == "default" {
            "default".to_string()
        } else if p.starts_with('"') {
            parse_quoted_string_token(p)?
        } else {
            parse_ident_token(p)?
        };
        out.push(token);
    }
    Ok(out)
}

fn parse_string_or_list_field(raw: &str, field: &str) -> Result<Vec<String>, String> {
    let t = raw.trim();
    if t.starts_with('[') {
        return parse_bracket_string_list(t);
    }
    if t == "default" {
        return Ok(vec!["default".to_string()]);
    }
    if t.starts_with('"') {
        return Ok(vec![parse_quoted_string_token(t)?]);
    }
    Ok(vec![
        parse_ident_token(t).map_err(|e| format!("{field}: {e}"))?,
    ])
}

fn parse_positive_u32(raw: &str, field: &str) -> Result<u32, String> {
    let t = raw.trim();
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "`{field}` must be a positive decimal integer, found `{t}`"
        ));
    }
    t.parse::<u32>()
        .map_err(|_| format!("`{field}` integer overflow or invalid: `{t}`"))
}

fn reject_corelib_opt_out_assignment(ctx: &LineCtx<'_>) -> Result<(), ProjectError> {
    let line = strip_comment(ctx.text).trim();
    let Some((left, right)) = line.split_once('=') else {
        return Ok(());
    };
    let key = left.trim();
    if key == "noCorelib" {
        return Err(ProjectError::meta_contract(
            "E1876",
            "manifest must not declare `noCorelib`; host projects always resolve corelib through toolchain defaults",
        ));
    }
    if key == "useCorelib" {
        let rhs = right.trim();
        let disables = rhs.eq_ignore_ascii_case("false")
            || rhs == "\"false\""
            || rhs == "'false'";
        if disables {
            return Err(ProjectError::meta_contract(
                "E1876",
                "manifest must not set `useCorelib = false`; host projects always resolve corelib through toolchain defaults",
            ));
        }
    }
    Ok(())
}

fn reject_corelib_opt_out_keys(fields: &HashMap<String, String>) -> Result<(), ProjectError> {
    if fields.contains_key("noCorelib") {
        return Err(ProjectError::meta_contract(
            "E1876",
            "manifest must not declare `noCorelib`; host projects always resolve corelib through toolchain defaults",
        ));
    }
    if fields
        .get("useCorelib")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("false"))
    {
        return Err(ProjectError::meta_contract(
            "E1876",
            "manifest must not set `useCorelib = false`; host projects always resolve corelib through toolchain defaults",
        ));
    }
    Ok(())
}

fn parse_template_block_contents(
    lines: &mut PhysicalLines<'_>,
    open_ctx: &LineCtx<'_>,
) -> Result<HashMap<String, String>, ProjectError> {
    let mut fields = HashMap::new();
    let mut closed = false;
    while let Some(ctx) = lines.next_line() {
        let body = strip_comment(ctx.text).trim();
        if body.is_empty() {
            continue;
        }
        if body == "}" {
            closed = true;
            break;
        }
        if body.ends_with('{') {
            return Err(parse_err(
                &ctx,
                "nested blocks are not allowed inside `template`",
                None,
            ));
        }
        let (key, value) = parse_assignment_line(&ctx)?;
        match key.as_str() {
            "shortName" | "identity" => {}
            other => {
                return Err(ProjectError::meta_contract(
                    "E1885",
                    format!("unknown `template` field `{other}`"),
                ));
            }
        }
        if fields.insert(key.clone(), value).is_some() {
            return Err(parse_err(
                &ctx,
                format!("duplicate `template` field `{key}`"),
                None,
            ));
        }
    }
    if !closed {
        return Err(ProjectError::ParseAt {
            line: open_ctx.line_1,
            message: "missing closing `}` for `template` block".to_string(),
            start: None,
            end: None,
        });
    }
    Ok(fields)
}

fn parse_mod_block_contents(
    lines: &mut PhysicalLines<'_>,
    open_ctx: &LineCtx<'_>,
) -> Result<HashMap<String, ModFieldValue>, ProjectError> {
    let mut fields = HashMap::new();
    let mut closed = false;
    while let Some(ctx) = lines.next_line() {
        let body = strip_comment(ctx.text).trim();
        if body.is_empty() {
            continue;
        }
        if body == "}" {
            closed = true;
            break;
        }
        if body.ends_with('{') {
            return Err(parse_err(
                &ctx,
                "nested blocks are not allowed inside `mod`",
                None,
            ));
        }
        let line = strip_comment(ctx.text).trim();
        let (left, right) = line
            .split_once('=')
            .ok_or_else(|| parse_err(&ctx, "expected key = value assignment inside `mod`", None))?;
        let key = left.trim();
        if key.is_empty() {
            return Err(parse_err(
                &ctx,
                "assignment key cannot be empty inside `mod`",
                None,
            ));
        }
        let value = parse_mod_field_value(key, right, &ctx)?;
        if fields.insert(key.to_string(), value).is_some() {
            return Err(parse_err(
                &ctx,
                format!("duplicate `mod` field `{key}`"),
                None,
            ));
        }
    }
    if !closed {
        return Err(ProjectError::ParseAt {
            line: open_ctx.line_1,
            message: "missing closing `}` for `mod` block".to_string(),
            start: None,
            end: None,
        });
    }
    Ok(fields)
}

fn parse_project_block(
    lines: &mut PhysicalLines<'_>,
    header_ctx: &LineCtx<'_>,
) -> Result<ParsedProjectBlock, ProjectError> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut mod_section: Option<HashMap<String, ModFieldValue>> = None;
    let mut template_section: Option<HashMap<String, String>> = None;
    let mut closed = false;
    while let Some(ctx) = lines.next_line() {
        let body = strip_comment(ctx.text).trim();
        if body.is_empty() {
            continue;
        }
        if body == "}" {
            closed = true;
            break;
        }
        if body.ends_with('{') {
            let (nested_kind, nested_label) = parse_block_header(&ctx)?;
            if (nested_kind == "mod" || nested_kind == "meta") && nested_label.is_none() {
                if mod_section.is_some() {
                    return Err(parse_err(
                        &ctx,
                        "duplicate `mod` block inside `project`",
                        None,
                    ));
                }
                mod_section = Some(parse_mod_block_contents(lines, &ctx)?);
                continue;
            }
            if nested_kind == "template" && nested_label.is_none() {
                if template_section.is_some() {
                    return Err(parse_err(
                        &ctx,
                        "duplicate `template` block inside `project`",
                        None,
                    ));
                }
                template_section = Some(parse_template_block_contents(lines, &ctx)?);
                continue;
            }
            return Err(parse_err(
                &ctx,
                format!("unknown nested block `{nested_kind}` inside `project`"),
                None,
            ));
        }
        reject_corelib_opt_out_assignment(&ctx)?;
        let (key, value) = parse_assignment_line(&ctx)?;
        if fields.insert(key.clone(), value).is_some() {
            return Err(parse_err(
                &ctx,
                format!("duplicate `project` field `{key}`"),
                None,
            ));
        }
    }
    if !closed {
        return Err(ProjectError::ParseAt {
            line: header_ctx.line_1,
            message: "missing closing `}` for `project` block".to_string(),
            start: None,
            end: None,
        });
    }
    Ok(ParsedProjectBlock {
        fields,
        mod_section,
        template_section,
    })
}

fn parse_mod_field_value(
    key: &str,
    raw_rhs: &str,
    ctx: &LineCtx<'_>,
) -> Result<ModFieldValue, ProjectError> {
    let trimmed = raw_rhs.trim();
    let span = value_span_in_source(ctx, trimmed);
    let err = |message: String| parse_err(ctx, message, span);
    match key {
        // Legacy meta-project keys are accepted and ignored during transition.
        "attachTo" | "entryModules" | "entryModule" => Ok(ModFieldValue::StringList(Vec::new())),
        "capabilities" => parse_string_or_list_field(trimmed, key)
            .map(ModFieldValue::StringList)
            .map_err(err),
        "maxGeneratorRounds" | "maxMetaRounds" => parse_positive_u32(trimmed, key)
            .map(ModFieldValue::U32)
            .map_err(err),
        "artifactPolicy" => {
            let token = if trimmed.starts_with('"') {
                parse_quoted_string_token(trimmed)
            } else {
                parse_ident_token(trimmed)
            };
            token.map(ModFieldValue::String).map_err(err)
        }
        other => Err(parse_err(
            ctx,
            format!("unknown `mod` field `{other}`"),
            span,
        )),
    }
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

#[derive(Debug)]
struct ParsedProjectBlock {
    fields: HashMap<String, String>,
    mod_section: Option<HashMap<String, ModFieldValue>>,
    template_section: Option<HashMap<String, String>>,
}

#[derive(Debug, Default)]
struct ParsedBlocks {
    project: Option<ParsedProjectBlock>,
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

fn parse_flat_block(
    lines: &mut PhysicalLines<'_>,
    header_ctx: &LineCtx<'_>,
    kind: &str,
) -> Result<ParsedBlock, ProjectError> {
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
            line: header_ctx.line_1,
            message: format!("missing closing `}}` for `{kind}` block"),
            start: None,
            end: None,
        });
    }

    Ok(ParsedBlock {
        kind: kind.to_string(),
        label: None,
        fields,
    })
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

        if kind == "project" {
            if label.is_some() {
                return Err(parse_err(
                    &line_ctx,
                    "`project` block cannot carry a label",
                    None,
                ));
            }
            let project_block = parse_project_block(&mut lines, &line_ctx)?;
            parsed.project = Some(project_block);
            continue;
        }

        let mut block = parse_flat_block(&mut lines, &line_ctx, &kind)?;
        block.label = label;

        match block.kind.as_str() {
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

fn build_project_kind(type_field: Option<&str>) -> Result<ProjectKind, ProjectError> {
    match type_field.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(ProjectKind::Host),
        Some("Mod") | Some("Meta") => Ok(ProjectKind::Mod),
        Some("Template") => Ok(ProjectKind::Template),
        Some(other) => Err(ProjectError::meta_contract(
            "E1807",
            format!(
                "unsupported project.type `{other}` (omit the field for ordinary host projects, or use `Mod` / `Template`)"
            ),
        )),
    }
}

fn build_project_mod_from_fields(
    mod_fields: &HashMap<String, ModFieldValue>,
) -> Result<ProjectModSection, ProjectError> {
    let max_generator_rounds = match mod_fields.get("maxGeneratorRounds") {
        None => match mod_fields.get("maxMetaRounds") {
            None => None,
            Some(ModFieldValue::U32(u)) => Some(*u),
            Some(ModFieldValue::StringList(_)) | Some(ModFieldValue::String(_)) => {
                return Err(ProjectError::meta_contract(
                    "E1872",
                    "`project.mod.maxMetaRounds` must be a positive integer",
                ));
            }
        },
        Some(ModFieldValue::U32(u)) => Some(*u),
        Some(ModFieldValue::StringList(_)) | Some(ModFieldValue::String(_)) => {
            return Err(ProjectError::meta_contract(
                "E1872",
                "`project.mod.maxGeneratorRounds` must be a positive integer",
            ));
        }
    };

    let capabilities = match mod_fields.get("capabilities") {
        None => None,
        Some(ModFieldValue::StringList(v)) => Some(v.clone()),
        Some(ModFieldValue::U32(_)) | Some(ModFieldValue::String(_)) => {
            return Err(ProjectError::meta_contract(
                "E1873",
                "`project.mod.capabilities` must be a list of capability names",
            ));
        }
    };

    let artifact_policy = match mod_fields.get("artifactPolicy") {
        None => None,
        Some(ModFieldValue::String(v)) => Some(v.clone()),
        Some(ModFieldValue::StringList(_)) | Some(ModFieldValue::U32(_)) => {
            return Err(ProjectError::meta_contract(
                "E1875",
                "`project.mod.artifactPolicy` must be a single identifier or quoted string",
            ));
        }
    };

    Ok(ProjectModSection {
        max_generator_rounds,
        capabilities,
        artifact_policy,
    })
}

fn build_project_template_from_fields(
    template_fields: &HashMap<String, String>,
) -> ProjectTemplateSection {
    ProjectTemplateSection {
        short_name: template_fields.get("shortName").cloned(),
        identity: template_fields.get("identity").cloned(),
    }
}

fn assemble_project_section(project: &ParsedProjectBlock) -> Result<ProjectSection, ProjectError> {
    reject_corelib_opt_out_keys(&project.fields)?;
    let kind = build_project_kind(project.fields.get("type").map(|s| s.as_str()))?;
    let mod_section = match (&kind, &project.mod_section) {
        (ProjectKind::Host | ProjectKind::Template, Some(_)) => {
            return Err(ProjectError::meta_contract(
                "E1874",
                "`project.mod` is only allowed when `type = Mod`",
            ));
        }
        (ProjectKind::Mod, Some(mod_fields)) => Some(build_project_mod_from_fields(mod_fields)?),
        _ => None,
    };
    let template_section = match (&kind, &project.template_section) {
        (ProjectKind::Host | ProjectKind::Mod, Some(_)) => {
            return Err(ProjectError::meta_contract(
                "E1879",
                "`project.template` is only allowed when `type = Template`",
            ));
        }
        (ProjectKind::Template, Some(template_fields)) => {
            Some(build_project_template_from_fields(template_fields))
        }
        _ => None,
    };

    Ok(ProjectSection {
        name: required_field(&project.fields, "name")?,
        version: required_field(&project.fields, "version")?,
        root: project
            .fields
            .get("root")
            .cloned()
            .unwrap_or_else(|| "Src".to_string()),
        root_namespace: project.fields.get("root_namespace").cloned(),
        kind,
        mod_section,
        template_section,
    })
}

fn build_manifest(parsed: ParsedBlocks) -> Result<ProjectManifest, ProjectError> {
    let project = parsed
        .project
        .ok_or_else(|| ProjectError::Validation("missing required `project` block".to_string()))?;

    let project_section = assemble_project_section(&project)?;

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
