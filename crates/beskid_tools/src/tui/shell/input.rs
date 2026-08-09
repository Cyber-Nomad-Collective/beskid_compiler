//! Keyboard and mouse routing for the unified shell.

mod base_key_mouse;
mod focus_hit_test;
mod navigation_scroll;
mod overlays;
mod selection_templates;
#[cfg(test)]
mod tests;

pub use base_key_mouse::{handle_base_input, handle_input_event};
pub use overlays::{
    handle_pckg_overlay_input, handle_simple_overlay_input, handle_summary_overlay_input,
    handle_templates_overlay_input, handle_tests_overlay_input,
};
