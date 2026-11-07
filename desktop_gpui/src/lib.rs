//! UI-agnostic workbench state and native desktop application services.
//! Tests can build this library without the heavyweight GPUI renderer.
pub mod editor;
pub mod editor_session;
pub mod model;
pub mod runtime_bridge;
pub mod session;
pub mod terminal;
pub mod vt_screen;

#[cfg(test)]
mod editor_safety;
