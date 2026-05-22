//! Orchestrate project, workspace, and item template instantiation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{TemplateError, TemplateResult};
use crate::manifest::{TemplateManifest, TemplateOutputKind};
use crate::post_actions::{PostActionContext, run_post_actions};
use crate::sources::{apply_write_plans, normalize_output_path, plan_source_writes};
use crate::substitute::build_substitution_map;
use crate::symbols::{collect_symbol_values, SymbolCollectOptions};

#[derive(Debug, Clone)]
pub struct InstantiateOptions {
    pub template_root: PathBuf,
    pub output: PathBuf,
    pub host_project: Option<PathBuf>,
    pub force: bool,
    pub allow_project_manifest: bool,
    pub strict_post_actions: bool,
    pub symbol_options: SymbolCollectOptions,
    pub skip_default_lock: bool,
    pub beskid_exe: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct InstantiateResult {
    pub output_root: PathBuf,
    pub workspace_root: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
}

pub fn instantiate(
    manifest: &TemplateManifest,
    options: &InstantiateOptions,
) -> TemplateResult<InstantiateResult> {
    let values = collect_symbol_values(manifest, &options.symbol_options)?;
    let substitution = build_substitution_map(manifest, &values)?;
    let output_kind = manifest.output_kind();

    let output_root = resolve_output_root(output_kind, &options, manifest)?;

    if output_root.exists() {
        let non_empty = fs::read_dir(&output_root)
            .ok()
            .map(|mut rd| rd.next().is_some())
            .unwrap_or(false);
        if non_empty && !options.force {
            return Err(TemplateError::OutputConflict {
                path: output_root.clone(),
            });
        }
    } else {
        fs::create_dir_all(&output_root)?;
    }

    validate_item_template(output_kind, &output_root, &options)?;

    let mut guids_map = std::collections::HashMap::new();
    let plans = plan_source_writes(
        &options.template_root,
        manifest,
        &output_root,
        &substitution,
        &mut guids_map,
    )?;

    if output_kind == TemplateOutputKind::Item {
        validate_item_outputs(&plans, &output_root, &options)?;
    }

    apply_write_plans(&output_root, &plans, options.force)?;

    ensure_no_corelib_opt_out(&output_root)?;

    let mut post_actions = manifest.post_actions.clone();
    if !options.skip_default_lock
        && !post_actions.iter().any(|a| a.action_id == "beskidLock")
    {
        post_actions.push(crate::manifest::TemplatePostAction {
            action_id: "beskidLock".to_string(),
            args: serde_json::json!({}),
        });
    }

    let lock_root = workspace_lock_root(output_kind, &output_root)?;
    let ctx = PostActionContext {
        output_root: output_root.clone(),
        lock_root,
        beskid_exe: options.beskid_exe.clone(),
        strict: options.strict_post_actions,
    };
    run_post_actions(&post_actions, &ctx)?;

    Ok(InstantiateResult {
        output_root: output_root.clone(),
        workspace_root: if output_kind == TemplateOutputKind::Workspace {
            Some(output_root.clone())
        } else {
            None
        },
        project_root: if output_kind == TemplateOutputKind::Project {
            Some(output_root.clone())
        } else {
            options.host_project.clone()
        },
    })
}

fn resolve_output_root(
    kind: TemplateOutputKind,
    options: &InstantiateOptions,
    _manifest: &TemplateManifest,
) -> TemplateResult<PathBuf> {
    let path = normalize_output_path(&options.output)?;
    match kind {
        TemplateOutputKind::Item => {
            if path.is_file() || path.extension().is_some() {
                Ok(path.parent().unwrap_or(Path::new(".")).to_path_buf())
            } else {
                Ok(path)
            }
        }
        _ => Ok(path),
    }
}

fn validate_item_template(
    kind: TemplateOutputKind,
    output_root: &Path,
    options: &InstantiateOptions,
) -> TemplateResult<()> {
    if kind != TemplateOutputKind::Item {
        return Ok(());
    }
    let host = options
        .host_project
        .as_ref()
        .map(|p| p.parent().unwrap_or(Path::new(".")).to_path_buf())
        .or_else(|| find_project_root(output_root));

    let Some(host_root) = host else {
        return Err(TemplateError::InvalidManifest(
            "item template requires --project or output under a project directory".to_string(),
        ));
    };

    if !output_root.starts_with(&host_root) {
        return Err(TemplateError::ItemOutsideProject {
            path: output_root.to_path_buf(),
        });
    }
    Ok(())
}

fn validate_item_outputs(
    plans: &[crate::sources::SourceWritePlan],
    output_root: &Path,
    options: &InstantiateOptions,
) -> TemplateResult<()> {
    for plan in plans {
        let rel = plan.relative_output.to_string_lossy();
        if rel.contains("Project.proj") && !options.allow_project_manifest {
            return Err(TemplateError::InvalidManifest(
                "item template cannot write Project.proj without --allow-project-manifest"
                    .to_string(),
            ));
        }
    }
    let _ = output_root;
    Ok(())
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join("Project.proj").is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn workspace_lock_root(kind: TemplateOutputKind, output_root: &Path) -> TemplateResult<PathBuf> {
    match kind {
        TemplateOutputKind::Workspace => Ok(output_root.to_path_buf()),
        TemplateOutputKind::Project => Ok(output_root.to_path_buf()),
        TemplateOutputKind::Item => find_project_root(output_root)
            .ok_or_else(|| TemplateError::InvalidManifest("host project not found".into())),
    }
}

fn ensure_no_corelib_opt_out(output_root: &Path) -> TemplateResult<()> {
    let manifest = output_root.join("Project.proj");
    if !manifest.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&manifest)?;
    for flag in ["noCorelib", "useCorelib: false", "useCorelib=false"] {
        if text.contains(flag) {
            return Err(TemplateError::InvalidManifest(
                "templates must not emit corelib opt-out flags".to_string(),
            ));
        }
    }
    Ok(())
}

pub fn fixture_manifest(
    short_name: &str,
    template_type: TemplateOutputKind,
) -> TemplateManifest {
    use crate::manifest::{
        TemplateManifest, TemplatePostAction, TemplateSource, TemplateSymbol, TemplateTags,
    };
    use crate::manifest::{SymbolType, TEMPLATE_SCHEMA};

    TemplateManifest {
        schema: TEMPLATE_SCHEMA.to_string(),
        identity: format!("test.fixture::{short_name}"),
        name: format!("Fixture {short_name}"),
        short_name: short_name.to_string(),
        author: None,
        description: None,
        classifications: None,
        tags: TemplateTags {
            template_type: Some(template_type),
        },
        source_name: Some("MyApp".to_string()),
        name_symbol: None,
        symbols: BTreeMap::from([(
            "name".to_string(),
            TemplateSymbol {
                symbol_type: SymbolType::String,
                description: None,
                default_value: Some("MyApp".to_string()),
                choices: None,
                is_required: true,
            },
        )]),
        sources: vec![TemplateSource {
            source: "./".to_string(),
            target: "./".to_string(),
            include: vec!["**/*".to_string()],
            exclude: crate::manifest::default_exclude_patterns(),
            copy_only: vec![],
            rename: BTreeMap::new(),
            condition: true,
            modifiers: vec![],
        }],
        guids: vec![],
        forms: BTreeMap::new(),
        post_actions: vec![TemplatePostAction {
            action_id: "beskidLock".to_string(),
            args: serde_json::json!({}),
        }],
        prefer_interactive: false,
    }
}
