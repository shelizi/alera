//! Compact, tooltip-backed toolbar controls for the experimental GPUI UI.
//!
//! Mirror Classic's semantic split: Lucide for ordinary UI controls and
//! VS Code Codicons for source-control actions and file-type fallbacks.
//! Keep this mapping centralized instead of inventing glyphs per toolbar.
use crate::native_file_icons::material_icon_svg;
use alera_gpui_desktop::visual_tokens::{MUTED, TAB_CLOSE_ICON_SIZE, TAB_CLOSE_TARGET_SIZE, TEXT};
use gpui::{div, px, rgb, Div, ElementId, ParentElement, SharedString, Styled};
use gpui_kit::assets::IconName as LucideIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiIcon {
    Add,
    NewFile,
    NewFolder,
    Folder,
    FolderOpen,
    Files,
    File,
    FileCode,
    FileMedia,
    FilePdf,
    FileZip,
    FileBinary,
    Open,
    ChevronUp,
    ChevronDown,
    ChevronRight,
    Refresh,
    CollapseAll,
    Visible,
    Hidden,
    Search,
    List,
    PullRequest,
    Edit,
    Delete,
    More,
    Settings,
    PanelLeft,
    PanelRight,
    ChevronsRight,
    ChevronsLeft,
    Close,
    Pin,
    Terminal,
    Graph,
    Branch,
    GitStage,
    GitUnstage,
    GitDiscard,
    GitRefresh,
    GitCommit,
    GitFetch,
    GitPull,
    GitPush,
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
            Self::File | Self::Files => '\u{ec5e}',
            Self::FileCode => '\u{eae9}',
            Self::FileMedia => '\u{eaea}',
            Self::FilePdf => '\u{eaeb}',
            Self::FileZip => '\u{eaef}',
            Self::FileBinary => '\u{eae8}',
            Self::Open => '\u{eaf7}',
            Self::ChevronUp => '\u{eab7}',
            Self::ChevronDown => '\u{eab4}',
            Self::ChevronRight => '\u{eab6}',
            Self::Refresh => '\u{eb37}',
            Self::CollapseAll => '\u{eac5}',
            Self::Visible | Self::Hidden => '\u{eb62}',
            Self::Search => '\u{ea6d}',
            Self::List => '\u{eb60}',
            Self::PullRequest => '\u{ea64}',
            Self::Edit => '\u{ea73}',
            Self::Delete => '\u{ea81}',
            Self::More => '\u{ea7c}',
            Self::Settings => '\u{eb52}',
            Self::PanelLeft | Self::PanelRight | Self::ChevronsRight => '\u{eab6}',
            Self::ChevronsLeft => '\u{eab5}',
            Self::Close => '\u{ea76}',
            Self::Pin => '\u{eb2b}',
            Self::Terminal => '\u{ea85}',
            Self::Graph => '\u{eb03}',
            Self::Branch => '\u{ec8e}',
            Self::GitUnstage => '\u{eb3b}',
            Self::GitDiscard => '\u{eae2}',
            Self::GitRefresh => '\u{eb37}',
            Self::GitCommit | Self::Check => '\u{eab2}',
            Self::GitFetch => '\u{ecb2}',
            Self::GitPull => '\u{eb40}',
            Self::GitPush => '\u{eb41}',
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

/// Classic's ordinary buttons use Lucide while Git actions use Codicons.
fn lucide_name(kind: UiIcon) -> Option<LucideIcon> {
    Some(match kind {
        UiIcon::Add => LucideIcon::Plus,
        UiIcon::NewFile => LucideIcon::FilePlus,
        UiIcon::NewFolder => LucideIcon::FolderPlus,
        UiIcon::Folder => LucideIcon::Folder,
        UiIcon::FolderOpen | UiIcon::Open => LucideIcon::FolderOpen,
        UiIcon::Files => LucideIcon::Files,
        UiIcon::File => LucideIcon::FileText,
        UiIcon::ChevronUp => LucideIcon::ChevronUp,
        UiIcon::ChevronDown => LucideIcon::ChevronDown,
        UiIcon::ChevronRight => LucideIcon::ChevronRight,
        UiIcon::Refresh => LucideIcon::RefreshCw,
        UiIcon::CollapseAll => LucideIcon::ChevronsDownUp,
        UiIcon::Visible => LucideIcon::Eye,
        UiIcon::Hidden => LucideIcon::EyeOff,
        UiIcon::Search => LucideIcon::Search,
        UiIcon::List => LucideIcon::List,
        UiIcon::PullRequest => LucideIcon::GitPullRequest,
        UiIcon::Edit => LucideIcon::Pencil,
        UiIcon::Delete => LucideIcon::Trash, // GPUI Kit 0.9's Trash SVG is Classic's trash2 shape.
        UiIcon::More => LucideIcon::Ellipsis,
        UiIcon::Settings => LucideIcon::Settings,
        UiIcon::PanelLeft => LucideIcon::PanelLeft,
        UiIcon::PanelRight => LucideIcon::PanelRight,
        UiIcon::ChevronsRight => LucideIcon::ChevronsRight,
        UiIcon::ChevronsLeft => LucideIcon::ChevronsLeft,
        UiIcon::Close => LucideIcon::X,
        UiIcon::Pin => LucideIcon::Pin,
        UiIcon::Terminal => LucideIcon::Terminal,
        UiIcon::Graph => LucideIcon::GitGraph,
        UiIcon::Branch => LucideIcon::GitBranch,
        UiIcon::GitMerge => LucideIcon::GitMerge,
        UiIcon::History => return None, // Draw the missing Classic history SVG below.
        UiIcon::Save => LucideIcon::Save,
        UiIcon::Check => LucideIcon::Check,
        // Keep Classic Codicons for source control, and the differentiated
        // Codicon file-type glyphs until Material SVG icons are integrated.
        UiIcon::FileCode
        | UiIcon::FileMedia
        | UiIcon::FilePdf
        | UiIcon::FileZip
        | UiIcon::FileBinary
        | UiIcon::GitStage
        | UiIcon::GitUnstage
        | UiIcon::GitDiscard
        | UiIcon::GitRefresh
        | UiIcon::GitCommit
        | UiIcon::GitFetch
        | UiIcon::GitPull
        | UiIcon::GitPush
        | UiIcon::SaveAs => return None,
    })
}

/// Classic uses Lucide `history`, which GPUI Kit 0.9 does not expose.
/// Use the original Lucide stroke paths instead of its unrelated clock icon.
/// Lucide artwork is ISC-licensed, matching the existing Lucide asset family.
const CLASSIC_HISTORY_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/>
<path d="M3 3v5h5"/>
<path d="M12 7v5l4 2"/>
</svg>"#;

pub fn icon(icon: UiIcon, size_px: f32, color: u32) -> Div {
    let frame = div()
        .w(px(size_px))
        .h(px(size_px))
        .flex()
        .items_center()
        .justify_center();
    if icon == UiIcon::History {
        return frame.child(
            Icon::default()
                .data(CLASSIC_HISTORY_SVG)
                .with_size(px(size_px))
                .text_color(rgb(color)),
        );
    }
    if let Some(name) = lucide_name(icon) {
        frame.child(
            Icon::new(name)
                .with_size(px(size_px))
                .text_color(rgb(color)),
        )
    } else {
        frame
            .font_family("codicon")
            .text_size(px(size_px))
            .text_color(rgb(color))
            .child(icon.glyph().to_string())
    }
}

/// Show Classic's bundled multicolor Material Icon Theme when available;
/// unknown files keep the old Lucide/Codicon fallback with no runtime IO.
pub fn explorer_file_icon_element(
    name: &str,
    is_directory: bool,
    expanded: bool,
    size_px: f32,
    fallback_color: u32,
) -> Div {
    let frame = div()
        .w(px(size_px))
        .h(px(size_px))
        .flex()
        .items_center()
        .justify_center();
    if let Some(svg) = material_icon_svg(name, is_directory, expanded) {
        frame.child(Icon::default().data(svg).with_size(px(size_px)))
    } else {
        frame.child(icon(
            explorer_file_icon(name, is_directory, expanded),
            size_px,
            fallback_color,
        ))
    }
}

/// Classic's Tab X is a 12px glyph in a compact target, unlike the
/// standard 30px toolbar controls. Preserve that hierarchy in both panes.
pub fn tab_close_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id)
        .tooltip(label)
        .compact()
        .w(px(TAB_CLOSE_TARGET_SIZE))
        .h(px(TAB_CLOSE_TARGET_SIZE))
        .p_0()
        .child(icon(UiIcon::Close, TAB_CLOSE_ICON_SIZE, MUTED))
        .ghost()
}

