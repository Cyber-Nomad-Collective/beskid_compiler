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
}

impl ShellLayer {
    /// Layers that consume keyboard input, highest priority first.
    pub const INPUT_PRIORITY: &'static [ShellLayer] = &[
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
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_priority_topmost_wins() {
        assert_eq!(ShellLayer::INPUT_PRIORITY[0], ShellLayer::Palette);
        assert_eq!(
            ShellLayer::INPUT_PRIORITY.last(),
            Some(&ShellLayer::Base)
        );
    }

    #[test]
    fn draw_order_topmost_is_palette() {
        assert_eq!(
            ShellLayer::DRAW_ORDER.last(),
            Some(&ShellLayer::Palette)
        );
    }
}
