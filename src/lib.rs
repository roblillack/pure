// Library interface for Pure editor
// This exposes internal modules for testing and benchmarking

pub mod app;
pub mod editor;
pub mod editor_display;
pub mod menu_bar;
pub mod render;
pub mod theme;

#[cfg(any(test, feature = "recorder"))]
pub mod test_harness;