/// GPUI Kit owns the button, hover, focus, and tooltip semantics. Keep
/// Alera's Codicon glyph mapping for Classic-compatible Git/file icons.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon_kind: UiIcon,
    label: impl Into<SharedString>,
    active: bool,
) -> Button {
    let button = Button::new(id)
        .tooltip(label)
        .compact()
        .w(px(30.0))
        .h(px(30.0))
        .p_0()
        .child(icon(icon_kind, 16.0, if active { TEXT } else { MUTED }));
    if active {
        button.secondary()
    } else {
        button.ghost()
    }
}

/// Compact Classic toolbar action for the default 315px sidebar.
pub fn sidebar_icon_button(
    id: impl Into<ElementId>,
    icon_kind: UiIcon,
    label: impl Into<SharedString>,
    active: bool,
) -> Button {
    let button = Button::new(id)
        .tooltip(label)
        .compact()
        .w(px(26.0))
        .h(px(26.0))
        .p_0()
        .child(icon(icon_kind, 15.0, if active { TEXT } else { MUTED }));
    if active {
        button.secondary()
    } else {
        button.ghost()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSIC_CODICONS: &str =
        include_str!("../../lib/src/design_system/icons/alera_codicons.dart");
    const CLASSIC_BUTTON: &str =
        include_str!("../../lib/src/design_system/buttons/alera_icon_button.dart");
    const CLASSIC_ICONS: &str = include_str!("../../lib/src/design_system/icons/alera_icons.dart");

    #[test]
    fn git_action_codicons_match_classic_semantic_registry() {
        for (name, icon) in [
            ("add", UiIcon::GitStage),
            ("remove", UiIcon::GitUnstage),
            ("discard", UiIcon::GitDiscard),
            ("check", UiIcon::GitCommit),
            ("gitFetch", UiIcon::GitFetch),
            ("repoPull", UiIcon::GitPull),
            ("repoPush", UiIcon::GitPush),
        ] {
            let expected = format!(
                "static const IconData {name} = IconData(0x{:x}, fontFamily: _family);",
                icon.glyph() as u32,
            );
            assert!(
                CLASSIC_CODICONS.contains(&expected),
                "Classic icon drift: {name}"
            );
        }
    }

    #[test]
    fn common_action_icons_match_classic_lucide_registry() {
        for (classic_name, classic_variant, native) in [
            ("add", "plus", UiIcon::Add),
            ("newFile", "filePlus", UiIcon::NewFile),
            ("newFolder", "folderPlus", UiIcon::NewFolder),
            ("copyFiles", "files", UiIcon::Files),
            ("refresh", "refreshCw", UiIcon::Refresh),
            ("search", "search", UiIcon::Search),
            ("delete", "trash2", UiIcon::Delete),
            ("gitBranch", "gitBranch", UiIcon::Branch),
            ("gitGraph", "gitGraph", UiIcon::Graph),
        ] {
            assert!(
                CLASSIC_ICONS.contains(&format!(
                    "static const IconData {classic_name} = LucideIcons.{classic_variant};"
                )),
                "Classic Lucide mapping drift: {classic_name}",
            );
            assert!(
                lucide_name(native).is_some(),
                "Missing Lucide: {classic_name}"
            );
        }
        assert_eq!(lucide_name(UiIcon::GitStage), None);
        assert_eq!(lucide_name(UiIcon::GitDiscard), None);
    }

    #[test]
    fn toolbar_button_size_matches_classic() {
        assert!(CLASSIC_BUTTON.contains("defaultIconSize = 16;"));
        assert!(CLASSIC_BUTTON.contains("defaultMinSize = 30;"));
    }
}
