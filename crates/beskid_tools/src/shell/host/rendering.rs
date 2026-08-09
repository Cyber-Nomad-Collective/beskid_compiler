use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::HiShellApp;
use crate::shell::context::WidgetContext;
use crate::shell::layers::ShellLayer;
use crate::shell::overlay_render::{self, HiOverlayWidgets, OverlayRenderContext};

impl HiShellApp {
    pub(crate) fn draw_shell(&mut self, frame: &mut Frame) {
        self.drain_messages();
        self.shortcut_clicks.clear();
        self.shell_state.tick = self.shell_state.tick.wrapping_add(1);
        if self.shell_state.tick.is_multiple_of(40) {
            self.try_refresh_scope();
        }
        let area = frame.area();
        self.set_frame_area(area);
        if let Some(kind) = self.layout.runtime.focused_kind() {
            self.focused_widget = kind.to_string();
        }
        let edit_active = self.layout.editor.active;
        let highlight = self.focused_widget.clone();
        let _page_title =
            self.layout.pages.page(&self.layout.active_page_id).map(|p| p.title.as_str()).unwrap_or("Beskid Hi");

        let control_mode = self.control_mode();
        let header_h = super::chrome::PINNED_TOP_ROWS.min(area.height);
        let chrome_h = 1u16.min(area.height.saturating_sub(header_h));
        let main_h = area.height.saturating_sub(header_h).saturating_sub(chrome_h);
        let header_area = ratatui::layout::Rect { width: area.width, height: header_h, x: area.x, y: area.y };
        let main_area = ratatui::layout::Rect { width: area.width, height: main_h, x: area.x, y: area.y + header_h };
        let chrome_area =
            ratatui::layout::Rect { width: area.width, height: chrome_h, x: area.x, y: area.y + header_h + main_h };

        let resolved = match super::layout::resolve::resolve_panels(&mut self.layout.runtime, area) {
            Ok(r) => r,
            Err(message) => {
                self.pinned_header = header_area;
                self.chrome.render_pinned_top_bar(header_area, frame, &self.scope);
                let error_area = if main_area.width == 0 || main_area.height == 0 { area } else { main_area };
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(format!("Layout error: {message}"))
                        .style(ratatui::style::Style::default().fg(ratatui::style::Color::Red)),
                    error_area,
                );
                self.chrome.render_footer(
                    chrome_area,
                    frame,
                    &self.hotkeys,
                    control_mode,
                    Some(self.focused_widget.as_str()),
                    self.layout.editor.drawer_visible,
                    &mut self.shortcut_clicks,
                );
                return;
            }
        };

        self.pinned_header = resolved.header_area;
        self.chrome.render_pinned_top_bar(resolved.header_area, frame, &self.scope);

        for entry in resolved.frame.panels() {
            let rect = entry.rect;
            let widget_id = entry.kind.to_string();
            if widget_id == "shell.scope" {
                continue;
            }
            let key_bindings = &mut self.key_bindings;
            let shortcut_clicks = &mut self.shortcut_clicks;
            let pending_shortcut_rebind = &mut self.pending_shortcut_rebind;
            let mut ctx = WidgetContext::new(
                &self.scope,
                &self.layout.doc,
                &mut self.shell_state,
                &mut self.palette,
                &self.focused_widget,
                key_bindings,
                shortcut_clicks,
                pending_shortcut_rebind,
            );
            if let Some(widget) = self.registry.get(&widget_id) {
                widget.render(rect, frame, &mut ctx);
            }
            if edit_active && highlight == entry.kind as &str {
                render_edit_highlight(frame, rect);
            }
        }

        self.chrome.render_footer(
            resolved.chrome_area,
            frame,
            &self.hotkeys,
            control_mode,
            Some(self.focused_widget.as_str()),
            self.layout.editor.drawer_visible,
            &mut self.shortcut_clicks,
        );

        for layer in ShellLayer::DRAW_ORDER {
            match layer {
                ShellLayer::Base => {}
                ShellLayer::PanelOverlay => {
                    let scope = &self.scope;
                    let layout_doc = &self.layout.doc;
                    let shell_state = &mut self.shell_state;
                    let palette = &mut self.palette;
                    let focused_widget = &self.focused_widget;
                    let key_bindings = &mut self.key_bindings;
                    let shortcut_clicks = &mut self.shortcut_clicks;
                    let pending_shortcut_rebind = &mut self.pending_shortcut_rebind;
                    let registry = &self.registry;
                    let mut ctx = WidgetContext::new(
                        scope,
                        layout_doc,
                        shell_state,
                        palette,
                        focused_widget,
                        key_bindings,
                        shortcut_clicks,
                        pending_shortcut_rebind,
                    );
                    overlay_render::render_panel_overlays(
                        frame,
                        area,
                        OverlayRenderContext::Hi(HiOverlayWidgets { ctx: &mut ctx, registry }),
                    );
                }
                ShellLayer::Help => {
                    if self.chrome.show_help {
                        let help_area = crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_TESTS, area);
                        let help_items = self.hotkeys.footer_items(Some(&self.focused_widget));
                        self.chrome.render_help_overlay(help_area, frame, &help_items, &mut self.shortcut_clicks);
                    }
                }
                ShellLayer::LayoutEditor => {
                    if self.layout.editor.active {
                        self.layout_editor.render(
                            area,
                            frame,
                            &self.layout.editor,
                            &self.layout.doc,
                            self.registry.descriptors(),
                        );
                    }
                }
                ShellLayer::ScopePicker => {
                    if let Some(picker) = &self.scope_picker {
                        let overlay = crate::tui::layout::overlay_rect_for(crate::tui::layout::OVERLAY_PCKG, area);
                        picker.render(overlay, frame);
                    }
                }
                ShellLayer::Palette => {
                    if self.palette.visible {
                        self.palette.render(area, frame, &self.key_bindings.palette_hint());
                    }
                }
            }
        }
    }
}

fn render_edit_highlight(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let style = Style::default().bg(Color::Indexed(236));
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.set_style(style);
            }
        }
    }
}
