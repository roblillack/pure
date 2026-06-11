// Library interface for Pure editor
// This exposes internal modules for testing and benchmarking

pub mod app;
pub mod editor;
pub mod editor_display;
pub mod render;
pub mod theme;

#[cfg(test)]
mod test_harness;
