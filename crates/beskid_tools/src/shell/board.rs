//! BSOL board layout: parse, lower, and resolve scope-specific configs.

use std::fs;

use bsol::{load_profile, parse_bsol_document, validate, ValidatedBlock, ValidatedDocument};
use ratatui::layout::{Constraint, Layout, Rect};

use super::scope::{ShellScope, user_board_path};

pub const EMBEDDED_DEFAULT: &str = include_str!("assets/default.board.bsol");
pub const EMBEDDED_HI_DEFAULT: &str = include_str!("assets/hi-default.board.bsol");

/// Named tile region in the shell layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoardRegion {
    Header,
    Stage,
    Detail,
    Log,
    Footer,
    Full,
    Main,
}

impl BoardRegion {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "header" => Some(Self::Header),
            "stage" => Some(Self::Stage),
            "detail" => Some(Self::Detail),
            "log" => Some(Self::Log),
            "footer" => Some(Self::Footer),
            "full" => Some(Self::Full),
            "main" => Some(Self::Main),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BoardTile {
    pub id: String,
    pub widget: String,
    pub region: BoardRegion,
    pub title: Option<String>,
    pub weight: Option<u32>,
    pub min_height: Option<u32>,
    pub min_width: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct BoardSplit {
    pub id: String,
    pub axis: SplitAxis,
    pub ratio: u16,
    pub first: String,
    pub second: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
pub struct BoardLayout {
    pub name: String,
    pub title: Option<String>,
    pub scope_hint: Option<String>,
    pub tiles: Vec<BoardTile>,
    pub splits: Vec<BoardSplit>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BoardRects {
    pub header: Rect,
    pub stage: Rect,
    pub detail: Rect,
    pub log: Rect,
    pub footer: Rect,
    pub chrome: Rect,
    pub full: Rect,
}

impl BoardLayout {
    pub fn load_hi(scope: &ShellScope) -> Result<Self, String> {
        Self::load_for_scope(scope, EMBEDDED_HI_DEFAULT)
    }

    pub fn load_compile(scope: &ShellScope) -> Result<Self, String> {
        Self::load_for_scope(scope, EMBEDDED_DEFAULT)
    }

    fn load_for_scope(scope: &ShellScope, embedded: &str) -> Result<Self, String> {
        let path = scope.board_config_path();
        if path.is_file() {
            let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            return Self::parse(&text);
        }
        if matches!(scope, ShellScope::User) {
            let user_path = user_board_path();
            if user_path.is_file() {
                let text = fs::read_to_string(&user_path).map_err(|e| e.to_string())?;
                return Self::parse(&text);
            }
        }
        Self::parse(embedded)
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let document = parse_bsol_document(source).map_err(|e| e.to_string())?;
        let profile = load_profile("board.v1").map_err(|e| e.to_string())?;
        let validated = validate(&document, &profile).map_err(|e| e.to_string())?;
        lower_board(validated)
    }

    pub fn tile_for_region(&self, region: BoardRegion) -> Option<&BoardTile> {
        self.tiles.iter().find(|t| t.region == region)
    }

    pub fn resolve_rects(&self, area: Rect) -> BoardRects {
        let chrome_h = 1u16;
        let [header, body, log, footer_main] = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(8),
            Constraint::Length(5),
        ])
        .areas(area);
        let [stage, detail] =
            Layout::horizontal([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)]).areas(body);
        let footer = Rect {
            height: footer_main.height.saturating_sub(chrome_h),
            ..footer_main
        };
        let chrome = Rect {
            y: footer_main.y + footer.height,
            height: chrome_h.min(footer_main.height),
            x: footer_main.x,
            width: footer_main.width,
        };
        BoardRects {
            header,
            stage,
            detail,
            log,
            footer,
            chrome,
            full: area,
        }
    }

    pub fn rect_for_region(&self, rects: &BoardRects, region: BoardRegion) -> Rect {
        match region {
            BoardRegion::Header => rects.header,
            BoardRegion::Stage => rects.stage,
            BoardRegion::Detail => rects.detail,
            BoardRegion::Log => rects.log,
            BoardRegion::Footer => rects.footer,
            BoardRegion::Full => rects.full,
            BoardRegion::Main => rects.stage,
        }
    }
}

fn lower_board(doc: ValidatedDocument) -> Result<BoardLayout, String> {
    let mut name = "default".into();
    let mut title = None;
    let mut scope_hint = None;
    let mut tiles = Vec::new();
    let mut splits = Vec::new();

    for block in &doc.blocks {
        match block.rule_id.as_str() {
            "board" => {
                name = block.label.clone().unwrap_or_else(|| "default".into());
                title = block.fields.get("title").cloned();
                scope_hint = block.fields.get("scope").cloned();
            }
            "tile" => tiles.push(lower_tile(block)?),
            "split" => splits.push(lower_split(block)?),
            other => return Err(format!("unexpected board block rule `{other}`")),
        }
    }

    Ok(BoardLayout {
        name,
        title,
        scope_hint,
        tiles,
        splits,
    })
}

fn lower_tile(block: &ValidatedBlock) -> Result<BoardTile, String> {
    let widget = block
        .fields
        .get("widget")
        .cloned()
        .ok_or_else(|| format!("tile `{}` missing widget", block.label.as_deref().unwrap_or("?")))?;
    let region = block
        .fields
        .get("region")
        .and_then(|r| BoardRegion::from_str(r))
        .ok_or_else(|| format!("tile `{}` has invalid region", block.label.as_deref().unwrap_or("?")))?;
    Ok(BoardTile {
        id: block.label.clone().unwrap_or_else(|| widget.clone()),
        widget,
        region,
        title: block.fields.get("title").cloned(),
        weight: block.fields.get("weight").and_then(|v| v.parse().ok()),
        min_height: block.fields.get("min_height").and_then(|v| v.parse().ok()),
        min_width: block.fields.get("min_width").and_then(|v| v.parse().ok()),
    })
}

fn lower_split(block: &ValidatedBlock) -> Result<BoardSplit, String> {
    let axis = match block.fields.get("axis").map(String::as_str) {
        Some("horizontal") => SplitAxis::Horizontal,
        Some("vertical") => SplitAxis::Vertical,
        _ => return Err(format!("split `{}` missing axis", block.label.as_deref().unwrap_or("?"))),
    };
    Ok(BoardSplit {
        id: block.label.clone().unwrap_or_else(|| "split".into()),
        axis,
        ratio: block
            .fields
            .get("ratio")
            .and_then(|v| v.parse().ok())
            .unwrap_or(50),
        first: block
            .fields
            .get("first")
            .cloned()
            .ok_or_else(|| "split missing first".to_string())?,
        second: block
            .fields
            .get("second")
            .cloned()
            .ok_or_else(|| "split missing second".to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_embedded_hi_default() {
        let layout = BoardLayout::parse(EMBEDDED_HI_DEFAULT).expect("parse");
        assert_eq!(layout.name, "hi-default");
        assert_eq!(layout.tiles.len(), 5);
    }
}
