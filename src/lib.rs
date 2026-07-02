// Library interface for Pure editor
// This exposes internal modules for testing and benchmarking

pub mod app;
pub mod file_dialog;
pub mod link_dialog;
pub mod menu_bar;
pub mod ratatui_draw_context;
pub mod theme;

#[cfg(any(test, feature = "recorder"))]
pub mod test_harness;
