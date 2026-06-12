//! Shell UI z-order: draw bottom-to-top, route input top-to-bottom.

/// Visual stacking order for shell chrome and modals (lowest = farthest back).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ShellLayer {
    Base = 0,
    PanelOverlay = 10,
    Help = 20,
    LayoutEditor = 30,
    ScopePicker = 40,
    Palette = 50,
    CommandDialog = 60,
    TopMenuDropdown = 70,
}

impl ShellLayer {
    /// Layers that consume keyboard input, highest priority first.
    pub const INPUT_PRIORITY: &'static [ShellLayer] = &[
        ShellLayer::TopMenuDropdown,
        ShellLayer::CommandDialog,
        ShellLayer::Palette,
        ShellLayer::ScopePicker,
        ShellLayer::LayoutEditor,
        ShellLayer::PanelOverlay,
        ShellLayer::Base,
    ];

    /// Layers painted in back-to-front order.
    pub const DRAW_ORDER: &'static [ShellLayer] = &[
        ShellLayer::Base,
        ShellLayer::PanelOverlay,
        ShellLayer::Help,
        ShellLayer::LayoutEditor,
        ShellLayer::ScopePicker,
        ShellLayer::Palette,
        ShellLayer::CommandDialog,
        ShellLayer::TopMenuDropdown,
    ];
}
