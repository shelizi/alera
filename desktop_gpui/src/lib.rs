//! UI-agnostic workbench state and native desktop application services.
//! Tests can build this library without the heavyweight GPUI renderer.
pub mod agent_profiles;
mod agent_runtime_bridge;
pub mod edition;
pub mod editor;
pub mod editor_search;
pub mod editor_session;
pub mod editor_tabs;
pub mod explorer_ops;
pub mod file_search;
pub mod git_actions;
pub mod git_history;
pub mod git_inspect;
pub mod model;
pub mod project_search;
pub mod project_tree;
pub mod runtime_bridge;
pub mod session;
pub mod terminal;
pub mod terminal_panes;
pub mod visible_line_hit;
pub mod visual_tokens;
pub mod vt_screen;
pub mod workbench_layout_store;
pub mod workbench_panes;
pub mod workbench_tabs;
pub mod workspace_tree;
pub mod worktree_manager;

#[cfg(test)]
mod editor_safety;
