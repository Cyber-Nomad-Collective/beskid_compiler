//! Pages document model (`shell.pages.v1` BSOL) and page switching.

use std::collections::HashMap;
use std::fs;

use bsol::{load_profile, parse_bsol_document, validate, ValidatedDocument};

use super::editor::HiLayoutState;
use crate::shell::nav::NavAction;
use crate::shell::scope::{ShellScope, user_pages_path};

pub const EMBEDDED_HI_PAGES: &str = include_str!("../assets/hi-default.pages.bsol");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageEntry {
    pub id: String,
    pub title: String,
    pub board_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavItemEntry {
    pub id: String,
    pub label: String,
    pub action: NavAction,
    pub target: Option<String>,
    pub parent: Option<String>,
    pub order: Option<u32>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagesDoc {
    pub name: String,
    pub version: u32,
    pub default_page: String,
    pub title: Option<String>,
    pub pages: HashMap<String, PageEntry>,
    pub nav_items: HashMap<String, NavItemEntry>,
}

impl PagesDoc {
    pub fn page(&self, id: &str) -> Option<&PageEntry> {
        self.pages.get(id)
    }

    pub fn nav_item(&self, id: &str) -> Option<&NavItemEntry> {
        self.nav_items.get(id)
    }
}

pub fn parse_pages(source: &str) -> Result<PagesDoc, String> {
    let document = parse_bsol_document(source).map_err(|e| e.to_string())?;
    let profile = load_profile("shell.pages.v1").map_err(|e| e.to_string())?;
    let validated = validate(&document, &profile).map_err(|e| e.to_string())?;
    lower_pages(validated)
}

pub fn emit_pages(doc: &PagesDoc) -> String {
    let mut out = String::new();
    out.push_str(&format!("pages \"{}\" {{\n", escape(&doc.name)));
    out.push_str(&format!("  version = {}\n", doc.version));
    out.push_str(&format!("  default_page = \"{}\"\n", escape(&doc.default_page)));
    if let Some(title) = &doc.title {
        out.push_str(&format!("  title = \"{}\"\n", escape(title)));
    }
    out.push_str("}\n");

    let mut page_ids: Vec<_> = doc.pages.keys().cloned().collect();
    page_ids.sort();
    for id in page_ids {
        if let Some(page) = doc.pages.get(&id) {
            emit_page(&mut out, page);
        }
    }

    let mut nav_ids: Vec<_> = doc.nav_items.keys().cloned().collect();
    nav_ids.sort();
    for id in nav_ids {
        if let Some(item) = doc.nav_items.get(&id) {
            emit_nav_item(&mut out, item);
        }
    }
    out
}

pub fn load_for_scope(scope: &ShellScope) -> Result<PagesDoc, String> {
    let source = read_scope_source(scope)?;
    parse_pages(&source)
}

pub fn save_for_scope(scope: &ShellScope, doc: &PagesDoc) -> Result<(), String> {
    let path = scope.pages_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = emit_pages(doc);
    fs::write(&path, text).map_err(|e| e.to_string())
}

pub fn embedded_default() -> Result<PagesDoc, String> {
    parse_pages(EMBEDDED_HI_PAGES)
}

pub fn switch_page(state: &mut HiLayoutState, page_id: &str) -> Result<(), String> {
    let page = state
        .pages
        .page(page_id)
        .ok_or_else(|| format!("unknown page `{page_id}`"))?
        .clone();
    state.active_page_id = page_id.to_string();
    if let Some(root) = page.board_root {
        if state.doc.nodes.contains_key(&root) {
            state.doc.root = root;
            state.rebuild_runtime()?;
        }
    }
    Ok(())
}

fn read_scope_source(scope: &ShellScope) -> Result<String, String> {
    let path = scope.pages_config_path();
    if path.is_file() {
        return fs::read_to_string(&path).map_err(|e| e.to_string());
    }
    if matches!(scope, ShellScope::User) {
        let user_path = user_pages_path();
        if user_path.is_file() {
            return fs::read_to_string(&user_path).map_err(|e| e.to_string());
        }
    }
    Ok(EMBEDDED_HI_PAGES.to_string())
}

fn lower_pages(doc: ValidatedDocument) -> Result<PagesDoc, String> {
    let mut name = "default".into();
    let mut version = 0u32;
    let mut default_page = String::new();
    let mut title = None;
    let mut pages = HashMap::new();
    let mut nav_items = HashMap::new();

    for block in &doc.blocks {
        match block.rule_id.as_str() {
            "pages" => {
                name = block.label.clone().unwrap_or_else(|| "default".into());
                version = block
                    .fields
                    .get("version")
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| "shell.pages.v1 requires version = 1".to_string())?;
                if version != 1 {
                    return Err(format!("unsupported pages version {version}"));
                }
                default_page = block
                    .fields
                    .get("default_page")
                    .cloned()
                    .ok_or_else(|| "pages missing default_page".to_string())?;
                title = block.fields.get("title").cloned();
            }
            "page" => {
                let id = block
                    .label
                    .clone()
                    .ok_or_else(|| "page missing label".to_string())?;
                let page_title = block
                    .fields
                    .get("title")
                    .cloned()
                    .ok_or_else(|| format!("page `{id}` missing title"))?;
                pages.insert(
                    id.clone(),
                    PageEntry {
                        id,
                        title: page_title,
                        board_root: block.fields.get("board_root").cloned(),
                    },
                );
            }
            "nav_item" => {
                let id = block
                    .label
                    .clone()
                    .ok_or_else(|| "nav_item missing label".to_string())?;
                let label = block
                    .fields
                    .get("label")
                    .cloned()
                    .ok_or_else(|| format!("nav_item `{id}` missing label"))?;
                let action_str = block
                    .fields
                    .get("action")
                    .ok_or_else(|| format!("nav_item `{id}` missing action"))?;
                let target = block.fields.get("target").cloned();
                let action = NavAction::from_str(action_str, target.as_deref())?;
                nav_items.insert(
                    id.clone(),
                    NavItemEntry {
                        id,
                        label,
                        action,
                        target,
                        parent: block.fields.get("parent").cloned(),
                        order: block.fields.get("order").and_then(|v| v.parse().ok()),
                        icon: block.fields.get("icon").cloned(),
                    },
                );
            }
            other => return Err(format!("unexpected shell.pages.v1 block `{other}`")),
        }
    }

    if default_page.is_empty() {
        return Err("shell.pages.v1 missing default_page".into());
    }

    Ok(PagesDoc {
        name,
        version,
        default_page,
        title,
        pages,
        nav_items,
    })
}

fn emit_page(out: &mut String, page: &PageEntry) {
    out.push_str(&format!("page \"{}\" {{\n", escape(&page.id)));
    out.push_str(&format!("  title = \"{}\"\n", escape(&page.title)));
    if let Some(root) = &page.board_root {
        out.push_str(&format!("  board_root = \"{}\"\n", escape(root)));
    }
    out.push_str("}\n");
}

fn emit_nav_item(out: &mut String, item: &NavItemEntry) {
    out.push_str(&format!("nav_item \"{}\" {{\n", escape(&item.id)));
    out.push_str(&format!("  label = \"{}\"\n", escape(&item.label)));
    out.push_str(&format!("  action = {}\n", nav_action_keyword(&item.action)));
    if let Some(target) = &item.target {
        out.push_str(&format!("  target = \"{}\"\n", escape(target)));
    }
    if let Some(parent) = &item.parent {
        out.push_str(&format!("  parent = \"{}\"\n", escape(parent)));
    }
    if let Some(order) = item.order {
        out.push_str(&format!("  order = {order}\n"));
    }
    if let Some(icon) = &item.icon {
        out.push_str(&format!("  icon = \"{}\"\n", escape(icon)));
    }
    out.push_str("}\n");
}

fn nav_action_keyword(action: &NavAction) -> &'static str {
    match action {
        NavAction::Page(_) => "page",
        NavAction::Overlay(_) => "overlay",
        NavAction::Widget(_) => "widget",
        NavAction::Cli(_) => "cli",
        NavAction::Group => "group",
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_embedded_hi_pages() {
        let doc = parse_pages(EMBEDDED_HI_PAGES).expect("parse pages");
        assert_eq!(doc.name, "hi-default");
        assert_eq!(doc.default_page, "home");
        assert!(doc.pages.contains_key("graphs"));
        assert!(doc.nav_items.contains_key("compiler"));
    }

    #[test]
    fn emit_roundtrip_embedded_pages() {
        let doc = parse_pages(EMBEDDED_HI_PAGES).expect("parse");
        let text = emit_pages(&doc);
        let again = parse_pages(&text).expect("re-parse");
        assert_eq!(again.default_page, doc.default_page);
        assert_eq!(again.pages.len(), doc.pages.len());
        assert_eq!(again.nav_items.len(), doc.nav_items.len());
    }
}
