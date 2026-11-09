//! UI-agnostic workbench state and native desktop application services.
//! Tests can build this library without the heavyweight GPUI renderer.
pub mod agent_profiles;
mod agent_runtime_bridge;
pub mod edition;
pub mod editor;
pub mod editor_search;
pub mod editor_session;
pub mod editor_tabs;
pub mod explorer_drag;
pub mod explorer_git_badges;
pub mod explorer_ops;
pub mod explorer_selection;
pub mod explorer_visibility;
pub mod file_search;
pub mod git_actions;
pub mod git_branch_compare;
pub mod git_graph_layout;
pub mod git_history;
pub mod git_inspect;
pub mod mcp_ui_bridge;
pub mod mcp_workspace_creation;
pub mod model;
pub mod native_modal;
pub mod native_settings;
pub mod project_search;
pub mod project_tree;
pub mod runtime_bridge;
pub mod session;
pub mod terminal;
pub mod terminal_bootstrap;
pub mod terminal_output_budget;
pub mod terminal_panes;
pub mod terminal_stream_bridge;
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
