//! Compact, tooltip-backed toolbar controls for the experimental GPUI UI.
//!
//! Use Alera's existing VS Code Codicon font instead of hard-coded emoji or
//! one-off icon substitutes. Flutter Classic already bundles this font and
//! assigns these same glyphs to version-control operations.
use gpui::{
    div, px, rgb, AppContext, Context, Div, ElementId, InteractiveElement, IntoElement,
    ParentElement, Render, Stateful, StatefulInteractiveElement, Styled, Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiIcon {
    Add,
    NewFile,
    NewFolder,
    Folder,
    FolderOpen,
    File,
    FileCode,
    FileMedia,
    FilePdf,
    FileZip,
    FileBinary,
    Open,
    ChevronDown,
    ChevronRight,
    Refresh,
    CollapseAll,
    Sync,
    Search,
    Edit,
    Copy,
    Delete,
    More,
    Settings,
    Close,
    Terminal,
    Graph,
    Branch,
    GitStage,
    GitUnstage,
    GitCommit,
    GitFetch,
    GitPull,
    GitPush,
    GitStash,
    GitStashApply,
    GitMerge,
    History,
    Save,
    SaveAs,
    Check,
}

impl UiIcon {
    pub const fn glyph(self) -> char {
        match self {
            Self::Add | Self::GitStage => '\u{ea60}',
            Self::NewFile => '\u{ea7f}',
            Self::NewFolder => '\u{ea80}',
            Self::Folder => '\u{ea83}',
            Self::FolderOpen => '\u{eaf7}',
            Self::File => '\u{ec5e}',
            Self::FileCode => '\u{eae9}',
            Self::FileMedia => '\u{eaea}',
            Self::FilePdf => '\u{eaeb}',
            Self::FileZip => '\u{eaef}',
            Self::FileBinary => '\u{eae8}',
            Self::Open => '\u{eaf7}',
            Self::ChevronDown => '\u{eab4}',
            Self::ChevronRight => '\u{eab6}',
            Self::Refresh => '\u{eb37}',
            Self::CollapseAll => '\u{eac5}',
            Self::Sync => '\u{ea77}',
            Self::Search => '\u{ea6d}',
            Self::Edit => '\u{ea73}',
            Self::Copy => '\u{ebcc}',
            Self::Delete => '\u{ea81}',
            Self::More => '\u{ea7c}',
            Self::Settings => '\u{eb52}',
            Self::Close => '\u{ea76}',
            Self::Terminal => '\u{ea85}',
            Self::Graph => '\u{eb03}',
            Self::Branch => '\u{ec8e}',
            Self::GitUnstage => '\u{eb3b}',
            Self::GitCommit | Self::Check => '\u{eab2}',
            Self::GitFetch => '\u{ecb2}',
            Self::GitPull => '\u{eb40}',
            Self::GitPush => '\u{eb41}',
            Self::GitStash => '\u{ec26}',
            Self::GitStashApply => '\u{ec27}',
            Self::GitMerge => '\u{eafe}',
            Self::History => '\u{ea82}',
            Self::Save => '\u{eb4b}',
            Self::SaveAs => '\u{eb4a}',
        }
    }
}

/// Lightweight approximation of Flutter Classic's Material Icon Theme:
/// one distinct file-type glyph, rather than the Native placeholder dot.
pub fn explorer_file_icon(name: &str, is_directory: bool, expanded: bool) -> UiIcon {
    if is_directory {
        return if expanded {
            UiIcon::FolderOpen
        } else {
            UiIcon::Folder
        };
    }
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" | "dart" | "js" | "jsx" | "ts" | "tsx" | "py" | "go" | "java" | "cpp" | "c" | "h"
        | "cs" | "php" | "swift" | "kt" | "json" | "yaml" | "yml" | "xml" | "toml" | "html"
        | "css" | "sh" | "ps1" | "sql" => UiIcon::FileCode,
        "png" | "jpg" | "jpeg" | "webp" | "svg" | "gif" | "ico" | "bmp" | "mp4" | "mp3"
        | "webm" | "mov" | "wav" => UiIcon::FileMedia,
        "pdf" => UiIcon::FilePdf,
        "zip" | "7z" | "rar" | "tar" | "gz" | "epub" => UiIcon::FileZip,
        "exe" | "dll" | "so" | "bin" | "wasm" | "db" => UiIcon::FileBinary,
        _ => UiIcon::File,
    }
}

pub fn icon(icon: UiIcon, size_px: f32, color: u32) -> Div {
    div()
        .font_family("codicon")
        .text_size(px(size_px))
        .text_color(rgb(color))
        .child(icon.glyph().to_string())
}

struct IconTooltip {
    label: &'static str,
}

impl Render for IconTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(0x252931))
            .border_1()
            .border_color(rgb(0x454b55))
            .text_size(px(12.0))
            .text_color(rgb(0xf1f3f7))
            .child(self.label)
    }
}

/// A 30 px hit target, uniform hover/focus treatment, and discoverable
/// semantic tooltip. The caller provides the click action; no inert buttons.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon_kind: UiIcon,
    label: &'static str,
    active: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(30.0))
        .h(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(if active { 0x343b46 } else { 0x22262e }))
        .hover(|style| style.bg(rgb(0x414956)))
        .tooltip(move |_, cx| cx.new(|_| IconTooltip { label }).into())
        .child(icon(
            icon_kind,
            16.0,
            if active { 0xf4f6fa } else { 0xc6cbd4 },
        ))
}
