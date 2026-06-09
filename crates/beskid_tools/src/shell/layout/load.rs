//! Load board layout for a shell scope.

use std::fs;

use panes::runtime::LayoutRuntime;

use super::lower::lower_runtime;
use super::model::BoardV2Doc;
use super::parse::{import_v1, parse_v2, EMBEDDED_HI_V2};
use crate::shell::board::BoardLayout;
use crate::shell::scope::{ShellScope, user_board_path};

pub fn load_for_scope(scope: &ShellScope) -> Result<(BoardV2Doc, LayoutRuntime), String> {
    let source = read_scope_source(scope)?;
    load_from_source(&source)
}

pub fn load_from_source(source: &str) -> Result<(BoardV2Doc, LayoutRuntime), String> {
    if let Ok(doc) = parse_v2(source) {
        let runtime = lower_runtime(&doc)?;
        return Ok((doc, runtime));
    }
    let v1 = BoardLayout::parse(source)?;
    let doc = import_v1(&v1);
    let runtime = lower_runtime(&doc)?;
    Ok((doc, runtime))
}

pub fn save_for_scope(scope: &ShellScope, doc: &BoardV2Doc) -> Result<(), String> {
    let path = scope.board_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = super::emit::emit_v2(doc);
    fs::write(&path, text).map_err(|e| e.to_string())
}

fn read_scope_source(scope: &ShellScope) -> Result<String, String> {
    let path = scope.board_config_path();
    if path.is_file() {
        return fs::read_to_string(&path).map_err(|e| e.to_string());
    }
    if matches!(scope, ShellScope::User) {
        let user_path = user_board_path();
        if user_path.is_file() {
            return fs::read_to_string(&user_path).map_err(|e| e.to_string());
        }
    }
    Ok(EMBEDDED_HI_V2.to_string())
}

pub fn embedded_default() -> Result<(BoardV2Doc, LayoutRuntime), String> {
    load_from_source(EMBEDDED_HI_V2)
}
