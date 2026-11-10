//! Adapt GPUI Kit's semantic component palette to the existing Alera dark
//! shell. Keep this at the application root, never hardcode per-widget fixes.

use alera_gpui_desktop::visual_tokens::{
    ACTIVE, BG, BORDER, ELEVATED, MUTED, PANEL, TEXT, TOOLBAR,
};
use gpui::{rgb, App};
use gpui_kit::component::theme::{Theme, ThemeColor, ThemeMode};

/// Kit explicitly defaults to Light even when Alera's shell paints dark.
/// Switching mode first is important: a later mode change reloads the palette
/// and would otherwise erase the Classic-derived color overrides.
pub(super) fn install_alera_dark_theme(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.colors = alera_dark_palette(theme.colors);
    });
}

fn alera_dark_palette(mut colors: ThemeColor) -> ThemeColor {
    // Main window surfaces and text must match Flutter AleraTokens.
    colors.background = rgb(BG).into();
    colors.foreground = rgb(TEXT).into();
    colors.muted_foreground = rgb(MUTED).into();
    colors.border = rgb(BORDER).into();
    colors.input = rgb(BORDER).into();
    colors.caret = rgb(TEXT).into();
    colors.selection = rgb(ACTIVE).into();
    colors.muted = rgb(PANEL).into();
    colors.accent = rgb(ACTIVE).into();
    colors.accent_foreground = rgb(TEXT).into();

    // Buttons: Kit's default Light buttons caused white blocks on the dark
    // Terminal tab strip. The outline/default/primary variants all inherit
    // dark surfaces without sacrificing text contrast.
    colors.button = rgb(PANEL).into();
    colors.button_foreground = rgb(TEXT).into();
    colors.button_hover = rgb(ACTIVE).into();
    colors.button_active = rgb(ACTIVE).into();
    colors.button_primary = rgb(ELEVATED).into();
    colors.button_primary_foreground = rgb(TEXT).into();
    colors.button_primary_hover = rgb(ACTIVE).into();
    colors.button_primary_active = rgb(ACTIVE).into();
    colors.button_secondary = rgb(PANEL).into();
    colors.button_secondary_foreground = rgb(TEXT).into();
    colors.button_secondary_hover = rgb(ACTIVE).into();
    colors.button_secondary_active = rgb(ACTIVE).into();
    colors.secondary = rgb(PANEL).into();
    colors.secondary_foreground = rgb(TEXT).into();
    colors.secondary_hover = rgb(ACTIVE).into();
    colors.secondary_active = rgb(ACTIVE).into();

    // Explorer/Git segmented tabs, Popups and Dialogs are also Kit-owned.
    colors.tab = rgb(PANEL).into();
    colors.tab_foreground = rgb(MUTED).into();
    colors.tab_active = rgb(ACTIVE).into();
    colors.tab_active_foreground = rgb(TEXT).into();
    colors.tab_bar = rgb(TOOLBAR).into();
    colors.tab_bar_segmented = rgb(PANEL).into();
    colors.popover = rgb(ELEVATED).into();
    colors.popover_foreground = rgb(TEXT).into();
    // Preserve Kit Dark's translucent dialog backdrop. Making this an
    // opaque Alera surface would hide the entire workbench under a modal.
    colors.list = rgb(PANEL).into();
    colors.list_active = rgb(ACTIVE).into();
    colors.list_hover = rgb(ACTIVE).into();
    colors.sidebar = rgb(PANEL).into();
    colors.sidebar_foreground = rgb(TEXT).into();
    colors.sidebar_border = rgb(BORDER).into();
    colors.sidebar_accent = rgb(ACTIVE).into();
    colors.sidebar_accent_foreground = rgb(TEXT).into();
    colors.title_bar = rgb(TOOLBAR).into();
    colors.title_bar_border = rgb(BORDER).into();
    colors.status_bar = rgb(TOOLBAR).into();
    colors.status_bar_border = rgb(BORDER).into();
    colors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kit_controls_share_alera_shell_colors() {
        let theme = alera_dark_palette(ThemeColor::default());
        assert_eq!(theme.background, rgb(BG).into());
        assert_eq!(theme.foreground, rgb(TEXT).into());
        assert_eq!(theme.button, rgb(PANEL).into());
        assert_eq!(theme.button_foreground, rgb(TEXT).into());
        assert_eq!(theme.tab_bar_segmented, rgb(PANEL).into());
        assert_eq!(theme.tab_active, rgb(ACTIVE).into());
        assert_eq!(theme.tab_active_foreground, rgb(TEXT).into());
        assert_eq!(theme.popover, rgb(ELEVATED).into());
    }
}
