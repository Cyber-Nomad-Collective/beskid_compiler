//! Layout editor state and mutations.

use std::time::{Duration, Instant};

use panes::runtime::LayoutRuntime;
use super::model::{BoardNode, BoardV2Doc, NodeKind};
use super::load::save_for_scope;
use super::pages::PagesDoc;
use crate::shell::descriptor::is_layout_locked_widget;
use crate::shell::registry::WidgetRegistry;
use crate::shell::scope::ShellScope;

fn focused_kind_locked(runtime: &LayoutRuntime) -> bool {
    runtime
        .focused_kind_arc()
        .is_some_and(|k| is_layout_locked_widget(k.as_ref()))
}

const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutOverlayTab {
    #[default]
    Templates,
    Widgets,
    Layouts,
    Structure,
}

impl LayoutOverlayTab {
    pub const ALL: [Self; 4] = [
        Self::Templates,
        Self::Widgets,
        Self::Layouts,
        Self::Structure,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Templates => "Templates",
            Self::Widgets => "Widgets",
            Self::Layouts => "Layouts",
            Self::Structure => "Structure",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutEditCommand {
    ToggleEdit,
    FocusNext,
    FocusPrev,
    ResizePlus,
    ResizeMinus,
    AddPanel,
    RemovePanel,
    WrapCol,
    WrapRow,
    ConvertTabs,
    ConvertStack,
    Save,
    Reset,
    SetWidget,
}

pub struct LayoutEditorState {
    pub active: bool,
    pub dirty: bool,
    pub last_edit: Option<Instant>,
    pub pending_widget: Option<String>,
    pub overlay_tab: LayoutOverlayTab,
    /// Right-side widget/template drawer visible (toggle with `w` in edit mode).
    pub drawer_visible: bool,
}

impl Default for LayoutEditorState {
    fn default() -> Self {
        Self {
            active: false,
            dirty: false,
            last_edit: None,
            pending_widget: None,
            overlay_tab: LayoutOverlayTab::default(),
            drawer_visible: false,
        }
    }
}

pub struct HiLayoutState {
    pub doc: BoardV2Doc,
    pub runtime: LayoutRuntime,
    pub pages: PagesDoc,
    pub active_page_id: String,
    pub editor: LayoutEditorState,
}

impl HiLayoutState {
    pub fn new(doc: BoardV2Doc, runtime: LayoutRuntime, pages: PagesDoc) -> Self {
        let active_page_id = pages.default_page.clone();
        Self {
            doc,
            runtime,
            pages,
            active_page_id,
            editor: LayoutEditorState::default(),
        }
    }

    pub fn title(&self) -> &str {
        self.doc.title.as_deref().unwrap_or("Beskid Hi")
    }

    pub fn rebuild_runtime(&mut self) -> Result<(), String> {
        self.runtime = super::lower::lower_runtime(&self.doc)?;
        Ok(())
    }

    pub fn mark_dirty(&mut self) {
        self.editor.dirty = true;
        self.editor.last_edit = Some(Instant::now());
    }

    pub fn maybe_autosave(&mut self, scope: &ShellScope) -> Result<(), String> {
        if !self.editor.dirty {
            return Ok(());
        }
        let Some(ts) = self.editor.last_edit else {
            return Ok(());
        };
        if ts.elapsed() < AUTOSAVE_DEBOUNCE {
            return Ok(());
        }
        save_for_scope(scope, &self.doc)?;
        self.editor.dirty = false;
        Ok(())
    }

    pub fn apply_command(
        &mut self,
        cmd: LayoutEditCommand,
        scope: &ShellScope,
        widget_id: Option<&str>,
    ) -> Result<(), String> {
        match cmd {
            LayoutEditCommand::ToggleEdit => {
                self.editor.active = !self.editor.active;
                if self.editor.active {
                    self.editor.drawer_visible = false;
                    self.editor.overlay_tab = LayoutOverlayTab::Widgets;
                } else {
                    self.editor.drawer_visible = false;
                }
                self.runtime
                    .set_collect_boundaries(self.editor.active);
            }
            LayoutEditCommand::FocusNext => {
                self.runtime.focus_next();
                if focused_kind_locked(&self.runtime) {
                    self.runtime.focus_next();
                }
            }
            LayoutEditCommand::FocusPrev => {
                self.runtime.focus_prev();
                if focused_kind_locked(&self.runtime) {
                    self.runtime.focus_prev();
                }
            }
            LayoutEditCommand::ResizePlus => {
                if focused_kind_locked(&self.runtime) {
                    return Ok(());
                }
                if let Some(pid) = self.runtime.focused() {
                    self.runtime.resize_boundary(pid, 0.02).ok();
                    self.mark_dirty();
                }
            }
            LayoutEditCommand::ResizeMinus => {
                if focused_kind_locked(&self.runtime) {
                    return Ok(());
                }
                if let Some(pid) = self.runtime.focused() {
                    self.runtime.resize_boundary(pid, -0.02).ok();
                    self.mark_dirty();
                }
            }
            LayoutEditCommand::AddPanel => {
                let widget = widget_id.unwrap_or("hi.welcome");
                let id = format!("panel_{}", self.doc.nodes.len());
                self.doc.nodes.insert(
                    id.clone(),
                    BoardNode {
                        kind: NodeKind::Panel,
                        widget: Some(widget.into()),
                        grow: Some(1),
                        ..BoardNode::default()
                    },
                );
                if let Some(root) = self.doc.nodes.get_mut(&self.doc.root) {
                    root.children.push(id);
                }
                self.rebuild_runtime()?;
                self.mark_dirty();
            }
            LayoutEditCommand::RemovePanel => {
                if focused_kind_locked(&self.runtime) {
                    return Ok(());
                }
                if let Some(kind) = self.runtime.focused_kind_arc() {
                    let id = self
                        .doc
                        .nodes
                        .iter()
                        .find(|(_, n)| n.widget.as_deref() == Some(kind.as_ref()))
                        .map(|(k, _)| k.clone());
                    if let Some(id) = id {
                        self.doc.nodes.remove(&id);
                        if let Some(root) = self.doc.nodes.get_mut(&self.doc.root) {
                            root.children.retain(|c| c != &id);
                        }
                        self.rebuild_runtime()?;
                        self.mark_dirty();
                    }
                }
            }
            LayoutEditCommand::WrapCol | LayoutEditCommand::WrapRow => {
                if let Some(kind) = self.runtime.focused_kind_arc() {
                    let panel_id = format!("wrap_{}", self.doc.nodes.len());
                    let wrapper_id = format!("group_{}", self.doc.nodes.len());
                    self.doc.nodes.insert(
                        panel_id.clone(),
                        BoardNode {
                            kind: NodeKind::Panel,
                            widget: Some(kind.to_string()),
                            grow: Some(1),
                            ..BoardNode::default()
                        },
                    );
                    self.doc.nodes.insert(
                        wrapper_id.clone(),
                        BoardNode {
                            kind: if cmd == LayoutEditCommand::WrapCol {
                                NodeKind::Col
                            } else {
                                NodeKind::Row
                            },
                            children: vec![panel_id],
                            grow: Some(1),
                            ..BoardNode::default()
                        },
                    );
                    self.rebuild_runtime()?;
                    self.mark_dirty();
                }
            }
            LayoutEditCommand::ConvertTabs => {
                self.convert_strategy(NodeKind::Tabs)?;
            }
            LayoutEditCommand::ConvertStack => {
                self.convert_strategy(NodeKind::Stack)?;
            }
            LayoutEditCommand::Save => {
                save_for_scope(scope, &self.doc)?;
                self.editor.dirty = false;
            }
            LayoutEditCommand::Reset => {
                let (doc, runtime) = super::load::embedded_default()?;
                let pages = super::pages::embedded_default()?;
                self.doc = doc;
                self.runtime = runtime;
                self.pages = pages;
                self.active_page_id = self.pages.default_page.clone();
                save_for_scope(scope, &self.doc)?;
                self.editor.dirty = false;
            }
            LayoutEditCommand::SetWidget => {
                if let (Some(widget), Some(kind)) = (widget_id, self.runtime.focused_kind_arc()) {
                    for node in self.doc.nodes.values_mut() {
                        if node.widget.as_deref() == Some(kind.as_ref()) {
                            node.widget = Some(widget.into());
                        }
                    }
                    self.rebuild_runtime()?;
                    self.mark_dirty();
                }
            }
        }
        Ok(())
    }

    fn convert_strategy(&mut self, kind: NodeKind) -> Result<(), String> {
        let widgets: Vec<String> = self
            .doc
            .nodes
            .values()
            .filter_map(|n| n.widget.clone())
            .collect();
        if widgets.is_empty() {
            return Ok(());
        }
        let children: Vec<String> = widgets
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let id = format!("tab_{i}");
                self.doc.nodes.insert(
                    id.clone(),
                    BoardNode {
                        kind: NodeKind::Panel,
                        widget: Some(w.clone()),
                        grow: Some(1),
                        ..BoardNode::default()
                    },
                );
                id
            })
            .collect();
        self.doc.root = "root".into();
        self.doc.nodes.insert(
            "root".into(),
            BoardNode {
                kind,
                children,
                ..BoardNode::default()
            },
        );
        self.rebuild_runtime()?;
        self.mark_dirty();
        Ok(())
    }

    pub fn layout_palette_commands(&self, _registry: &WidgetRegistry) -> Vec<super::super::catalog::CommandItem> {
        super::super::catalog::layout_editor_commands(self.editor.active)
    }
}
