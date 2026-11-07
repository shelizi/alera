//! UI-agnostic workbench state and native desktop application services.
//! Tests can build this library without the heavyweight GPUI renderer.
pub mod editor;
pub mod editor_search;
pub mod editor_session;
pub mod editor_tabs;
pub mod file_search;
pub mod model;
pub mod project_search;
pub mod runtime_bridge;
pub mod session;
pub mod terminal;
pub mod visible_line_hit;
pub mod vt_screen;
pub mod workspace_tree;

#[cfg(test)]
mod editor_safety;
